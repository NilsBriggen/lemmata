<script lang="ts">
  // Full-screen slide mode for beamer PDFs: one page at a time. With a
  // presenter window open (key P, or automatically with two screens) the two
  // follow each other; PDFs with Beamer notes show only the slide here.
  import { onMount } from "svelte";
  import type { PDFDocumentProxy } from "pdfjs-dist";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { clampPage, regions, type Region } from "./presenter/layout";
  import { renderRegion } from "./presenter/render";
  import { onEnd, onPage, sendEnd, sendPage } from "./presenter/sync";
  import { holdDocument } from "./PdfView.svelte";
  import { closePresenter, hasSecondScreen, moveBack, moveToProjector, openPresenter } from "./presenter/window";

  interface Props {
    doc: PDFDocumentProxy;
    start: number;
    onClose: () => void;
  }
  let { doc, start, onClose }: Props = $props();

  let root = $state<HTMLDivElement>();
  let canvas = $state<HTMLCanvasElement>();
  let page = $state(1);
  let region = $state<Region>("full");
  let seq = 0;
  $effect.pre(() => {
    page = Math.min(Math.max(1, start), doc.numPages);
  });

  async function draw() {
    if (!canvas || !root) return;
    const mine = ++seq;
    await renderRegion(doc, page, canvas, root.clientWidth, root.clientHeight, region, () => mine === seq).catch((e) => console.warn("slide render failed:", e));
  }

  $effect(() => {
    void page;
    void canvas;
    void region;
    draw();
  });

  /** `tell` is false when the presenter window asked for the change, so it isn't sent back. */
  function go(to: number, tell = true) {
    const target = clampPage(to, doc.numPages);
    if (target === page) return;
    page = target;
    if (tell) sendPage(target);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") finish();
    else if (["ArrowRight", "ArrowDown", "PageDown", " ", "Enter"].includes(e.key)) (e.preventDefault(), go(page + 1));
    else if (["ArrowLeft", "ArrowUp", "PageUp", "Backspace"].includes(e.key)) (e.preventDefault(), go(page - 1));
    else if (e.key === "Home") go(1);
    else if (e.key === "End") go(doc.numPages);
    else if (e.key === "p" || e.key === "P") openPresenter().then(() => sendPage(page)).catch(() => {});
  }

  // Our own sendEnd comes back through onEnd, so closing must only happen once.
  let closed = false;
  function close() {
    if (closed) return;
    closed = true;
    onClose();
  }

  function finish() {
    sendEnd();
    close();
  }

  onMount(() => {
    const win = getCurrentWindow();
    const stops: Array<() => void> = [];
    // Keeps the old document alive if a rebuild replaces it during the show.
    const release = holdDocument(doc);
    let entering: Promise<unknown> = Promise.resolve();
    (async () => {
      const first = await doc.getPage(1);
      const v = first.getViewport({ scale: 1 });
      region = regions(v.width, v.height).slide;
      stops.push(await onPage((p) => go(p, false)), await onEnd(close));
      if (closed) return stops.forEach((s) => s());
      // Like PowerPoint: with two screens the slides go to the projector and the presenter view opens here.
      if (await hasSecondScreen()) {
        await openPresenter().catch(() => {});
        await moveToProjector().catch(() => {});
        sendPage(page);
      }
      if (closed) return;
      // Full screen comes from the app window, not from the web page's Fullscreen
      // API: WebKitGTK aborts the whole app in that API on some Linux setups
      // (the AppImage's bundled WebKit on Wayland), taking the editor with it.
      entering = win.setFullscreen(true).catch(() => {});
    })();
    window.addEventListener("resize", draw);
    return () => {
      closed = true;
      release();
      stops.forEach((s) => s());
      window.removeEventListener("resize", draw);
      // Leave only after a pending enter settled, or an early Esc leaves the window stuck fullscreen.
      entering.then(() => win.setFullscreen(false)).catch(() => {});
      moveBack();
      closePresenter();
    };
  });
</script>

<svelte:window onkeydown={onKey} />
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="stage" bind:this={root} onclick={(e) => go(page + (e.clientX > window.innerWidth / 3 ? 1 : -1))}>
  <canvas bind:this={canvas}></canvas>
  <div class="count">{page} / {doc.numPages} · P: presenter view</div>
</div>

<style>
  .stage { position: fixed; inset: 0; z-index: 100; background: #000; display: grid; place-items: center; cursor: none; }
  .stage:hover { cursor: default; }
  canvas { display: block; background: #fff; }
  .count { position: absolute; right: 16px; bottom: 12px; color: rgba(255, 255, 255, 0.5); font: 12px var(--f-body); font-variant-numeric: tabular-nums; }
</style>
