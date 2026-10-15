<script lang="ts">
  import Modal from "./ui/Modal.svelte";
  import { SHORTCUTS, keyLabel, matchesQuery } from "./shortcuts";

  let { onClose }: { onClose: () => void } = $props();
  const groups = ["Project", "Navigation", "Files", "Writing", "Editing", "PDF", "Mouse"] as const;
  let query = $state("");
  // The search box is the first input in the modal, so Modal focuses it on open.
  const shown = $derived(SHORTCUTS.filter((s) => matchesQuery(s, query)));
</script>

<Modal title="Keyboard shortcuts" width={620} {onClose}>
  <input class="search" type="search" placeholder="Search shortcuts" aria-label="Search shortcuts" bind:value={query} />
  {#if shown.length === 0}
    <p class="empty">No shortcut matches</p>
  {:else}
    <div class="cols">
      {#each groups as g (g)}
        {@const items = shown.filter((s) => s.group === g)}
        {#if items.length}
          <section>
            <h3>{g}</h3>
            {#each items as s (s.id)}
              <div class="row"><span>{s.label}</span>{#if keyLabel(s)}<kbd>{keyLabel(s)}</kbd>{/if}</div>
            {/each}
          </section>
        {/if}
      {/each}
    </div>
  {/if}
  {#snippet footer()}<button class="btn primary" onclick={onClose}>Done</button>{/snippet}
</Modal>

<style>
  .search { width: 100%; box-sizing: border-box; margin-bottom: 14px; }
  .empty { color: var(--muted); font-size: 13px; margin: 8px 0 0; }
  .cols { columns: 2; column-gap: 28px; }
  section { break-inside: avoid; margin-bottom: 16px; }
  h3 { font: 600 11px var(--f-body); letter-spacing: 0.08em; text-transform: uppercase; color: var(--muted); margin: 0 0 6px; }
  .row { display: flex; justify-content: space-between; gap: 12px; padding: 4px 0; border-bottom: 1px solid var(--border); font-size: 13px; }
  kbd { white-space: nowrap; font-size: 11.5px; color: var(--ink-2); background: var(--surface-2); border: 1px solid var(--border); border-radius: 4px; padding: 1px 6px; }
  @media (max-width: 640px) { .cols { columns: 1; } }
</style>
