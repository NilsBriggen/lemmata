<script lang="ts">
  // Everything the file tree can do to files: the right-click menu, the
  // dialogs behind it (new, rename, move, delete, unused files), clipboard
  // operations and Undo. Keyboard shortcuts in the tree call the exported
  // functions too.
  import FilePlus from "@lucide/svelte/icons/file-plus";
  import FolderPlus from "@lucide/svelte/icons/folder-plus";
  import Upload from "@lucide/svelte/icons/upload";
  import SquarePen from "@lucide/svelte/icons/square-pen";
  import Trash2 from "@lucide/svelte/icons/trash";
  import Copy from "@lucide/svelte/icons/copy";
  import Files from "@lucide/svelte/icons/files";
  import FolderSearch from "@lucide/svelte/icons/folder-search";
  import Scissors from "@lucide/svelte/icons/scissors";
  import ClipboardPaste from "@lucide/svelte/icons/clipboard-paste";
  import Flag from "@lucide/svelte/icons/flag";
  import ChevronsDownUp from "@lucide/svelte/icons/chevrons-down-up";
  import ChevronsUpDown from "@lucide/svelte/icons/chevrons-up-down";
  import ScanSearch from "@lucide/svelte/icons/scan-search";
  import Undo2 from "@lucide/svelte/icons/undo-2";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { tick } from "svelte";
  import Modal from "./ui/Modal.svelte";
  import { api, errorMessage, type Entry } from "./api";
  import { UndoLog } from "./fileUndo";
  import { baseName, copyName, findEntry, joinPath, namesIn, parentOf, topLevel } from "./treeModel";
  import { collapseAll, expandAll, tree as view, targets } from "./treeSelection.svelte";
  import { settings } from "./settings.svelte";

  type Ref = { file: string; line: number; text: string };

  interface Props {
    /** The project folder, for "Show in file manager". */
    root: string;
    /** The file tree, for finding names and folders. */
    entries: Entry[];
    main: string;
    /** Saves any unsaved edits before files move. */
    beforeChange: () => Promise<void>;
    onCreated: (path: string, dir: boolean) => void;
    onRenamed: (from: string, to: string) => void;
    onTrashed: (path: string) => void;
    /** Something changed on disk that the callbacks above don't cover (a restore, a paste). */
    onChanged: () => void;
    onUpload: (folder: string) => void;
    onSetMain: (path: string) => void;
    /** A short message, optionally with a button (used for Undo). */
    onNotice: (text: string, action?: { label: string; run: () => void }) => void;
  }
  let { root, entries, main, beforeChange, onCreated, onRenamed, onTrashed, onChanged, onUpload, onSetMain, onNotice }: Props = $props();

  const log = new UndoLog();
  let menu = $state<{ entry: Entry | null; x: number; y: number } | null>(null);
  let menuEl = $state<HTMLDivElement>();
  let dialog = $state<
    | { kind: "new"; dir: boolean; folder: string; name: string }
    | { kind: "rename"; entry: Entry; name: string; refs: Ref[]; update: boolean }
    | { kind: "trash"; paths: string[]; refs: Ref[] }
    | { kind: "move"; paths: string[]; folder: string; refs: Ref[]; update: boolean }
    | { kind: "unused"; files: string[]; picked: string[]; loading: boolean }
    | null
  >(null);
  let error = $state<string | null>(null);

  const folderOf = (e: Entry | null) => (!e ? "" : e.is_dir ? e.path : parentOf(e.path));
  const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
  const describe = (paths: string[]) => (paths.length === 1 ? baseName(paths[0]) : plural(paths.length, "item", "items"));
  const failure = (e: unknown) => onNotice(errorMessage(e));
  const undoAction = { label: "Undo", run: () => void undo() };

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

  export async function rename(entry: Entry) {
    error = null;
    const refs = await api.findReferences(entry.path).catch(() => []);
    dialog = { kind: "rename", entry, name: entry.name, refs, update: refs.length > 0 };
  }

  async function refsOf(paths: string[]): Promise<Ref[]> {
    const all = await Promise.all(paths.map((p) => api.findReferences(p).catch(() => [] as Ref[])));
    // A reference from a file that moves or goes away together with its target isn't a problem.
    return all.flat().filter((r) => !paths.some((p) => r.file === p || r.file.startsWith(p + "/")));
  }

  /** Moves entries into a folder ("" is the project root); asks first when other files point at them. */
  export async function move(paths: string[], folder: string) {
    error = null;
    const list = topLevel(paths).filter((p) => parentOf(p) !== folder);
    if (!list.length) return;
    const refs = await refsOf(list);
    if (refs.length) {
      dialog = { kind: "move", paths: list, folder, refs, update: true };
      return;
    }
    await runMove(list, folder, false);
  }

  async function runMove(paths: string[], folder: string, update: boolean) {
    await beforeChange();
    const moved = await api.movePaths(paths, folder, update);
    if (!moved.length) return;
    for (const m of moved) onRenamed(m.from, m.to);
    log.push({
      label: `Move ${describe(paths)}`,
      undo: async () => {
        for (const m of [...moved].reverse()) {
          await api.renamePath(m.to, m.from, update);
          onRenamed(m.to, m.from);
        }
      },
    });
    onNotice(`Moved ${describe(moved.map((m) => m.from))}${folder ? ` into ${baseName(folder)}` : " to the project folder"}.`, undoAction);
  }

  /** Deletes entries. The project's own trash can be undone; the system trash cannot. */
  export async function trash(paths: string[]) {
    error = null;
    const list = topLevel(paths);
    if (!list.length) return;
    const refs = await refsOf(list);
    if (refs.length || settings.trashMode === "system") {
      dialog = { kind: "trash", paths: list, refs };
      return;
    }
    await runTrash(list);
  }

  async function runTrash(paths: string[]) {
    await beforeChange();
    if (settings.trashMode === "system") {
      for (const p of paths) {
        await api.trashPath(p);
        onTrashed(p);
      }
      onNotice(`Moved ${describe(paths)} to the system trash.`);
      return;
    }
    const batch = await api.trashPaths(paths);
    for (const p of batch.paths) onTrashed(p);
    log.push({
      label: `Delete ${describe(paths)}`,
      undo: async () => {
        await api.restoreTrash(batch.id);
        onChanged();
      },
    });
    onNotice(`Deleted ${describe(paths)}.`, undoAction);
  }

  export async function duplicate(paths: string[]) {
    const list = topLevel(paths);
    await beforeChange();
    const made: string[] = [];
    try {
      for (const p of list) {
        const e = findEntry(entries, p);
        if (!e) continue;
        const dir = parentOf(p);
        const taken = namesIn(entries, dir);
        made.forEach((m) => parentOf(m) === dir && taken.add(baseName(m)));
        const to = joinPath(dir, copyName(e.name, e.is_dir, taken));
        await api.copyPath(p, to);
        made.push(to);
      }
    } catch (e) {
      failure(e);
    }
    if (!made.length) return;
    onChanged();
    onCreated(made[0], !!findEntry(entries, list[0])?.is_dir);
    log.push({ label: `Duplicate ${describe(list)}`, undo: async () => void (await api.trashPaths(made), onChanged()) });
  }

  export function copy(paths: string[]) {
    if (!paths.length) return;
    view.clipboard = { mode: "copy", paths: topLevel(paths) };
    onNotice(`Copied ${describe(paths)}. Paste it into a folder.`);
  }

  export function cut(paths: string[]) {
    if (!paths.length) return;
    view.clipboard = { mode: "cut", paths: topLevel(paths) };
    onNotice(`Cut ${describe(paths)}. Paste it into a folder.`);
  }

  export async function paste(folder: string) {
    const clip = view.clipboard;
    if (!clip) return;
    if (clip.mode === "cut") {
      view.clipboard = null;
      await move(clip.paths, folder).catch(failure);
      return;
    }
    await beforeChange();
    const made: string[] = [];
    try {
      const taken = namesIn(entries, folder);
      for (const p of clip.paths) {
        const e = findEntry(entries, p);
        if (!e) continue;
        if (folder === p || folder.startsWith(p + "/")) throw new Error(`${e.name} can't be copied into itself.`);
        const name = taken.has(e.name) ? copyName(e.name, e.is_dir, taken) : e.name;
        taken.add(name);
        const to = joinPath(folder, name);
        await api.copyPath(p, to);
        made.push(to);
      }
    } catch (e) {
      failure(e);
    }
    if (!made.length) return;
    onChanged();
    log.push({ label: `Paste ${describe(clip.paths)}`, undo: async () => void (await api.trashPaths(made), onChanged()) });
    onNotice(`Pasted ${describe(made)}.`, undoAction);
  }

  /** Undoes the last file operation made in the tree. Returns false when there is nothing to undo. */
  export async function undo(): Promise<boolean> {
    if (!log.size) {
      onNotice("Nothing to undo in the file tree.");
      return false;
    }
    try {
      const step = await log.undo();
      if (step) onNotice(`Undid: ${step.label}.`);
    } catch (e) {
      failure(e);
    }
    return true;
  }

  export const canUndo = () => log.size > 0;
  export const forgetHistory = () => log.clear();

  export async function showUnused() {
    dialog = { kind: "unused", files: [], picked: [], loading: true };
    try {
      const files = await api.unusedFiles();
      if (dialog?.kind === "unused") dialog = { kind: "unused", files, picked: [], loading: false };
    } catch (e) {
      dialog = null;
      failure(e);
    }
  }

  const absolute = (rel: string) => {
    const sep = root.includes("\\") ? "\\" : "/";
    return `${root.replace(/[\\/]+$/, "")}${sep}${rel.split("/").join(sep)}`;
  };

  function choose(action: () => unknown) {
    error = null;
    // The action reads `target` and `picked`, which are derived from `menu`: start it before closing.
    let run: Promise<unknown>;
    try {
      run = Promise.resolve(action());
    } catch (e) {
      run = Promise.reject(e);
    }
    menu = null;
    run.catch(failure);
  }

  async function submit(e?: SubmitEvent) {
    e?.preventDefault();
    if (!dialog) return;
    error = null;
    try {
      await beforeChange();
      if (dialog.kind === "new") {
        const name = dialog.name.trim();
        const path = joinPath(dialog.folder, dialog.dir || name.includes(".") ? name : `${name}.tex`);
        await api.createPath(path, dialog.dir);
        onCreated(path, dialog.dir);
        log.push({ label: `Create ${baseName(path)}`, undo: async () => void (await api.trashPaths([path]), onChanged()) });
      } else if (dialog.kind === "move") {
        const { paths, folder, update } = dialog;
        dialog = null;
        await runMove(paths, folder, update);
      } else if (dialog.kind === "rename") {
        const from = dialog.entry.path;
        const to = joinPath(parentOf(from), dialog.name.trim());
        const update = dialog.update;
        await api.renamePath(from, to, update);
        onRenamed(from, to);
        log.push({
          label: `Rename ${baseName(from)}`,
          undo: async () => {
            await api.renamePath(to, from, update);
            onRenamed(to, from);
          },
        });
      } else if (dialog.kind === "trash") {
        const { paths } = dialog;
        dialog = null;
        await runTrash(paths);
      } else {
        const { picked } = dialog;
        dialog = null;
        if (picked.length) await runTrash(picked);
      }
      dialog = null;
    } catch (err) {
      error = errorMessage(err);
    }
  }

  const title = $derived(
    !dialog
      ? ""
      : dialog.kind === "new"
        ? dialog.dir ? "New folder" : "New file"
        : dialog.kind === "move"
          ? `Move ${describe(dialog.paths)}`
          : dialog.kind === "rename"
            ? `Rename ${dialog.entry.name}`
            : dialog.kind === "unused"
              ? "Files nothing refers to"
              : settings.trashMode === "system" ? `Move ${describe(dialog.paths)} to the trash?` : `Delete ${describe(dialog.paths)}?`,
  );
  const submitLabel = $derived(
    dialog?.kind === "new" ? "Create" : dialog?.kind === "rename" ? "Rename" : dialog?.kind === "move" ? "Move" : dialog?.kind === "unused" ? "Delete selected" : "Delete",
  );
</script>

<svelte:window onmousedown={(e) => menu && !(e.target as Element).closest(".tree-menu") && (menu = null)} onkeydown={(e) => menu && e.key === "Escape" && (menu = null)} />

{#if menu}
  {@const target = menu.entry}
  {@const picked = target ? targets(target.path) : []}
  {@const many = picked.length > 1}
  <div class="menu tree-menu" role="menu" bind:this={menuEl} style:left="{menu.x}px" style:top="{menu.y}px">
    {#if many}<div class="heading">{picked.length} items selected</div>{/if}
    {#if !many}
      <button role="menuitem" onclick={() => choose(() => newItem(false, folderOf(target)))}><FilePlus size={15} /> New file</button>
      <button role="menuitem" onclick={() => choose(() => newItem(true, folderOf(target)))}><FolderPlus size={15} /> New folder</button>
      <button role="menuitem" onclick={() => choose(() => onUpload(folderOf(target)))}><Upload size={15} /> Add files…</button>
      <hr />
    {/if}
    {#if target}
      <button role="menuitem" onclick={() => choose(() => cut(picked))}><Scissors size={15} /> Cut</button>
      <button role="menuitem" onclick={() => choose(() => copy(picked))}><Copy size={15} /> Copy</button>
    {/if}
    {#if view.clipboard && !many}
      <button role="menuitem" onclick={() => choose(() => paste(folderOf(target)))}><ClipboardPaste size={15} /> Paste{target ? "" : " here"}</button>
    {/if}
    {#if target && !many}
      <button role="menuitem" onclick={() => choose(() => rename(target))}><SquarePen size={15} /> Rename…</button>
    {/if}
    {#if target}
      <button role="menuitem" onclick={() => choose(() => duplicate(picked))}><Files size={15} /> Duplicate</button>
    {/if}
    {#if target && !many}
      {#if !target.is_dir && target.path !== main && /\.tex$/i.test(target.name)}
        <button role="menuitem" onclick={() => choose(() => onSetMain(target.path))}><Flag size={15} /> Set as main file</button>
      {/if}
      <button role="menuitem" onclick={() => choose(() => navigator.clipboard?.writeText(target.path).catch(() => {}))}><Copy size={15} /> Copy path</button>
      <button role="menuitem" onclick={() => choose(() => revealItemInDir(absolute(target.path)).catch(() => {}))}><FolderSearch size={15} /> Show in file manager</button>
    {/if}
    {#if target}
      <button role="menuitem" class="danger" onclick={() => choose(() => trash(picked))}><Trash2 size={15} /> {settings.trashMode === "system" ? "Move to trash…" : "Delete"}</button>
    {:else}
      <button role="menuitem" onclick={() => choose(() => collapseAll(entries))}><ChevronsDownUp size={15} /> Collapse all</button>
      <button role="menuitem" onclick={() => choose(() => expandAll())}><ChevronsUpDown size={15} /> Expand all</button>
      <button role="menuitem" onclick={() => choose(() => showUnused())}><ScanSearch size={15} /> Find unused files…</button>
      {#if log.size}<button role="menuitem" onclick={() => choose(() => undo())}><Undo2 size={15} /> Undo {log.last?.label.toLowerCase()}</button>{/if}
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
          <input id="fa-name" class="input mono" bind:value={dialog.name} autofocus required onfocus={(e) => e.currentTarget.setSelectionRange(0, dialog && dialog.kind === "rename" ? dialog.name.lastIndexOf(".") > 0 ? dialog.name.lastIndexOf(".") : dialog.name.length : 0)} />
        </div>
        {#if dialog.refs.length}
          <label class="check" for="fa-refs">
            <input id="fa-refs" type="checkbox" bind:checked={dialog.update} />
            Also update {plural(dialog.refs.length, "reference", "references")} to it
          </label>
          <ul class="refs">
            {#each dialog.refs as r (r.file + r.line)}<li><code>{r.file}:{r.line}</code> {r.text}</li>{/each}
          </ul>
        {/if}
      {:else if dialog.kind === "move"}
        <p class="muted">Move <b>{describe(dialog.paths)}</b> to {dialog.folder || "the project folder"}.</p>
        <label class="check" for="fa-refs-mv">
          <input id="fa-refs-mv" type="checkbox" bind:checked={dialog.update} />
          Also update {plural(dialog.refs.length, "reference", "references")} to {dialog.paths.length === 1 ? "it" : "them"}
        </label>
        <ul class="refs">
          {#each dialog.refs as r (r.file + r.line)}<li><code>{r.file}:{r.line}</code> {r.text}</li>{/each}
        </ul>
      {:else if dialog.kind === "unused"}
        {#if dialog.loading}
          <p class="muted">Looking…</p>
        {:else if !dialog.files.length}
          <p class="muted">Every image, .tex file and bibliography is referenced somewhere.</p>
        {:else}
          <p class="muted">No \input, \include, \includegraphics or bibliography command names these. Check the ones to delete; they can be restored with Undo.</p>
          <ul class="unused">
            {#each dialog.files as f (f)}
              <li><label><input type="checkbox" checked={dialog.picked.includes(f)} onchange={(ev) => dialog && dialog.kind === "unused" && (dialog.picked = ev.currentTarget.checked ? [...dialog.picked, f] : dialog.picked.filter((x) => x !== f))} /> <code>{f}</code></label></li>
            {/each}
          </ul>
        {/if}
      {:else}
        {#if settings.trashMode === "system"}
          <p class="muted">You can restore {dialog.paths.length === 1 ? "it" : "them"} from your system's trash.</p>
        {:else}
          <p class="muted">You can bring {dialog.paths.length === 1 ? "it" : "them"} back with Undo.</p>
        {/if}
        {#if dialog.refs.length}
          <p class="warn">{plural(dialog.refs.length, "line still refers", "lines still refer")} to {dialog.paths.length === 1 ? "it" : "these"}:</p>
          <ul class="refs">
            {#each dialog.refs as r (r.file + r.line)}<li><code>{r.file}:{r.line}</code> {r.text}</li>{/each}
          </ul>
        {/if}
      {/if}
      {#if error}<p class="error">{error}</p>{/if}
    </form>
    {#snippet footer()}
      <button class="btn" onclick={() => (dialog = null)}>{dialog?.kind === "unused" && !dialog.files.length ? "Close" : "Cancel"}</button>
      {#if !(dialog?.kind === "unused" && (dialog.loading || !dialog.files.length))}
        <button class="btn primary" class:danger-btn={dialog?.kind === "trash" || dialog?.kind === "unused"} type="submit" form="file-action" disabled={dialog?.kind === "unused" && !dialog.picked.length}>
          {submitLabel}
        </button>
      {/if}
    {/snippet}
  </Modal>
{/if}

<style>
  form { display: flex; flex-direction: column; gap: 12px; }
  .muted { color: var(--muted); margin: 0; }
  .warn { margin: 0; color: var(--warning, var(--error)); }
  .error { color: var(--error); margin: 0; }
  .refs, .unused { list-style: none; margin: 0; padding: 8px 10px; max-height: 190px; overflow: auto; background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--r); font: 11.5px var(--f-mono); }
  .refs li { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .unused li { padding: 2px 0; }
  .unused label { display: flex; gap: 8px; align-items: center; cursor: pointer; }
  .menu :global(.danger) { color: var(--error); }
  .menu :global(.danger svg) { color: var(--error); }
  .danger-btn { background: var(--error) !important; border-color: var(--error) !important; }
</style>
