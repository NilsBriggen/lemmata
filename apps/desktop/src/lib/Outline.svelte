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
</script>

{#if headings.length === 0}
  <p class="empty">Sections appear here as you add them.</p>
{:else}
  <ul>
    {#each headings as h, i (i)}
      <li>
        <button
          class:on={i === current}
          style:padding-left="{12 + (h.level - minLevel) * 12}px"
          title="{h.file}:{h.line}"
          onclick={() => onJump(h.file, h.line)}
        >
          {h.title || "(untitled)"}
        </button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  ul { list-style: none; margin: 0; padding: 0 0 8px; }
  button {
    display: block; width: 100%; border: none; border-radius: 0; background: none; text-align: left;
    font: 12px var(--f-body); color: var(--ink); padding-block: 3px; padding-right: 10px;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  button:hover { background: color-mix(in srgb, var(--blue) 8%, transparent); }
  button.on { color: var(--blue); box-shadow: inset 2px 0 var(--blue); background: var(--blue-soft); }
  .empty { color: var(--muted); margin: 0; padding: 4px 14px 8px; font-size: 12px; }
</style>
