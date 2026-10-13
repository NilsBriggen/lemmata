<script lang="ts" module>
  import type { OutlineNode } from "./PdfView.svelte";
  export type { OutlineNode };
</script>

<script lang="ts">
  // The PDF's own bookmarks (from hyperref), as a tree.
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import PdfOutline from "./PdfOutline.svelte";

  interface Props {
    nodes: OutlineNode[];
    onGo: (node: OutlineNode) => void;
    depth?: number;
  }
  let { nodes, onGo, depth = 0 }: Props = $props();
  let closed = $state<Record<number, boolean>>({});
</script>

<ul style:--depth={depth}>
  {#each nodes as n, i (i)}
    <li>
      <div class="row">
        {#if n.items.length}
          <button class="twist" class:open={!closed[i]} aria-label={closed[i] ? "Expand" : "Collapse"} onclick={() => (closed[i] = !closed[i])}><ChevronRight size={12} /></button>
        {:else}
          <span class="twist"></span>
        {/if}
        <button class="title" onclick={() => onGo(n)} title={n.title}>{n.title}</button>
      </div>
      {#if n.items.length && !closed[i]}
        <PdfOutline nodes={n.items} {onGo} depth={depth + 1} />
      {/if}
    </li>
  {/each}
</ul>

<style>
  ul { list-style: none; margin: 0; padding: 0 0 0 calc(var(--depth) * 4px); }
  .row { display: flex; align-items: center; gap: 2px; }
  .twist { width: 18px; height: 22px; flex: none; display: grid; place-items: center; border: none; background: none; color: var(--muted); padding: 0; }
  .twist :global(svg) { transition: transform 0.12s; }
  .twist.open :global(svg) { transform: rotate(90deg); }
  .title { flex: 1; min-width: 0; text-align: left; border: none; background: none; color: var(--ink); padding: 3px 6px; border-radius: var(--r-sm); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; font-size: 12.5px; }
  .title:hover { background: var(--accent-soft); }
</style>
