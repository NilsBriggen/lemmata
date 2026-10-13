<script lang="ts">
  // Right pane, after Overleaf's: a PDF toolbar (logs, pages, zoom, export),
  // the PDF itself, or the logs in its place.
  import ScrollText from "@lucide/svelte/icons/scroll-text";
  import ChevronLeft from "@lucide/svelte/icons/chevron-left";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import ZoomIn from "@lucide/svelte/icons/zoom-in";
  import ZoomOut from "@lucide/svelte/icons/zoom-out";
  import Download from "@lucide/svelte/icons/download";
  import Moon from "@lucide/svelte/icons/moon";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import PdfView, { type Fit } from "./PdfView.svelte";
  import Problems from "./Problems.svelte";
  import DropMenu from "./ui/DropMenu.svelte";
  import { settings, updateSettings } from "./settings.svelte";
  import type { Diagnostic, PdfBox } from "./api";

  interface Props {
    diagnostics: Diagnostic[];
    building: boolean;
    buildId: number;
    logsOpen: boolean;
    onLogs: (open: boolean) => void;
    onSourceClick: (page: number, x: number, y: number) => void;
    onGoto: (file: string, line: number) => void;
    onExport: () => void;
  }
  let p: Props = $props();

  let view = $state<PdfView>();
  let pageCount = $state(0);
  let currentPage = $state(1);
  let percent = $state(100);
  let zoomBtn = $state<HTMLButtonElement>();
  let zoomMenu = $state(false);
  let pageInput = $state("");

  const errors = $derived(p.diagnostics.filter((d) => d.severity === "error").length);
  const warnings = $derived(p.diagnostics.filter((d) => d.severity === "warning").length);

  export const load = (data: Uint8Array) => view?.load(data);
  export const showBoxes = (b: PdfBox[]) => {
    p.onLogs(false);
    view?.showBoxes(b);
  };
  export const thumbnail = () => view?.thumbnail() ?? Promise.resolve(null);
  export const getPosition = () => view?.getPosition() ?? null;
  export const setPosition = (pos: { fit: Fit; scroll: number }) => view?.setPosition(pos);

  function fit(f: Fit) {
    view?.setFit(f);
  }

  function goto(e: SubmitEvent) {
    e.preventDefault();
    const n = parseInt(pageInput, 10);
    if (n) view?.gotoPage(n);
    pageInput = "";
    (document.activeElement as HTMLElement)?.blur();
  }
</script>

<section class="pane">
  <div class="bar">
    <button class="btn sm logs" class:on={p.logsOpen} aria-pressed={p.logsOpen} onclick={() => p.onLogs(!p.logsOpen)} title="Logs and problems">
      <ScrollText size={15} /> Logs
      {#if errors}<span class="badge error">{errors}</span>{/if}
      {#if warnings}<span class="badge warn">{warnings}</span>{/if}
    </button>

    {#if !p.logsOpen && pageCount}
      <div class="group pages">
        <button class="btn ghost icon sm" aria-label="Previous page" disabled={currentPage <= 1} onclick={() => view?.gotoPage(currentPage - 1)}><ChevronLeft size={16} /></button>
        <form onsubmit={goto}>
          <input class="page-input" aria-label="Page" placeholder={String(currentPage)} bind:value={pageInput} inputmode="numeric" />
        </form>
        <span class="of">/ {pageCount}</span>
        <button class="btn ghost icon sm" aria-label="Next page" disabled={currentPage >= pageCount} onclick={() => view?.gotoPage(currentPage + 1)}><ChevronRight size={16} /></button>
      </div>
      <div class="group">
        <button class="btn ghost icon sm" aria-label="Zoom out" onclick={() => view?.zoomBy(1 / 1.15)}><ZoomOut size={16} /></button>
        <button class="btn ghost sm zoom" bind:this={zoomBtn} aria-label="Zoom level" onclick={() => (zoomMenu = !zoomMenu)}>{percent}% <ChevronDown size={13} /></button>
        <button class="btn ghost icon sm" aria-label="Zoom in" onclick={() => view?.zoomBy(1.15)}><ZoomIn size={16} /></button>
      </div>
      <div class="group end">
        <button class="btn ghost icon sm" class:on={settings.pdfDark} aria-pressed={settings.pdfDark} title="Dark pages in dark mode" aria-label="Dark pages" onclick={() => updateSettings({ pdfDark: !settings.pdfDark })}><Moon size={15} /></button>
        <button class="btn ghost icon sm" title="Export PDF" aria-label="Export PDF" onclick={p.onExport}><Download size={15} /></button>
      </div>
    {/if}
  </div>
  <div class="progress" class:on={p.building} aria-hidden="true"></div>

  <div class="body">
    <div class="pdf" class:hidden={p.logsOpen}>
      <PdfView bind:this={view} bind:pageCount bind:currentPage bind:percent onSourceClick={p.onSourceClick} />
    </div>
    {#if p.logsOpen}
      <div class="pdf"><Problems diagnostics={p.diagnostics} buildId={p.buildId} onGoto={p.onGoto} /></div>
    {/if}
  </div>
</section>

{#if zoomMenu && zoomBtn}
  <DropMenu anchor={zoomBtn} onClose={() => (zoomMenu = false)}>
    <button onclick={() => fit("width")}>Fit width</button>
    <button onclick={() => fit("page")}>Fit page</button>
    <hr />
    {#each [0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4] as z (z)}
      <button onclick={() => fit(z)}>{z * 100}%</button>
    {/each}
  </DropMenu>
{/if}

<style>
  .pane { display: flex; flex-direction: column; height: 100%; min-width: 0; background: var(--pdf-ground); }
  .bar {
    display: flex; align-items: center; gap: 10px; height: 40px; padding: 0 8px; flex: none;
    background: var(--surface); border-bottom: 1px solid var(--border);
  }
  .group { display: flex; align-items: center; gap: 2px; }
  .group.end { margin-left: auto; }
  .logs { gap: 6px; }
  .logs.on { border-color: var(--accent); color: var(--accent); background: var(--accent-soft); }
  .btn.on { color: var(--accent); }
  .page-input {
    width: 38px; height: 24px; text-align: center; border: 1px solid var(--border-strong); border-radius: var(--r-sm);
    background: var(--surface); color: var(--ink); font: 12px var(--f-body);
  }
  .page-input::placeholder { color: var(--ink); }
  .page-input:focus { outline: none; border-color: var(--accent); }
  .page-input:focus::placeholder { color: var(--faint); }
  .of { color: var(--muted); font-size: 12px; margin-right: 2px; font-variant-numeric: tabular-nums; }
  .zoom { min-width: 64px; gap: 2px; font-variant-numeric: tabular-nums; }
  .progress { height: 2px; background: transparent; flex: none; overflow: hidden; position: relative; }
  .progress.on::after {
    content: ""; position: absolute; inset: 0; width: 35%; background: var(--accent);
    animation: slide 1.1s var(--ease) infinite;
  }
  @keyframes slide { from { transform: translateX(-100%); } to { transform: translateX(300%); } }
  .body { flex: 1; min-height: 0; position: relative; }
  .pdf { position: absolute; inset: 0; }
  .pdf.hidden { visibility: hidden; }
</style>
