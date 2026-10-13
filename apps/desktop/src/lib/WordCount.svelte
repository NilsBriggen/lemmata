<script lang="ts">
  // Word count and document statistics.
  import Modal from "./ui/Modal.svelte";
  import { api, errorMessage, type WordCounts } from "./api";

  let { onClose }: { onClose: () => void } = $props();
  let counts = $state<WordCounts | null>(null);
  let error = $state<string | null>(null);
  const fmt = new Intl.NumberFormat();

  $effect(() => {
    api.wordCount().then((c) => (counts = c)).catch((e) => (error = errorMessage(e)));
  });

  const stats = $derived(
    counts
      ? [
          ["Headings", counts.headings],
          ["Inline maths", counts.inline_math],
          ["Display maths", counts.display_math],
          ["Figures and tables", counts.floats],
          ["Citations", counts.citations],
        ] as [string, number][]
      : [],
  );
</script>

<Modal title="Word count" subtitle="Text the reader sees: no commands, maths or comments." width={460} {onClose}>
  {#if error}
    <p class="err">{error}</p>
  {:else if !counts}
    <p class="muted">Counting…</p>
  {:else}
    <div class="total"><b>{fmt.format(counts.total_words)}</b> words</div>
    <dl>
      {#each stats as [name, n] (name)}<div><dt>{name}</dt><dd>{fmt.format(n)}</dd></div>{/each}
    </dl>
    {#if counts.files.length > 1}
      <h3>By file</h3>
      <ul>
        {#each counts.files as f (f.file)}<li><span class="mono">{f.file}</span><span>{fmt.format(f.words)}</span></li>{/each}
      </ul>
    {/if}
  {/if}
  {#snippet footer()}<button class="btn primary" onclick={onClose}>Done</button>{/snippet}
</Modal>

<style>
  .total { font: 400 15px var(--f-body); color: var(--muted); margin-bottom: 12px; }
  .total b { font: 400 40px var(--f-display); color: var(--ink); margin-right: 6px; letter-spacing: -0.01em; }
  dl { display: grid; grid-template-columns: repeat(2, 1fr); gap: 8px 24px; margin: 0 0 16px; }
  dl div { display: flex; justify-content: space-between; border-bottom: 1px solid var(--border); padding: 4px 0; }
  dt { color: var(--muted); }
  dd { margin: 0; font-variant-numeric: tabular-nums; }
  h3 { font: 600 11px var(--f-body); letter-spacing: 0.08em; text-transform: uppercase; color: var(--muted); margin: 0 0 6px; }
  ul { list-style: none; margin: 0; padding: 0; max-height: 200px; overflow: auto; }
  li { display: flex; justify-content: space-between; padding: 3px 0; font-variant-numeric: tabular-nums; }
  .mono { font-family: var(--f-mono); font-size: 12px; }
  .muted { color: var(--muted); }
  .err { color: var(--error); }
</style>
