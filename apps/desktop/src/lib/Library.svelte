<script lang="ts">
  // The start screen, after Overleaf's project dashboard: actions on the left; on the right the
  // folders and the projects in none, as a list (or grid with thumbnails). Projects are dragged into
  // a folder, or moved with "Move to folder…" in their menu.
  import { onMount, untrack } from "svelte";
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
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import LayoutTemplate from "@lucide/svelte/icons/layout-template";
  import DropMenu from "./ui/DropMenu.svelte";
  import Modal from "./ui/Modal.svelte";
  import { dragState, startDrag, wasDrag } from "./treeDrag.svelte";
  import { createGroup, deleteGroup, forgetPath, inView, moveProject, nameProblem, readGroups, renameGroup, restoreGroup, writeGroups, type Groups, type View } from "./groups";
  import NewProjectDialog from "./NewProjectDialog.svelte";
  import SaveTemplateDialog from "./SaveTemplateDialog.svelte";
  import ImportDialog from "./ImportDialog.svelte";
  import CloneDialog from "./git/CloneDialog.svelte";
  import GitBranch from "@lucide/svelte/icons/git-branch";
  import { api, errorMessage } from "./api";
  import type { Recent } from "./recent";
  import { settings, updateSettings } from "./settings.svelte";

  interface Props {
    recent: Recent[];
    error: string | null;
    /** The folder that failed to open because it has no main document; offers to create one. */
    noMainIn?: string | null;
    onCreateMain?: (dir: string) => void;
    onOpen: (path: string) => void;
    onForget: (path: string) => void;
    onImported: (paths: string[]) => void;
    onExample: () => void;
    onWelcome: () => void;
    onPreferences: () => void;
    /** A lemmata:// link asked to clone a repository or start from a template. */
    linkRequest?: { kind: "clone"; url: string } | { kind: "new_project"; template: string } | null;
    onLinkHandled?: () => void;
  }
  let p: Props = $props();

  let query = $state("");
  let thumbs = $state<Record<string, string>>({});
  let dialog = $state<null | "new" | "clone" | { zip: string }>(null);
  let linkedUrl = $state("");
  let linkedTemplate = $state("");
  $effect(() => {
    const r = p.linkRequest;
    if (!r) return;
    if (r.kind === "clone") (linkedUrl = r.url), (dialog = "clone");
    else (linkedTemplate = r.template), (dialog = "new");
    untrack(() => p.onLinkHandled?.());
  });

  // Folders: named groups of projects. They only organise this list.
  let groups = $state<Groups>(readGroups());
  let view = $state<View>("root");
  /** The folder being created (id null) or renamed, with the text typed so far. */
  let editing = $state<{ id: string | null; name: string; error: string | null; project?: string } | null>(null);
  /** The menu of one project: open, show, move, remove, delete. */
  let projectMenu = $state<{ path: string; anchor: HTMLElement } | null>(null);
  /** The menu of one folder: open, rename, delete. */
  let folderMenu = $state<{ id: string; anchor: HTMLElement } | null>(null);
  /** The project whose "Move to folder…" dialog is open. */
  let moving = $state<string | null>(null);
  let deleting = $state<{ path: string; busy: boolean; error: string | null } | null>(null);
  /** The project being saved as a template, and the note once it is. */
  let templating = $state<string | null>(null);
  let templateNote = $state<string | null>(null);
  $effect(() => {
    if (!templateNote) return;
    const timer = setTimeout(() => (templateNote = null), 8000);
    return () => clearTimeout(timer);
  });

  function templateSaved(name: string, trash: boolean) {
    const path = templating!;
    templating = null;
    templateNote = `Template “${name}” saved. It's offered under My templates when you create a project.`;
    if (trash) {
      deleting = { path, busy: false, error: null };
      void deleteProject();
    }
  }
  /** The folder just deleted, and the folders as they were before, so Undo can put it back. */
  let undoable = $state.raw<{ id: string; name: string; before: Groups } | null>(null);
  const activeGroup = $derived(groups.groups.find((g) => g.id === view));

  // Any other change to the folders ends the Undo notice.
  function commit(next: Groups) {
    groups = next;
    undoable = null;
    writeGroups(next);
  }

  /** `project`: a new folder made from a project's menu takes that project in. */
  function startEdit(id: string | null, project?: string) {
    undoable = null;
    editing = { id, name: id ? (groups.groups.find((g) => g.id === id)?.name ?? "") : "", error: null, project };
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
      commit(editing.project ? moveProject(made.state, editing.project, made.id) : made.state);
    }
    editing = null;
  }

  function removeGroup(id: string) {
    const before = groups;
    const name = before.groups.find((g) => g.id === id)?.name ?? "";
    commit(deleteGroup(groups, id));
    if (view === id) view = "root";
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
  /** A search looks through every folder; otherwise the view's projects. */
  const shown = $derived.by(() => {
    const term = query.trim().toLowerCase();
    const list = p.recent.filter((r) => (term ? r.path.toLowerCase().includes(term) : inView(groups, r.path, view)));
    if (sort === "name") list.sort((a, b) => collator.compare(nameOf(a.path), nameOf(b.path)));
    // Name matches come before matches that are only in the folder path. The sort is stable, so each part keeps its order.
    if (term) list.sort((a, b) => Number(!nameOf(a.path).toLowerCase().includes(term)) - Number(!nameOf(b.path).toLowerCase().includes(term)));
    return list;
  });

  /** The folders, shown at the top level (not during a search), by name, with what's in them. */
  const folders = $derived.by(() => {
    if (view !== "root" || query.trim()) return [];
    return [...groups.groups]
      .sort((a, b) => collator.compare(a.name, b.name))
      .map((g) => {
        const inside = p.recent.filter((r) => groups.members[r.path] === g.id);
        return { ...g, count: inside.length, last: Math.max(0, ...inside.map((r) => r.lastOpened)) };
      });
  });

  /** Dragging a project onto a folder (`data-drop` = its id) or onto "Your projects" (`""`, no folder). */
  function press(e: PointerEvent, path: string) {
    startDrag(
      e,
      { path, name: nameOf(path) },
      () => [path],
      ([dragged], target) => commit(moveProject(groups, dragged, target || null)),
      ([dragged], target) => (groups.members[dragged] ?? "") !== target,
    );
  }
  const dropping = (target: string) => dragState.active && dragState.target === target;

  /** Right-clicking a folder opens the same menu as its ⋯ button. */
  function folderContext(e: MouseEvent, id: string) {
    const more = (e.currentTarget as HTMLElement).querySelector<HTMLElement>(".more");
    if (!more) return;
    e.preventDefault();
    folderMenu = { id, anchor: more };
  }

  const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
  function ago(t: number) {
    if (!t) return "—";
    const s = (t - Date.now()) / 1000;
    for (const [unit, secs] of [["year", 31536000], ["month", 2592000], ["week", 604800], ["day", 86400], ["hour", 3600], ["minute", 60]] as const) {
      if (Math.abs(s) >= secs) return rtf.format(Math.round(s / secs), unit);
    }
    return "just now";
  }

  // Thumbnails are read when their card or row scrolls into view, a few at a time.
  // Each project is asked for once; a failed one just keeps its placeholder.
  const THUMB_IN_FLIGHT = 6;
  const thumbAsked = new Set<string>();
  const thumbQueue: string[] = [];
  const thumbPathOf = new Map<Element, string>();
  let thumbBusy = 0;
  let thumbsAlive = true;
  let thumbObserver: IntersectionObserver | undefined;

  function askThumb(path: string) {
    if (thumbAsked.has(path)) return;
    thumbAsked.add(path);
    thumbQueue.push(path);
    pumpThumbs();
  }

  function pumpThumbs() {
    while (thumbBusy < THUMB_IN_FLIGHT && thumbQueue.length) {
      const path = thumbQueue.shift()!;
      thumbBusy++;
      api
        .loadThumbnail(path)
        .then((png) => {
          if (thumbsAlive) thumbs[path] = URL.createObjectURL(new Blob([png.buffer as ArrayBuffer], { type: "image/png" }));
        })
        .catch(() => {})
        .finally(() => {
          thumbBusy--;
          pumpThumbs();
        });
    }
  }

  /** Svelte action: asks for the thumbnail of `path` once `el` is near the visible area. */
  function thumbWhenVisible(el: HTMLElement, path: string) {
    thumbPathOf.set(el, path);
    thumbObserver ??= new IntersectionObserver(
      (entries, observer) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          observer.unobserve(entry.target);
          askThumb(thumbPathOf.get(entry.target)!);
        }
      },
      { rootMargin: "200px 0px" },
    );
    thumbObserver.observe(el);
    return {
      destroy() {
        thumbObserver?.unobserve(el);
        thumbPathOf.delete(el);
      },
    };
  }

  onMount(() => () => {
    thumbsAlive = false;
    thumbObserver?.disconnect();
    thumbQueue.length = 0;
    for (const url of Object.values(thumbs)) URL.revokeObjectURL(url);
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
      <button class="nav" onclick={() => (dialog = "clone")}><GitBranch size={16} /> Clone a repository…</button>
      <button class="nav" onclick={importZip}><FileArchive size={16} /> Import from Overleaf…</button>
      <button class="nav" onclick={p.onExample}><Sparkles size={16} /> Example document</button>
    </nav>
    {#if templateNote}
      <p class="undo" role="status">{templateNote}</p>
    {/if}
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
      <h1 class="crumbs">
        {#if activeGroup && !query.trim()}
          <!-- Also a drop target: dropping a project here takes it out of its folder. -->
          <button class="crumb" data-drop="" class:drop={dropping("")} onclick={() => (view = "root")}>Your projects</button>
          <ChevronRight size={20} />
          <span class="here">{activeGroup.name}</span>
        {:else}
          Your projects
        {/if}
      </h1>
      <div class="tools">
        {#if activeGroup && !query.trim()}
          <button class="btn sm" onclick={() => startEdit(activeGroup.id)}><SquarePen size={14} /> Rename</button>
          <button class="btn sm" onclick={() => removeGroup(activeGroup.id)}><Trash2 size={14} /> Delete folder</button>
        {:else if !query.trim()}
          <button class="btn sm" onclick={() => startEdit(null)}><FolderPlus size={14} /> New folder</button>
        {/if}
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

    {#if p.error}
      <p class="error" role="alert">
        {p.noMainIn ? "This folder has no main document: no .tex file in it starts with \\documentclass." : p.error}
        {#if p.noMainIn && p.onCreateMain}<button class="btn sm" onclick={() => p.onCreateMain?.(p.noMainIn!)}>Create main.tex and open</button>{/if}
      </p>
    {/if}

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
    {:else if folders.length + shown.length === 0}
      {#if query.trim()}
        <p class="none">No project matches “{query}”.</p>
      {:else if activeGroup}
        <p class="none">No projects in this folder yet. Drag one onto the folder in <i>Your projects</i>, use <i>Move to folder…</i> in its menu, or create a new project here.</p>
      {/if}
    {:else if settings.libraryView === "list"}
      <div class="table" role="table" aria-label="Projects">
        <div class="tr th" role="row">
          <span role="columnheader">Name</span>
          <span role="columnheader">Location</span>
          <span role="columnheader">Last opened</span>
          <span></span>
        </div>
        {#each folders as f (f.id)}
          <!-- svelte-ignore a11y_interactive_supports_focus (right-click is a shortcut; the ⋯ button is the keyboard way) -->
          <div class="tr folder" role="row" data-drop={f.id} class:drop={dropping(f.id)} oncontextmenu={(e) => folderContext(e, f.id)}>
            <button class="name" role="cell" onclick={() => (view = f.id)}>
              <span class="mini fold"><Folder size={18} /></span>
              <span class="n">{f.name}</span>
            </button>
            <span class="loc count" role="cell">{f.count === 1 ? "1 project" : `${f.count} projects`}</span>
            <span class="when" role="cell">{f.count ? ago(f.last) : "—"}</span>
            <span class="acts" role="cell">
              <button class="btn ghost icon sm more" title="More actions" aria-label="Actions for folder {f.name}" aria-haspopup="menu" onclick={(e) => (folderMenu = { id: f.id, anchor: e.currentTarget })}><Ellipsis size={16} /></button>
            </span>
          </div>
        {/each}
        {#each shown as r (r.path)}
          <!-- svelte-ignore a11y_interactive_supports_focus (right-click is a shortcut; the ⋯ button is the keyboard way) -->
          <div class="tr" role="row" class:dragging={dragState.active && dragState.paths.includes(r.path)} use:thumbWhenVisible={r.path} oncontextmenu={(e) => contextMenu(e, r.path)} onpointerdown={(e) => press(e, r.path)}>
            <button class="name" role="cell" onclick={() => !wasDrag() && p.onOpen(r.path)}>
              <span class="mini">{#if thumbs[r.path]}<img src={thumbs[r.path]} alt="" />{:else}<FileText size={16} />{/if}</span>
              <span class="n">{nameOf(r.path)}</span>
              {#if query.trim() && folderName(r.path)}<span class="chip">{folderName(r.path)}</span>{/if}
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
        {#each folders as f (f.id)}
          <div class="card folder" role="presentation" data-drop={f.id} class:drop={dropping(f.id)} oncontextmenu={(e) => folderContext(e, f.id)}>
            <button class="open" onclick={() => (view = f.id)}>
              <div class="thumb fold"><Folder size={56} strokeWidth={1.25} /></div>
              <div class="info"><b>{f.name}</b><span>{f.count === 1 ? "1 project" : `${f.count} projects`}</span></div>
            </button>
            <button class="btn icon sm more" title="More actions" aria-label="Actions for folder {f.name}" aria-haspopup="menu" onclick={(e) => (folderMenu = { id: f.id, anchor: e.currentTarget })}><Ellipsis size={15} /></button>
          </div>
        {/each}
        {#each shown as r (r.path)}
          <div class="card" role="presentation" class:dragging={dragState.active && dragState.paths.includes(r.path)} use:thumbWhenVisible={r.path} oncontextmenu={(e) => contextMenu(e, r.path)} onpointerdown={(e) => press(e, r.path)}>
            <button class="open" onclick={() => !wasDrag() && p.onOpen(r.path)}>
              <div class="thumb">
                {#if thumbs[r.path]}<img src={thumbs[r.path]} alt="" />{:else}<div class="blank"><i></i><i></i><i></i><i></i><i></i></div>{/if}
              </div>
              <div class="info"><b>{nameOf(r.path)}</b><span>{query.trim() && folderName(r.path) ? `${folderName(r.path)} · ` : ""}{ago(r.lastOpened)}</span></div>
            </button>
            <button class="btn icon sm more" title="More actions" aria-label="Actions for {nameOf(r.path)}" aria-haspopup="menu" onclick={(e) => (projectMenu = { path: r.path, anchor: e.currentTarget })}><Ellipsis size={15} /></button>
          </div>
        {/each}
      </div>
    {/if}
  </main>
</div>

{#if editing}
  <Modal title={editing.id ? "Rename folder" : "New folder"} width={380} onClose={() => (editing = null)}>
    <form id="folder-name" class="fedit" onsubmit={finishEdit}>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="input" bind:value={editing.name} placeholder="Folder name" aria-label="Folder name" autofocus />
      {#if editing.error}<span class="ferr">{editing.error}</span>{/if}
    </form>
    {#snippet footer()}
      <button class="btn" onclick={() => (editing = null)}>Cancel</button>
      <button class="btn primary" type="submit" form="folder-name" disabled={!editing?.name.trim()}>{editing?.id ? "Rename" : "Create"}</button>
    {/snippet}
  </Modal>
{/if}

{#if projectMenu}
  {@const path = projectMenu.path}
  <DropMenu anchor={projectMenu.anchor} align="end" onClose={() => (projectMenu = null)}>
    <button role="menuitem" onclick={() => p.onOpen(path)}><ExternalLink size={15} /> Open</button>
    <button role="menuitem" onclick={() => revealItemInDir(path).catch(() => {})}><FolderSearch size={15} /> Show in file manager</button>
    <button role="menuitem" onclick={() => (templating = path)}><LayoutTemplate size={15} /> Save as template…</button>
    <button role="menuitem" onclick={() => (moving = path)}><Folder size={15} /> Move to folder…</button>
    <hr />
    <button role="menuitem" onclick={() => forget(path)}><X size={15} /> Remove from list <span class="hint">keeps files</span></button>
    <button role="menuitem" class="danger" onclick={() => (deleting = { path, busy: false, error: null })}><Trash2 size={15} /> Delete project…</button>
  </DropMenu>
{/if}

{#if folderMenu}
  {@const id = folderMenu.id}
  <DropMenu anchor={folderMenu.anchor} align="end" onClose={() => (folderMenu = null)}>
    <button role="menuitem" onclick={() => (view = id)}><FolderOpen size={15} /> Open</button>
    <button role="menuitem" onclick={() => startEdit(id)}><SquarePen size={15} /> Rename…</button>
    <button role="menuitem" class="danger" onclick={() => removeGroup(id)}><Trash2 size={15} /> Delete folder <span class="hint">keeps projects</span></button>
  </DropMenu>
{/if}

{#if moving}
  {@const path = moving}
  <Modal title="Move {nameOf(path)}" width={380} onClose={() => (moving = null)}>
    <div class="moves" role="radiogroup" aria-label="Folder">
      {#each [{ id: null, name: "No folder" }, ...[...groups.groups].sort((a, b) => collator.compare(a.name, b.name))] as g (g.id ?? "")}
        {@const here = (groups.members[path] ?? null) === g.id}
        <button role="radio" aria-checked={here} class:on={here} onclick={() => (commit(moveProject(groups, path, g.id)), (moving = null))}>
          {#if g.id}<Folder size={15} />{:else}<FileText size={15} />{/if} {g.name}
        </button>
      {/each}
      <button onclick={() => (startEdit(null, path), (moving = null))}><FolderPlus size={15} /> New folder…</button>
    </div>
  </Modal>
{/if}

{#if dragState.active}
  <div class="ghost" style:left="{dragState.x + 12}px" style:top="{dragState.y + 8}px">{dragState.name}</div>
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

{#if templating}
  <SaveTemplateDialog path={templating} projectName={nameOf(templating)} onSaved={templateSaved} onClose={() => (templating = null)} />
{/if}

{#if dialog === "new"}
  <NewProjectDialog {defaultParent} initialTemplate={linkedTemplate} onCreated={(path) => ((dialog = null), filed([path]), p.onOpen(path))} onClose={() => ((dialog = null), (linkedTemplate = ""))} />
{:else if dialog === "clone"}
  <CloneDialog {defaultParent} initialUrl={linkedUrl} onCloned={(path) => ((dialog = null), filed([path]), p.onOpen(path))} onClose={() => ((dialog = null), (linkedUrl = ""))} />
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

  .undo { margin: 8px 6px 0; font-size: 12.5px; color: var(--chrome-muted); overflow-wrap: anywhere; }
  .undo button { border: none; background: none; padding: 0; font: inherit; color: var(--chrome-ink); text-decoration: underline; cursor: pointer; }
  .tools select.sort { width: auto; }
  .fedit { display: flex; flex-direction: column; gap: 4px; }
  .ferr { color: var(--error); font-size: 11.5px; padding-left: 4px; }
  .chip { margin-left: 8px; padding: 0 7px; border-radius: 9px; background: var(--surface-3); color: var(--muted); font-size: 11px; line-height: 18px; flex: none; }
  :global(.menu) .danger, :global(.menu) .danger :global(svg) { color: var(--error); }
  .dmsg { margin: 0 0 10px; color: var(--muted); }
  .dpath { display: block; padding: 8px 10px; background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--r); font: 12px var(--f-mono); overflow-wrap: anywhere; }
  .derr { color: var(--error); margin: 10px 0 0; }
  .danger-btn { background: var(--error) !important; border-color: var(--error) !important; }

  .crumbs { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .crumbs :global(svg) { color: var(--faint); flex: none; }
  .crumb { border: none; background: none; padding: 2px 6px; margin-left: -6px; border-radius: var(--r); font: inherit; color: var(--muted); cursor: pointer; }
  .crumb:hover { color: var(--accent); }
  .here { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* Projects drag (see press()); the text under the pointer shouldn't get selected. */
  .tr, .card { user-select: none; }
  .dragging { opacity: 0.5; }
  .drop, .crumb.drop { outline: 2px solid var(--accent); outline-offset: -2px; background: var(--accent-soft) !important; }
  .card.drop .open { border-color: var(--accent); }
  .mini.fold { background: none; border: none; color: var(--accent); }
  .thumb.fold { align-items: center; padding: 0; color: var(--accent); background: var(--surface-2); }
  .loc.count { font-family: var(--f-body); }
  .moves { display: flex; flex-direction: column; gap: 2px; }
  .moves button { display: flex; align-items: center; gap: 10px; height: 34px; padding: 0 10px; border: none; border-radius: var(--r); background: none; color: var(--ink); text-align: left; font: 13px var(--f-body); }
  .moves button:hover { background: var(--surface-2); }
  .moves button.on { background: var(--accent-soft); }
  .moves :global(svg) { color: var(--muted); }
  .ghost { position: fixed; z-index: 60; pointer-events: none; padding: 3px 9px; border-radius: var(--r-sm); background: var(--surface); color: var(--ink); border: 1px solid var(--border-strong); font: 12.5px var(--f-body); box-shadow: var(--shadow); }
</style>
