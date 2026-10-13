<script lang="ts">
  // New project from a template: pick one, name the folder, choose where.
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { documentDir } from "@tauri-apps/api/path";
  import FileText from "@lucide/svelte/icons/file-text";
  import GraduationCap from "@lucide/svelte/icons/graduation-cap";
  import Presentation from "@lucide/svelte/icons/presentation";
  import IdCard from "@lucide/svelte/icons/id-card";
  import Mail from "@lucide/svelte/icons/mail";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import Modal from "./ui/Modal.svelte";
  import { api, errorMessage, type Template } from "./api";

  interface Props {
    /** A sensible default location, e.g. next to the last project. */
    defaultParent?: string;
    onCreated: (path: string) => void;
    onClose: () => void;
  }
  let { defaultParent = "", onCreated, onClose }: Props = $props();

  let templates = $state<Template[]>([]);
  let chosen = $state("article");
  let name = $state("");
  let parent = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  const icons: Record<string, typeof FileText> = { article: FileText, thesis: GraduationCap, slides: Presentation, cv: IdCard, letter: Mail };

  onMount(async () => {
    templates = await api.listTemplates().catch(() => []);
    chosen = templates[0]?.id ?? "article";
    parent = defaultParent || (await documentDir().catch(() => ""));
  });

  async function chooseParent() {
    const dir = await open({ directory: true, title: "Choose where to put the project", defaultPath: parent || undefined });
    if (typeof dir === "string") parent = dir;
  }

  async function create(e?: SubmitEvent) {
    e?.preventDefault();
    busy = true;
    error = null;
    try {
      onCreated(await api.createProject(chosen, parent, name));
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="New project" subtitle="Start from a template. It's a normal folder you can open in any editor." width={640} {onClose}>
  <form id="new-project" onsubmit={create}>
    <div class="templates" role="radiogroup" aria-label="Template">
      {#each templates as t (t.id)}
        {@const Icon = icons[t.id] ?? FileText}
        <button type="button" role="radio" aria-checked={chosen === t.id} class:on={chosen === t.id} onclick={() => (chosen = t.id)}>
          <span class="ico"><Icon size={20} /></span>
          <b>{t.name}</b>
          <span class="desc">{t.description}</span>
        </button>
      {/each}
    </div>
    <div class="row">
      <div class="field grow">
        <label for="np-name">Folder name</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input id="np-name" class="input" bind:value={name} required placeholder="master-thesis" autofocus />
      </div>
    </div>
    <div class="field">
      <label for="np-parent">Location</label>
      <div class="row">
        <input id="np-parent" class="input mono" bind:value={parent} required />
        <button type="button" class="btn" onclick={chooseParent}><FolderOpen size={15} /> Choose…</button>
      </div>
    </div>
    {#if error}<p class="error">{error}</p>{/if}
  </form>
  {#snippet footer()}
    <button class="btn" onclick={onClose}>Cancel</button>
    <button class="btn primary" type="submit" form="new-project" disabled={busy || !name.trim() || !parent}>Create project</button>
  {/snippet}
</Modal>

<style>
  form { display: flex; flex-direction: column; gap: 16px; }
  .templates { display: grid; grid-template-columns: repeat(auto-fill, minmax(176px, 1fr)); gap: 8px; }
  .templates button {
    display: flex; flex-direction: column; align-items: flex-start; gap: 4px; padding: 12px; text-align: left;
    background: var(--surface); border: 1px solid var(--border); border-radius: var(--r-lg); color: var(--ink);
    transition: border-color 0.12s, box-shadow 0.12s;
  }
  .templates button:hover { border-color: var(--border-strong); }
  .templates button.on { border-color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 18%, transparent); }
  .ico { display: inline-flex; color: var(--accent); margin-bottom: 4px; }
  .desc { color: var(--muted); font-size: 12px; line-height: 1.4; }
  .row { display: flex; gap: 8px; }
  .grow { flex: 1; }
  .row .input { flex: 1; min-width: 0; }
  .error { color: var(--error); margin: 0; }
</style>
