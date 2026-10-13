<script lang="ts">
  // A few lines of source around a problem, so it can be read without
  // switching to the editor.
  interface Props {
    file: string;
    line: number;
    getSource: (file: string) => Promise<string | null>;
  }
  let { file, line, getSource }: Props = $props();

  let rows = $state<{ n: number; text: string }[]>([]);

  $effect(() => {
    let live = true;
    getSource(file).then((src) => {
      if (!live || src === null) return;
      const lines = src.split("\n");
      const from = Math.max(1, line - 2);
      const to = Math.min(lines.length, line + 2);
      rows = Array.from({ length: to - from + 1 }, (_, i) => ({ n: from + i, text: lines[from + i - 1] }));
    });
    return () => (live = false);
  });
</script>

{#if rows.length}
  <pre class="excerpt" aria-label="Source around line {line}">{#each rows as r (r.n)}<span class="row" class:hit={r.n === line}><i>{r.n}</i>{r.text || " "}
</span>{/each}</pre>
{/if}

<style>
  .excerpt { margin: 0 0 0 24px; padding: 6px 0; border: 1px solid var(--border); border-radius: var(--r-sm); background: var(--code-bg); font: 12px/1.55 var(--f-mono); color: var(--ink-2); overflow: auto; }
  .row { display: block; padding: 0 10px 0 0; white-space: pre; }
  .row i { display: inline-block; width: 38px; padding-right: 10px; text-align: right; font-style: normal; color: var(--faint); user-select: none; }
  .row.hit { background: var(--error-soft); color: var(--ink); }
</style>
