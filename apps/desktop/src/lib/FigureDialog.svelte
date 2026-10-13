<script lang="ts">
  // Insert figure: pick an image from the project or the computer, add a caption.
  import { open } from "@tauri-apps/plugin-dialog";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import Modal from "./ui/Modal.svelte";
  import { api, errorMessage, type Entry } from "./api";
  import { figureBlock } from "./writing/table";

  interface Props {
    tree: Entry[];
    onInsert: (snippet: { text: string; cursor: number }) => void;
    onClose: () => void;
  }
  let { tree, onInsert, onClose }: Props = $props();

  const IMAGE = /\.(png|jpe?g|pdf|svg|eps|gif|webp)$/i;
  const images = $derived(walk(tree));
  function walk(entries: Entry[]): string[] {
    return entries.flatMap((e) => (e.is_dir ? walk(e.children) : IMAGE.test(e.path) ? [e.path] : []));
  }

  let path = $state("");
  let caption = $state("");
  let label = $state("");
  let width = $state(80);
  let placement = $state("htbp");
  let busy = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    if (!path && images.length) path = images[0];
  });

  async function browse() {
    const file = await open({ title: "Choose an image", filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "pdf", "svg", "eps", "gif", "webp"] }] });
    if (typeof file !== "string") return;
    busy = true;
    error = null;
    try {
      path = await api.importFigure(file);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }

  function submit(e: Event) {
    e.preventDefault();
    if (!path) return;
    onInsert(figureBlock({ path, caption, label, width, placement }));
    onClose();
  }
</script>

<Modal title="Insert figure" subtitle="Images stay in your project's folder." width={520} {onClose}>
  <form id="figure-form" onsubmit={submit}>
    <div class="field">
      <label for="fig-path">Image</label>
      <div class="pick">
        {#if images.length || path}
          <select id="fig-path" class="input mono" bind:value={path}>
            {#if path && !images.includes(path)}<option value={path}>{path}</option>{/if}
            {#each images as p (p)}<option value={p}>{p}</option>{/each}
          </select>
        {:else}
          <span class="empty">No images in this project yet.</span>
        {/if}
        <button type="button" class="btn" disabled={busy} onclick={browse}><FolderOpen size={15} /> Browse…</button>
      </div>
      {#if error}<p class="err">{error}</p>{/if}
    </div>
    <div class="field">
      <label for="fig-caption">Caption</label>
      <input id="fig-caption" class="input" bind:value={caption} placeholder="Optional" />
    </div>
    <div class="grid">
      <div class="field">
        <label for="fig-label">Label</label>
        <input id="fig-label" class="input mono" bind:value={label} placeholder="made from the caption" />
      </div>
      <div class="field">
        <label for="fig-width">Width</label>
        <select id="fig-width" class="input" bind:value={width}>
          {#each [30, 50, 80, 100] as w (w)}<option value={w}>{w}% of the text width</option>{/each}
        </select>
      </div>
    </div>
    <div class="field">
      <span class="label">Placement</span>
      <div class="segmented" role="group" aria-label="Placement">
        {#each [["htbp", "Automatic"], ["H", "Right here"], ["t", "Top"], ["", "Default"]] as [v, name] (name)}
          <button type="button" aria-pressed={placement === v} onclick={() => (placement = v)}>{name}</button>
        {/each}
      </div>
    </div>
  </form>
  {#snippet footer()}
    <button class="btn" onclick={onClose}>Cancel</button>
    <button class="btn primary" type="submit" form="figure-form" disabled={!path}>Insert</button>
  {/snippet}
</Modal>

<style>
  form { display: flex; flex-direction: column; gap: 14px; }
  .pick { display: flex; gap: 8px; }
  .pick .input { flex: 1; min-width: 0; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .empty { flex: 1; align-self: center; color: var(--muted); }
  .err { color: var(--error); margin: 4px 0 0; font-size: 12px; }
  .segmented { justify-self: start; }
</style>
