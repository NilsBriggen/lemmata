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
  import Ellipsis from "@lucide/svelte/icons/ellipsis";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import Folder from "@lucide/svelte/icons/folder";
  import FolderPlus from "@lucide/svelte/icons/folder-plus";
  import SquarePen from "@lucide/svelte/icons/square-pen";
  import Trash2 from "@lucide/svelte/icons/trash";
  import Check from "@lucide/svelte/icons/check";
  import DropMenu from "./ui/DropMenu.svelte";
  import Modal from "./ui/Modal.svelte";
  import { countIn, createGroup, deleteGroup, forgetPath, inView, moveProject, nameProblem, readGroups, renameGroup, restoreGroup, writeGroups, type Groups, type View } from "./groups";
  import NewProjectDialog from "./NewProjectDialog.svelte";
  import ImportDialog from "./ImportDialog.svelte";
  import { api, errorMessage } from "./api";
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

  // Folders: named groups of projects. They only organise this list.
  let groups = $state<Groups>(readGroups());
  let view = $state<View>("all");
  /** The folder being created ("") or renamed (its id), with the text typed so far. */
  let editing = $state<{ id: string | null; name: string; error: string | null } | null>(null);
  /** The menu of one project: open, show, folder, remove, delete. */
  let projectMenu = $state<{ path: string; anchor: HTMLElement } | null>(null);
  let deleting = $state<{ path: string; busy: boolean; error: string | null } | null>(null);
  /** The folder just deleted, and the folders as they were before, so Undo can put it back. */
  let undoable = $state.raw<{ id: string; name: string; before: Groups } | null>(null);
  const activeGroup = $derived(groups.groups.find((g) => g.id === view));
  const unfiled = $derived(p.recent.filter((r) => !(r.path in groups.members)).length);

  // Any other change to the folders ends the Undo notice.
  function commit(next: Groups) {
    groups = next;
    undoable = null;
    writeGroups(next);
  }

  function startEdit(id: string | null) {
    undoable = null;
    editing = { id, name: id ? (groups.groups.find((g) => g.id === id)?.name ?? "") : "", error: null };
  }

  $effect(() => {
    if (!undoable) return;
    const timer = setTimeout(() => (undoable = null), 8000);
    return () => clearTimeout(timer);
  });

  function finishEdit(e?: SubmitEvent) {
    e?.preventDefault();
    if (!editing) return;
    const problem = nameProblem(groups, editing.name, editing.id ?? undefined);
    if (problem) {
      editing.error = problem;
      return;
    }
    if (editing.id) commit(renameGroup(groups, editing.id, editing.name)!);
    else {
      const made = createGroup(groups, editing.name)!;
      commit(made.state);
      view = made.id;
    }
    editing = null;
  }

  function removeGroup(id: string) {
    const before = groups;
    const name = before.groups.find((g) => g.id === id)?.name ?? "";
    commit(deleteGroup(groups, id));
    if (view === id) view = "all";
    undoable = { id, name, before };
  }

  function undoRemove() {
    if (undoable) commit(restoreGroup(groups, undoable.before, undoable.id));
  }

  /** New and imported projects land in the folder being looked at. */
  function filed(paths: string[]) {
    if (!activeGroup) return;
    let next = groups;
    for (const path of paths) next = moveProject(next, path, activeGroup.id);
    commit(next);
  }

  const folderName = (path: string) => groups.groups.find((g) => g.id === groups.members[path])?.name;

  const nameOf = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;
  const folderOf = (path: string) => path.replace(/[\\/][^\\/]+[\\/]?$/, "");
  const home = (path: string) => path.replace(/^\/home\/[^/]+|^\/Users\/[^/]+|^[A-Z]:\\Users\\[^\\]+/, "~");
  const defaultParent = $derived(p.recent[0] ? folderOf(p.recent[0].path) : "");

  // The sort choice is kept in localStorage, like the recent list.
  const SORT_KEY = "lemmata.librarySort";
  function readSort(): "opened" | "name" {
    try {
      return localStorage.getItem(SORT_KEY) === "name" ? "name" : "opened";
    } catch {
      return "opened";
    }
  }
  let sort = $state<"opened" | "name">(readSort());
  function setSort(next: "opened" | "name") {
    sort = next;
    try {
      localStorage.setItem(SORT_KEY, next);
    } catch {
      // Storage unavailable: the choice just isn't remembered.
    }
  }

  const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });
  const shown = $derived.by(() => {
    const term = query.trim().toLowerCase();
    const list = p.recent.filter((r) => inView(groups, r.path, view) && (!term || r.path.toLowerCase().includes(term)));
    if (sort === "name") list.sort((a, b) => collator.compare(nameOf(a.path), nameOf(b.path)));
    // Name matches come before matches that are only in the folder path. The sort is stable, so each part keeps its order.
    if (term) list.sort((a, b) => Number(!nameOf(a.path).toLowerCase().includes(term)) - Number(!nameOf(b.path).toLowerCase().includes(term)));
    return list;
  });

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
    if (typeof dir === "string") {
      filed([dir]);
      p.onOpen(dir);
    }
  }

  /** Removing a project from the list also takes it out of its folder. */
  function forget(path: string) {
    commit(forgetPath(groups, path));
    p.onForget(path);
  }

  /** Right-clicking a project opens the same menu as its ⋯ button. */
  function contextMenu(e: MouseEvent, path: string) {
    const more = (e.currentTarget as HTMLElement).querySelector<HTMLElement>(".more");
    if (!more) return;
    e.preventDefault();
    projectMenu = { path, anchor: more };
  }

  async function deleteProject() {
    if (!deleting) return;
    const { path } = deleting;
    deleting = { path, busy: true, error: null };
    try {
      await api.trashProject(path);
      if (thumbs[path]) URL.revokeObjectURL(thumbs[path]);
      delete thumbs[path];
      forget(path);
      deleting = null;
    } catch (e) {
      deleting = { path, busy: false, error: errorMessage(e) };
    }
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
    <div class="folders" role="group" aria-label="Folders">
      <div class="fhead"><span>Folders</span><button class="btn ghost icon sm" aria-label="New folder" title="New folder" onclick={() => startEdit(null)}><FolderPlus size={14} /></button></div>
      <button class="nav" class:on={view === "all"} aria-pressed={view === "all"} onclick={() => (view = "all")}><FileText size={16} /> All projects <i>{p.recent.length}</i></button>
      {#each groups.groups as g (g.id)}
        {#if editing?.id === g.id}
          {@render editor()}
        {:else}
          <div class="frow" class:on={view === g.id}>
            <button class="nav" aria-pressed={view === g.id} onclick={() => (view = g.id)}><Folder size={16} /> <span class="fname">{g.name}</span> <i>{countIn(groups, g.id)}</i></button>
            <span class="facts">
              <button class="btn ghost icon sm" aria-label="Rename {g.name}" title="Rename" onclick={() => startEdit(g.id)}><SquarePen size={13} /></button>
              <button class="btn ghost icon sm" aria-label="Delete {g.name}" title="Delete the folder (its projects stay in All projects)" onclick={() => removeGroup(g.id)}><Trash2 size={13} /></button>
            </span>
          </div>
        {/if}
      {/each}
      {#if groups.groups.length && unfiled !== p.recent.length}
        <button class="nav" class:on={view === "none"} aria-pressed={view === "none"} onclick={() => (view = "none")}><FolderOpen size={16} /> No folder <i>{unfiled}</i></button>
      {/if}
      {#if editing && editing.id === null}{@render editor()}{/if}
    </div>
    {#if undoable}
      <p class="undo" role="status">Folder “{undoable.name}” deleted · <button onclick={undoRemove}>Undo</button></p>
    {/if}
    <div class="foot">
      <button class="nav" onclick={p.onPreferences}><Settings size={16} /> Preferences</button>
      <button class="nav subtle" onclick={p.onWelcome}>Show welcome screen</button>
    </div>
  </aside>

  <main>
    <header>
      <h1>{activeGroup ? activeGroup.name : view === "none" ? "No folder" : "Your projects"}</h1>
      <div class="tools">
        <div class="search">
          <Search size={15} />
          <input id="library-search" type="search" placeholder="Search projects" bind:value={query} aria-label="Search projects" />
        </div>
        <div class="segmented" role="group" aria-label="View">
          <button aria-pressed={settings.libraryView === "list"} aria-label="List" onclick={() => updateSettings({ libraryView: "list" })}><List size={15} /></button>
          <button aria-pressed={settings.libraryView === "grid"} aria-label="Grid" onclick={() => updateSettings({ libraryView: "grid" })}><LayoutGrid size={15} /></button>
        </div>
        <select class="input sort" aria-label="Sort projects" value={sort} onchange={(e) => setSort(e.currentTarget.value as "opened" | "name")}>
          <option value="opened">Last opened</option>
          <option value="name">Name</option>
        </select>
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
      {#if query.trim()}
        <p class="none">No project matches “{query}”.</p>
      {:else if activeGroup}
        <p class="none">No projects in this folder yet. Right-click a project (or use its ⋯ button) to put it here, or create a new project here.</p>
      {:else}
        <p class="none">Every project is in a folder.</p>
      {/if}
    {:else if settings.libraryView === "list"}
      <div class="table" role="table" aria-label="Projects">
        <div class="tr th" role="row">
          <span role="columnheader">Name</span>
          <span role="columnheader">Location</span>
          <span role="columnheader">Last opened</span>
          <span></span>
        </div>
        {#each shown as r (r.path)}
          <!-- svelte-ignore a11y_interactive_supports_focus (right-click is a shortcut; the ⋯ button is the keyboard way) -->
          <div class="tr" role="row" oncontextmenu={(e) => contextMenu(e, r.path)}>
            <button class="name" role="cell" onclick={() => p.onOpen(r.path)}>
              <span class="mini">{#if thumbs[r.path]}<img src={thumbs[r.path]} alt="" />{:else}<FileText size={16} />{/if}</span>
              <span class="n">{nameOf(r.path)}</span>
              {#if view === "all" && folderName(r.path)}<span class="chip">{folderName(r.path)}</span>{/if}
            </button>
            <span class="loc" role="cell" title={r.path}>{home(folderOf(r.path))}</span>
            <span class="when" role="cell">{ago(r.lastOpened)}</span>
            <span class="acts" role="cell">
              <button class="btn ghost icon sm more" title="More actions" aria-label="Actions for {nameOf(r.path)}" aria-haspopup="menu" onclick={(e) => (projectMenu = { path: r.path, anchor: e.currentTarget })}><Ellipsis size={16} /></button>
            </span>
          </div>
        {/each}
      </div>
    {:else}
      <div class="grid">
        {#each shown as r (r.path)}
          <div class="card" role="presentation" oncontextmenu={(e) => contextMenu(e, r.path)}>
            <button class="open" onclick={() => p.onOpen(r.path)}>
              <div class="thumb">
                {#if thumbs[r.path]}<img src={thumbs[r.path]} alt="" />{:else}<div class="blank"><i></i><i></i><i></i><i></i><i></i></div>{/if}
              </div>
              <div class="info"><b>{nameOf(r.path)}</b><span>{ago(r.lastOpened)}</span></div>
            </button>
            <button class="btn icon sm more" title="More actions" aria-label="Actions for {nameOf(r.path)}" aria-haspopup="menu" onclick={(e) => (projectMenu = { path: r.path, anchor: e.currentTarget })}><Ellipsis size={15} /></button>
          </div>
        {/each}
      </div>
    {/if}
  </main>
</div>

{#snippet editor()}
  <form class="fedit" onsubmit={finishEdit}>
    <!-- svelte-ignore a11y_autofocus -->
    <input class="input" bind:value={editing!.name} placeholder="Folder name" aria-label="Folder name" autofocus onkeydown={(e) => e.key === "Escape" && (editing = null)} onblur={() => editing && !editing.name.trim() && (editing = null)} />
    {#if editing?.error}<span class="ferr">{editing.error}</span>{/if}
  </form>
{/snippet}

{#if projectMenu}
  {@const path = projectMenu.path}
  <DropMenu anchor={projectMenu.anchor} align="end" onClose={() => (projectMenu = null)}>
    <button role="menuitem" onclick={() => p.onOpen(path)}><ExternalLink size={15} /> Open</button>
    <button role="menuitem" onclick={() => revealItemInDir(path).catch(() => {})}><FolderSearch size={15} /> Show in file manager</button>
    <hr />
    <div class="heading">Put in folder</div>
    {#each groups.groups as g (g.id)}
      <button role="menuitemradio" aria-checked={groups.members[path] === g.id} onclick={() => commit(moveProject(groups, path, g.id))}>
        <span class="tick">{#if groups.members[path] === g.id}<Check size={14} />{:else}<Folder size={14} />{/if}</span> {g.name}
      </button>
    {/each}
    {#if groups.members[path]}
      <button role="menuitem" onclick={() => commit(moveProject(groups, path, null))}><span class="tick"></span> Take out of its folder</button>
    {/if}
    <button role="menuitem" onclick={() => startEdit(null)}><span class="tick"><FolderPlus size={14} /></span> New folder…</button>
    <hr />
    <button role="menuitem" onclick={() => forget(path)}><X size={15} /> Remove from list <span class="hint">keeps files</span></button>
    <button role="menuitem" class="danger" onclick={() => (deleting = { path, busy: false, error: null })}><Trash2 size={15} /> Delete project…</button>
  </DropMenu>
{/if}

{#if deleting}
  {@const name = nameOf(deleting.path)}
  <Modal title="Delete {name}?" width={460} onClose={() => !deleting?.busy && (deleting = null)}>
    <p class="dmsg">This moves the folder and everything in it to your system's trash. You can restore it from there.</p>
    <code class="dpath">{deleting.path}</code>
    {#if deleting.error}<p class="derr" role="alert">{deleting.error}</p>{/if}
    {#snippet footer()}
      <button class="btn" disabled={deleting?.busy} onclick={() => (deleting = null)}>Cancel</button>
      <button class="btn primary danger-btn" disabled={deleting?.busy} onclick={deleteProject}>Move to trash</button>
    {/snippet}
  </Modal>
{/if}

{#if dialog === "new"}
  <NewProjectDialog {defaultParent} onCreated={(path) => ((dialog = null), filed([path]), p.onOpen(path))} onClose={() => (dialog = null)} />
{:else if dialog}
  <ImportDialog zip={dialog.zip} {defaultParent} onImported={(paths) => ((dialog = null), filed(paths), p.onImported(paths))} onClose={() => (dialog = null)} />
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
  .tr { display: grid; grid-template-columns: minmax(200px, 2fr) minmax(160px, 2fr) 130px 48px; align-items: center; border-top: 1px solid var(--border); }
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
  .acts { display: flex; padding-right: 8px; justify-content: flex-end; opacity: 0.45; }
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
  .card .more { position: absolute; top: 8px; right: 8px; opacity: 0.45; }
  .card:hover .more, .card:focus-within .more { opacity: 1; }

  .empty { max-width: 460px; margin: 10vh auto 0; text-align: center; display: flex; flex-direction: column; align-items: center; gap: 8px; color: var(--faint); }
  .empty h2 { font: 400 22px var(--f-display); color: var(--ink); margin: 6px 0 0; }
  .empty p { color: var(--muted); margin: 0 0 10px; line-height: 1.6; }
  .row { display: flex; gap: 8px; }

  .folders { display: flex; flex-direction: column; gap: 2px; margin-top: 14px; min-height: 0; overflow: auto; }
  .fhead { display: flex; align-items: center; justify-content: space-between; padding: 0 4px 0 10px; font: 600 11px var(--f-body); letter-spacing: 0.08em; text-transform: uppercase; color: var(--chrome-muted); }
  .fhead .btn { color: var(--chrome-muted); }
  .nav i { margin-left: auto; font-style: normal; font-size: 11.5px; color: var(--chrome-muted); }
  .nav.on, .frow.on > .nav { background: var(--chrome-active); }
  .frow { display: flex; align-items: center; }
  .frow.on { background: var(--chrome-active); border-radius: var(--r); }
  .frow > .nav { flex: 1; min-width: 0; }
  .fname { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* Visible but faint, so they can be found; full strength on hover or keyboard focus. */
  .facts { display: flex; flex: none; opacity: 0.45; }
  .frow:hover .facts, .frow:focus-within .facts { opacity: 1; }
  .undo { margin: 8px 6px 0; font-size: 12.5px; color: var(--chrome-muted); overflow-wrap: anywhere; }
  .undo button { border: none; background: none; padding: 0; font: inherit; color: var(--chrome-ink); text-decoration: underline; cursor: pointer; }
  .tools select.sort { width: auto; }
  .fedit { display: flex; flex-direction: column; gap: 3px; padding: 2px 4px; }
  .fedit .input { height: 30px; }
  .ferr { color: var(--error); font-size: 11.5px; padding-left: 4px; }
  .chip { margin-left: 8px; padding: 0 7px; border-radius: 9px; background: var(--surface-3); color: var(--muted); font-size: 11px; line-height: 18px; flex: none; }
  :global(.menu) .danger, :global(.menu) .danger :global(svg) { color: var(--error); }
  .dmsg { margin: 0 0 10px; color: var(--muted); }
  .dpath { display: block; padding: 8px 10px; background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--r); font: 12px var(--f-mono); overflow-wrap: anywhere; }
  .derr { color: var(--error); margin: 10px 0 0; }
  .danger-btn { background: var(--error) !important; border-color: var(--error) !important; }
  .tick { display: inline-flex; width: 16px; justify-content: center; }
</style>
