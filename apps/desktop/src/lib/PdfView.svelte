<script lang="ts" module>
  export type Fit = "width" | "page" | number;
</script>

<script lang="ts">
  // Renders the build's PDF with PDF.js. A new PDF replaces the old one page
  // by page in place, so the reader keeps their position across rebuilds.
  import { onDestroy, onMount } from "svelte";
  import * as pdfjs from "pdfjs-dist";
  import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
  import type { PDFDocumentProxy } from "pdfjs-dist";
  import type { PdfBox } from "./api";
  import { settings } from "./settings.svelte";

  pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

  interface Props {
    /** Called when the user double-clicks or Ctrl/Cmd-clicks a point. Coordinates are PDF points from the top-left. */
    onSourceClick: (page: number, x: number, y: number) => void;
    pageCount?: number;
    currentPage?: number;
    /** Current scale in percent of the PDF's real size, for display. */
    percent?: number;
  }
  let { onSourceClick, pageCount = $bindable(0), currentPage = $bindable(1), percent = $bindable(100) }: Props = $props();

  let scroller: HTMLDivElement;
  let pagesEl = $state<HTMLDivElement>()!;
  let doc: PDFDocumentProxy | null = null;
  /** "width" and "page" fit the pane; a number is a fixed scale (1 = 100%). */
  let fit = $state<Fit>("width");
  let highlight = $state<{ page: number; left: number; top: number; width: number; height: number } | null>(null);
  let loadSeq = 0;

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

  export async function load(data: Uint8Array) {
    const seq = ++loadSeq;
    const next = await pdfjs.getDocument({ data }).promise;
    if (seq !== loadSeq) {
      next.loadingTask.destroy();
      return;
    }
    const old = doc;
    doc = next;
    pageCount = next.numPages;
    await renderAll(seq);
    old?.loadingTask.destroy();
    if (pendingScroll !== null) {
      scroller.scrollTop = pendingScroll * scroller.scrollHeight;
      pendingScroll = null;
    }
  }

  async function renderAll(seq: number) {
    if (!doc) return;
    // Reuse page containers so scroll position survives the swap.
    while (pagesEl.children.length > doc.numPages) pagesEl.lastElementChild!.remove();
    // First size every page so the layout (and scroll position) is final,
    // then render, starting with the page in view and working outwards.
    const pages = [];
    for (let n = 1; n <= doc.numPages; n++) {
      const page = await doc.getPage(n);
      if (seq !== loadSeq) return;
      const base = page.getViewport({ scale: 1 });
      const scale = scaleFor(base.width, base.height);
      scales[n - 1] = scale;
      if (n === 1) percent = Math.round((scale / PT) * 100);

      let wrap = pagesEl.children[n - 1] as HTMLDivElement | undefined;
      if (!wrap) {
        wrap = document.createElement("div");
        wrap.className = "page";
        wrap.dataset.page = String(n);
        pagesEl.appendChild(wrap);
      }
      wrap.style.width = `${base.width * scale}px`;
      wrap.style.height = `${base.height * scale}px`;
      pages.push({ n, page, scale, wrap });
    }
    const order = [...pages].sort((a, b) => Math.abs(a.n - currentPage) - Math.abs(b.n - currentPage));
    for (const { page, scale, wrap } of order) {
      const viewport = page.getViewport({ scale: scale * dpr() });
      const canvas = document.createElement("canvas");
      canvas.width = Math.floor(viewport.width);
      canvas.height = Math.floor(viewport.height);
      await page.render({ canvas, canvasContext: canvas.getContext("2d")!, viewport }).promise;
      if (seq !== loadSeq) return;
      wrap.querySelector("canvas")?.remove();
      wrap.prepend(canvas);
    }
  }

  export function getPosition(): { fit: Fit; scroll: number } {
    return { fit, scroll: scroller.scrollTop / Math.max(1, scroller.scrollHeight) };
  }

  /** Restores zoom and scroll; applied after the next PDF loads if none is shown yet. */
  export async function setPosition(pos: { fit: Fit; scroll: number }) {
    fit = pos.fit;
    if (doc) await renderAll(loadSeq);
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
    await renderAll(loadSeq);
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

  onDestroy(() => doc?.loadingTask.destroy());
</script>

<div class="pdf" class:dark-pages={settings.pdfDark}>
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="scroller" bind:this={scroller} onscroll={onScroll} onclick={handleClick} ondblclick={handleClick} onwheel={onWheel}>
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
  .scroller { flex: 1; overflow: auto; position: relative; }
  .pages { display: flex; flex-direction: column; align-items: center; gap: 14px; padding: 18px 24px 40px; }
  .pages :global(.page) { position: relative; background: #fff; box-shadow: 0 1px 3px rgba(0, 0, 0, 0.18), 0 6px 18px -6px rgba(0, 0, 0, 0.25); flex: none; }
  .pages :global(.page canvas) { width: 100%; height: 100%; display: block; }
  /* Dark pages only while the app itself is dark. */
  @media (prefers-color-scheme: dark) {
    :global(:root:not([data-theme="light"])) .dark-pages :global(.page canvas) { filter: invert(0.92) hue-rotate(180deg); }
  }
  :global(:root[data-theme="dark"]) .dark-pages :global(.page canvas) { filter: invert(0.92) hue-rotate(180deg); }
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
