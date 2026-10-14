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
  import Search from "@lucide/svelte/icons/search";
  import ListTree from "@lucide/svelte/icons/list-tree";
  import Presentation from "@lucide/svelte/icons/presentation";
  import X from "@lucide/svelte/icons/x";
  import ChevronUp from "@lucide/svelte/icons/chevron-up";
  import PdfView, { type Fit, type OutlineNode } from "./PdfView.svelte";
  import PdfOutline from "./PdfOutline.svelte";
  import PresentationMode from "./Presentation.svelte";
  import Problems from "./Problems.svelte";
  import DropMenu from "./ui/DropMenu.svelte";
  import { settings, updateSettings } from "./settings.svelte";
  import type { Diagnostic, PdfBox } from "./api";
  import { display, keysOf } from "./shortcuts";

  interface Props {
    diagnostics: Diagnostic[];
    building: boolean;
    /** The last build ended without writing a PDF. */
    failed?: boolean;
    buildId: number;
    logsOpen: boolean;
    onLogs: (open: boolean) => void;
    onSourceClick: (page: number, x: number, y: number) => void;
    onGoto: (file: string, line: number) => void;
    onAddPackage: (pkg: string) => void;
    getSource: (file: string) => Promise<string | null>;
    onExport: () => void;
  }
  let p: Props = $props();

  let view = $state<PdfView>();

  /** Is a PDF loaded in the viewer? */
  export const hasPdf = () => pageCount > 0;
  let pageCount = $state(0);
  let currentPage = $state(1);
  let percent = $state(100);
  let zoomBtn = $state<HTMLButtonElement>();
  let zoomMenu = $state(false);
  let pageInput = $state("");
  let searchOpen = $state(false);
  let query = $state("");
  let matchCount = $state(0);
  let matchIndex = $state(0);
  let searchInput = $state<HTMLInputElement>();
  let outlineOpen = $state(false);
  let outline = $state<OutlineNode[]>([]);
  let presenting = $state(false);
  let searchTimer: ReturnType<typeof setTimeout> | undefined;

  const errors = $derived(p.diagnostics.filter((d) => d.severity === "error").length);
  const warnings = $derived(p.diagnostics.filter((d) => d.severity === "warning").length);

  export const load = async (data: Uint8Array) => {
    await view?.load(data);
    if (outlineOpen) refreshOutline();
  };

  async function refreshOutline() {
    outline = (await view?.getOutline()) ?? [];
  }
  async function toggleOutline() {
    outlineOpen = !outlineOpen;
    if (outlineOpen) await refreshOutline();
  }

  export function openSearch() {
    p.onLogs(false);
    searchOpen = true;
    queueMicrotask(() => (searchInput?.focus(), searchInput?.select()));
  }
  function closeSearch() {
    searchOpen = false;
    query = "";
    view?.clearSearch();
  }
  function onQuery() {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(() => view?.search(query), 180);
  }
  function onSearchKey(e: KeyboardEvent) {
    if (e.key === "Escape") closeSearch();
    else if (e.key === "Enter") {
      e.preventDefault();
      view?.searchStep(e.shiftKey ? -1 : 1);
    }
  }
  /** Ctrl+F inside the PDF pane opens its search instead of the editor's. */
  function onPaneKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && e.key.toLowerCase() === "f") {
      e.preventDefault();
      e.stopPropagation();
      openSearch();
    }
  }
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
        <button class="btn ghost icon sm optional" aria-label="Zoom out" onclick={() => view?.zoomBy(1 / 1.15)}><ZoomOut size={16} /></button>
        <button class="btn ghost sm zoom" bind:this={zoomBtn} aria-label="Zoom level" onclick={() => (zoomMenu = !zoomMenu)}>{percent}% <ChevronDown size={13} /></button>
        <button class="btn ghost icon sm optional" aria-label="Zoom in" onclick={() => view?.zoomBy(1.15)}><ZoomIn size={16} /></button>
      </div>
      <div class="group end">
        <button class="btn ghost icon sm" class:on={searchOpen} aria-pressed={searchOpen} title="Search in the PDF ({display(keysOf("pdf-search"))})" aria-label="Search in the PDF" onclick={() => (searchOpen ? closeSearch() : openSearch())}><Search size={15} /></button>
        <button class="btn ghost icon sm optional" class:on={outlineOpen} aria-pressed={outlineOpen} title="Document outline" aria-label="Document outline" onclick={toggleOutline}><ListTree size={15} /></button>
        <button class="btn ghost icon sm optional" title="Present full screen" aria-label="Present full screen" onclick={() => (presenting = true)}><Presentation size={15} /></button>
        <button class="btn ghost icon sm" class:on={settings.pdfDark} aria-pressed={settings.pdfDark} title="Dark pages in dark mode" aria-label="Dark pages" onclick={() => updateSettings({ pdfDark: !settings.pdfDark })}><Moon size={15} /></button>
        <button class="btn ghost icon sm" title="Export PDF" aria-label="Export PDF" onclick={p.onExport}><Download size={15} /></button>
      </div>
    {/if}
  </div>
  <div class="progress" class:on={p.building} aria-hidden="true"></div>
  {#if searchOpen && !p.logsOpen}
    <div class="search" role="search">
      <Search size={14} />
      <input bind:this={searchInput} bind:value={query} oninput={onQuery} onkeydown={onSearchKey} placeholder="Search the PDF" aria-label="Search the PDF" />
      <span class="count">{query.trim() ? (matchCount ? `${matchIndex} of ${matchCount}` : "No matches") : ""}</span>
      <button class="btn ghost icon sm" aria-label="Previous match" disabled={!matchCount} onclick={() => view?.searchStep(-1)}><ChevronUp size={15} /></button>
      <button class="btn ghost icon sm" aria-label="Next match" disabled={!matchCount} onclick={() => view?.searchStep(1)}><ChevronDown size={15} /></button>
      <button class="btn ghost icon sm" aria-label="Close search" onclick={closeSearch}><X size={15} /></button>
    </div>
  {/if}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="body" onkeydown={onPaneKey}>
    {#if outlineOpen && !p.logsOpen}
      <aside class="outline" aria-label="Document outline">
        {#if outline.length}
          <PdfOutline nodes={outline} onGo={(n) => (n.dest ? view?.goTo(n.dest) : null)} />
        {:else}
          <p>This PDF has no bookmarks. Load <code>hyperref</code> to get them from your sections.</p>
        {/if}
      </aside>
    {/if}
    <div class="pdf" class:hidden={p.logsOpen} class:with-outline={outlineOpen && !p.logsOpen}>
      <PdfView bind:this={view} bind:pageCount bind:currentPage bind:percent bind:matchCount bind:matchIndex building={p.building} failed={p.failed} onShowLogs={() => p.onLogs(true)} onSourceClick={p.onSourceClick} />
    </div>
    {#if p.logsOpen}
      <div class="pdf"><Problems diagnostics={p.diagnostics} buildId={p.buildId} onGoto={p.onGoto} onAddPackage={p.onAddPackage} getSource={p.getSource} /></div>
    {/if}
  </div>
</section>

{#if presenting && view?.getDocument()}
  <PresentationMode doc={view.getDocument()!} start={currentPage} onClose={() => (presenting = false)} />
{/if}

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
  .pane { display: flex; flex-direction: column; height: 100%; min-width: 0; background: var(--pdf-ground); container-type: inline-size; }
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
  .of { color: var(--muted); font-size: 12px; margin-right: 2px; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .bar { overflow: hidden; }
  @container (max-width: 560px) { .optional { display: none; } }
  @container (max-width: 420px) { .pages { display: none; } }
  .zoom { min-width: 64px; gap: 2px; font-variant-numeric: tabular-nums; }
  .progress { height: 2px; background: transparent; flex: none; overflow: hidden; position: relative; }
  .progress.on::after {
    content: ""; position: absolute; inset: 0; width: 35%; background: var(--accent);
    animation: slide 1.1s var(--ease) infinite;
  }
  @keyframes slide { from { transform: translateX(-100%); } to { transform: translateX(300%); } }
  .body { flex: 1; min-height: 0; position: relative; }
  .pdf { position: absolute; inset: 0; }
  .pdf.with-outline { left: 220px; }
  .outline { position: absolute; left: 0; top: 0; bottom: 0; width: 220px; overflow: auto; padding: 8px 6px; background: var(--surface); border-right: 1px solid var(--border); }
  .outline p { color: var(--muted); font-size: 12px; margin: 6px; line-height: 1.5; }
  .search { display: flex; align-items: center; gap: 6px; padding: 4px 8px; background: var(--surface); border-bottom: 1px solid var(--border); color: var(--muted); flex: none; }
  .search input { flex: 1; min-width: 0; height: 26px; border: 1px solid var(--border-strong); border-radius: var(--r-sm); background: var(--surface); color: var(--ink); padding: 0 8px; font: 13px var(--f-body); }
  .search input:focus { outline: none; border-color: var(--accent); }
  .search .count { font-size: 12px; min-width: 64px; text-align: right; font-variant-numeric: tabular-nums; }
  .pdf.hidden { visibility: hidden; }
</style>
