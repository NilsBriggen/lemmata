<script lang="ts">
  import Modal from "./ui/Modal.svelte";
  import { SHORTCUTS, display } from "./shortcuts";

  let { onClose }: { onClose: () => void } = $props();
  const groups = ["Project", "Navigation", "Files", "Writing", "Editing", "PDF"] as const;
</script>

<Modal title="Keyboard shortcuts" width={620} {onClose}>
  <div class="cols">
    {#each groups as g (g)}
      <section>
        <h3>{g}</h3>
        {#each SHORTCUTS.filter((s) => s.group === g) as s (s.id)}
          <div class="row"><span>{s.label}</span><kbd>{display(s.keys)}</kbd></div>
        {/each}
      </section>
    {/each}
  </div>
  {#snippet footer()}<button class="btn primary" onclick={onClose}>Done</button>{/snippet}
</Modal>

<style>
  .cols { columns: 2; column-gap: 28px; }
  section { break-inside: avoid; margin-bottom: 16px; }
  h3 { font: 600 11px var(--f-body); letter-spacing: 0.08em; text-transform: uppercase; color: var(--muted); margin: 0 0 6px; }
  .row { display: flex; justify-content: space-between; gap: 12px; padding: 4px 0; border-bottom: 1px solid var(--border); font-size: 13px; }
  kbd { white-space: nowrap; font-size: 11.5px; color: var(--ink-2); background: var(--surface-2); border: 1px solid var(--border); border-radius: 4px; padding: 1px 6px; }
  @media (max-width: 640px) { .cols { columns: 1; } }
</style>
