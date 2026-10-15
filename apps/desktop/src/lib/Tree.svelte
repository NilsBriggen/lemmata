<script lang="ts">
  import Tree from "./Tree.svelte";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import Folder from "@lucide/svelte/icons/folder";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import FileText from "@lucide/svelte/icons/file-text";
  import FileImage from "@lucide/svelte/icons/file-image";
  import BookOpen from "@lucide/svelte/icons/book-open";
  import Braces from "@lucide/svelte/icons/braces";
  import Settings from "@lucide/svelte/icons/settings";
  import File from "@lucide/svelte/icons/file";
  import type { Entry } from "./api";
  import { dragState, parentOf, startDrag, wasDrag } from "./treeDrag.svelte";
  import { isSelected, targets, tree } from "./treeSelection.svelte";
  import { countFiles, formatSize, isTextFile, isViewerFile } from "./treeModel";

  interface Props {
    entries: Entry[];
    active: string | null;
    main: string;
    depth?: number;
    onOpen: (path: string) => void;
    /** Right-click on an entry. */
    onContext?: (entry: Entry, x: number, y: number) => void;
    /** Entries were dragged onto a folder ("" for the project root). */
    onMove?: (paths: string[], folder: string) => void;
    /** A row was clicked; Ctrl/Shift extend the selection instead of opening. */
    onSelect?: (path: string, mods: { ctrl: boolean; shift: boolean }) => void;
  }
  let { entries, active, main, depth = 0, onOpen, onContext, onMove, onSelect }: Props = $props();

  /** Text files open in the editor. Anything not listed here is binary and stays unopened. */
  const editable = isTextFile;
  const viewable = isViewerFile;
  const press = (ev: PointerEvent, e: Entry) => onMove && startDrag(ev, e, () => targets(e.path), onMove);
  const mods = (ev: MouseEvent) => ({ ctrl: ev.ctrlKey || ev.metaKey, shift: ev.shiftKey });
  /** Returns true when the click only changed the selection. */
  function selectOnly(ev: MouseEvent, path: string) {
    if (wasDrag()) return true;
    const m = mods(ev);
    onSelect?.(path, m);
    return m.ctrl || m.shift;
  }
  const GIT_TITLES: Record<string, string> = {
    M: "Changed since the last git commit",
    A: "Added to git, not committed yet",
    D: "Deleted, not committed yet",
    R: "Renamed, not committed yet",
    "?": "Not tracked by git",
    U: "Merge conflict",
  };
  const modified = (secs: number) => (secs ? new Date(secs * 1000).toLocaleString() : "");
  const kind = (name: string) =>
    /\.tex$/i.test(name) ? "tex"
    : /\.bib$/i.test(name) ? "bib"
    : /\.(png|jpe?g|pdf|eps|svg|gif)$/i.test(name) ? "image"
    : /\.(sty|cls|bst|bbx|cbx|tikz)$/i.test(name) ? "code"
    : /\.toml$/i.test(name) ? "config"
    : "file";

  function context(e: MouseEvent, entry: Entry) {
    if (!onContext) return;
    e.preventDefault();
    e.stopPropagation();
    // Right-clicking outside the selection selects that row alone.
    if (!isSelected(entry.path)) onSelect?.(entry.path, { ctrl: false, shift: false });
    onContext(entry, e.clientX, e.clientY);
  }
</script>

<ul role={depth === 0 ? "tree" : "group"} aria-multiselectable={depth === 0 ? true : undefined}>
  {#each entries as e (e.path)}
    <li role="treeitem" aria-selected={isSelected(e.path) || e.path === active} aria-expanded={e.is_dir ? !tree.collapsed[e.path] : undefined}>
      {#if e.is_dir}
        <button
          class="row"
          class:drop={(dragState.active && dragState.target === e.path) || dragState.external === e.path}
          class:sel={isSelected(e.path)}
          class:cursor={tree.cursor === e.path}
          class:dragging={dragState.active && dragState.paths.includes(e.path)}
          data-drop={e.path}
          data-path={e.path}
          style:padding-left="{8 + depth * 14}px"
          onclick={(ev) => !selectOnly(ev, e.path) && (tree.collapsed[e.path] = !tree.collapsed[e.path])}
          onpointerdown={(ev) => press(ev, e)}
          oncontextmenu={(ev) => context(ev, e)}
        >
          <span class="caret" class:open={!tree.collapsed[e.path]}><ChevronRight size={13} /></span>
          <span class="ico folder">{#if tree.collapsed[e.path]}<Folder size={15} />{:else}<FolderOpen size={15} />{/if}</span>
          <span class="name">{e.name}</span>
          {#if tree.collapsed[e.path]}<span class="count" title="Files inside">{countFiles(e)}</span>{/if}
          {#if tree.gitFolders[e.path]}<span class="gitdot" title="Contains changes not yet committed to git"></span>{/if}
        </button>
        {#if !tree.collapsed[e.path]}
          <Tree entries={e.children} {active} {main} depth={depth + 1} {onOpen} {onContext} {onMove} {onSelect} />
        {/if}
      {:else}
        {@const k = kind(e.name)}
        <button
          class="row"
          class:on={e.path === active}
          class:sel={isSelected(e.path)}
          class:cursor={tree.cursor === e.path}
          class:dim={!editable(e.name) && !viewable(e.name)}
          class:dragging={dragState.active && dragState.paths.includes(e.path)}
          data-drop={parentOf(e.path)}
          data-path={e.path}
          style:padding-left="{8 + depth * 14 + 15}px"
          title="{e.path}{editable(e.name) || viewable(e.name) ? '' : ' (not a text file)'}{e.size ? `\n${formatSize(e.size)}` : ''}{e.modified ? `\n${modified(e.modified)}` : ''}"
          onclick={(ev) => !selectOnly(ev, e.path) && (editable(e.name) || viewable(e.name)) && onOpen(e.path)}
          onpointerdown={(ev) => press(ev, e)}
          oncontextmenu={(ev) => context(ev, e)}
        >
          <span class="ico {k}">
            {#if k === "tex"}<FileText size={15} />{:else if k === "bib"}<BookOpen size={15} />{:else if k === "image"}<FileImage size={15} />{:else if k === "code"}<Braces size={15} />{:else if k === "config"}<Settings size={15} />{:else}<File size={15} />{/if}
          </span>
          <span class="name">{e.name}</span>
          {#if e.path === main}<span class="main" title="Main document">main</span>{/if}
          {#if tree.git[e.path]}<span class="git s{tree.git[e.path] === "?" ? "u" : tree.git[e.path]}" title={GIT_TITLES[tree.git[e.path]] ?? "Changed"}>{tree.git[e.path]}</span>{/if}
        </button>
      {/if}
    </li>
  {/each}
</ul>

{#if depth === 0 && dragState.active}
  <div class="ghost" style:left="{dragState.x + 12}px" style:top="{dragState.y + 8}px">{dragState.name}</div>
{/if}

<style>
  ul { list-style: none; margin: 0; padding: 0; }
  .row {
    display: flex; align-items: center; gap: 6px; width: 100%; height: 28px;
    border: none; border-radius: 0; background: none; text-align: left;
    font: 13px var(--f-body); color: var(--chrome-ink); padding-right: 10px;
  }
  .row:hover { background: var(--chrome-hover); }
  .row.on { background: var(--chrome-active); color: #fff; box-shadow: inset 3px 0 var(--accent-on-dark); }
  .row.drop { background: color-mix(in srgb, var(--accent-on-dark) 28%, transparent); box-shadow: inset 0 0 0 1px var(--accent-on-dark); }
  .row.sel { background: color-mix(in srgb, var(--accent-on-dark) 20%, transparent); }
  .row.sel.on { background: var(--chrome-active); }
  :global(.tree-scroll:focus-within) .row.cursor { box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent-on-dark) 55%, transparent); }
  .row.dragging { opacity: 0.5; }
  .git { font: 600 10.5px var(--f-mono); width: 14px; text-align: center; flex: none; }
  .git.sM, .git.sR { color: #e5b567; }
  .git.sA, .git.su { color: #8fcf9f; }
  .git.sD, .git.sU { color: #e58c8c; }
  .gitdot { width: 6px; height: 6px; border-radius: 50%; background: #e5b567; flex: none; }
  .count { font: 11px var(--f-body); color: var(--chrome-muted); background: var(--chrome-3); border-radius: 8px; padding: 0 6px; line-height: 16px; }
  .ghost { position: fixed; z-index: 60; pointer-events: none; padding: 3px 9px; border-radius: var(--r-sm); background: var(--chrome-3); color: var(--chrome-ink); border: 1px solid var(--chrome-border); font: 12.5px var(--f-body); box-shadow: var(--shadow); }
  .row.dim { color: var(--chrome-muted); cursor: default; }
  .row:focus-visible { box-shadow: inset 0 0 0 2px var(--accent-on-dark); }
  .caret { display: inline-flex; color: var(--chrome-muted); transition: transform 0.12s var(--ease); }
  .caret.open { transform: rotate(90deg); }
  .ico { display: inline-flex; flex: none; color: var(--chrome-muted); }
  .ico.tex { color: var(--accent-on-dark); }
  .ico.bib { color: #c7a2e0; }
  .ico.image { color: #8fcf9f; }
  .ico.folder { color: #d9b56a; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .main {
    font: 600 9.5px var(--f-body); letter-spacing: 0.08em; text-transform: uppercase;
    color: var(--accent-on-dark); border: 1px solid color-mix(in srgb, var(--accent-on-dark) 45%, transparent);
    border-radius: 3px; padding: 0 4px; line-height: 14px;
  }
</style>
