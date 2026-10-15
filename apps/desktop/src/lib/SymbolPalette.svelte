<script lang="ts">
  // Maths symbols rendered with KaTeX; click one to insert it.
  import { loadKatex, renderTex } from "./writing/katexRender";
  import Modal from "./ui/Modal.svelte";
  import { SYMBOLS, GROUP_NAMES, type Symbol } from "./writing/symbols";

  interface Props {
    onInsert: (tex: string, needs?: string) => void;
    onClose: () => void;
  }
  let { onInsert, onClose }: Props = $props();

  let query = $state("");
  let group = $state<string>("All");
  const shown = $derived(
    SYMBOLS.filter((s) => (group === "All" || s.group === group) && (!query || s.tex.toLowerCase().includes(query.toLowerCase().replace(/^\\/, "")))),
  );

  let ready = $state(false);
  loadKatex().then(() => (ready = true), () => {});
  const html = (s: Symbol) => (ready ? renderTex(s.tex) : null);

  function pick(s: Symbol, keep: boolean) {
    onInsert(s.tex, s.pkg ?? undefined);
    if (!keep) onClose();
  }
</script>

<Modal title="Symbols" subtitle="Click to insert. Hold Shift to keep this open." width={640} top {onClose}>
  <div class="bar">
    <input class="input" placeholder="Search by command name" bind:value={query} />
    <select class="input" bind:value={group} aria-label="Category">
      <option>All</option>
      {#each GROUP_NAMES as g (g)}<option>{g}</option>{/each}
    </select>
  </div>
  <div class="grid">
    {#each shown as s (s.group + s.tex)}
      <button class="sym" title={s.tex + (s.pkg ? `  (needs ${s.pkg})` : "")} onclick={(e) => pick(s, e.shiftKey)}>
        {#if html(s)}{@html html(s)}{:else}<code>{s.tex}</code>{/if}
      </button>
    {:else}
      <p class="none">No symbol matches “{query}”.</p>
    {/each}
  </div>
</Modal>

<style>
  .bar { display: grid; grid-template-columns: 1fr 160px; gap: 8px; margin-bottom: 12px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(56px, 1fr)); gap: 6px; max-height: 52vh; overflow: auto; padding: 2px; }
  .sym { height: 48px; display: grid; place-items: center; border: 1px solid var(--border); border-radius: var(--r); background: var(--surface); color: var(--ink); overflow: hidden; font-size: 15px; }
  .sym:hover { border-color: var(--accent); background: var(--accent-soft); }
  .sym code { font-size: 10px; color: var(--muted); }
  .none { grid-column: 1 / -1; color: var(--muted); }
</style>
