<script lang="ts">
  // New project from a template: pick one, name the folder, choose where.
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { documentDir } from "@tauri-apps/api/path";
  import FileText from "@lucide/svelte/icons/file-text";
  import GraduationCap from "@lucide/svelte/icons/graduation-cap";
  import BookOpen from "@lucide/svelte/icons/book-open";
  import Presentation from "@lucide/svelte/icons/presentation";
  import PanelTop from "@lucide/svelte/icons/layout-panel-top";
  import Mail from "@lucide/svelte/icons/mail";
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

  /** The category whose templates are shown; null shows the categories. */
  let shown = $state<string | null>(null);
  /** The template collection: being fetched, or why it couldn't be. */
  let online = $state<{ loading: boolean; error: string | null }>({ loading: true, error: null });

  // A big button per everyday kind of document, a few templates each. The user's own templates
  // and the publishers' (IEEE, ACM, Springer, …) sit in smaller bars below, keeping the grid at six.
  const ORDER = ["Papers", "School", "Theses & reports", "Presentations", "Posters", "Letters & CV"];
  const ICONS: Record<string, typeof FileText> = {
    Papers: FileText, School: GraduationCap, "Theses & reports": BookOpen,
    Presentations: Presentation, Posters: PanelTop, "Letters & CV": Mail,
  };
  const MINE = "My templates";
  const FOLDER = "Journals & conferences";
  const catOf = (t: Template) => (t.user ? MINE : t.category || "Other");
  const inCat = (c: string) => templates.filter((t) => catOf(t) === c);
  const categories = $derived(
    [...new Set(templates.map(catOf))]
      .filter((c) => c !== FOLDER && c !== MINE)
      .sort((a, b) => (ORDER.indexOf(a) + 1 || 99) - (ORDER.indexOf(b) + 1 || 99)),
  );
  const folderCount = $derived(inCat(FOLDER).length);
  const mineCount = $derived(inCat(MINE).length);
  /** What's inside, by name: "Article, Preprint, Two-column paper, …". */
  const summary = (list: Template[]) => list.slice(0, 3).map((t) => t.name).join(", ") + (list.length > 3 ? ", …" : "");

  function show(c: string) {
    shown = c;
    if (!inCat(c).some((t) => t.id === chosen)) chosen = inCat(c)[0]?.id ?? chosen;
  }
  /** A template chosen by a link opens its category. */
  const showChosen = () => {
    const t = templates.find((x) => x.id === chosen);
    if (t) shown = catOf(t);
  };

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
      if (shown && !inCat(shown).length) shown = null;
      if (chosen === t.id) chosen = (shown && inCat(shown)[0]?.id) || "article";
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

<Modal title="New project" subtitle="Start from a template. It's a normal folder you can open in any editor." width={720} {onClose}>
  <form id="new-project" onsubmit={create}>
    <div class="gallery">
      {#if shown === null}
        <div class="cats">
          {#each categories as c (c)}
            {@const Icon = ICONS[c] ?? FileText}
            <button type="button" class="cat" onclick={() => show(c)}>
              <span class="ico"><Icon size={20} /></span>
              <b>{c}</b>
              <span class="desc">{summary(inCat(c))}</span>
            </button>
          {/each}
        </div>
        {#if mineCount}
          <button type="button" class="folder" onclick={() => show(MINE)}>
            <Star size={18} />
            <span><b>{MINE}</b><span class="desc">{summary(inCat(MINE))}</span></span>
          </button>
        {/if}
        {#if folderCount}
          <button type="button" class="folder" onclick={() => show(FOLDER)}>
            <FolderOpen size={18} />
            <span><b>{FOLDER}</b><span class="desc">IEEE, ACM, Springer, Elsevier and more: {folderCount} templates with a publisher's own class</span></span>
          </button>
        {/if}
        {#if online.loading}
          <p class="online">Getting more templates…</p>
        {:else if online.error}
          <p class="online" title={online.error}>More templates are online, but they couldn't be loaded right now.</p>
        {/if}
      {:else}
        <div class="head">
          <button type="button" class="back" onclick={() => (shown = null)}><ArrowLeft size={14} /> All categories</button>
          <h3>{shown}</h3>
        </div>
        {#if shown === FOLDER}<p class="hint">Templates with a publisher's own class, for submitting to their journals and conferences.</p>{/if}
        <div class="templates" role="radiogroup" aria-label="Template">
          {#each inCat(shown) as t (t.id)}{@render card(t)}{/each}
        </div>
      {/if}
    </div>
    <div class="field">
      <label for="np-name">Folder name</label>
      <!-- svelte-ignore a11y_autofocus -->
      <input id="np-name" class="input" bind:value={name} required placeholder="master-thesis" autofocus />
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
    <button class="btn primary" type="submit" form="new-project" disabled={busy || shown === null || !name.trim() || !parent}>Create project</button>
  {/snippet}
</Modal>

<style>
  form { display: flex; flex-direction: column; gap: 16px; }
  /* Sized by its content: a fixed height squeezed the fields below on short windows. */
  .gallery { flex: none; display: flex; flex-direction: column; gap: 8px; padding: 2px; }
  .cats { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 8px; }
  .cat {
    display: flex; flex-direction: column; align-items: flex-start; gap: 4px; padding: 14px; text-align: left; cursor: pointer;
    background: var(--surface); border: 1px solid var(--border); border-radius: var(--r-lg); color: var(--ink);
    transition: border-color 0.12s, box-shadow 0.12s;
  }
  .cat:hover { border-color: var(--accent); }
  .ico { display: inline-flex; color: var(--accent); margin-bottom: 4px; }
  .folder { display: flex; align-items: center; gap: 10px; width: 100%; padding: 8px 12px; text-align: left; border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--surface); color: var(--ink); cursor: pointer; }
  .folder:hover { border-color: var(--border-strong); }
  .folder :global(svg) { color: var(--accent); flex: none; }
  .folder > span { display: flex; flex-direction: column; gap: 1px; }
  .head { display: flex; align-items: center; gap: 10px; }
  h3 { margin: 0; font-size: 14px; font-weight: 600; }
  .hint { margin: 0; color: var(--muted); }
  .back { display: inline-flex; align-items: center; gap: 4px; padding: 2px 6px; border: 0; background: none; color: var(--accent); cursor: pointer; border-radius: var(--r-sm); }
  .back:hover { background: var(--surface-2); }
  .templates { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 10px; align-content: start; padding: 2px; }
  .online { margin: 0; font-size: 12px; color: var(--muted); }
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
  .row .input { flex: 1; min-width: 0; }
  .error { color: var(--error); margin: 0; }
</style>
