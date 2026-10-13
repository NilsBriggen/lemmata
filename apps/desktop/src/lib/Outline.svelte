<script lang="ts">
  import type { Heading } from "./api";

  interface Props {
    headings: Heading[];
    /** The heading the cursor is in, by index. */
    current: number;
    onJump: (file: string, line: number) => void;
  }
  let { headings, current, onJump }: Props = $props();
  const minLevel = $derived(Math.min(...headings.map((h) => h.level), 4));

  /** Words in each heading including its subsections. */
  const totals = $derived(
    headings.map((h, i) => {
      let sum = h.words;
      for (let j = i + 1; j < headings.length && headings[j].level > h.level; j++) sum += headings[j].words;
      return sum;
    }),
  );
  const fmt = new Intl.NumberFormat();
</script>

{#if headings.length === 0}
  <p class="empty">Add a \section to see the outline here.</p>
{:else}
  <ul>
    {#each headings as h, i (i)}
      <li>
        <button
          class:on={i === current}
          class:top={h.level === minLevel}
          style:padding-left="{14 + (h.level - minLevel) * 13}px"
          title="{h.file}:{h.line}"
          onclick={() => onJump(h.file, h.line)}
        >
          <span class="t">{h.title || "(untitled)"}</span>
          <span class="n">{fmt.format(totals[i])}</span>
        </button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  ul { list-style: none; margin: 0; padding: 0 0 10px; }
  button {
    display: flex; gap: 8px; align-items: center; width: 100%; height: 26px; border: none; border-radius: 0;
    background: none; text-align: left; font: 12.5px var(--f-body); color: var(--chrome-muted); padding-right: 12px;
  }
  button.top { color: var(--chrome-ink); }
  button:hover { background: var(--chrome-hover); color: var(--chrome-ink); }
  button.on { color: #fff; box-shadow: inset 3px 0 var(--accent-on-dark); background: var(--chrome-active); }
  .t { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .n { font: 10.5px var(--f-mono); color: var(--chrome-muted); font-variant-numeric: tabular-nums; opacity: 0.8; }
  .empty { color: var(--chrome-muted); margin: 0; padding: 2px 14px 12px; font-size: 12px; }
</style>
