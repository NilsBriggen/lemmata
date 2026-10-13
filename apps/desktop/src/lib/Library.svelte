<script lang="ts">
  // The start screen, after Overleaf's project dashboard: actions on the
  // left, the projects as a list (or grid with thumbnails) on the right.
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import Plus from "@lucide/svelte/icons/plus";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import FileArchive from "@lucide/svelte/icons/file-archive";
  import Search from "@lucide/svelte/icons/search";
  import List from "@lucide/svelte/icons/list";
  import LayoutGrid from "@lucide/svelte/icons/layout-grid";
  import Settings from "@lucide/svelte/icons/settings";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import FileText from "@lucide/svelte/icons/file-text";
  import FolderSearch from "@lucide/svelte/icons/folder-search";
  import X from "@lucide/svelte/icons/x";
  import NewProjectDialog from "./NewProjectDialog.svelte";
  import ImportDialog from "./ImportDialog.svelte";
  import { api } from "./api";
  import type { Recent } from "./recent";
  import { settings, updateSettings } from "./settings.svelte";

  interface Props {
    recent: Recent[];
    error: string | null;
    onOpen: (path: string) => void;
    onForget: (path: string) => void;
    onImported: (paths: string[]) => void;
    onExample: () => void;
    onWelcome: () => void;
    onPreferences: () => void;
  }
  let p: Props = $props();

  let query = $state("");
  let thumbs = $state<Record<string, string>>({});
  let dialog = $state<null | "new" | { zip: string }>(null);

  const nameOf = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;
  const folderOf = (path: string) => path.replace(/[\\/][^\\/]+[\\/]?$/, "");
  const home = (path: string) => path.replace(/^\/home\/[^/]+|^\/Users\/[^/]+|^[A-Z]:\\Users\\[^\\]+/, "~");
  const defaultParent = $derived(p.recent[0] ? folderOf(p.recent[0].path) : "");

  const shown = $derived(
    p.recent.filter((r) => !query.trim() || r.path.toLowerCase().includes(query.trim().toLowerCase())),
  );

  const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
  function ago(t: number) {
    if (!t) return "—";
    const s = (t - Date.now()) / 1000;
    for (const [unit, secs] of [["year", 31536000], ["month", 2592000], ["week", 604800], ["day", 86400], ["hour", 3600], ["minute", 60]] as const) {
      if (Math.abs(s) >= secs) return rtf.format(Math.round(s / secs), unit);
    }
    return "just now";
  }

  onMount(() => {
    for (const r of p.recent) {
      api
        .loadThumbnail(r.path)
        .then((png) => (thumbs[r.path] = URL.createObjectURL(new Blob([png.buffer as ArrayBuffer], { type: "image/png" }))))
        .catch(() => {});
    }
  });

  async function openFolder() {
    const dir = await open({ directory: true, title: "Open a LaTeX project folder" });
    if (typeof dir === "string") p.onOpen(dir);
  }

  async function importZip() {
    const zip = await open({ title: "Choose a zip downloaded from Overleaf", filters: [{ name: "Zip", extensions: ["zip"] }] });
    if (typeof zip === "string") dialog = { zip };
  }
</script>

<div class="library">
  <aside class="rail chrome">
    <div class="brand">Lemmata</div>
    <button class="btn primary lg new" onclick={() => (dialog = "new")}><Plus size={17} /> New project</button>
    <nav>
      <button class="nav" onclick={openFolder}><FolderOpen size={16} /> Open folder…</button>
      <button class="nav" onclick={importZip}><FileArchive size={16} /> Import from Overleaf…</button>
      <button class="nav" onclick={p.onExample}><Sparkles size={16} /> Example document</button>
    </nav>
    <div class="foot">
      <button class="nav" onclick={p.onPreferences}><Settings size={16} /> Preferences</button>
      <button class="nav subtle" onclick={p.onWelcome}>Show welcome screen</button>
    </div>
  </aside>

  <main>
    <header>
      <h1>Your projects</h1>
      <div class="tools">
        <div class="search">
          <Search size={15} />
          <input id="library-search" type="search" placeholder="Search projects" bind:value={query} aria-label="Search projects" />
        </div>
        <div class="segmented" role="group" aria-label="View">
          <button aria-pressed={settings.libraryView === "list"} aria-label="List" onclick={() => updateSettings({ libraryView: "list" })}><List size={15} /></button>
          <button aria-pressed={settings.libraryView === "grid"} aria-label="Grid" onclick={() => updateSettings({ libraryView: "grid" })}><LayoutGrid size={15} /></button>
        </div>
      </div>
    </header>

    {#if p.error}<p class="error" role="alert">{p.error}</p>{/if}

    {#if p.recent.length === 0}
      <div class="empty">
        <FileText size={36} />
        <h2>No projects yet</h2>
        <p>Start a new one from a template, open a folder you already have, or bring your projects over from Overleaf (select them in Overleaf, then <i>Download</i>).</p>
        <div class="row">
          <button class="btn primary" onclick={() => (dialog = "new")}><Plus size={15} /> New project</button>
          <button class="btn" onclick={importZip}><FileArchive size={15} /> Import from Overleaf</button>
        </div>
      </div>
    {:else if shown.length === 0}
      <p class="none">No project matches “{query}”.</p>
    {:else if settings.libraryView === "list"}
      <div class="table" role="table" aria-label="Projects">
        <div class="tr th" role="row">
          <span role="columnheader">Name</span>
          <span role="columnheader">Location</span>
          <span role="columnheader">Last opened</span>
          <span></span>
        </div>
        {#each shown as r (r.path)}
          <div class="tr" role="row">
            <button class="name" role="cell" onclick={() => p.onOpen(r.path)}>
              <span class="mini">{#if thumbs[r.path]}<img src={thumbs[r.path]} alt="" />{:else}<FileText size={16} />{/if}</span>
              <span class="n">{nameOf(r.path)}</span>
            </button>
            <span class="loc" role="cell" title={r.path}>{home(folderOf(r.path))}</span>
            <span class="when" role="cell">{ago(r.lastOpened)}</span>
            <span class="acts" role="cell">
              <button class="btn ghost icon sm" title="Show in file manager" aria-label="Show {nameOf(r.path)} in file manager" onclick={() => revealItemInDir(r.path).catch(() => {})}><FolderSearch size={15} /></button>
              <button class="btn ghost icon sm" title="Remove from this list (the folder stays on disk)" aria-label="Remove {nameOf(r.path)} from the list" onclick={() => p.onForget(r.path)}><X size={15} /></button>
            </span>
          </div>
        {/each}
      </div>
    {:else}
      <div class="grid">
        {#each shown as r (r.path)}
          <div class="card">
            <button class="open" onclick={() => p.onOpen(r.path)}>
              <div class="thumb">
                {#if thumbs[r.path]}<img src={thumbs[r.path]} alt="" />{:else}<div class="blank"><i></i><i></i><i></i><i></i><i></i></div>{/if}
              </div>
              <div class="info"><b>{nameOf(r.path)}</b><span>{ago(r.lastOpened)}</span></div>
            </button>
            <button class="btn icon sm forget" aria-label="Remove {nameOf(r.path)} from the list" title="Remove from this list" onclick={() => p.onForget(r.path)}><X size={14} /></button>
          </div>
        {/each}
      </div>
    {/if}
  </main>
</div>

{#if dialog === "new"}
  <NewProjectDialog {defaultParent} onCreated={(path) => ((dialog = null), p.onOpen(path))} onClose={() => (dialog = null)} />
{:else if dialog}
  <ImportDialog zip={dialog.zip} {defaultParent} onImported={(paths) => ((dialog = null), p.onImported(paths))} onClose={() => (dialog = null)} />
{/if}

<style>
  .library { display: grid; grid-template-columns: 248px 1fr; height: 100%; }
  .rail { background: var(--chrome); color: var(--chrome-ink); border-right: 1px solid var(--chrome-border); display: flex; flex-direction: column; gap: 6px; padding: 20px 14px 14px; }
  .brand { font: 24px var(--f-display); padding: 0 8px 14px; }
  .new { width: 100%; margin-bottom: 12px; }
  nav, .foot { display: flex; flex-direction: column; gap: 2px; }
  .foot { margin-top: auto; }
  .nav {
    display: flex; align-items: center; gap: 10px; height: 34px; padding: 0 10px; border: none; border-radius: var(--r);
    background: none; color: var(--chrome-ink); text-align: left; font: 13px var(--f-body);
  }
  .nav:hover { background: var(--chrome-hover); }
  .nav :global(svg) { color: var(--chrome-muted); }
  .nav.subtle { color: var(--chrome-muted); font-size: 12px; height: 28px; }

  main { overflow: auto; padding: 28px 36px 40px; background: var(--app); }
  header { display: flex; align-items: center; justify-content: space-between; gap: 16px; flex-wrap: wrap; margin-bottom: 18px; }
  h1 { font: 400 26px var(--f-display); margin: 0; }
  .tools { display: flex; gap: 8px; align-items: center; }
  .search { display: flex; align-items: center; gap: 6px; height: 32px; padding: 0 10px; width: 260px; background: var(--surface); border: 1px solid var(--border-strong); border-radius: var(--r); color: var(--muted); }
  .search:focus-within { border-color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 22%, transparent); }
  .search input { border: none; outline: none; background: none; flex: 1; font: 13px var(--f-body); color: var(--ink); min-width: 0; }
  .error { color: var(--error); background: var(--error-soft); padding: 8px 12px; border-radius: var(--r); margin: 0 0 14px; }
  .none { color: var(--muted); }

  .table { background: var(--surface); border: 1px solid var(--border); border-radius: var(--r-lg); overflow: hidden; }
  .tr { display: grid; grid-template-columns: minmax(200px, 2fr) minmax(160px, 2fr) 130px 76px; align-items: center; border-top: 1px solid var(--border); }
  .tr:first-child { border-top: none; }
  .tr:not(.th):hover { background: var(--surface-2); }
  .th { height: 34px; font: 600 11px var(--f-body); letter-spacing: 0.06em; text-transform: uppercase; color: var(--muted); background: var(--surface-2); }
  .th > span { padding: 0 14px; }
  .name { display: flex; align-items: center; gap: 12px; height: 52px; padding: 0 14px; border: none; background: none; text-align: left; min-width: 0; }
  .name:hover .n { color: var(--accent); text-decoration: underline; }
  .mini { width: 30px; height: 38px; flex: none; display: grid; place-items: center; background: var(--surface-3); border: 1px solid var(--border); border-radius: 3px; overflow: hidden; color: var(--faint); }
  .mini img { width: 100%; height: 100%; object-fit: cover; object-position: top; }
  .n { font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .loc { padding: 0 14px; color: var(--muted); font: 12px var(--f-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .when { padding: 0 14px; color: var(--muted); }
  .acts { display: flex; gap: 2px; padding-right: 8px; justify-content: flex-end; opacity: 0; }
  .tr:hover .acts, .acts:focus-within { opacity: 1; }

  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 18px; }
  .card { position: relative; }
  .open { width: 100%; padding: 0; overflow: hidden; display: flex; flex-direction: column; text-align: left; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r-lg); transition: box-shadow 0.15s, transform 0.15s; }
  .open:hover { box-shadow: var(--shadow); transform: translateY(-1px); }
  .thumb { height: 210px; background: var(--surface-3); display: flex; justify-content: center; padding-top: 16px; overflow: hidden; border-bottom: 1px solid var(--border); }
  .thumb img { width: 76%; align-self: flex-start; box-shadow: var(--shadow-sm); background: #fff; }
  .blank { width: 70%; height: 100%; background: #fff; padding: 18px 14px; display: flex; flex-direction: column; gap: 8px; box-shadow: var(--shadow-sm); }
  .blank i { display: block; height: 4px; background: #dfe3e7; border-radius: 2px; }
  .blank i:first-child { width: 55%; height: 6px; margin: 0 auto 6px; background: #a6aeb6; }
  .info { padding: 10px 12px; display: flex; flex-direction: column; gap: 1px; }
  .info span { color: var(--muted); font-size: 12px; }
  .forget { position: absolute; top: 8px; right: 8px; opacity: 0; }
  .card:hover .forget, .forget:focus-visible { opacity: 1; }

  .empty { max-width: 460px; margin: 10vh auto 0; text-align: center; display: flex; flex-direction: column; align-items: center; gap: 8px; color: var(--faint); }
  .empty h2 { font: 400 22px var(--f-display); color: var(--ink); margin: 6px 0 0; }
  .empty p { color: var(--muted); margin: 0 0 10px; line-height: 1.6; }
  .row { display: flex; gap: 8px; }
</style>
