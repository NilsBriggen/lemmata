<script lang="ts" module>
  export type Fit = "width" | "page" | number;
</script>

<script lang="ts">
  // Renders the build's PDF with PDF.js. A new PDF replaces the old one page
  // by page in place, so the reader keeps their position across rebuilds.
  import { onDestroy, onMount } from "svelte";
  import * as pdfjs from "pdfjs-dist";
  import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
  import type { PDFDocumentProxy, PDFPageProxy, RenderTask } from "pdfjs-dist";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import type { PdfBox } from "./api";
  import { settings } from "./settings.svelte";

  pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

  export interface OutlineNode { title: string; dest: unknown; url: string | null; items: OutlineNode[] }

  interface Props {
    /** Called when the user double-clicks or Ctrl/Cmd-clicks a point. Coordinates are PDF points from the top-left. */
    onSourceClick: (page: number, x: number, y: number) => void;
    pageCount?: number;
    currentPage?: number;
    /** Current scale in percent of the PDF's real size, for display. */
    percent?: number;
    /** Search results: how many, and which one is current (1-based, 0 when none). */
    matchCount?: number;
    matchIndex?: number;
  }
  let {
    onSourceClick,
    pageCount = $bindable(0),
    currentPage = $bindable(1),
    percent = $bindable(100),
    matchCount = $bindable(0),
    matchIndex = $bindable(0),
  }: Props = $props();

  interface Slot {
    n: number;
    page: PDFPageProxy;
    wrap: HTMLDivElement;
    /** Page size in PDF points. */
    w: number;
    h: number;
    scale: number;
    token: number;
    task: RenderTask | null;
    text: pdfjs.TextLayer | null;
  }
  interface Hit { page: number; rects: { x: number; y: number; w: number; h: number }[] }

  let scroller: HTMLDivElement;
  let pagesEl = $state<HTMLDivElement>()!;
  let doc: PDFDocumentProxy | null = null;
  let slots: Slot[] = [];
  /** Pages near the viewport; only these hold a canvas, text layer and links. */
  const near = new Set<number>();
  let io: IntersectionObserver | null = null;
  /** "width" and "page" fit the pane; a number is a fixed scale (1 = 100%). */
  let fit = $state<Fit>("width");
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
    const availW = Math.max(160, scroller.clientWidth - 48);
    const availH = Math.max(160, scroller.clientHeight - 40);
    if (fit === "width") return availW / w;
    if (fit === "page") return Math.min(availW / w, availH / h);
    return fit * PT;
  }

  function observer() {
    return (io ??= new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          const n = Number((e.target as HTMLElement).dataset.page);
          if (e.isIntersecting) {
            near.add(n);
            renderSlot(n);
          } else {
            near.delete(n);
            releaseSlot(n);
          }
        }
        nearPages = [...near];
      },
      { root: scroller, rootMargin: "100% 0px" },
    ));
  }

  export function getDocument() {
    return doc;
  }

  export async function load(data: Uint8Array) {
    const seq = ++loadSeq;
    const next = await pdfjs.getDocument({ data }).promise;
    if (seq !== loadSeq) {
      next.loadingTask.destroy();
      return;
    }
    const old = doc;
    doc = next;
    textCache.clear();
    pageCount = next.numPages;
    if (await layout(seq)) for (const n of near) renderSlot(n);
    old?.loadingTask.destroy();
    if (pendingScroll !== null) {
      scroller.scrollTop = pendingScroll * scroller.scrollHeight;
      pendingScroll = null;
    }
    if (hits.length) lastQuery && search(lastQuery, false);
  }

  /** Sizes every page's box, so layout and scroll position are final before
   *  anything is drawn. Boxes are reused across loads to keep the position. */
  async function layout(seq: number): Promise<boolean> {
    if (!doc) return false;
    const d = doc;
    const pages = await Promise.all(Array.from({ length: d.numPages }, (_, i) => d.getPage(i + 1)));
    if (seq !== loadSeq) return false;
    while (pagesEl.children.length > d.numPages) {
      const last = pagesEl.lastElementChild as HTMLElement;
      io?.unobserve(last);
      near.delete(Number(last.dataset.page));
      last.remove();
    }
    slots.length = 0;
    pages.forEach((page, i) => {
      const n = i + 1;
      const base = page.getViewport({ scale: 1 });
      const scale = scaleFor(base.width, base.height);
      scales[i] = scale;
      if (n === 1) percent = Math.round((scale / PT) * 100);
      let wrap = pagesEl.children[i] as HTMLDivElement | undefined;
      if (!wrap) {
        wrap = document.createElement("div");
        wrap.className = "page";
        wrap.dataset.page = String(n);
        pagesEl.appendChild(wrap);
        observer().observe(wrap);
      }
      wrap.style.width = `${base.width * scale}px`;
      wrap.style.height = `${base.height * scale}px`;
      slots.push({ n, page, wrap, w: base.width, h: base.height, scale, token: 0, task: null, text: null });
      // Pages out of view keep nothing from the previous document.
      if (!near.has(n)) clearLayers(wrap, true);
    });
    layoutVersion++;
    return true;
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
    slot.task = slot.text = null;
    clearLayers(slot.wrap, true);
  }

  async function renderSlot(n: number) {
    const slot = slots[n - 1];
    if (!slot) return;
    const token = ++slot.token;
    slot.task?.cancel();
    slot.text?.cancel();
    const viewport = slot.page.getViewport({ scale: slot.scale * dpr() });
    const canvas = document.createElement("canvas");
    canvas.width = Math.floor(viewport.width);
    canvas.height = Math.floor(viewport.height);
    const task = slot.page.render({ canvas, canvasContext: canvas.getContext("2d")!, viewport });
    slot.task = task;
    try {
      await task.promise;
    } catch (e) {
      if (e instanceof pdfjs.RenderingCancelledException) return;
      throw e;
    }
    if (token !== slot.token || slots[n - 1] !== slot) return;
    slot.wrap.querySelectorAll("canvas").forEach((c) => c.remove());
    slot.wrap.prepend(canvas);
    await decorate(slot, token);
  }

  /** The selectable text layer and clickable links over a rendered page. */
  async function decorate(slot: Slot, token: number) {
    const { wrap, page } = slot;
    const viewport = page.getViewport({ scale: slot.scale });
    clearLayers(wrap, false);
    wrap.style.setProperty("--scale-factor", String(slot.scale));
    wrap.style.setProperty("--total-scale-factor", String(slot.scale));
    wrap.style.setProperty("--user-unit", "1");

    const textDiv = document.createElement("div");
    textDiv.className = "textLayer";
    wrap.append(textDiv);
    const layer = new pdfjs.TextLayer({ textContentSource: page.streamTextContent(), container: textDiv, viewport });
    slot.text = layer;
    try {
      await layer.render();
    } catch {
      return; // cancelled by a newer render
    }
    if (token !== slot.token) return;

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

  onDestroy(() => {
    io?.disconnect();
    doc?.loadingTask.destroy();
  });
</script>

<div class="pdf" class:dark-pages={settings.pdfDark}>
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="scroller" tabindex="-1" bind:this={scroller} onscroll={onScroll} onclick={handleClick} ondblclick={handleClick} onwheel={onWheel}>
    <div class="pages" bind:this={pagesEl}></div>
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
        <p><b>Your PDF appears here</b> after the first compile.</p>
        <p>Double-click in the PDF to jump to the source. Press <kbd>Ctrl</kbd> <kbd>J</kbd> in the editor to find the current line here.</p>
      </div>
    {/if}
  </div>
</div>

<style>
  .pdf { display: flex; flex-direction: column; height: 100%; background: var(--pdf-ground); min-width: 0; }
  .scroller { flex: 1; overflow: auto; position: relative; outline: none; }
  .pages { display: flex; flex-direction: column; align-items: center; gap: 14px; padding: 18px 24px 40px; }
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
