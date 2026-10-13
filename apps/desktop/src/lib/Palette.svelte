<script lang="ts" module>
  export interface PaletteItem {
    id: string;
    label: string;
    /** Shown on the right: a shortcut or a folder. */
    detail?: string;
    run: () => void;
  }
</script>

<script lang="ts">
  // Command palette (Ctrl+Shift+P) and quick open (Ctrl+P) share this list.
  import { onMount, tick } from "svelte";
  import { fuzzyFilter } from "./fuzzy";

  interface Props {
    items: PaletteItem[];
    placeholder: string;
    onClose: () => void;
  }
  let { items, placeholder, onClose }: Props = $props();

  let query = $state("");
  let selected = $state(0);
  let input = $state<HTMLInputElement>();
  let list = $state<HTMLUListElement>();

  const results = $derived(fuzzyFilter(items, query, (i) => `${i.label} ${i.detail ?? ""}`).slice(0, 100));

  onMount(() => input?.focus());

  function run(item: PaletteItem | undefined) {
    if (!item) return;
    onClose();
    item.run();
  }

  async function move(delta: number) {
    selected = Math.max(0, Math.min(results.length - 1, selected + delta));
    await tick();
    list?.querySelector(".on")?.scrollIntoView({ block: "nearest" });
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "ArrowDown") (e.preventDefault(), move(1));
    else if (e.key === "ArrowUp") (e.preventDefault(), move(-1));
    else if (e.key === "Enter") (e.preventDefault(), run(results[selected]));
    else if (e.key === "Escape") onClose();
  }
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onClose()}>
  <div class="palette" role="dialog" aria-label={placeholder}>
    <input
      id="palette-input"
      bind:this={input}
      bind:value={query}
      oninput={() => (selected = 0)}
      onkeydown={onKey}
      {placeholder}
      autocomplete="off"
      spellcheck="false"
    />
    <ul bind:this={list} role="listbox">
      {#each results as item, i (item.id)}
        <li role="option" aria-selected={i === selected}>
          <button class:on={i === selected} onclick={() => run(item)} onmouseenter={() => (selected = i)}>
            <span>{item.label}</span>
            {#if item.detail}<kbd>{item.detail}</kbd>{/if}
          </button>
        </li>
      {:else}
        <li class="none">Nothing matches.</li>
      {/each}
    </ul>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(10, 15, 20, 0.3); display: grid; place-items: start center; padding: 12vh 24px 24px; z-index: 30; }
  .palette { width: min(600px, 100%); background: var(--sheet); border: 1px solid var(--rule); border-radius: 8px; overflow: hidden; box-shadow: 0 20px 50px -20px rgba(0, 0, 0, 0.5); }
  input { width: 100%; border: none; border-bottom: 1px solid var(--rule); padding: 12px 14px; font: 14px var(--f-body); background: var(--sheet); color: var(--ink); outline: none; }
  ul { list-style: none; margin: 0; padding: 4px; max-height: 50vh; overflow: auto; }
  button { width: 100%; display: flex; justify-content: space-between; gap: 12px; border: none; background: none; text-align: left; padding: 7px 10px; font-weight: 400; border-radius: 5px; }
  button.on { background: var(--blue-soft); }
  kbd { font: 11px var(--f-mono); color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 50%; }
  .none { padding: 10px; color: var(--muted); }
</style>
