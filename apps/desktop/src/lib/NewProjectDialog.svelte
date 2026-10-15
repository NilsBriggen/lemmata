<script lang="ts">
  // New project from a template: pick one, name the folder, choose where.
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { documentDir } from "@tauri-apps/api/path";
  import FileText from "@lucide/svelte/icons/file-text";
  import Search from "@lucide/svelte/icons/search";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import Trash2 from "@lucide/svelte/icons/trash";
  import Star from "@lucide/svelte/icons/star";
  import Modal from "./ui/Modal.svelte";
  import { api, errorMessage, type Template } from "./api";

  interface Props {
    /** A sensible default location, e.g. next to the last project. */
    defaultParent?: string;
    /** From a lemmata://new link: the template to choose, bundled or from the collection. */
    initialTemplate?: string;
    onCreated: (path: string) => void;
    onClose: () => void;
  }
  let { defaultParent = "", initialTemplate = "", onCreated, onClose }: Props = $props();
  /** The id of `initialTemplate` in the list, once it's there (the collection's arrive later). */
  const linked = (list: Template[]) => list.find((t) => t.id === initialTemplate || t.id === `online:${initialTemplate}`)?.id;

  let templates = $state<Template[]>([]);
  let chosen = $state("article");
  let name = $state("");
  let parent = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  let query = $state("");
  /** Showing the folder of publisher templates instead of the everyday sections. */
  let inFolder = $state(false);
  /** The template collection: being fetched, or why it couldn't be. */
  let online = $state<{ loading: boolean; error: string | null }>({ loading: true, error: null });

  // Everyday kinds of document first, a few templates each; the publishers' own templates
  // (IEEE, ACM, Springer, …) are in one folder, since most people never need them.
  const ORDER = ["My templates", "Papers", "School", "Theses & reports", "Presentations", "Posters", "Letters & CV"];
  const FOLDER = "Journals & conferences";
  const catOf = (t: Template) => t.category || "Other";
  const sections = $derived(
    [...new Set(templates.map(catOf))]
      .filter((c) => c !== FOLDER)
      .sort((a, b) => (ORDER.indexOf(a) + 1 || 99) - (ORDER.indexOf(b) + 1 || 99))
      .map((c) => [c, templates.filter((t) => catOf(t) === c)] as const),
  );
  const folder = $derived(templates.filter((t) => catOf(t) === FOLDER));
  // Every word of the query must appear in the name, description, tags or category.
  const matches = $derived.by(() => {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    return templates.filter((t) => {
      const hay = [t.name, t.description, catOf(t), ...(t.tags ?? [])].join(" ").toLowerCase();
      return words.every((w) => hay.includes(w));
    });
  });
  /** A template chosen by a link opens the folder when that's where it is. */
  const showChosen = () => (inFolder = folder.some((t) => t.id === chosen));

  onMount(async () => {
    templates = await api.listTemplates().catch(() => []);
    chosen = linked(templates) ?? templates[0]?.id ?? "article";
    if (initialTemplate) showChosen();
    parent = defaultParent || (await documentDir().catch(() => ""));
    // The collection's templates arrive in the background; ones seen before are already listed.
    try {
      await api.refreshTemplates();
      templates = await api.listTemplates();
      chosen = (initialTemplate && linked(templates)) || chosen;
      if (initialTemplate) showChosen();
      online = { loading: false, error: null };
    } catch (err) {
      online = { loading: false, error: errorMessage(err) };
    }
  });

  async function remove(t: Template) {
    try {
      await api.deleteTemplate(t.id);
      templates = templates.filter((x) => x.id !== t.id);
      if (chosen === t.id) chosen = templates[0]?.id ?? "article";
    } catch (err) {
      error = errorMessage(err);
    }
  }

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

{#snippet card(t: Template)}
  <div class="tpl">
    <button type="button" role="radio" aria-checked={chosen === t.id} class:on={chosen === t.id} onclick={() => (chosen = t.id)}>
      <span class="thumb">
        {#if t.preview}
          <img src={t.preview} alt="" loading="lazy" />
        {:else}
          {#if t.user}<Star size={28} />{:else}<FileText size={28} />{/if}
        {/if}
      </span>
      <b>{t.name}</b>
      <span class="desc">{t.description || (t.user ? "Your template" : "")}</span>
    </button>
    {#if t.user}
      <button type="button" class="del" aria-label="Delete template {t.name}" title="Delete this template" onclick={() => remove(t)}><Trash2 size={14} /></button>
    {/if}
  </div>
{/snippet}

<Modal title="New project" subtitle="Start from a template. It's a normal folder you can open in any editor." width={900} {onClose}>
  <form id="new-project" onsubmit={create}>
    <div class="gallery">
      <label class="search">
        <Search size={15} />
        <input class="input" type="search" bind:value={query} placeholder="Search templates, e.g. thesis, poster, ieee" aria-label="Search templates" />
      </label>
      {#if online.loading}
        <p class="online">Getting more templates…</p>
      {:else if online.error}
        <p class="online" title={online.error}>More templates are online, but they couldn't be loaded right now.</p>
      {/if}
      <div class="list" role="radiogroup" aria-label="Template">
        {#if query.trim()}
          <div class="templates">
            {#each matches as t (t.id)}{@render card(t)}{:else}<p class="none">No template matches "{query}".</p>{/each}
          </div>
        {:else if inFolder}
          <button type="button" class="back" onclick={() => (inFolder = false)}><ArrowLeft size={14} /> All templates</button>
          <h3>{FOLDER}</h3>
          <p class="hint">Templates with a publisher's own class, for submitting to their journals and conferences.</p>
          <div class="templates">{#each folder as t (t.id)}{@render card(t)}{/each}</div>
        {:else}
          {#each sections as [name, list] (name)}
            <h3>{name}</h3>
            <div class="templates">{#each list as t (t.id)}{@render card(t)}{/each}</div>
          {/each}
          {#if folder.length}
            <h3>More</h3>
            <button type="button" class="folder" onclick={() => (inFolder = true)}>
              <FolderOpen size={22} />
              <span><b>{FOLDER}</b><span class="desc">IEEE, ACM, Springer, Elsevier and more: {folder.length} templates with a publisher's own class</span></span>
            </button>
          {/if}
        {/if}
      </div>
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
  .gallery { display: flex; flex-direction: column; gap: 10px; height: min(440px, 54vh); }
  .list { flex: 1; min-height: 0; overflow-y: auto; padding: 2px; }
  h3 { margin: 14px 0 8px; font-size: 12px; font-weight: 600; color: var(--muted); text-transform: uppercase; letter-spacing: 0.04em; }
  .list > h3:first-child, .back + h3 { margin-top: 2px; }
  .hint { margin: -4px 0 10px; color: var(--muted); }
  .back { display: inline-flex; align-items: center; gap: 4px; margin-bottom: 8px; padding: 2px 6px; border: 0; background: none; color: var(--accent); cursor: pointer; border-radius: var(--r-sm); }
  .back:hover { background: var(--surface-2); }
  .folder { display: flex; align-items: center; gap: 12px; width: 100%; padding: 12px 14px; text-align: left; border: 1px solid var(--border); border-radius: var(--r-md); background: var(--surface); color: var(--ink); cursor: pointer; }
  .folder:hover { border-color: var(--border-strong); }
  .folder :global(svg) { color: var(--accent); flex: none; }
  .folder > span { display: flex; flex-direction: column; gap: 2px; }
  .search { position: relative; display: block; }
  .search :global(svg) { position: absolute; left: 10px; top: 50%; transform: translateY(-50%); color: var(--faint); pointer-events: none; }
  .search .input { width: 100%; padding-left: 32px; }
  .templates { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 10px; align-content: start; padding: 2px; }
  .none { grid-column: 1 / -1; color: var(--muted); margin: 8px 0; }
  .online { margin: -4px 0 0; font-size: 12px; color: var(--muted); }
  .tpl { position: relative; display: flex; }
  .tpl > button:first-child { flex: 1; }
  .templates .del { position: absolute; top: 6px; right: 6px; width: 24px; height: 24px; padding: 0; align-items: center; justify-content: center; border-radius: var(--r-sm); border-color: transparent; background: var(--surface); color: var(--faint); }
  .templates .del:hover { color: var(--error); background: var(--surface-2); }
  .templates button {
    display: flex; flex-direction: column; align-items: flex-start; gap: 4px; padding: 8px; text-align: left;
    background: var(--surface); border: 1px solid var(--border); border-radius: var(--r-lg); color: var(--ink);
    transition: border-color 0.12s, box-shadow 0.12s;
  }
  .templates button:hover { border-color: var(--border-strong); }
  .templates button.on { border-color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 18%, transparent); }
  .thumb {
    display: flex; align-items: center; justify-content: center; width: 100%; height: 92px; margin-bottom: 4px;
    background: var(--surface-2); border-radius: var(--r-sm); color: var(--faint); overflow: hidden;
  }
  /* Pages are white whatever the theme; the border keeps them from melting into dark cards. */
  /* The top of page 1 (title, first lines) is what tells templates apart, at a size that fits several rows. */
  .thumb img { width: 100%; height: 100%; object-fit: cover; object-position: top; background: #fff; }
  .desc { color: var(--muted); font-size: 12px; line-height: 1.4; display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; }
  .row { display: flex; gap: 8px; }
  .grow { flex: 1; }
  .row .input { flex: 1; min-width: 0; }
  .error { color: var(--error); margin: 0; }
</style>
