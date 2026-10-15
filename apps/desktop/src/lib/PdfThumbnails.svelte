<script lang="ts">
  // Small renders of every page, drawn as they scroll into view and released
  // when far away. A rebuilt document redraws only the thumbnails in view.
  import { onDestroy } from "svelte";
  import { settings } from "./settings.svelte";
  import type { PDFDocumentProxy, RenderTask } from "pdfjs-dist";

  interface Props {
    doc: PDFDocumentProxy | null;
    current: number;
    onGo: (page: number) => void;
  }
  let { doc, current, onGo }: Props = $props();

  /** Thumbnail width in CSS pixels; drawn at the device's pixel ratio. */
  const WIDTH = 168;
  const count = $derived(doc?.numPages ?? 0);
  let btns: HTMLButtonElement[] = [];
  let io: IntersectionObserver | null = null;
  let far: IntersectionObserver | null = null;
  const near = new Set<number>();
  /** The document each thumbnail was drawn from, and its render in flight. */
  const drawn = new Map<number, PDFDocumentProxy>();
  const tasks = new Map<number, RenderTask>();
  const tokens = new Map<number, number>();

  async function draw(n: number) {
    const d = doc;
    const btn = btns[n - 1];
    if (!d || !btn || drawn.get(n) === d) return;
    const token = (tokens.get(n) ?? 0) + 1;
    tokens.set(n, token);
    tasks.get(n)?.cancel();
    try {
      const page = await d.getPage(n);
      if (tokens.get(n) !== token) return;
      const base = page.getViewport({ scale: 1 });
      const viewport = page.getViewport({ scale: (WIDTH * (window.devicePixelRatio || 1)) / base.width });
      const canvas = document.createElement("canvas");
      canvas.width = Math.floor(viewport.width);
      canvas.height = Math.floor(viewport.height);
      const task = page.render({ canvas, canvasContext: canvas.getContext("2d")!, viewport });
      tasks.set(n, task);
      await task.promise;
      if (tokens.get(n) !== token) return;
      tasks.delete(n);
      const img = btn.querySelector<HTMLElement>(".img")!;
      img.style.aspectRatio = `${base.width} / ${base.height}`;
      img.replaceChildren(canvas); // the old thumbnail stays until this one is ready
      drawn.set(n, d);
    } catch {
      /* cancelled, or the document was replaced under us */
    }
  }

  function release(n: number) {
    tokens.set(n, (tokens.get(n) ?? 0) + 1);
    tasks.get(n)?.cancel();
    tasks.delete(n);
    drawn.delete(n);
    btns[n - 1]?.querySelectorAll("canvas").forEach((c) => ((c.width = 0), c.remove()));
  }

  function watch(node: HTMLElement) {
    const root = node.parentElement;
    io ??= new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          const n = Number((e.target as HTMLElement).dataset.page);
          if (e.isIntersecting) (near.add(n), draw(n));
          else near.delete(n);
        }
      },
      { root, rootMargin: "50% 0px" },
    );
    far ??= new IntersectionObserver(
      (entries) => {
        for (const e of entries) if (!e.isIntersecting) release(Number((e.target as HTMLElement).dataset.page));
      },
      { root, rootMargin: "300% 0px" },
    );
    io.observe(node);
    far.observe(node);
    return {
      destroy() {
        io?.unobserve(node);
        far?.unobserve(node);
        near.delete(Number(node.dataset.page));
      },
    };
  }

  // A new document: the visible thumbnails redraw, the rest when they come into view.
  $effect(() => {
    void doc;
    for (const n of near) draw(n);
  });

  // Keep the current page's thumbnail in view.
  $effect(() => {
    btns[current - 1]?.scrollIntoView({ block: "nearest" });
  });

  onDestroy(() => {
    io?.disconnect();
    far?.disconnect();
    for (const n of [...tasks.keys()]) release(n);
  });
</script>

<div class="thumbs" class:dark-pages={settings.pdfDark} aria-label="Page thumbnails">
  {#each { length: count } as _, i (i)}
    {@const n = i + 1}
    <button bind:this={btns[i]} use:watch class="thumb" class:current={n === current} data-page={n} aria-label="Page {n}" aria-current={n === current ? "page" : undefined} onclick={() => onGo(n)}>
      <span class="img"></span>
      <span class="num">{n}</span>
    </button>
  {/each}
</div>

<style>
  .thumbs { display: flex; flex-direction: column; align-items: center; gap: 10px; }
  .thumb {
    width: 100%; max-width: 176px; padding: 0; border: none; background: none; cursor: pointer;
    display: flex; flex-direction: column; align-items: center; gap: 3px; flex: none;
  }
  .img { display: block; width: 100%; aspect-ratio: 3 / 4; background: #fff; border: 2px solid transparent; box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2); border-radius: 2px; }
  .img :global(canvas) { display: block; width: 100%; height: 100%; }
  /* Like the pages: dark only while the app itself is dark. */
  @media (prefers-color-scheme: dark) {
    :global(:root:not([data-theme="light"])) .dark-pages .img :global(canvas) { filter: invert(0.92) hue-rotate(180deg); }
  }
  :global(:root[data-theme="dark"]) .dark-pages .img :global(canvas) { filter: invert(0.92) hue-rotate(180deg); }
  .thumb:hover .img { border-color: var(--border-strong); }
  .thumb.current .img { border-color: var(--accent); }
  .num { font-size: 11px; color: var(--muted); font-variant-numeric: tabular-nums; }
  .thumb.current .num { color: var(--accent); }
</style>
