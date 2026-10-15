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
  import PanelLeft from "@lucide/svelte/icons/panel-left";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import Presentation from "@lucide/svelte/icons/presentation";
  import X from "@lucide/svelte/icons/x";
  import ChevronUp from "@lucide/svelte/icons/chevron-up";
  import PdfView, { type Fit, type OutlineNode, type PageLayout } from "./PdfView.svelte";
  import PdfOutline from "./PdfOutline.svelte";
  import PdfThumbnails from "./PdfThumbnails.svelte";
  import type { PDFDocumentProxy } from "pdfjs-dist";
  import PresentationMode from "./Presentation.svelte";
  import Problems from "./Problems.svelte";
  import DropMenu from "./ui/DropMenu.svelte";
  import { settings, updateSettings } from "./settings.svelte";
  import type { Diagnostic, PdfBox } from "./api";
  import { display, keysOf } from "./shortcuts";

  interface Props {
    diagnostics: Diagnostic[];
    building: boolean;
    /** A plain-language note on what a long build is doing, shown while building. */
    buildNote?: string | null;
    /** The last build ended without writing a PDF. */
    failed?: boolean;
    /** Outcome of the last build, or null before the first one. */
    status?: string | null;
    buildId: number;
    logsOpen: boolean;
    onLogs: (open: boolean) => void;
    onSourceClick: (page: number, x: number, y: number) => void;
    onGoto: (file: string, line: number) => void;
    onAddPackage: (pkg: string) => void;
    getSource: (file: string) => Promise<string | null>;
    onExport: () => void;
    /** Opens the PDF in the system's default viewer; the button shows only when set. */
    onOpenExternally?: () => void;
    /** Single page or spreads (see PageLayout); the pane follows this and reports the user's choice. */
    pageLayout?: PageLayout;
    onPageLayout?: (layout: PageLayout) => void;
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
  /** The side area: the document outline or page thumbnails. */
  let side = $state<"outline" | "thumbs" | null>(null);
  let layout = $state<PageLayout>("single");
  $effect(() => {
    layout = p.pageLayout ?? "single";
  });
  const setLayout = (l: PageLayout) => {
    layout = l;
    p.onPageLayout?.(l);
  };
  /** The loaded document, for the thumbnails; replaced on every rebuild. */
  let thumbDoc = $state.raw<PDFDocumentProxy | null>(null);
  let outline = $state<OutlineNode[]>([]);
  let presenting = $state(false);
  let searchTimer: ReturnType<typeof setTimeout> | undefined;

  const errors = $derived(p.diagnostics.filter((d) => d.severity === "error").length);
  const warnings = $derived(p.diagnostics.filter((d) => d.severity === "warning").length);

  export const load = async (data: Uint8Array) => {
    await view?.load(data);
    thumbDoc = view?.getDocument() ?? null;
    if (side === "outline") refreshOutline();
  };

  async function refreshOutline() {
    outline = (await view?.getOutline()) ?? [];
  }
  async function toggleSide(which: "outline" | "thumbs") {
    side = side === which ? null : which;
    if (side === "outline") await refreshOutline();
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
        <button class="btn ghost sm zoom" bind:this={zoomBtn} aria-label="Zoom level" aria-haspopup="menu" aria-expanded={zoomMenu} onclick={() => (zoomMenu = !zoomMenu)}>{percent}% <ChevronDown size={13} /></button>
        <button class="btn ghost icon sm optional" aria-label="Zoom in" onclick={() => view?.zoomBy(1.15)}><ZoomIn size={16} /></button>
      </div>
      <div class="group end">
        <button class="btn ghost icon sm" class:on={searchOpen} aria-pressed={searchOpen} title="Search in the PDF ({display(keysOf("pdf-search"))})" aria-label="Search in the PDF" onclick={() => (searchOpen ? closeSearch() : openSearch())}><Search size={15} /></button>
        <button class="btn ghost icon sm optional" class:on={side === "outline"} aria-pressed={side === "outline"} title="Document outline" aria-label="Document outline" onclick={() => toggleSide("outline")}><ListTree size={15} /></button>
        <button class="btn ghost icon sm optional" class:on={side === "thumbs"} aria-pressed={side === "thumbs"} title="Page thumbnails" aria-label="Page thumbnails" onclick={() => toggleSide("thumbs")}><PanelLeft size={15} /></button>
        <button class="btn ghost icon sm optional" title="Present full screen" aria-label="Present full screen" onclick={() => (presenting = true)}><Presentation size={15} /></button>
        <button class="btn ghost icon sm" class:on={settings.pdfDark} aria-pressed={settings.pdfDark} title="Dark pages in dark mode" aria-label="Dark pages" onclick={() => updateSettings({ pdfDark: !settings.pdfDark })}><Moon size={15} /></button>
        {#if p.onOpenExternally}
          <button class="btn ghost icon sm optional" title="Open in the system's PDF viewer" aria-label="Open in external viewer" onclick={p.onOpenExternally}><ExternalLink size={15} /></button>
        {/if}
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
    {#if side && !p.logsOpen}
      <aside class="outline" aria-label={side === "outline" ? "Document outline" : "Page thumbnails"}>
        {#if side === "thumbs"}
          <PdfThumbnails doc={thumbDoc} current={currentPage} onGo={(n) => view?.gotoPage(n)} />
        {:else if outline.length}
          <PdfOutline nodes={outline} onGo={(n) => (n.dest ? view?.goTo(n.dest) : null)} />
        {:else}
          <p>This PDF has no bookmarks. Load <code>hyperref</code> to get them from your sections.</p>
        {/if}
      </aside>
    {/if}
    {#if p.building && p.buildNote}
      <div class="note" role="status">{p.buildNote}</div>
    {/if}
    {#if p.failed && pageCount > 0 && !p.logsOpen && !p.building}
      <div class="stale" role="status">
        <span>The latest build failed{errors ? ` with ${errors} error${errors === 1 ? "" : "s"}` : ""}. This is the last PDF that built.</span>
        <button class="btn sm" onclick={() => p.onLogs(true)}>Show logs</button>
      </div>
    {/if}
    <div class="pdf" class:hidden={p.logsOpen} class:with-outline={side && !p.logsOpen}>
      <PdfView bind:this={view} bind:pageCount bind:currentPage bind:percent bind:matchCount bind:matchIndex pageLayout={layout} building={p.building} failed={p.failed} onShowLogs={() => p.onLogs(true)} onSourceClick={p.onSourceClick} />
    </div>
    {#if p.logsOpen}
      <div class="pdf"><Problems diagnostics={p.diagnostics} status={p.status ?? null} buildId={p.buildId} onGoto={p.onGoto} onAddPackage={p.onAddPackage} getSource={p.getSource} /></div>
    {/if}
  </div>
</section>

{#if presenting && view?.getDocument()}
  <PresentationMode doc={view.getDocument()!} start={currentPage} onClose={() => (presenting = false)} />
{/if}

{#if zoomMenu && zoomBtn}
  <DropMenu anchor={zoomBtn} onClose={() => (zoomMenu = false)}>
    <button role="menuitem" onclick={() => fit("width")}>Fit width</button>
    <button role="menuitem" onclick={() => fit("page")}>Fit page</button>
    <hr />
    <button role="menuitemradio" aria-checked={layout === "single"} onclick={() => setLayout("single")}>Single page</button>
    <button role="menuitemradio" aria-checked={layout === "spread"} onclick={() => setLayout("spread")}>Two pages</button>
    <button role="menuitemradio" aria-checked={layout === "spread-first"} onclick={() => setLayout("spread-first")}>Start with two pages</button>
    <hr />
    {#each [0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4] as z (z)}
      <button role="menuitem" onclick={() => fit(z)}>{z * 100}%</button>
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
  .note { padding: 6px 10px; background: var(--accent-soft); color: var(--ink); border-bottom: 1px solid var(--border); font-size: 12.5px; }
  .stale { display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 6px 10px; background: var(--error-soft); color: var(--ink); border-bottom: 1px solid var(--border); font-size: 12.5px; }
</style>
