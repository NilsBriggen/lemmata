<script lang="ts">
  // Left panel: the project's files with quick actions, and the outline.
  import FilePlus from "@lucide/svelte/icons/file-plus";
  import FolderPlus from "@lucide/svelte/icons/folder-plus";
  import Upload from "@lucide/svelte/icons/upload";
  import Search from "@lucide/svelte/icons/search";
  import ArrowUpDown from "@lucide/svelte/icons/arrow-up-down";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Check from "@lucide/svelte/icons/check";
  import Tree from "./Tree.svelte";
  import Outline from "./Outline.svelte";
  import DropMenu from "./ui/DropMenu.svelte";
  import type { Entry, Heading } from "./api";
  import type FileActions from "./FileActions.svelte";
  import { matches } from "./shortcuts";
  import { settings, updateSettings } from "./settings.svelte";
  import { cursorTo, click, moveCursor, selectAll, select, targets, tree as view, collapseAll, expandAll } from "./treeSelection.svelte";
  import { findEntry, flatten, parentOf, sortEntries, type SortMode } from "./treeModel";
  import { dragState } from "./treeDrag.svelte";

  interface Props {
    tree: Entry[];
    active: string | null;
    main: string;
    headings: Heading[];
    currentHeading: number;
    actions: FileActions | undefined;
    onOpen: (path: string) => void;
    onJump: (file: string, line: number) => void;
    onContext: (entry: Entry | null, x: number, y: number) => void;
    onNewFile: () => void;
    onNewFolder: () => void;
    onUpload: () => void;
    onSearch: () => void;
  }
  let p: Props = $props();
  let outlineOpen = $state(true);
  let sortMenu = $state<HTMLElement | null>(null);
  let sortBtn = $state<HTMLButtonElement>();

  const sorted = $derived(sortEntries(p.tree, settings.treeSort, settings.treeFoldersFirst));
  const rows = $derived(flatten(sorted, view.collapsed));
  const SORTS: [SortMode, string][] = [
    ["name", "Name"],
    ["type", "Type"],
    ["modified", "Last modified"],
    ["size", "Size"],
  ];

  const folderOfCursor = () => {
    const e = view.cursor ? findEntry(p.tree, view.cursor) : undefined;
    return !e ? "" : e.is_dir ? e.path : parentOf(e.path);
  };

  let typed = "";
  let typedTimer: ReturnType<typeof setTimeout>;

  function onKey(e: KeyboardEvent) {
    if ((e.target as HTMLElement).closest("input, textarea")) return;
    const a = p.actions;
    const act = (id: string, run: () => void) => {
      if (!matches(e, id)) return false;
      e.preventDefault();
      run();
      return true;
    };
    const extend = e.shiftKey;
    const cur = view.cursor ? findEntry(p.tree, view.cursor) : undefined;
    if (!e.ctrlKey && !e.metaKey && !e.altKey) {
      switch (e.key) {
        case "ArrowDown": e.preventDefault(); return moveCursor(rows, 1, extend);
        case "ArrowUp": e.preventDefault(); return moveCursor(rows, -1, extend);
        case "Home": e.preventDefault(); return cursorTo(rows, 0, extend);
        case "End": e.preventDefault(); return cursorTo(rows, -1, extend);
        case "ArrowRight":
          e.preventDefault();
          if (cur?.is_dir) {
            if (view.collapsed[cur.path]) delete view.collapsed[cur.path];
            else moveCursor(rows, 1, false);
          }
          return;
        case "ArrowLeft":
          e.preventDefault();
          if (cur?.is_dir && !view.collapsed[cur.path]) view.collapsed[cur.path] = true;
          else if (cur && parentOf(cur.path)) select(parentOf(cur.path));
          return;
        case "Enter":
        case " ":
          e.preventDefault();
          if (!cur) return;
          if (cur.is_dir) view.collapsed[cur.path] = !view.collapsed[cur.path];
          else p.onOpen(cur.path);
          return;
        case "Escape":
          view.selected = view.cursor ? [view.cursor] : [];
          return;
      }
    }
    if (!a) return;
    if (
      act("tree-rename", () => cur && a.rename(cur)) ||
      act("tree-delete", () => a.trash(targets())) ||
      act("tree-cut", () => a.cut(targets())) ||
      act("tree-copy", () => a.copy(targets())) ||
      act("tree-paste", () => a.paste(folderOfCursor())) ||
      act("tree-duplicate", () => a.duplicate(targets())) ||
      act("tree-undo", () => void a.undo()) ||
      act("tree-select-all", () => selectAll(sorted)) ||
      act("tree-new-file", () => a.newItem(false, folderOfCursor())) ||
      act("tree-new-folder", () => a.newItem(true, folderOfCursor()))
    )
      return;
    // Type a few letters to jump to the next row starting with them.
    if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
      clearTimeout(typedTimer);
      typed += e.key.toLowerCase();
      typedTimer = setTimeout(() => (typed = ""), 700);
      const start = Math.max(0, rows.findIndex((r) => r.path === view.cursor) + (typed.length === 1 ? 1 : 0));
      const hit = [...rows.slice(start), ...rows.slice(0, start)].find((r) => r.name.toLowerCase().startsWith(typed));
      if (hit) select(hit.path);
    }
  }

  // Keep the cursor row in view.
  $effect(() => {
    const path = view.cursor;
    if (!path) return;
    document.querySelector<HTMLElement>(`.tree-scroll [data-path="${CSS.escape(path)}"]`)?.scrollIntoView({ block: "nearest" });
  });

  async function setSort(mode: SortMode) {
    await updateSettings({ treeSort: mode });
  }
</script>

<aside class="sidebar chrome">
  <section class="files">
    <div class="head">
      <span class="title">Files</span>
      <div class="actions">
        <button class="btn icon sm" title="New file" aria-label="New file" onclick={p.onNewFile}><FilePlus size={15} /></button>
        <button class="btn icon sm" title="New folder" aria-label="New folder" onclick={p.onNewFolder}><FolderPlus size={15} /></button>
        <button class="btn icon sm" title="Add files to the project" aria-label="Add files" onclick={p.onUpload}><Upload size={15} /></button>
        <button class="btn icon sm" bind:this={sortBtn} title="Sort and view" aria-label="Sort and view" aria-haspopup="menu" onclick={() => (sortMenu = sortMenu ? null : sortBtn!)}><ArrowUpDown size={15} /></button>
        <button class="btn icon sm" title="Search in project (Ctrl+Shift+F)" aria-label="Search in project" onclick={p.onSearch}><Search size={15} /></button>
      </div>
    </div>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="scroll tree-scroll"
      data-drop=""
      class:drop={(dragState.active && dragState.target === "") || dragState.external === ""}
      tabindex="0"
      role="presentation"
      onkeydown={onKey}
      oncontextmenu={(e) => { e.preventDefault(); p.onContext(null, e.clientX, e.clientY); }}
      onclick={(e) => { if (e.target === e.currentTarget) view.selected = []; }}
    >
      <Tree
        entries={sorted}
        active={p.active}
        main={p.main}
        onOpen={p.onOpen}
        onContext={(entry, x, y) => p.onContext(entry, x, y)}
        onMove={(paths, folder) => p.actions?.move(paths, folder)}
        onSelect={(path, mods) => click(path, rows, mods)}
      />
    </div>
  </section>
  <section class="outline" class:closed={!outlineOpen}>
    <button class="head toggle" aria-expanded={outlineOpen} onclick={() => (outlineOpen = !outlineOpen)}>
      <span class="title">Outline</span>
      <span class="chev" class:closed={!outlineOpen}><ChevronDown size={14} /></span>
    </button>
    {#if outlineOpen}
      <div class="scroll"><Outline headings={p.headings} current={p.currentHeading} onJump={p.onJump} /></div>
    {/if}
  </section>
</aside>

{#if sortMenu}
  <DropMenu anchor={sortMenu} align="end" onClose={() => (sortMenu = null)}>
    <div class="menu" role="menu">
      <div class="heading">Sort by</div>
      {#each SORTS as [mode, label] (mode)}
        <button role="menuitemradio" aria-checked={settings.treeSort === mode} onclick={() => setSort(mode)}>
          <span class="tick">{#if settings.treeSort === mode}<Check size={14} />{/if}</span> {label}
        </button>
      {/each}
      <hr />
      <button role="menuitemcheckbox" aria-checked={settings.treeFoldersFirst} onclick={() => updateSettings({ treeFoldersFirst: !settings.treeFoldersFirst })}>
        <span class="tick">{#if settings.treeFoldersFirst}<Check size={14} />{/if}</span> Folders first
      </button>
      <hr />
      <button role="menuitem" onclick={() => { collapseAll(sorted); sortMenu = null; }}><span class="tick"></span> Collapse all folders</button>
      <button role="menuitem" onclick={() => { expandAll(); sortMenu = null; }}><span class="tick"></span> Expand all folders</button>
      <button role="menuitem" onclick={() => { sortMenu = null; p.actions?.showUnused(); }}><span class="tick"></span> Find unused files…</button>
    </div>
  </DropMenu>
{/if}

<style>
  .sidebar { display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--chrome-2); color: var(--chrome-ink); border-right: 1px solid var(--chrome-border); }
  section { display: flex; flex-direction: column; min-height: 0; }
  .files { flex: 1 1 60%; }
  .outline { flex: 1 1 40%; border-top: 1px solid var(--chrome-border); }
  .outline.closed { flex: none; }
  .head { display: flex; align-items: center; height: 36px; padding: 0 6px 0 14px; flex: none; }
  .toggle { width: 100%; border: none; background: none; color: inherit; text-align: left; cursor: pointer; }
  .toggle:hover { background: var(--chrome-hover); }
  .title { font: 600 11px var(--f-body); letter-spacing: 0.08em; text-transform: uppercase; color: var(--chrome-muted); flex: 1; }
  .actions { display: flex; gap: 1px; }
  .actions .btn { color: var(--chrome-muted); }
  .actions .btn:hover { color: var(--chrome-ink); }
  .chev { display: inline-flex; color: var(--chrome-muted); padding-right: 8px; transition: transform 0.12s; }
  .chev.closed { transform: rotate(-90deg); }
  .scroll { overflow: auto; min-height: 0; flex: 1; }
  .tree-scroll { outline: none; }
  .tree-scroll.drop { box-shadow: inset 0 0 0 2px var(--accent-on-dark); }
  .tick { display: inline-flex; width: 16px; justify-content: center; }
</style>
