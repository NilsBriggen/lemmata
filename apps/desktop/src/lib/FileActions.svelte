<script lang="ts">
  // Right-click menu for the file tree, and the dialogs it opens: new file
  // or folder, rename (offering to update \input references), move to trash.
  import FilePlus from "@lucide/svelte/icons/file-plus";
  import FolderPlus from "@lucide/svelte/icons/folder-plus";
  import Upload from "@lucide/svelte/icons/upload";
  import SquarePen from "@lucide/svelte/icons/square-pen";
  import Trash2 from "@lucide/svelte/icons/trash";
  import { tick } from "svelte";
  import Modal from "./ui/Modal.svelte";
  import { api, errorMessage, type Entry } from "./api";

  interface Props {
    /** Saves any unsaved edits before files move. */
    beforeChange: () => Promise<void>;
    onCreated: (path: string, dir: boolean) => void;
    onRenamed: (from: string, to: string) => void;
    onTrashed: (path: string) => void;
    onUpload: (folder: string) => void;
  }
  let { beforeChange, onCreated, onRenamed, onTrashed, onUpload }: Props = $props();

  let menu = $state<{ entry: Entry | null; x: number; y: number } | null>(null);
  let menuEl = $state<HTMLDivElement>();
  let dialog = $state<
    | { kind: "new"; dir: boolean; folder: string; name: string }
    | { kind: "rename"; entry: Entry; name: string; refs: { file: string; line: number; text: string }[]; update: boolean }
    | { kind: "trash"; entry: Entry }
    | null
  >(null);
  let error = $state<string | null>(null);

  export async function openMenu(entry: Entry | null, x: number, y: number) {
    menu = { entry, x, y };
    await tick();
    if (!menuEl) return;
    // Keep the menu on screen.
    const r = menuEl.getBoundingClientRect();
    menu = { entry, x: Math.min(x, innerWidth - r.width - 8), y: Math.min(y, innerHeight - r.height - 8) };
    menuEl.querySelector("button")?.focus();
  }

  export function newItem(dir: boolean, folder = "") {
    error = null;
    dialog = { kind: "new", dir, folder, name: "" };
  }

  const folderOf = (e: Entry | null) => (!e ? "" : e.is_dir ? e.path : e.path.split("/").slice(0, -1).join("/"));
  const parentOf = (path: string) => path.split("/").slice(0, -1).join("/");
  const join = (folder: string, name: string) => (folder ? `${folder}/${name}` : name);

  async function startRename(entry: Entry) {
    const refs = await api.findReferences(entry.path).catch(() => []);
    dialog = { kind: "rename", entry, name: entry.name, refs, update: refs.length > 0 };
  }

  function choose(action: () => void) {
    menu = null;
    error = null;
    action();
  }

  async function submit(e?: SubmitEvent) {
    e?.preventDefault();
    if (!dialog) return;
    error = null;
    try {
      await beforeChange();
      if (dialog.kind === "new") {
        const name = dialog.name.trim();
        const path = join(dialog.folder, dialog.dir || name.includes(".") ? name : `${name}.tex`);
        await api.createPath(path, dialog.dir);
        onCreated(path, dialog.dir);
      } else if (dialog.kind === "rename") {
        const to = join(parentOf(dialog.entry.path), dialog.name.trim());
        await api.renamePath(dialog.entry.path, to, dialog.update);
        onRenamed(dialog.entry.path, to);
      } else {
        await api.trashPath(dialog.entry.path);
        onTrashed(dialog.entry.path);
      }
      dialog = null;
    } catch (err) {
      error = errorMessage(err);
    }
  }

  const title = $derived(
    !dialog ? "" : dialog.kind === "new" ? (dialog.dir ? "New folder" : "New file") : dialog.kind === "rename" ? `Rename ${dialog.entry.name}` : `Move ${dialog.entry.name} to the trash?`,
  );
</script>

<svelte:window onmousedown={(e) => menu && !(e.target as Element).closest(".tree-menu") && (menu = null)} onkeydown={(e) => menu && e.key === "Escape" && (menu = null)} />

{#if menu}
  {@const target = menu.entry}
  <div class="menu tree-menu" role="menu" bind:this={menuEl} style:left="{menu.x}px" style:top="{menu.y}px">
    <button role="menuitem" onclick={() => choose(() => newItem(false, folderOf(target)))}><FilePlus size={15} /> New file</button>
    <button role="menuitem" onclick={() => choose(() => newItem(true, folderOf(target)))}><FolderPlus size={15} /> New folder</button>
    <button role="menuitem" onclick={() => choose(() => onUpload(folderOf(target)))}><Upload size={15} /> Add files…</button>
    {#if target}
      <hr />
      <button role="menuitem" onclick={() => choose(() => startRename(target))}><SquarePen size={15} /> Rename…</button>
      <button role="menuitem" class="danger" onclick={() => choose(() => (dialog = { kind: "trash", entry: target }))}><Trash2 size={15} /> Move to trash…</button>
    {/if}
  </div>
{/if}

{#if dialog}
  <Modal {title} width={460} onClose={() => (dialog = null)}>
    <form id="file-action" onsubmit={submit}>
      {#if dialog.kind === "new"}
        <div class="field">
          <label for="fa-name">Name</label>
          <!-- svelte-ignore a11y_autofocus -->
          <input id="fa-name" class="input mono" bind:value={dialog.name} autofocus required placeholder={dialog.dir ? "chapters" : "introduction"} />
        </div>
        <p class="muted">In {dialog.folder || "the project folder"}.{dialog.dir ? "" : " Without an extension, .tex is added."}</p>
      {:else if dialog.kind === "rename"}
        <div class="field">
          <label for="fa-name">New name</label>
          <!-- svelte-ignore a11y_autofocus -->
          <input id="fa-name" class="input mono" bind:value={dialog.name} autofocus required />
        </div>
        {#if dialog.refs.length}
          <label class="check" for="fa-refs">
            <input id="fa-refs" type="checkbox" bind:checked={dialog.update} />
            Also update {dialog.refs.length} reference{dialog.refs.length === 1 ? "" : "s"} to it
          </label>
          <ul class="refs">
            {#each dialog.refs as r (r.file + r.line)}<li><code>{r.file}:{r.line}</code> {r.text}</li>{/each}
          </ul>
        {/if}
      {:else}
        <p class="muted">You can restore it from your system's trash.</p>
      {/if}
      {#if error}<p class="error">{error}</p>{/if}
    </form>
    {#snippet footer()}
      <button class="btn" onclick={() => (dialog = null)}>Cancel</button>
      <button class="btn primary" class:danger-btn={dialog?.kind === "trash"} type="submit" form="file-action">
        {dialog?.kind === "new" ? "Create" : dialog?.kind === "rename" ? "Rename" : "Move to trash"}
      </button>
    {/snippet}
  </Modal>
{/if}

<style>
  form { display: flex; flex-direction: column; gap: 12px; }
  .muted { color: var(--muted); margin: 0; }
  .error { color: var(--error); margin: 0; }
  .refs { list-style: none; margin: 0; padding: 8px 10px; max-height: 150px; overflow: auto; background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--r); font: 11.5px var(--f-mono); }
  .refs li { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .menu :global(.danger) { color: var(--error); }
  .menu :global(.danger svg) { color: var(--error); }
  .danger-btn { background: var(--error) !important; border-color: var(--error) !important; }
</style>
