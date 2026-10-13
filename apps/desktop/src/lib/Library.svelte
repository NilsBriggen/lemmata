<script lang="ts">
  // Start screen: recent projects with thumbnails, plus creating a project
  // from a template or importing one from Overleaf.
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { documentDir } from "@tauri-apps/api/path";
  import { api, errorMessage, type Template } from "./api";

  interface Props {
    recent: string[];
    error: string | null;
    onOpen: (path: string) => void;
    onForget: (path: string) => void;
    onPreferences: () => void;
  }
  let { recent, error, onOpen, onForget, onPreferences }: Props = $props();

  let query = $state("");
  let thumbs = $state<Record<string, string>>({});
  let templates = $state<Template[]>([]);
  let dialog = $state<null | "new" | "import">(null);
  let chosen = $state("article");
  let name = $state("");
  let zipPath = $state("");
  let parent = $state("");
  let busy = $state(false);
  let dialogError = $state<string | null>(null);

  const nameOf = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
  const parentOf = (p: string) => p.replace(/[\\/][^\\/]+[\\/]?$/, "");
  const shown = $derived(
    recent.filter((r) => !query || r.toLowerCase().includes(query.trim().toLowerCase())),
  );

  onMount(async () => {
    templates = await api.listTemplates().catch(() => []);
    parent = recent[0] ? parentOf(recent[0]) : await documentDir().catch(() => "");
    for (const r of recent) {
      api
        .loadThumbnail(r)
        .then((png) => (thumbs[r] = URL.createObjectURL(new Blob([png.buffer as ArrayBuffer], { type: "image/png" }))))
        .catch(() => {});
    }
  });

  async function pickFolder() {
    const dir = await open({ directory: true, title: "Open a LaTeX project folder" });
    if (typeof dir === "string") onOpen(dir);
  }

  async function chooseParent() {
    const dir = await open({ directory: true, title: "Choose where to put the project", defaultPath: parent || undefined });
    if (typeof dir === "string") parent = dir;
  }

  function startNew() {
    dialogError = null;
    name = "";
    chosen = templates[0]?.id ?? "article";
    dialog = "new";
  }

  async function startImport() {
    const zip = await open({ title: "Choose the zip downloaded from Overleaf", filters: [{ name: "Zip", extensions: ["zip"] }] });
    if (typeof zip !== "string") return;
    dialogError = null;
    zipPath = zip;
    name = nameOf(zip).replace(/\.zip$/i, "");
    dialog = "import";
  }

  async function confirm(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    dialogError = null;
    try {
      const path =
        dialog === "new"
          ? await api.createProject(chosen, parent, name)
          : await api.importOverleafZip(zipPath, parent, name);
      dialog = null;
      onOpen(path);
    } catch (err) {
      dialogError = errorMessage(err);
    } finally {
      busy = false;
    }
  }
</script>

<div class="library">
  <header>
    <h1>Lemmata</h1>
    <input id="library-search" type="search" placeholder="Search projects" bind:value={query} aria-label="Search projects" />
    <button onclick={pickFolder}>Open folder…</button>
    <button onclick={startImport}>Import Overleaf .zip…</button>
    <button class="primary" onclick={startNew}>New project</button>
    <button onclick={onPreferences}>Preferences</button>
  </header>

  {#if error}<p class="error">{error}</p>{/if}

  {#if recent.length === 0}
    <div class="empty">
      <p>No projects yet. Start one from a template, open an existing folder, or import a zip you downloaded from Overleaf (Menu → Download → Source).</p>
    </div>
  {:else}
    <div class="grid">
      {#each shown as r (r)}
        <div class="card">
          <button class="open" onclick={() => onOpen(r)}>
            <div class="thumb">
              {#if thumbs[r]}<img src={thumbs[r]} alt="" />{:else}<div class="blank"><i></i><i></i><i></i><i></i></div>{/if}
            </div>
            <div class="info"><b>{nameOf(r)}</b><span title={r}>{r}</span></div>
          </button>
          <button class="forget" aria-label="Remove {nameOf(r)} from recent projects" title="Remove from this list (the folder stays on disk)" onclick={() => onForget(r)}>×</button>
        </div>
      {/each}
    </div>
  {/if}
</div>

{#if dialog}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && (dialog = null)}>
    <form class="dialog" onsubmit={confirm}>
      <h2>{dialog === "new" ? "New project" : "Import from Overleaf"}</h2>
      {#if dialog === "new"}
        <div class="templates" role="radiogroup" aria-label="Template">
          {#each templates as t (t.id)}
            <button type="button" role="radio" aria-checked={chosen === t.id} class:on={chosen === t.id} onclick={() => (chosen = t.id)}>
              <b>{t.name}</b><span>{t.description}</span>
            </button>
          {/each}
        </div>
      {:else}
        <p class="muted">From <code>{zipPath}</code></p>
      {/if}
      <label for="project-name">Folder name</label>
      <input id="project-name" bind:value={name} required placeholder="e.g. master-thesis" />
      <label for="project-parent">Location</label>
      <div class="row">
        <input id="project-parent" bind:value={parent} required />
        <button type="button" onclick={chooseParent}>Choose…</button>
      </div>
      {#if dialogError}<p class="error">{dialogError}</p>{/if}
      <div class="actions">
        <button type="button" onclick={() => (dialog = null)}>Cancel</button>
        <button class="primary" type="submit" disabled={busy || !name.trim() || !parent}>{dialog === "new" ? "Create" : "Import"}</button>
      </div>
    </form>
  </div>
{/if}

<style>
  .library { height: 100%; overflow: auto; padding: 28px 36px; display: flex; flex-direction: column; gap: 20px; }
  header { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
  h1 { font: 400 30px var(--f-display); margin: 0 auto 0 0; }
  input {
    font: 13px var(--f-body); color: var(--ink); background: var(--sheet);
    border: 1px solid var(--rule); border-radius: 5px; padding: 6px 10px;
  }
  input:focus-visible { outline: 2px solid var(--blue); outline-offset: 1px; }
  #library-search { width: 220px; }
  .error { color: var(--mark); margin: 0; }
  .muted { color: var(--muted); margin: 0; word-break: break-all; }
  .empty { color: var(--muted); max-width: 60ch; font-size: 14px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(190px, 1fr)); gap: 18px; }
  .card { position: relative; }
  .open { width: 100%; padding: 0; overflow: hidden; display: flex; flex-direction: column; text-align: left; font-weight: 400; }
  .thumb { height: 200px; background: var(--chrome-2); display: flex; justify-content: center; align-items: flex-end; padding-top: 14px; overflow: hidden; }
  .thumb img { width: 76%; box-shadow: 0 1px 6px rgba(0, 0, 0, 0.2); background: #fff; align-self: flex-start; }
  .blank { width: 70%; height: 100%; background: #fff; padding: 16px 12px; display: flex; flex-direction: column; gap: 7px; }
  .blank i { display: block; height: 4px; background: #d4d8dc; border-radius: 2px; }
  .blank i:first-child { width: 55%; height: 6px; margin: 0 auto 6px; background: #9aa2aa; }
  .info { padding: 10px 12px; display: flex; flex-direction: column; gap: 2px; border-top: 1px solid var(--rule); width: 100%; }
  .info span { font: 11px var(--f-mono); color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .forget { position: absolute; top: 6px; right: 6px; padding: 0 7px; opacity: 0; }
  .card:hover .forget, .forget:focus-visible { opacity: 1; }

  .backdrop { position: fixed; inset: 0; background: rgba(10, 15, 20, 0.45); display: grid; place-items: center; padding: 24px; z-index: 10; }
  .dialog {
    width: min(620px, 100%); max-height: 100%; overflow: auto; background: var(--sheet); border: 1px solid var(--rule);
    border-radius: 8px; padding: 22px 24px; display: flex; flex-direction: column; gap: 8px;
  }
  h2 { font: 400 22px var(--f-display); margin: 0 0 6px; }
  label { font: 500 11px var(--f-mono); letter-spacing: 0.08em; text-transform: uppercase; color: var(--muted); margin-top: 8px; }
  .templates { display: grid; grid-template-columns: repeat(auto-fill, minmax(170px, 1fr)); gap: 8px; }
  .templates button { text-align: left; display: flex; flex-direction: column; gap: 3px; padding: 10px 12px; font-weight: 400; }
  .templates button.on { border-color: var(--blue); background: var(--blue-soft); }
  .templates span { color: var(--muted); font-size: 12px; }
  .row { display: flex; gap: 8px; }
  .row input { flex: 1; min-width: 0; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 12px; }
</style>
