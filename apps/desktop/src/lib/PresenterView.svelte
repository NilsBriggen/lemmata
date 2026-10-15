<script lang="ts">
  // The presenter's screen: the current slide, the next one, the speaker
  // notes, a timer and the clock. It runs in its own window and follows the
  // audience window through events, so either side can change slide.
  import { onMount } from "svelte";
  import type { PDFDocumentProxy } from "pdfjs-dist";
  import { api } from "./api";
  import { clampPage, formatElapsed, regions, type Region } from "./presenter/layout";
  import { openPdf, renderRegion } from "./presenter/render";
  import { onEnd, onPage, sendEnd, sendPage } from "./presenter/sync";
  import { getCurrentWindow } from "@tauri-apps/api/window";

  let doc = $state<PDFDocumentProxy | null>(null);
  let page = $state(1);
  let error = $state<string | null>(null);
  let slideRegion = $state<Region>("full");
  let notesRegion = $state<Region | null>(null);

  let cur = $state<HTMLCanvasElement>();
  let next = $state<HTMLCanvasElement>();
  let notes = $state<HTMLCanvasElement>();
  let curBox = $state<HTMLDivElement>();
  let nextBox = $state<HTMLDivElement>();
  let notesBox = $state<HTMLDivElement>();

  // Timer: starts with the first slide change, or by pressing the button.
  let started = $state<number | null>(null);
  let paused = $state<number | null>(null);
  let carried = $state(0);
  let now = $state(Date.now());
  const elapsed = $derived(carried + (started && paused === null ? (now - started) / 1000 : 0));
  const clock = $derived(new Date(now).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }));
  const total = $derived(doc?.numPages ?? 0);

  function toggleTimer() {
    if (started && paused === null) {
      carried += (Date.now() - started) / 1000;
      paused = Date.now();
    } else {
      started = Date.now();
      paused = null;
    }
  }
  function resetTimer() {
    carried = 0;
    started = paused === null && started ? Date.now() : started;
  }

  function go(to: number, tell = true) {
    const target = clampPage(to, total);
    if (target === page) return;
    page = target;
    if (!started) toggleTimer();
    if (tell) sendPage(target);
  }

  let drawSeq = 0;
  async function draw() {
    if (!doc || !cur || !curBox) return;
    const mine = ++drawSeq;
    const alive = () => mine === drawSeq;
    const tasks = [renderRegion(doc, page, cur, curBox.clientWidth, curBox.clientHeight, slideRegion, alive)];
    if (next && nextBox && page < doc.numPages) tasks.push(renderRegion(doc, page + 1, next, nextBox.clientWidth, nextBox.clientHeight, slideRegion, alive));
    if (notes && notesBox && notesRegion) tasks.push(renderRegion(doc, page, notes, notesBox.clientWidth, notesBox.clientHeight, notesRegion, alive));
    await Promise.all(tasks).catch((e) => (error = String(e)));
  }

  $effect(() => {
    void page;
    void doc;
    void cur;
    draw();
  });

  async function end() {
    await sendEnd();
    getCurrentWindow().close().catch(() => {});
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") end();
    else if (["ArrowRight", "ArrowDown", "PageDown", " ", "Enter"].includes(e.key)) (e.preventDefault(), go(page + 1));
    else if (["ArrowLeft", "ArrowUp", "PageUp", "Backspace"].includes(e.key)) (e.preventDefault(), go(page - 1));
    else if (e.key === "Home") go(1);
    else if (e.key === "End") go(total);
    else if (e.key === "t" || e.key === "T") toggleTimer();
    else if (e.key === "r" || e.key === "R") resetTimer();
  }

  onMount(() => {
    const tick = setInterval(() => (now = Date.now()), 500);
    const stops: Array<() => void> = [];
    (async () => {
      try {
        doc = await openPdf(await api.readPdf());
        const first = await doc.getPage(1);
        const v = first.getViewport({ scale: 1 });
        ({ slide: slideRegion, notes: notesRegion } = regions(v.width, v.height));
      } catch (e) {
        error = "There is no PDF to present yet. Build the project first.";
        console.warn(e);
      }
      stops.push(await onPage((p) => go(p, false)), await onEnd(() => getCurrentWindow().close().catch(() => {})));
    })();
    window.addEventListener("resize", draw);
    return () => {
      clearInterval(tick);
      stops.forEach((s) => s());
      window.removeEventListener("resize", draw);
    };
  });
</script>

<svelte:window onkeydown={onKey} />

<div class="presenter">
  <header>
    <div class="slide">Slide <b>{page}</b> / {total || "…"}</div>
    <div class="timer" role="group" aria-label="Timer">
      <span class="time" class:idle={!started || paused !== null}>{formatElapsed(elapsed)}</span>
      <button onclick={toggleTimer} title="Start or pause the timer (T)">{started && paused === null ? "Pause" : started ? "Resume" : "Start"}</button>
      <button onclick={resetTimer} title="Reset the timer (R)">Reset</button>
    </div>
    <div class="clock" title="Time of day">{clock}</div>
  </header>

  {#if error}
    <p class="error">{error}</p>
  {:else}
    <main>
      <div class="current" bind:this={curBox}><canvas bind:this={cur}></canvas></div>
      <aside>
        <div class="label">Next</div>
        <div class="next" bind:this={nextBox}>
          {#if page < total}<canvas bind:this={next}></canvas>{:else}<span class="end">End of slides</span>{/if}
        </div>
        <div class="label">Notes</div>
        <div class="notes" bind:this={notesBox}>
          {#if notesRegion}
            <canvas bind:this={notes}></canvas>
          {:else}
            <p class="hint">
              No speaker notes in this PDF. Add <code>\setbeameroption&#123;show notes on second screen=right&#125;</code> to the preamble and
              <code>\note&#123;…&#125;</code> after a slide, then recompile.
            </p>
          {/if}
        </div>
      </aside>
    </main>
  {/if}

  <footer>
    <button onclick={() => go(page - 1)} disabled={page <= 1}>◀ Back</button>
    <button onclick={() => go(page + 1)} disabled={page >= total}>Next ▶</button>
    <span class="keys">← → space: change slide · T: timer · R: reset · Esc: end</span>
    <button class="end-btn" onclick={end}>End presentation</button>
  </footer>
</div>

<style>
  :global(body) { background: #14191f; }
  .presenter { display: flex; flex-direction: column; height: 100vh; color: #dbe3ea; font: 14px var(--f-body); background: #14191f; }
  header, footer { display: flex; align-items: center; gap: 18px; padding: 10px 18px; background: #1d2630; flex: none; }
  header { border-bottom: 1px solid #33404d; justify-content: space-between; }
  footer { border-top: 1px solid #33404d; }
  .slide { font-size: 16px; }
  .timer { display: flex; align-items: center; gap: 10px; }
  .time { font: 600 30px var(--f-mono); font-variant-numeric: tabular-nums; min-width: 4.5ch; text-align: center; }
  .time.idle { color: #8d9cab; }
  .clock { font: 500 18px var(--f-mono); color: #8d9cab; }
  button { height: 30px; padding: 0 12px; border: 1px solid #3a4856; border-radius: 6px; background: #2b3743; color: #dbe3ea; font: inherit; }
  button:hover:not(:disabled) { background: #364555; }
  button:disabled { opacity: 0.4; }
  main { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(0, 1.7fr) minmax(0, 1fr); gap: 14px; padding: 14px; }
  .current, .next, .notes { min-height: 0; display: grid; place-items: center; background: #0d1116; border-radius: 6px; overflow: hidden; }
  .current { min-height: 0; }
  aside { display: grid; grid-template-rows: auto minmax(0, 0.8fr) auto minmax(0, 1fr); gap: 6px; min-height: 0; }
  .label { font: 600 11px var(--f-body); letter-spacing: 0.1em; text-transform: uppercase; color: #8d9cab; }
  canvas { display: block; background: #fff; max-width: 100%; max-height: 100%; }
  .end { color: #8d9cab; }
  .hint { color: #8d9cab; padding: 14px; margin: 0; line-height: 1.55; font-size: 13px; }
  .hint code { background: #232e39; padding: 1px 5px; border-radius: 3px; font: 12px var(--f-mono); color: #dbe3ea; }
  .error { padding: 40px; color: #e58c8c; }
  .keys { color: #8d9cab; font-size: 12.5px; margin-left: auto; }
  .end-btn { background: #6d2f2f; border-color: #8a3d3d; }
</style>
