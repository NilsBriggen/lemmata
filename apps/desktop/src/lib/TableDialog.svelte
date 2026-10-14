<script lang="ts">
  // Insert table: pick the size on a grid, choose the rule style.
  import Modal from "./ui/Modal.svelte";
  import { tableBlock } from "./writing/table";

  interface Props {
    /** The table as a snippet: Tab steps through the placeholder cells. */
    onInsert: (template: string, needs?: string) => void;
    onClose: () => void;
  }
  let { onInsert, onClose }: Props = $props();

  const MAX = 8;
  let rows = $state(3);
  let cols = $state(3);
  let hoverRows = $state(0);
  let hoverCols = $state(0);
  let header = $state(true);
  let booktabs = $state(true);
  let caption = $state("");
  let label = $state("");

  const shownRows = $derived(hoverRows || rows);
  const shownCols = $derived(hoverCols || cols);

  function submit(e: Event) {
    e.preventDefault();
    onInsert(tableBlock({ rows, cols, header, booktabs, caption, label }, true), booktabs ? "booktabs" : undefined);
    onClose();
  }
</script>

<Modal title="Insert table" width={460} {onClose}>
  <form id="table-form" onsubmit={submit}>
    <div class="field">
      <span class="label">Size: {shownCols} × {shownRows}</span>
      <div class="grid" role="group" aria-label="Table size" onmouseleave={() => ((hoverRows = 0), (hoverCols = 0))}>
        {#each Array.from({ length: MAX }, (_, r) => r + 1) as r (r)}
          {#each Array.from({ length: MAX }, (_, c) => c + 1) as c (c)}
            <button
              type="button"
              class="cell"
              class:on={r <= shownRows && c <= shownCols}
              aria-label="{c} columns by {r} rows"
              onmouseenter={() => ((hoverRows = r), (hoverCols = c))}
              onfocus={() => ((hoverRows = r), (hoverCols = c))}
              onclick={() => ((rows = r), (cols = c))}
            ></button>
          {/each}
        {/each}
      </div>
    </div>
    <div class="size">
      <div class="field"><label for="tb-cols">Columns</label><input id="tb-cols" class="input" type="number" min="1" max="20" bind:value={cols} /></div>
      <div class="field"><label for="tb-rows">Rows</label><input id="tb-rows" class="input" type="number" min="1" max="100" bind:value={rows} /></div>
    </div>
    <label class="check" for="tb-header"><input id="tb-header" type="checkbox" bind:checked={header} /> First row is a header</label>
    <label class="check" for="tb-book"><input id="tb-book" type="checkbox" bind:checked={booktabs} /> Clean rules (booktabs)</label>
    <div class="field"><label for="tb-caption">Caption</label><input id="tb-caption" class="input" bind:value={caption} placeholder="Leave empty to type it in the document" /></div>
    <div class="field"><label for="tb-label">Label</label><input id="tb-label" class="input mono" bind:value={label} placeholder="made from the caption" /></div>
  </form>
  {#snippet footer()}
    <button class="btn" onclick={onClose}>Cancel</button>
    <button class="btn primary" type="submit" form="table-form">Insert</button>
  {/snippet}
</Modal>

<style>
  form { display: flex; flex-direction: column; gap: 12px; }
  .grid { display: grid; grid-template-columns: repeat(8, 22px); gap: 3px; width: max-content; }
  .cell { width: 22px; height: 22px; padding: 0; border: 1px solid var(--border-strong); border-radius: 4px; background: var(--surface); }
  .cell.on { background: var(--accent-soft); border-color: var(--accent); }
  .size { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
</style>
