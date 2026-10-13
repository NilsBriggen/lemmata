<script lang="ts">
  // Left panel: the project's files with quick actions, and the outline.
  import FilePlus from "@lucide/svelte/icons/file-plus";
  import FolderPlus from "@lucide/svelte/icons/folder-plus";
  import Upload from "@lucide/svelte/icons/upload";
  import Search from "@lucide/svelte/icons/search";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Tree from "./Tree.svelte";
  import Outline from "./Outline.svelte";
  import type { Entry, Heading } from "./api";

  interface Props {
    tree: Entry[];
    active: string | null;
    main: string;
    headings: Heading[];
    currentHeading: number;
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
</script>

<aside class="sidebar chrome">
  <section class="files">
    <div class="head">
      <span class="title">Files</span>
      <div class="actions">
        <button class="btn icon sm" title="New file" aria-label="New file" onclick={p.onNewFile}><FilePlus size={15} /></button>
        <button class="btn icon sm" title="New folder" aria-label="New folder" onclick={p.onNewFolder}><FolderPlus size={15} /></button>
        <button class="btn icon sm" title="Add files to the project" aria-label="Add files" onclick={p.onUpload}><Upload size={15} /></button>
        <button class="btn icon sm" title="Search in project (Ctrl+Shift+F)" aria-label="Search in project" onclick={p.onSearch}><Search size={15} /></button>
      </div>
    </div>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="scroll" oncontextmenu={(e) => { e.preventDefault(); p.onContext(null, e.clientX, e.clientY); }}>
      <Tree entries={p.tree} active={p.active} main={p.main} onOpen={p.onOpen} onContext={(entry, x, y) => p.onContext(entry, x, y)} />
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
</style>
