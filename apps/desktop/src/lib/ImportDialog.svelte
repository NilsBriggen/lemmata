<script lang="ts">
  // Import from Overleaf: one project ("Download → Source") or a bulk
  // download (a zip of project zips, from selecting several projects).
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { documentDir } from "@tauri-apps/api/path";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import FileArchive from "@lucide/svelte/icons/file-archive";
  import Modal from "./ui/Modal.svelte";
  import { api, errorMessage } from "./api";

  interface Props {
    zip: string;
    defaultParent?: string;
    onImported: (paths: string[]) => void;
    onClose: () => void;
  }
  let { zip, defaultParent = "", onImported, onClose }: Props = $props();

  let info = $state<{ bulk: boolean; projects: string[] } | null>(null);
  let name = $state("");
  let parent = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  const fileName = $derived(zip.split(/[\\/]/).pop() ?? zip);

  onMount(async () => {
    parent = defaultParent || (await documentDir().catch(() => ""));
    try {
      info = await api.inspectZip(zip);
      name = info.projects[0] ?? "";
    } catch (e) {
      error = errorMessage(e);
    }
  });

  async function chooseParent() {
    const dir = await open({ directory: true, title: "Choose where to put the projects", defaultPath: parent || undefined });
    if (typeof dir === "string") parent = dir;
  }

  async function run(e?: SubmitEvent) {
    e?.preventDefault();
    busy = true;
    error = null;
    try {
      onImported(await api.importOverleafZip(zip, parent, name));
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Import from Overleaf" subtitle={fileName} width={520} {onClose}>
  <form id="import-form" onsubmit={run}>
    {#if !info && !error}
      <p class="muted">Reading the zip…</p>
    {:else if info?.bulk}
      <p>This is a download of <b>{info.projects.length} projects</b>. Each one gets its own folder.</p>
      <ul class="projects">
        {#each info.projects as p (p)}<li><FileArchive size={14} /> {p}</li>{/each}
      </ul>
    {:else if info}
      <div class="field">
        <label for="imp-name">Folder name</label>
        <input id="imp-name" class="input" bind:value={name} required />
      </div>
    {/if}
    {#if info}
      <div class="field">
        <label for="imp-parent">{info.bulk ? "Import into" : "Location"}</label>
        <div class="row">
          <input id="imp-parent" class="input mono" bind:value={parent} required />
          <button type="button" class="btn" onclick={chooseParent}><FolderOpen size={15} /> Choose…</button>
        </div>
      </div>
    {/if}
    {#if error}<p class="error">{error}</p>{/if}
  </form>
  {#snippet footer()}
    <button class="btn" onclick={onClose}>Cancel</button>
    <button class="btn primary" type="submit" form="import-form" disabled={busy || !info || !parent || (!info.bulk && !name.trim())}>
      {busy ? "Importing…" : info?.bulk ? `Import ${info.projects.length} projects` : "Import"}
    </button>
  {/snippet}
</Modal>

<style>
  form { display: flex; flex-direction: column; gap: 14px; }
  p { margin: 0; }
  .muted { color: var(--muted); }
  .projects {
    list-style: none; margin: 0; padding: 6px; max-height: 210px; overflow: auto;
    border: 1px solid var(--border); border-radius: var(--r); background: var(--surface-2);
    display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 2px 8px;
  }
  .projects li { display: flex; gap: 6px; align-items: center; padding: 4px 6px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--ink-2); }
  .projects :global(svg) { color: var(--muted); flex: none; }
  .row { display: flex; gap: 8px; }
  .row .input { flex: 1; min-width: 0; }
  .error { color: var(--error); }
</style>
