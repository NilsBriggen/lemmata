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

  // Options are not tab stops (focus stays in the input, as a combobox does), so Tab must not leave the dialog.
  function onKey(e: KeyboardEvent) {
    if (e.key === "ArrowDown") (e.preventDefault(), move(1));
    else if (e.key === "ArrowUp") (e.preventDefault(), move(-1));
    else if (e.key === "Enter") (e.preventDefault(), run(results[selected]));
    else if (e.key === "Escape") onClose();
    else if (e.key === "Tab") e.preventDefault();
  }

  const optionId = (i: number) => `palette-option-${i}`;
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onClose()}>
  <div class="palette" role="dialog" aria-modal="true" aria-label={placeholder}>
    <input
      id="palette-input"
      role="combobox"
      aria-expanded="true"
      aria-controls="palette-list"
      aria-autocomplete="list"
      aria-activedescendant={results.length ? optionId(selected) : undefined}
      bind:this={input}
      bind:value={query}
      oninput={() => (selected = 0)}
      onkeydown={onKey}
      {placeholder}
      autocomplete="off"
      spellcheck="false"
    />
    <ul id="palette-list" bind:this={list} role="listbox">
      {#each results as item, i (item.id)}
        <li role="presentation">
          <button
            id={optionId(i)}
            role="option"
            tabindex="-1"
            aria-selected={i === selected}
            class:on={i === selected}
            onclick={() => run(item)}
            onmouseenter={() => (selected = i)}
          >
            <span>{item.label}</span>
            {#if item.detail}<kbd>{item.detail}</kbd>{/if}
          </button>
        </li>
      {:else}
        <li class="none" role="presentation">Nothing matches.</li>
      {/each}
    </ul>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(12, 17, 22, 0.32); display: grid; place-items: start center; padding: 12vh 24px 24px; z-index: 55; animation: fade-in 0.1s ease; }
  .palette { width: min(620px, 100%); background: var(--surface); border: 1px solid var(--border); border-radius: 12px; overflow: hidden; box-shadow: var(--shadow-lg); animation: pop 0.12s var(--ease); }
  input { width: 100%; border: none; border-bottom: 1px solid var(--border); padding: 14px 16px; font: 15px var(--f-body); background: var(--surface); color: var(--ink); outline: none; }
  input:focus-visible { box-shadow: none; }
  ul { list-style: none; margin: 0; padding: 6px; max-height: 52vh; overflow: auto; }
  button { width: 100%; display: flex; justify-content: space-between; align-items: center; gap: 12px; border: none; background: none; text-align: left; padding: 8px 10px; border-radius: var(--r); color: var(--ink); }
  button.on { background: var(--accent-soft); }
  kbd { font: 11px var(--f-mono); color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 50%; border: none; background: none; padding: 0; }
  .none { padding: 12px; color: var(--muted); }
</style>
