<script lang="ts">
  // New project from a template: pick one, name the folder, choose where.
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { documentDir } from "@tauri-apps/api/path";
  import FileText from "@lucide/svelte/icons/file-text";
  import Search from "@lucide/svelte/icons/search";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
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
  let category = $state("All");
  /** The template collection: being fetched, or why it couldn't be. */
  let online = $state<{ loading: boolean; error: string | null }>({ loading: true, error: null });

  const ORDER = ["Papers", "Theses & books", "Presentations", "Posters", "Letters & CV", "Teaching", "Basics", "My templates"];
  const catOf = (t: Template) => t.category || "Other";
  const categories = $derived(
    [...new Set(templates.map(catOf))].sort((a, b) => (ORDER.indexOf(a) + 1 || 99) - (ORDER.indexOf(b) + 1 || 99)),
  );
  // Every word of the query must appear in the name, description, tags or category.
  const shown = $derived.by(() => {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    return templates.filter((t) => {
      if (category !== "All" && catOf(t) !== category) return false;
      const hay = [t.name, t.description, catOf(t), ...(t.tags ?? [])].join(" ").toLowerCase();
      return words.every((w) => hay.includes(w));
    });
  });

  onMount(async () => {
    templates = await api.listTemplates().catch(() => []);
    chosen = linked(templates) ?? templates[0]?.id ?? "article";
    parent = defaultParent || (await documentDir().catch(() => ""));
    // The collection's templates arrive in the background; ones seen before are already listed.
    try {
      await api.refreshTemplates();
      templates = await api.listTemplates();
      chosen = (initialTemplate && linked(templates)) || chosen;
      online = { loading: false, error: null };
    } catch (err) {
      online = { loading: false, error: errorMessage(err) };
    }
  });

  async function remove(t: Template) {
    try {
      await api.deleteTemplate(t.id);
      templates = templates.filter((x) => x.id !== t.id);
      if (category !== "All" && !templates.some((x) => catOf(x) === category)) category = "All";
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

<Modal title="New project" subtitle="Start from a template. It's a normal folder you can open in any editor." width={900} {onClose}>
  <form id="new-project" onsubmit={create}>
    <div class="gallery">
      <nav class="cats" aria-label="Categories">
        {#each ["All", ...categories] as c (c)}
          <button type="button" class:on={category === c} aria-pressed={category === c} onclick={() => (category = c)}>
            {c}<span class="n">{c === "All" ? templates.length : templates.filter((t) => catOf(t) === c).length}</span>
          </button>
        {/each}
      </nav>
      <div class="main">
        <label class="search">
          <Search size={15} />
          <input class="input" type="search" bind:value={query} placeholder="Search templates, e.g. ieee, poster, thesis" aria-label="Search templates" />
        </label>
        {#if online.loading}
          <p class="online">Getting more templates…</p>
        {:else if online.error}
          <p class="online" title={online.error}>More templates are online, but they couldn't be loaded right now.</p>
        {/if}
        <div class="templates" role="radiogroup" aria-label="Template">
          {#each shown as t (t.id)}
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
          {:else}
            <p class="none">No template matches "{query}".</p>
          {/each}
        </div>
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
  .gallery { display: flex; gap: 16px; height: min(420px, 52vh); }
  .cats { display: flex; flex-direction: column; gap: 2px; width: 150px; flex: none; overflow-y: auto; }
  .cats button {
    display: flex; justify-content: space-between; align-items: center; gap: 8px; padding: 6px 10px; text-align: left;
    background: transparent; border: 0; border-radius: var(--r-sm); color: var(--muted); cursor: pointer;
  }
  .cats button:hover { background: var(--surface-2); color: var(--ink); }
  .cats button.on { background: color-mix(in srgb, var(--accent) 14%, transparent); color: var(--ink); font-weight: 600; }
  .n { font-size: 11px; color: var(--faint); font-weight: 400; }
  .main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 10px; }
  .search { position: relative; display: block; }
  .search :global(svg) { position: absolute; left: 10px; top: 50%; transform: translateY(-50%); color: var(--faint); pointer-events: none; }
  .search .input { width: 100%; padding-left: 32px; }
  .templates { flex: 1; overflow-y: auto; display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 10px; align-content: start; padding: 2px; }
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
    display: flex; align-items: center; justify-content: center; width: 100%; aspect-ratio: 3 / 4; margin-bottom: 4px;
    background: var(--surface-2); border-radius: var(--r-sm); color: var(--faint); overflow: hidden;
  }
  /* Pages are white whatever the theme; the border keeps them from melting into dark cards. */
  .thumb img { width: 100%; height: 100%; object-fit: contain; background: #fff; }
  .desc { color: var(--muted); font-size: 12px; line-height: 1.4; display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; }
  .row { display: flex; gap: 8px; }
  .grow { flex: 1; }
  .row .input { flex: 1; min-width: 0; }
  .error { color: var(--error); margin: 0; }
</style>
