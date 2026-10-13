<script lang="ts">
  import Tree from "./Tree.svelte";
  import type { Entry } from "./api";

  interface Props {
    entries: Entry[];
    active: string | null;
    main: string;
    depth?: number;
    onOpen: (path: string) => void;
    /** Right-click on an entry (or empty space when `entry` is null). */
    onContext?: (entry: Entry | null, x: number, y: number) => void;
  }
  let { entries, active, main, depth = 0, onOpen, onContext }: Props = $props();

  function context(e: MouseEvent, entry: Entry) {
    if (!onContext) return;
    e.preventDefault();
    e.stopPropagation();
    onContext(entry, e.clientX, e.clientY);
  }
  let collapsed = $state<Record<string, boolean>>({});

  const editable = (name: string) => /\.(tex|bib|sty|cls|txt|md|toml|bst|tikz)$/i.test(name);
</script>

<ul class:root={depth === 0}>
  {#each entries as e (e.path)}
    <li>
      {#if e.is_dir}
        <button class="row dir" style:padding-left="{12 + depth * 14}px" onclick={() => (collapsed[e.path] = !collapsed[e.path])} oncontextmenu={(ev) => context(ev, e)}>
          <span class="caret">{collapsed[e.path] ? "▸" : "▾"}</span>{e.name}
        </button>
        {#if !collapsed[e.path]}
          <Tree entries={e.children} {active} {main} depth={depth + 1} {onOpen} {onContext} />
        {/if}
      {:else}
        <button
          class="row"
          class:on={e.path === active}
          class:dim={!editable(e.name)}
          aria-disabled={!editable(e.name)}
          style:padding-left="{12 + depth * 14 + 12}px"
          onclick={() => editable(e.name) && onOpen(e.path)}
          oncontextmenu={(ev) => context(ev, e)}
          title={e.path === main ? "Main file" : e.path}
        >
          {e.name}{#if e.path === main}<span class="main">main</span>{/if}
        </button>
      {/if}
    </li>
  {/each}
</ul>

<style>
  ul { list-style: none; margin: 0; padding: 0; }
  .row {
    display: flex; align-items: center; gap: 4px; width: 100%;
    border: none; border-radius: 0; background: none; text-align: left;
    font: 12px var(--f-mono); color: var(--ink); padding-block: 3px; padding-right: 10px;
  }
  .row:hover { background: color-mix(in srgb, var(--blue) 8%, transparent); }
  .row.on { background: var(--blue-soft); color: var(--blue); box-shadow: inset 2px 0 var(--blue); }
  .row.dir { color: var(--muted); }
  .row.dim { color: var(--muted); opacity: 0.7; cursor: default; }
  .caret { width: 10px; display: inline-block; }
  .main { margin-left: auto; font-size: 10px; color: var(--blue); text-transform: uppercase; letter-spacing: 0.08em; }
</style>
