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

  /** Folded headings, keyed by file and line so they stay folded while editing elsewhere. */
  let folded = $state<Record<string, boolean>>({});
  const key = (h: Heading) => `${h.file}:${h.line}`;
  const hasChildren = (i: number) => i + 1 < headings.length && headings[i + 1].level > headings[i].level;
  /** Headings hidden because an ancestor is folded. */
  const visible = $derived.by(() => {
    const out: boolean[] = [];
    let hideBelow: number | null = null;
    headings.forEach((h, i) => {
      if (hideBelow !== null && h.level > hideBelow) {
        out.push(false);
        return;
      }
      hideBelow = null;
      out.push(true);
      if (folded[key(h)] && hasChildren(i)) hideBelow = h.level;
    });
    return out;
  });
</script>

{#if headings.length === 0}
  <p class="empty">Add a \section to see the outline here.</p>
{:else}
  <ul>
    {#each headings as h, i (i)}
      {#if visible[i]}
      <li class="item" style:padding-left="{4 + (h.level - minLevel) * 13}px">
        {#if hasChildren(i)}
          <button class="twist" aria-label={folded[key(h)] ? "Expand" : "Collapse"} aria-expanded={!folded[key(h)]} onclick={() => (folded[key(h)] = !folded[key(h)])}>
            <span class:closed={folded[key(h)]}>▾</span>
          </button>
        {:else}
          <span class="twist"></span>
        {/if}
        <button
          class:on={i === current}
          class:top={h.level === minLevel}
          class="entry"
          title="{h.file}:{h.line}"
          onclick={() => onJump(h.file, h.line)}
        >
          <span class="t">{h.title || "(untitled)"}</span>
          <span class="n">{fmt.format(totals[i])}</span>
        </button>
      </li>
      {/if}
    {/each}
  </ul>
{/if}

<style>
  ul { list-style: none; margin: 0; padding: 0 0 10px; }
  button {
    display: flex; gap: 8px; align-items: center; width: 100%; height: 26px; border: none; border-radius: 0;
    background: none; text-align: left; font: 12.5px var(--f-body); color: var(--chrome-muted); padding-right: 12px;
  }
  .item { display: flex; align-items: center; }
  .twist { width: 14px; flex: none; padding: 0; justify-content: center; color: var(--chrome-muted); font-size: 11px; }
  .twist span { display: inline-block; transition: transform 0.12s; }
  .twist span.closed { transform: rotate(-90deg); }
  .entry { flex: 1; min-width: 0; padding-left: 4px; }
  button.top { color: var(--chrome-ink); }
  button:hover { background: var(--chrome-hover); color: var(--chrome-ink); }
  button.on { color: #fff; box-shadow: inset 3px 0 var(--accent-on-dark); background: var(--chrome-active); }
  .t { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .n { font: 10.5px var(--f-mono); color: var(--chrome-muted); font-variant-numeric: tabular-nums; opacity: 0.8; }
  .empty { color: var(--chrome-muted); margin: 0; padding: 2px 14px 12px; font-size: 12px; }
</style>
