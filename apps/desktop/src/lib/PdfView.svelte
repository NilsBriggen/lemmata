<script lang="ts" module>
  import type { PDFDocumentProxy } from "pdfjs-dist";
  export type Fit = "width" | "page" | number;
  /** One page per row, or spreads: "spread" leaves page 1 alone (2–3, 4–5 …, like a book), "spread-first" pairs 1–2, 3–4 … */
  export type PageLayout = "single" | "spread" | "spread-first";

  // A running presentation keeps rendering from a document a rebuild has replaced,
  // so destroying it waits until the presentation lets go.
  const holds = new Map<object, number>();
  const retired = new Set<PDFDocumentProxy>();

  function retire(d: PDFDocumentProxy) {
    if (holds.has(d)) retired.add(d);
    else d.loadingTask.destroy();
  }

  /** Keeps `d` alive until the returned function is called. */
  export function holdDocument(d: PDFDocumentProxy): () => void {
    holds.set(d, (holds.get(d) ?? 0) + 1);
    return () => {
      const n = (holds.get(d) ?? 1) - 1;
      if (n > 0) return void holds.set(d, n);
      holds.delete(d);
      if (retired.delete(d)) d.loadingTask.destroy();
    };
  }
</script>

<script lang="ts">
  // Renders the build's PDF with PDF.js. A new PDF replaces the old one page
  // by page in place, so the reader keeps their position across rebuilds.
  import { onDestroy, onMount, untrack } from "svelte";
  import type { PDFPageProxy, RenderTask, TextLayer } from "pdfjs-dist";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import type { PdfBox } from "./api";
  import { loadPdfjs } from "./presenter/render";
  import { settings } from "./settings.svelte";

  /** Set by load(); every later use of pdf.js happens after a document exists. */
  let lib: Awaited<ReturnType<typeof loadPdfjs>> | null = null;

  export interface OutlineNode { title: string; dest: unknown; url: string | null; items: OutlineNode[] }

  interface Props {
    /** Called when the user double-clicks or Ctrl/Cmd-clicks a point. Coordinates are PDF points from the top-left. */
    onSourceClick: (page: number, x: number, y: number) => void;
    pageCount?: number;
    currentPage?: number;
    pageLayout?: PageLayout;
    /** Current scale in percent of the PDF's real size, for display. */
    percent?: number;
    /** Search results: how many, and which one is current (1-based, 0 when none). */
    matchCount?: number;
    matchIndex?: number;
    /** For the empty state: a build is running, or the last one ended without a PDF. */
    building?: boolean;
    failed?: boolean;
    onShowLogs?: () => void;
  }
  let {
    onSourceClick,
    building = false,
    failed = false,
    onShowLogs,
    pageLayout = "single",
    pageCount = $bindable(0),
    currentPage = $bindable(1),
    percent = $bindable(100),
    matchCount = $bindable(0),
    matchIndex = $bindable(0),
  }: Props = $props();

  interface Slot {
    n: number;
    doc: PDFDocumentProxy;
    /** Fetched lazily; until then w/h are page 1's size. */
    page: PDFPageProxy | null;
    wrap: HTMLDivElement;
    /** Page size in PDF points. */
    w: number;
    h: number;
    scale: number;
    token: number;
    task: RenderTask | null;
    text: TextLayer | null;
    /** 0 nothing drawn, 1 rendering, 2 canvas in place. */
    state: 0 | 1 | 2;
    /** The text/link layer was built (or is being built) for the current scale. */
    decorated: boolean;
    cancelIdle: (() => void) | null;
  }
  interface Hit { page: number; rects: { x: number; y: number; w: number; h: number }[] }

  let scroller: HTMLDivElement;
  let pagesEl = $state<HTMLDivElement>()!;
  let doc: PDFDocumentProxy | null = null;
  let slots: Slot[] = [];
  /** Pages near the viewport; only these hold a canvas, text layer and links. */
  const near = new Set<number>();
  let io: IntersectionObserver | null = null;
  /** Wider margin: a page that leaves it gives its canvas back. */
  let far: IntersectionObserver | null = null;
  /** Largest canvas backing store, in pixels. */
  const MAX_PIXELS = 16e6;
  /** "width" and "page" fit the pane; a number is a fixed scale (1 = 100%). */
  let fit = $state<Fit>("width");
  /** Pages are laid out side by side (a spread layout and more than one page). */
  let twoUp = $state(false);
  let appliedLayout: PageLayout = "single";
  const GAP = 14;
  let highlight = $state<{ page: number; left: number; top: number; width: number; height: number } | null>(null);
  let loadSeq = 0;
  /** Bumped whenever page boxes move, so overlays recompute their positions. */
  let layoutVersion = $state(0);
  let nearPages = $state<number[]>([]);
  let hits = $state<Hit[]>([]);
  let searchSeq = 0;
  const textCache = new Map<number, { full: string; starts: number[]; items: { str: string; transform: number[]; width: number; height: number }[] }>();

  /** CSS pixels per PDF point for each page at the current zoom. */
  const scales: number[] = [];
  const dpr = () => window.devicePixelRatio || 1;

  /** PDF points are 1/72 in; CSS pixels are 1/96 in. */
  const PT = 96 / 72;

  function scaleFor(w: number, h: number) {
    const cols = twoUp ? 2 : 1;
    const availW = Math.max(160, (scroller.clientWidth - 48 - GAP * (cols - 1)) / cols);
    const availH = Math.max(160, scroller.clientHeight - 40);
    if (fit === "width") return availW / w;
    if (fit === "page") return Math.min(availW / w, availH / h);
    return fit * PT;
  }

  function observe(wrap: HTMLElement) {
    io ??= new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          const n = Number((e.target as HTMLElement).dataset.page);
          if (e.isIntersecting) {
            near.add(n);
            renderSlot(n);
          } else near.delete(n);
        }
        nearPages = [...near];
      },
      { root: scroller, rootMargin: "50% 0px" },
    );
    far ??= new IntersectionObserver(
      (entries) => {
        for (const e of entries) if (!e.isIntersecting) releaseSlot(Number((e.target as HTMLElement).dataset.page));
      },
      { root: scroller, rootMargin: "200% 0px" },
    );
    io.observe(wrap);
    far.observe(wrap);
  }

  const idle = (fn: () => void) => {
    if ("requestIdleCallback" in window) {
      const h = requestIdleCallback(fn, { timeout: 1000 });
      return () => cancelIdleCallback(h);
    }
    const h = setTimeout(fn, 150);
    return () => clearTimeout(h);
  };

  export function getDocument() {
    return doc;
  }

  export async function load(data: Uint8Array) {
    const seq = ++loadSeq;
    lib = await loadPdfjs();
    const next = await lib.getDocument({ data }).promise;
    if (seq !== loadSeq) {
      next.loadingTask.destroy();
      return;
    }
    const old = doc;
    doc = next;
    textCache.clear();
    pageCount = next.numPages;
    if (await layout(seq)) for (const n of near) renderSlot(n);
    if (old) retire(old);
    if (pendingScroll !== null) {
      scroller.scrollTop = pendingScroll * scroller.scrollHeight;
      pendingScroll = null;
    }
    if (hits.length) lastQuery && search(lastQuery, false);
  }

  /** Sizes every page's box from page 1 (or what is already known), so layout
   *  is final before anything is drawn. Other pages correct themselves when
   *  fetched (see ensurePage). Boxes are reused across loads to keep the position. */
  async function layout(seq: number): Promise<boolean> {
    if (!doc) return false;
    const d = doc;
    const first = await d.getPage(1);
    if (seq !== loadSeq) return false;
    appliedLayout = pageLayout;
    twoUp = pageLayout !== "single" && d.numPages > 1;
    const base = first.getViewport({ scale: 1 });
    const prev = slots;
    for (const s of prev) {
      s.token++;
      s.task?.cancel();
      s.text?.cancel();
      s.cancelIdle?.();
    }
    while (pagesEl.children.length > d.numPages) {
      const last = pagesEl.lastElementChild as HTMLElement;
      io?.unobserve(last);
      far?.unobserve(last);
      near.delete(Number(last.dataset.page));
      last.remove();
    }
    slots = [];
    for (let i = 0; i < d.numPages; i++) {
      const n = i + 1;
      const old = prev[i]?.doc === d ? prev[i] : null;
      const page = old?.page ?? (i === 0 ? first : null);
      const w = page ? (old?.w ?? base.width) : base.width;
      const h = page ? (old?.h ?? base.height) : base.height;
      const scale = scaleFor(w, h);
      scales[i] = scale;
      if (n === 1) percent = Math.round((scale / PT) * 100);
      let wrap = pagesEl.children[i] as HTMLDivElement | undefined;
      if (!wrap) {
        wrap = document.createElement("div");
        wrap.className = "page";
        wrap.dataset.page = String(n);
        pagesEl.appendChild(wrap);
        observe(wrap);
      }
      wrap.style.width = `${w * scale}px`;
      wrap.style.height = `${h * scale}px`;
      slots.push({ n, doc: d, page, wrap, w, h, scale, token: 0, task: null, text: null, state: 0, decorated: false, cancelIdle: null });
      // Text and links are at the old scale; pages out of view keep nothing from the previous document.
      clearLayers(wrap, !near.has(n));
    }
    layoutVersion++;
    return true;
  }

  /** The page proxy; a page whose size differs from the guess resizes its box,
   *  keeping the scroll position when it sits above the viewport. */
  async function ensurePage(slot: Slot): Promise<PDFPageProxy | null> {
    if (slot.page) return slot.page;
    const page = await slot.doc.getPage(slot.n);
    if (slots[slot.n - 1] !== slot) return null;
    slot.page = page;
    const v = page.getViewport({ scale: 1 });
    if (Math.abs(v.width - slot.w) > 0.01 || Math.abs(v.height - slot.h) > 0.01) {
      const { wrap } = slot;
      const before = wrap.offsetHeight;
      const above = wrap.offsetTop + before <= scroller.scrollTop;
      slot.w = v.width;
      slot.h = v.height;
      slot.scale = scales[slot.n - 1] = scaleFor(v.width, v.height);
      wrap.style.width = `${v.width * slot.scale}px`;
      wrap.style.height = `${v.height * slot.scale}px`;
      if (above) scroller.scrollTop += wrap.offsetHeight - before;
      layoutVersion++;
    }
    return page;
  }

  function clearLayers(wrap: HTMLElement, canvasToo: boolean) {
    wrap.querySelectorAll(".textLayer, .links").forEach((el) => el.remove());
    if (canvasToo) wrap.querySelectorAll("canvas").forEach((c) => ((c.width = 0), c.remove()));
  }

  function releaseSlot(n: number) {
    const slot = slots[n - 1];
    if (!slot) return;
    slot.token++;
    slot.task?.cancel();
    slot.text?.cancel();
    slot.cancelIdle?.();
    slot.task = slot.text = slot.cancelIdle = null;
    slot.state = 0;
    slot.decorated = false;
    clearLayers(slot.wrap, true);
  }

  async function renderSlot(n: number) {
    const slot = slots[n - 1];
    if (!slot || slot.state) return;
    slot.state = 1;
    const token = ++slot.token;
    const page = await ensurePage(slot).catch(() => null); // the document was replaced under us
    if (!page || token !== slot.token) return;
    const viewport = page.getViewport({ scale: slot.scale * dpr() });
    // Huge zoom or pages: shrink the backing store; CSS stretches it back.
    const k = Math.min(1, Math.sqrt(MAX_PIXELS / (viewport.width * viewport.height)));
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.floor(viewport.width * k));
    canvas.height = Math.max(1, Math.floor(viewport.height * k));
    const ctx = canvas.getContext("2d")!;
    const task = page.render({ canvas, canvasContext: ctx, viewport: k < 1 ? page.getViewport({ scale: slot.scale * dpr() * k }) : viewport });
    slot.task = task;
    try {
      await task.promise;
    } catch (e) {
      if ((e as { name?: string })?.name === "RenderingCancelledException") return;
      slot.state = 0;
      throw e;
    }
    // Swap on a frame boundary so the old canvas stays up until the new one can replace it.
    await new Promise((r) => requestAnimationFrame(r));
    if (token !== slot.token || slots[n - 1] !== slot) return;
    slot.task = null;
    slot.state = 2;
    clearLayers(slot.wrap, true);
    slot.wrap.prepend(canvas);
    slot.cancelIdle = idle(() => {
      slot.cancelIdle = null;
      decorate(slot);
    });
  }

  /** The selectable text layer and clickable links over a page, built when the
   *  page is idle and visible, or at once when the pointer goes down on it. */
  async function decorate(slot: Slot) {
    if (slot.decorated || slots[slot.n - 1] !== slot) return;
    try {
      await buildLayers(slot);
    } catch {
      /* the document was replaced while this was building */
    }
  }

  async function buildLayers(slot: Slot) {
    slot.decorated = true;
    slot.cancelIdle?.();
    slot.cancelIdle = null;
    const token = slot.token;
    const { wrap } = slot;
    const page = await ensurePage(slot);
    if (!page || token !== slot.token) return;
    const viewport = page.getViewport({ scale: slot.scale });
    wrap.style.setProperty("--scale-factor", String(slot.scale));
    wrap.style.setProperty("--total-scale-factor", String(slot.scale));
    wrap.style.setProperty("--user-unit", "1");

    const textDiv = document.createElement("div");
    textDiv.className = "textLayer";
    wrap.append(textDiv);
    const layer = new lib!.TextLayer({ textContentSource: page.streamTextContent(), container: textDiv, viewport });
    slot.text = layer;
    const annotations = await page.getAnnotations({ intent: "display" });
    if (token !== slot.token) return;
    const links = document.createElement("div");
    links.className = "links";
    for (const a of annotations) {
      if (a.subtype !== "Link" || !(a.dest || a.url || a.unsafeUrl)) continue;
      const [x1, y1] = viewport.convertToViewportPoint(a.rect[0], a.rect[1]);
      const [x2, y2] = viewport.convertToViewportPoint(a.rect[2], a.rect[3]);
      const el = document.createElement("a");
      el.style.cssText = `left:${Math.min(x1, x2)}px;top:${Math.min(y1, y2)}px;width:${Math.abs(x2 - x1)}px;height:${Math.abs(y2 - y1)}px`;
      const url: string | null = a.url ?? a.unsafeUrl ?? null;
      if (url) el.title = url;
      el.onclick = (e) => {
        e.preventDefault();
        e.stopPropagation();
        if (a.dest) goTo(a.dest);
        else if (url && /^https?:\/\//i.test(url)) openUrl(url).catch(() => {});
      };
      links.append(el);
    }
    wrap.append(links);
    try {
      await layer.render();
    } catch {
      /* cancelled by a newer render */
    }
  }

  function onPointerDown(e: PointerEvent) {
    const wrap = (e.target as HTMLElement).closest(".page") as HTMLElement | null;
    const slot = wrap && slots[Number(wrap.dataset.page) - 1];
    if (slot) decorate(slot);
  }

  /** Scrolls to a PDF destination: a name or an explicit [page, "XYZ", x, y, zoom]. */
  export async function goTo(dest: unknown) {
    if (!doc) return;
    try {
      const explicit = typeof dest === "string" ? await doc.getDestination(dest) : dest;
      if (!Array.isArray(explicit)) return;
      const ref = explicit[0];
      const index = typeof ref === "object" && ref !== null ? await doc.getPageIndex(ref as never) : Number(ref);
      const slot = slots[index];
      if (!slot) return;
      await ensurePage(slot);
      const kind = explicit[1]?.name;
      const top = kind === "XYZ" ? explicit[3] : kind === "FitH" || kind === "FitBH" ? explicit[2] : null;
      const y = typeof top === "number" ? (slot.h - top) * slot.scale : 0;
      scroller.scrollTo({ top: slot.wrap.offsetTop + y - 16, behavior: "smooth" });
    } catch {
      /* a broken link does nothing */
    }
  }

  export async function getOutline(): Promise<OutlineNode[]> {
    const raw = (await doc?.getOutline()) ?? [];
    const conv = (items: typeof raw): OutlineNode[] =>
      items.map((i) => ({ title: i.title, dest: i.dest, url: i.url ?? null, items: conv(i.items as typeof raw) }));
    return conv(raw);
  }

  // ---- Search -----------------------------------------------------------

  let lastQuery = "";

  async function pageText(n: number) {
    let t = textCache.get(n);
    if (t) return t;
    const page = slots[n - 1]?.page ?? (await doc!.getPage(n));
    const content = await page.getTextContent();
    let full = "";
    const starts: number[] = [];
    const items: { str: string; transform: number[]; width: number; height: number }[] = [];
    for (const it of content.items) {
      if (!("str" in it)) continue;
      starts.push(full.length);
      items.push({ str: it.str, transform: it.transform, width: it.width, height: it.height });
      full += it.str + (it.hasEOL ? " " : "");
    }
    t = { full, starts, items };
    textCache.set(n, t);
    return t;
  }

  /** Finds `query` on every page and highlights the matches. */
  export async function search(query: string, jump = true) {
    const seq = ++searchSeq;
    lastQuery = query;
    hits = [];
    matchCount = 0;
    matchIndex = 0;
    const q = query.trim().toLowerCase();
    if (!q || !doc) return;
    const found: Hit[] = [];
    for (let n = 1; n <= doc.numPages; n++) {
      const t = await pageText(n);
      if (seq !== searchSeq) return;
      const hay = t.full.toLowerCase();
      const vp = (slots[n - 1]?.page ?? (await doc.getPage(n))).getViewport({ scale: 1 });
      for (let at = hay.indexOf(q); at !== -1; at = hay.indexOf(q, at + q.length)) {
        const rects: Hit["rects"] = [];
        for (let i = 0; i < t.items.length; i++) {
          const s = t.starts[i];
          const len = t.items[i].str.length;
          if (!len || s + len <= at || s >= at + q.length) continue;
          const a = (Math.max(at, s) - s) / len;
          const b = (Math.min(at + q.length, s + len) - s) / len;
          const it = t.items[i];
          const [px, py] = vp.convertToViewportPoint(it.transform[4] + it.width * a, it.transform[5]);
          const height = it.height || Math.abs(it.transform[3]);
          rects.push({ x: px, y: py - height * 0.85, w: it.width * (b - a), h: height * 1.05 });
        }
        if (rects.length) found.push({ page: n, rects });
      }
      if (n % 10 === 0) await new Promise((r) => setTimeout(r));
    }
    if (seq !== searchSeq) return;
    hits = found;
    matchCount = found.length;
    if (found.length && jump) {
      const first = found.findIndex((h) => h.page >= currentPage);
      step(first === -1 ? 0 : first);
    }
  }

  function step(index: number) {
    if (!hits.length) return;
    matchIndex = ((index % hits.length) + hits.length) % hits.length;
    const h = hits[matchIndex];
    matchIndex = matchIndex + 1;
    const slot = slots[h.page - 1];
    if (!slot) return;
    const y = h.rects[0].y * slot.scale;
    scroller.scrollTo({ top: slot.wrap.offsetTop + y - scroller.clientHeight / 3, behavior: "smooth" });
  }

  export function searchStep(dir: 1 | -1) {
    step(matchIndex - 1 + dir);
  }

  export function clearSearch() {
    searchSeq++;
    lastQuery = "";
    hits = [];
    matchCount = 0;
    matchIndex = 0;
  }

  export function getPosition(): { fit: Fit; scroll: number } {
    return { fit, scroll: scroller.scrollTop / Math.max(1, scroller.scrollHeight) };
  }

  /** Restores zoom and scroll; applied after the next PDF loads if none is shown yet. */
  export async function setPosition(pos: { fit: Fit; scroll: number }) {
    fit = pos.fit;
    if (doc && (await layout(loadSeq))) for (const n of near) renderSlot(n);
    scroller.scrollTop = pos.scroll * scroller.scrollHeight;
    pendingScroll = doc ? null : pos.scroll;
  }
  let pendingScroll: number | null = null;

  /** Page 1 as a small PNG, for the project library. */
  export async function thumbnail(width = 360): Promise<Uint8Array | null> {
    if (!doc) return null;
    const page = await doc.getPage(1);
    const base = page.getViewport({ scale: 1 });
    const viewport = page.getViewport({ scale: width / base.width });
    const canvas = document.createElement("canvas");
    canvas.width = Math.floor(viewport.width);
    canvas.height = Math.floor(viewport.height);
    await page.render({ canvas, canvasContext: canvas.getContext("2d")!, viewport }).promise;
    const blob = await new Promise<Blob | null>((r) => canvas.toBlob(r, "image/png"));
    return blob ? new Uint8Array(await blob.arrayBuffer()) : null;
  }

  export function showBoxes(boxes: PdfBox[]) {
    const b = boxes[0];
    if (!b) return;
    const s = scales[b.page - 1] ?? 1;
    const height = Math.max(b.height, 10);
    highlight = { page: b.page, left: b.h * s - 4, top: (b.v - height) * s - 3, width: Math.max(b.width, 40) * s + 8, height: height * s + 6 };
    const wrap = pagesEl.children[b.page - 1] as HTMLElement | undefined;
    if (wrap) {
      scroller.scrollTo({ top: wrap.offsetTop + highlight.top - scroller.clientHeight / 3, behavior: "smooth" });
    }
    setTimeout(() => (highlight = null), 1800);
  }

  function handleClick(e: MouseEvent) {
    const isSync = e.type === "dblclick" || e.ctrlKey || e.metaKey;
    if (!isSync) return;
    window.getSelection()?.removeAllRanges();
    const wrap = (e.target as HTMLElement).closest(".page") as HTMLElement | null;
    if (!wrap) return;
    const n = Number(wrap.dataset.page);
    const rect = wrap.getBoundingClientRect();
    const s = scales[n - 1] ?? 1;
    onSourceClick(n, (e.clientX - rect.left) / s, (e.clientY - rect.top) / s);
  }

  function onScroll() {
    const mid = scroller.scrollTop + scroller.clientHeight / 2;
    for (const el of pagesEl.children as HTMLCollectionOf<HTMLElement>) {
      if (el.offsetTop + el.offsetHeight > mid) {
        currentPage = Number(el.dataset.page);
        break;
      }
    }
  }

  /** Changes the zoom, keeping the same part of the document in view. */
  export async function setFit(next: Fit) {
    const ratio = scroller.scrollTop / Math.max(1, scroller.scrollHeight);
    fit = typeof next === "number" ? Math.min(4, Math.max(0.25, next)) : next;
    if (await layout(loadSeq)) for (const n of near) renderSlot(n);
    scroller.scrollTop = ratio * scroller.scrollHeight;
  }

  export function zoomBy(factor: number) {
    return setFit(Math.round((percent / 100) * factor * 20) / 20);
  }

  export function gotoPage(n: number) {
    const el = pagesEl.children[Math.max(0, Math.min(pageCount - 1, n - 1))] as HTMLElement | undefined;
    if (el) scroller.scrollTo({ top: el.offsetTop - 12, behavior: "smooth" });
  }

  function onWheel(e: WheelEvent) {
    // Ctrl+wheel zooms, like a browser's PDF viewer.
    if (!e.ctrlKey || !doc) return;
    e.preventDefault();
    zoomBy(e.deltaY < 0 ? 1.1 : 1 / 1.1);
  }

  // Fitted layouts follow the pane's size (window resize, splitter drag).
  let resizeTimer: ReturnType<typeof setTimeout> | undefined;
  onMount(() => {
    let lastWidth = scroller.clientWidth;
    const ro = new ResizeObserver(() => {
      if (typeof fit === "number" || !doc || Math.abs(scroller.clientWidth - lastWidth) < 4) return;
      lastWidth = scroller.clientWidth;
      clearTimeout(resizeTimer);
      resizeTimer = setTimeout(() => setFit(fit), 120);
    });
    ro.observe(scroller);
    return () => ro.disconnect();
  });

  // A changed layout re-lays the pages out, keeping the same part in view.
  $effect(() => {
    if (pageLayout === appliedLayout || !untrack(() => doc)) return;
    const n = untrack(() => currentPage);
    untrack(() => setFit(fit)).then(() => {
      // Rows move when pages pair up, so keep the page rather than the scroll ratio.
      const el = pagesEl.children[n - 1] as HTMLElement | undefined;
      if (el) scroller.scrollTop = el.offsetTop - 12;
    });
  });

  onDestroy(() => {
    io?.disconnect();
    far?.disconnect();
    if (doc) retire(doc);
  });
</script>

<div class="pdf" class:dark-pages={settings.pdfDark}>
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events, a11y_no_noninteractive_tabindex -->
  <div class="scroller" tabindex="0" bind:this={scroller} onscroll={onScroll} onpointerdown={onPointerDown} onclick={handleClick} ondblclick={handleClick} onwheel={onWheel}>
    <div class="pages" class:two={twoUp} class:first={pageLayout === "spread-first"} bind:this={pagesEl}></div>
    {#if highlight}
      {@const wrap = pagesEl.children[highlight.page - 1] as HTMLElement | undefined}
      {#if wrap}
        <div
          class="sync"
          style:left="{wrap.offsetLeft + highlight.left}px"
          style:top="{wrap.offsetTop + highlight.top}px"
          style:width="{highlight.width}px"
          style:height="{highlight.height}px"
        ></div>
      {/if}
    {/if}
    {#each hits as h, i (i)}
      {#if nearPages.includes(h.page)}
        {@const wrap = (void layoutVersion, pagesEl.children[h.page - 1] as HTMLElement | undefined)}
        {#if wrap}
          {#each h.rects as r}
            <div
              class="hit"
              class:current={i === matchIndex - 1}
              style:left="{wrap.offsetLeft + r.x * scales[h.page - 1]}px"
              style:top="{wrap.offsetTop + r.y * scales[h.page - 1]}px"
              style:width="{r.w * scales[h.page - 1]}px"
              style:height="{r.h * scales[h.page - 1]}px"
            ></div>
          {/each}
        {/if}
      {/if}
    {/each}
    {#if !pageCount}
      <div class="empty">
        {#if building}
          <p><b>Compiling…</b> your PDF appears here when it's done.</p>
        {:else if failed}
          <p><b>No PDF was produced.</b> The last compile failed before it could write one.</p>
          <p>Open the logs to see what went wrong; fixing the first error is usually enough.</p>
          {#if onShowLogs}<p><button class="btn sm" onclick={onShowLogs}>Show logs</button></p>{/if}
        {:else}
          <p><b>Your PDF appears here</b> after the first compile.</p>
          <p>Double-click in the PDF to jump to the source. Press <kbd>Ctrl</kbd> <kbd>J</kbd> in the editor to find the current line here.</p>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .pdf { display: flex; flex-direction: column; height: 100%; background: var(--pdf-ground); min-width: 0; }
  .scroller { flex: 1; overflow: auto; position: relative; }
  .scroller:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
  .pages { display: flex; flex-direction: column; align-items: center; gap: 14px; padding: 18px 24px 40px; }
  .pages.two { display: grid; grid-template-columns: repeat(2, max-content); justify-content: center; align-items: start; }
  .pages.two :global(.page) { justify-self: start; }
  /* Left column: even pages after a lone cover, odd pages when pairing from 1. */
  .pages.two:not(.first) :global(.page:nth-child(even)), .pages.two.first :global(.page:nth-child(odd)) { justify-self: end; }
  .pages.two:not(.first) :global(.page:first-child) { grid-column: 2; }
  .pages :global(.page) { position: relative; background: #fff; box-shadow: 0 1px 3px rgba(0, 0, 0, 0.18), 0 6px 18px -6px rgba(0, 0, 0, 0.25); flex: none; }
  .pages :global(.page canvas) { width: 100%; height: 100%; display: block; }
  /* Dark pages only while the app itself is dark. */
  @media (prefers-color-scheme: dark) {
    :global(:root:not([data-theme="light"])) .dark-pages :global(.page canvas) { filter: invert(0.92) hue-rotate(180deg); }
  }
  :global(:root[data-theme="dark"]) .dark-pages :global(.page canvas) { filter: invert(0.92) hue-rotate(180deg); }
  .pages :global(.textLayer) {
    position: absolute; inset: 0; overflow: clip; line-height: 1; text-size-adjust: none; forced-color-adjust: none;
    transform-origin: 0 0; z-index: 1; opacity: 1; user-select: text;
  }
  .pages :global(.textLayer :is(span, br)) { color: transparent; position: absolute; white-space: pre; cursor: text; transform-origin: 0 0; }
  .pages :global(.textLayer ::selection) { background: color-mix(in srgb, var(--accent) 35%, transparent); }
  .pages :global(.links) { position: absolute; inset: 0; z-index: 2; pointer-events: none; }
  .pages :global(.links a) { position: absolute; pointer-events: auto; cursor: pointer; border-radius: 2px; }
  .pages :global(.links a:hover) { background: color-mix(in srgb, var(--accent) 14%, transparent); outline: 1px solid color-mix(in srgb, var(--accent) 50%, transparent); }
  .hit { position: absolute; pointer-events: none; border-radius: 2px; background: color-mix(in srgb, #f2c230 42%, transparent); mix-blend-mode: multiply; }
  .hit.current { background: color-mix(in srgb, #f08a24 55%, transparent); outline: 1.5px solid #f08a24; }
  .sync {
    position: absolute; pointer-events: none; border-radius: 3px;
    background: color-mix(in srgb, var(--accent) 22%, transparent);
    outline: 1.5px solid var(--accent);
    animation: fade 1.8s ease-out forwards;
  }
  @keyframes fade { 0%, 60% { opacity: 1; } 100% { opacity: 0; } }
  .empty {
    position: absolute; top: 34%; left: 50%; transform: translateX(-50%); width: min(340px, 80%);
    text-align: center; color: var(--muted); line-height: 1.6;
  }
  .empty p { margin: 0 0 8px; }
  .empty b { color: var(--ink-2); font-weight: 600; }
</style>
