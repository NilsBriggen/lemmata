<script lang="ts">
  // Renders the build's PDF with PDF.js. A new PDF replaces the old one page
  // by page in place, so the reader keeps their position across rebuilds.
  import { onDestroy } from "svelte";
  import * as pdfjs from "pdfjs-dist";
  import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
  import type { PDFDocumentProxy } from "pdfjs-dist";
  import type { PdfBox } from "./api";

  pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

  interface Props {
    /** Called when the user double-clicks or Ctrl/Cmd-clicks a point. Coordinates are PDF points from the top-left. */
    onSourceClick: (page: number, x: number, y: number) => void;
  }
  let { onSourceClick }: Props = $props();

  let scroller: HTMLDivElement;
  let pagesEl = $state<HTMLDivElement>()!;
  let doc: PDFDocumentProxy | null = null;
  let zoom = $state(1);
  let pageCount = $state(0);
  let currentPage = $state(1);
  let highlight = $state<{ page: number; left: number; top: number; width: number; height: number } | null>(null);
  let loadSeq = 0;

  /** CSS pixels per PDF point for each page at the current zoom. */
  const scales: number[] = [];
  const dpr = () => window.devicePixelRatio || 1;

  function fitScale(pageWidthPts: number) {
    const available = Math.max(200, scroller.clientWidth - 48);
    return (available / pageWidthPts) * zoom;
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
      const scale = fitScale(base.width);
      scales[n - 1] = scale;

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

  async function setZoom(z: number) {
    const ratio = scroller.scrollTop / Math.max(1, scroller.scrollHeight);
    zoom = Math.min(3, Math.max(0.4, z));
    await renderAll(loadSeq);
    scroller.scrollTop = ratio * scroller.scrollHeight;
  }

  onDestroy(() => doc?.loadingTask.destroy());
</script>

<div class="pdf">
  <div class="bar">
    <span>{pageCount ? `page ${currentPage} / ${pageCount}` : "no PDF yet"}</span>
    <div class="zoom">
      <button onclick={() => setZoom(zoom - 0.1)} aria-label="Zoom out">−</button>
      <span>{Math.round(zoom * 100)}%</span>
      <button onclick={() => setZoom(zoom + 0.1)} aria-label="Zoom in">+</button>
      <button onclick={() => setZoom(1)}>Fit width</button>
    </div>
  </div>
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="scroller" bind:this={scroller} onscroll={onScroll} onclick={handleClick} ondblclick={handleClick}>
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
      <p class="empty">The PDF appears here after the first build.<br />Double-click the PDF to jump to the source. Press <kbd>Ctrl</kbd>+<kbd>J</kbd> in the editor to jump to the PDF.</p>
    {/if}
  </div>
</div>

<style>
  .pdf { display: flex; flex-direction: column; height: 100%; background: var(--pdf-ground); min-width: 0; }
  .bar {
    display: flex; align-items: center; gap: 10px; padding: 5px 12px;
    background: var(--chrome); border-bottom: 1px solid var(--rule);
    font: 12px var(--f-mono); color: var(--muted);
  }
  .zoom { margin-left: auto; display: flex; gap: 6px; align-items: center; }
  .zoom button { padding: 2px 8px; }
  .scroller { flex: 1; overflow: auto; position: relative; }
  .pages { display: flex; flex-direction: column; align-items: center; gap: 16px; padding: 20px 24px 40px; }
  .pages :global(.page) { position: relative; background: #fff; box-shadow: 0 2px 10px rgba(0, 0, 0, 0.22); flex: none; }
  .pages :global(.page canvas) { width: 100%; height: 100%; display: block; }
  .sync {
    position: absolute; pointer-events: none; border-radius: 3px;
    background: color-mix(in srgb, var(--blue) 22%, transparent);
    outline: 1.5px solid var(--blue);
    animation: fade 1.8s ease-out forwards;
  }
  @keyframes fade { 0%, 60% { opacity: 1; } 100% { opacity: 0; } }
  .empty { position: absolute; inset: 40% 24px auto; text-align: center; color: var(--muted); margin: 0; line-height: 1.7; }
  kbd { font-size: 11px; border: 1px solid var(--rule); border-radius: 3px; padding: 0 4px; background: var(--sheet); }
</style>
