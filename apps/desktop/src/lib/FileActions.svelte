<script lang="ts">
  // Right-click menu for the file tree, and the dialogs it opens: new file
  // or folder, rename (offering to update \input references), move to trash.
  import { api, errorMessage, type Entry } from "./api";

  interface Props {
    /** Saves any unsaved edits before files move. */
    beforeChange: () => Promise<void>;
    onCreated: (path: string, dir: boolean) => void;
    onRenamed: (from: string, to: string) => void;
    onTrashed: (path: string) => void;
  }
  let { beforeChange, onCreated, onRenamed, onTrashed }: Props = $props();

  let menu = $state<{ entry: Entry | null; x: number; y: number } | null>(null);
  let dialog = $state<
    | { kind: "new"; dir: boolean; folder: string; name: string }
    | { kind: "rename"; entry: Entry; name: string; refs: { file: string; line: number; text: string }[]; update: boolean }
    | { kind: "trash"; entry: Entry }
    | null
  >(null);
  let error = $state<string | null>(null);

  export function openMenu(entry: Entry | null, x: number, y: number) {
    menu = { entry, x, y };
  }

  const folderOf = (e: Entry | null) => (!e ? "" : e.is_dir ? e.path : e.path.split("/").slice(0, -1).join("/"));
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

  async function submit(e: SubmitEvent) {
    e.preventDefault();
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
        const to = join(folderOf({ ...dialog.entry, is_dir: false }), dialog.name.trim());
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
</script>

<svelte:window onmousedown={(e) => menu && !(e.target as Element).closest(".tree-menu") && (menu = null)} />

{#if menu}
  {@const target = menu.entry}
  <div class="tree-menu" role="menu" style:left="{menu.x}px" style:top="{menu.y}px">
    <button role="menuitem" onclick={() => choose(() => (dialog = { kind: "new", dir: false, folder: folderOf(target), name: "" }))}>New file…</button>
    <button role="menuitem" onclick={() => choose(() => (dialog = { kind: "new", dir: true, folder: folderOf(target), name: "" }))}>New folder…</button>
    {#if target}
      <hr />
      <button role="menuitem" onclick={() => choose(() => startRename(target))}>Rename…</button>
      <button role="menuitem" onclick={() => choose(() => (dialog = { kind: "trash", entry: target }))}>Move to trash…</button>
    {/if}
  </div>
{/if}

{#if dialog}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && (dialog = null)}>
    <form class="dialog" onsubmit={submit}>
      {#if dialog.kind === "new"}
        <h2>{dialog.dir ? "New folder" : "New file"}</h2>
        <p class="muted">In {dialog.folder || "the project folder"}{dialog.dir ? "" : ". Without an extension, .tex is added."}</p>
        <!-- svelte-ignore a11y_autofocus -->
        <input id="fa-name" bind:value={dialog.name} autofocus required placeholder={dialog.dir ? "chapters" : "introduction"} />
      {:else if dialog.kind === "rename"}
        <h2>Rename {dialog.entry.name}</h2>
        <!-- svelte-ignore a11y_autofocus -->
        <input id="fa-name" bind:value={dialog.name} autofocus required />
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
        <h2>Move {dialog.entry.name} to the trash?</h2>
        <p class="muted">You can restore it from your system's trash.</p>
      {/if}
      {#if error}<p class="error">{error}</p>{/if}
      <div class="actions">
        <button type="button" onclick={() => (dialog = null)}>Cancel</button>
        <button class="primary" type="submit">
          {dialog.kind === "new" ? "Create" : dialog.kind === "rename" ? "Rename" : "Move to trash"}
        </button>
      </div>
    </form>
  </div>
{/if}

<style>
  .tree-menu {
    position: fixed; z-index: 40; min-width: 170px; padding: 4px; background: var(--sheet);
    border: 1px solid var(--rule); border-radius: 6px; box-shadow: 0 8px 24px -8px rgba(0, 0, 0, 0.35);
    display: flex; flex-direction: column;
  }
  .tree-menu button { border: none; background: none; text-align: left; padding: 5px 10px; font-weight: 400; border-radius: 4px; }
  .tree-menu button:hover { background: var(--blue-soft); }
  hr { border: none; border-top: 1px solid var(--rule); margin: 4px 0; }
  .backdrop { position: fixed; inset: 0; background: rgba(10, 15, 20, 0.45); display: grid; place-items: center; padding: 24px; z-index: 30; }
  .dialog { width: min(480px, 100%); background: var(--sheet); border: 1px solid var(--rule); border-radius: 8px; padding: 20px 22px; display: flex; flex-direction: column; gap: 8px; }
  h2 { font: 400 20px var(--f-display); margin: 0; }
  .muted { color: var(--muted); margin: 0; }
  #fa-name { font: 13px var(--f-mono); padding: 7px 10px; border: 1px solid var(--rule); border-radius: 5px; background: var(--sheet); color: var(--ink); }
  .check { display: flex; gap: 8px; align-items: center; margin-top: 6px; }
  .refs { list-style: none; margin: 0; padding: 6px 8px; max-height: 140px; overflow: auto; background: var(--code-bg); border-radius: 5px; font: 11.5px var(--f-mono); }
  .refs li { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .error { color: var(--mark); margin: 0; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 8px; }
</style>
