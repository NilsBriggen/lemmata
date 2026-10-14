<script lang="ts">
  // Full-screen slide mode for beamer PDFs: one page at a time.
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import type { PDFDocumentProxy } from "pdfjs-dist";

  interface Props {
    doc: PDFDocumentProxy;
    start: number;
    onClose: () => void;
  }
  let { doc, start, onClose }: Props = $props();

  let root = $state<HTMLDivElement>();
  let canvas = $state<HTMLCanvasElement>();
  let page = $state(1);
  let seq = 0;
  $effect.pre(() => {
    page = Math.min(Math.max(1, start), doc.numPages);
  });

  async function draw() {
    if (!canvas || !root) return;
    const mine = ++seq;
    const p = await doc.getPage(page);
    const base = p.getViewport({ scale: 1 });
    const fit = Math.min(root.clientWidth / base.width, root.clientHeight / base.height);
    const ratio = window.devicePixelRatio || 1;
    const viewport = p.getViewport({ scale: fit * ratio });
    if (mine !== seq) return;
    const next = document.createElement("canvas");
    next.width = Math.floor(viewport.width);
    next.height = Math.floor(viewport.height);
    await p.render({ canvas: next, canvasContext: next.getContext("2d")!, viewport }).promise;
    if (mine !== seq || !canvas) return;
    canvas.width = next.width;
    canvas.height = next.height;
    canvas.style.width = `${next.width / ratio}px`;
    canvas.style.height = `${next.height / ratio}px`;
    canvas.getContext("2d")!.drawImage(next, 0, 0);
  }

  $effect(() => {
    void page;
    void canvas;
    draw();
  });

  function go(delta: number) {
    page = Math.min(doc.numPages, Math.max(1, page + delta));
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
    else if (["ArrowRight", "ArrowDown", "PageDown", " ", "Enter"].includes(e.key)) (e.preventDefault(), go(1));
    else if (["ArrowLeft", "ArrowUp", "PageUp", "Backspace"].includes(e.key)) (e.preventDefault(), go(-1));
    else if (e.key === "Home") page = 1;
    else if (e.key === "End") page = doc.numPages;
  }

  // Full screen comes from the app window, not from the web page's Fullscreen
  // API: WebKitGTK aborts the whole app in that API on some Linux setups
  // (the AppImage's bundled WebKit on Wayland), taking the editor with it.
  onMount(() => {
    const win = getCurrentWindow();
    win.setFullscreen(true).catch(() => {});
    window.addEventListener("resize", draw);
    return () => {
      window.removeEventListener("resize", draw);
      win.setFullscreen(false).catch(() => {});
    };
  });
</script>

<svelte:window onkeydown={onKey} />
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="stage" bind:this={root} onclick={(e) => go(e.clientX > window.innerWidth / 3 ? 1 : -1)}>
  <canvas bind:this={canvas}></canvas>
  <div class="count">{page} / {doc.numPages}</div>
</div>

<style>
  .stage { position: fixed; inset: 0; z-index: 100; background: #000; display: grid; place-items: center; cursor: none; }
  .stage:hover { cursor: default; }
  canvas { display: block; background: #fff; }
  .count { position: absolute; right: 16px; bottom: 12px; color: rgba(255, 255, 255, 0.5); font: 12px var(--f-body); font-variant-numeric: tabular-nums; }
</style>
