<script lang="ts">
  // Rename a label or citation key everywhere it appears: the \label, every
  // \ref or \cite that uses it, and the entry in the .bib file.
  import Modal from "./ui/Modal.svelte";
  import { api, errorMessage, type Occurrence, type SymbolKind } from "./api";

  interface Props {
    kind: SymbolKind;
    name: string;
    /** Saves open files and takes a snapshot, so History can undo the rename. */
    beforeApply: () => Promise<void>;
    onApplied: (files: string[], text: string) => void;
    onClose: () => void;
  }
  let { kind: startKind, name: startName, beforeApply, onApplied, onClose }: Props = $props();

  // The dialog is created fresh each time, so the starting values are only read once.
  // svelte-ignore state_referenced_locally
  let kind = $state<SymbolKind>(startKind);
  // svelte-ignore state_referenced_locally
  let oldName = $state(startName);
  // svelte-ignore state_referenced_locally
  let newName = $state(startName);
  let uses = $state<Occurrence[]>([]);
  let known = $state<string[]>([]);
  let error = $state<string | null>(null);
  let busy = $state(false);

  $effect(() => {
    api.symbolDefs().then((defs) => (known = [...new Set(defs.filter((d) => d.kind === kind).map((d) => d.name))].sort())).catch(() => {});
  });
  $effect(() => {
    const k = kind;
    const n = oldName.trim();
    if (!n) {
      uses = [];
      return;
    }
    api.symbolUses(k, n).then((u) => (uses = u)).catch(() => (uses = []));
  });

  const valid = $derived(!!newName.trim() && !/[\s,{}()\\%#~]/.test(newName) && newName !== oldName && uses.length > 0);
  const files = $derived(new Set(uses.map((u) => u.file)).size);

  async function apply(e: SubmitEvent) {
    e.preventDefault();
    if (!valid || busy) return;
    busy = true;
    error = null;
    try {
      await beforeApply();
      const changed = await api.renameSymbol(kind, oldName.trim(), newName.trim());
      onApplied(changed, `Renamed ${oldName.trim()} to ${newName.trim()} in ${changed.length} file${changed.length === 1 ? "" : "s"}.`);
      onClose();
    } catch (err) {
      error = errorMessage(err);
      busy = false;
    }
  }
</script>

<Modal title="Rename label or citation key" subtitle="Changes every use in the project. A snapshot is saved first." width={560} {onClose}>
  <form id="rename-symbol" onsubmit={apply}>
    <div class="segmented" role="group" aria-label="What to rename">
      <button type="button" aria-pressed={kind === "label"} onclick={() => (kind = "label")}>Label</button>
      <button type="button" aria-pressed={kind === "cite"} onclick={() => (kind = "cite")}>Citation key</button>
    </div>
    <div class="field">
      <label for="rs-old">Current name</label>
      <input id="rs-old" class="input mono" list="rs-known" bind:value={oldName} spellcheck="false" autocomplete="off" />
      <datalist id="rs-known">{#each known as k (k)}<option value={k}></option>{/each}</datalist>
    </div>
    <div class="field">
      <label for="rs-new">New name</label>
      <!-- svelte-ignore a11y_autofocus -->
      <input id="rs-new" class="input mono" bind:value={newName} spellcheck="false" autocomplete="off" autofocus />
    </div>
    {#if oldName.trim()}
      {#if uses.length}
        <p class="muted">{uses.length} place{uses.length === 1 ? "" : "s"} in {files} file{files === 1 ? "" : "s"}:</p>
        <ul class="refs">
          {#each uses.slice(0, 60) as u (u.file + u.line + u.col)}
            <li><code>{u.file}:{u.line}</code> {u.text}</li>
          {/each}
          {#if uses.length > 60}<li>… and {uses.length - 60} more</li>{/if}
        </ul>
      {:else}
        <p class="muted">Nothing in the project uses "{oldName.trim()}".</p>
      {/if}
    {/if}
    {#if error}<p class="error">{error}</p>{/if}
  </form>
  {#snippet footer()}
    <button class="btn" onclick={onClose}>Cancel</button>
    <button class="btn primary" type="submit" form="rename-symbol" disabled={!valid || busy}>Rename</button>
  {/snippet}
</Modal>

<style>
  form { display: flex; flex-direction: column; gap: 12px; }
  .muted { color: var(--muted); margin: 0; }
  .error { color: var(--error); margin: 0; }
  .refs { list-style: none; margin: 0; padding: 8px 10px; max-height: 190px; overflow: auto; background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--r); font: 11.5px var(--f-mono); }
  .refs li { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
