<script lang="ts">
  // Search the project's bibliography and insert a citation, or add a new
  // reference by DOI or arXiv ID.
  import { onMount, tick } from "svelte";
  import { api, errorMessage, type BibEntry } from "./api";

  interface Props {
    onInsert: (keys: string[]) => void;
    onClose: () => void;
    onBibChanged: (file: string) => void;
    onZotero: () => void;
  }
  let { onInsert, onClose, onBibChanged, onZotero }: Props = $props();
  let zotero = $state<"ready" | "starting" | "unavailable">("unavailable");

  let entries = $state<BibEntry[]>([]);
  let files = $state<string[]>([]);
  let query = $state("");
  let selected = $state(0);
  let picked = $state<string[]>([]);
  let loading = $state(true);
  let addId = $state("");
  let addFile = $state("refs.bib");
  let adding = $state(false);
  let addMessage = $state<{ ok: boolean; text: string } | null>(null);
  let searchEl = $state<HTMLInputElement>();
  let listEl = $state<HTMLUListElement>();

  const results = $derived.by(() => {
    const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
    return entries.filter((e) => {
      const hay = `${e.key} ${e.authors} ${e.title} ${e.year}`.toLowerCase();
      return terms.every((t) => hay.includes(t));
    });
  });

  async function load() {
    try {
      const bib = await api.bibEntries();
      entries = bib.entries;
      files = bib.files;
      if (files.length && !files.includes(addFile)) addFile = files[0];
    } catch (e) {
      addMessage = { ok: false, text: errorMessage(e) };
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    load();
    api.zoteroStatus().then((z) => (zotero = z)).catch(() => {});
    searchEl?.focus();
  });

  async function scrollToSelected() {
    await tick();
    listEl?.querySelector(".on")?.scrollIntoView({ block: "nearest" });
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      selected = Math.min(selected + 1, results.length - 1);
      scrollToSelected();
      e.preventDefault();
    } else if (e.key === "ArrowUp") {
      selected = Math.max(selected - 1, 0);
      scrollToSelected();
      e.preventDefault();
    } else if (e.key === "Enter" && results[selected]) {
      e.preventDefault();
      // Ctrl+Enter adds to the selection; Enter inserts.
      if (e.ctrlKey || e.metaKey) toggle(results[selected].key);
      else onInsert(picked.length ? [...picked] : [results[selected].key]);
    } else if (e.key === "Escape") {
      onClose();
    }
  }

  function toggle(key: string) {
    picked = picked.includes(key) ? picked.filter((k) => k !== key) : [...picked, key];
  }

  async function addReference(e: SubmitEvent) {
    e.preventDefault();
    adding = true;
    addMessage = null;
    try {
      const added = await api.bibAddReference(addId, addFile);
      addMessage = { ok: true, text: `Added ${added.key} to ${added.file}.` };
      addId = "";
      onBibChanged(added.file);
      await load();
      query = added.key;
      selected = 0;
      searchEl?.focus();
    } catch (err) {
      addMessage = { ok: false, text: errorMessage(err) };
    } finally {
      adding = false;
    }
  }
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onClose()}>
  <div class="dialog" role="dialog" aria-labelledby="cite-title">
    <div class="head">
      <h2 id="cite-title">Insert citation</h2>
      {#if zotero !== "unavailable"}
        <button onclick={onZotero} disabled={zotero === "starting"} title="Opens Zotero's citation picker">
          {zotero === "starting" ? "Zotero is starting…" : "Cite from Zotero…"}
        </button>
      {/if}
    </div>
    <input
      id="cite-search"
      bind:this={searchEl}
      bind:value={query}
      oninput={() => (selected = 0)}
      onkeydown={onKey}
      placeholder="Search by author, title, year or key"
      autocomplete="off"
    />
    <ul bind:this={listEl}>
      {#if loading}
        <li class="muted">Reading the bibliography…</li>
      {:else if entries.length === 0}
        <li class="muted">No entries yet. Add one by DOI or arXiv ID below.</li>
      {:else if results.length === 0}
        <li class="muted">Nothing matches “{query}”.</li>
      {/if}
      {#each results.slice(0, 200) as e, i (e.key + e.file)}
        <li>
          <button class:on={i === selected} onclick={() => onInsert([e.key])} onmouseenter={() => (selected = i)}>
            <span class="top">
              <input type="checkbox" checked={picked.includes(e.key)} onclick={(ev) => { ev.stopPropagation(); toggle(e.key); }} aria-label="Select {e.key}" />
              <b>{e.authors || "Unknown author"}</b>{#if e.year}<span class="year">{e.year}</span>{/if}
              <code>{e.key}</code>
            </span>
            <span class="title">{e.title || "(no title)"}</span>
          </button>
        </li>
      {/each}
    </ul>
    <p class="hint">
      <kbd>Enter</kbd> inserts · <kbd>Ctrl</kbd>+<kbd>Enter</kbd> or the checkbox selects several
      {#if picked.length}· <button class="link" onclick={() => onInsert([...picked])}>Insert {picked.length} selected</button>{/if}
    </p>

    <form class="add" onsubmit={addReference}>
      <label for="cite-add">Add a reference by DOI or arXiv ID</label>
      <div class="row">
        <input id="cite-add" bind:value={addId} placeholder="10.1038/nature14539 or arXiv:1707.06347" />
        <select aria-label="Bibliography file" bind:value={addFile}>
          {#each files.length ? files : ["refs.bib"] as f (f)}<option value={f}>{f}</option>{/each}
        </select>
        <button type="submit" disabled={adding || !addId.trim()}>{adding ? "Looking up…" : "Add"}</button>
      </div>
      {#if addMessage}<p class:error={!addMessage.ok} class="msg">{addMessage.text}</p>{/if}
      <p class="muted small">Looks the reference up on doi.org. This is the only time Lemmata goes online for references.</p>
    </form>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(10, 15, 20, 0.45); display: grid; place-items: start center; padding: 10vh 24px 24px; z-index: 10; }
  .dialog {
    width: min(680px, 100%); max-height: 80vh; background: var(--sheet); border: 1px solid var(--rule);
    border-radius: 8px; padding: 18px 20px; display: flex; flex-direction: column; gap: 10px;
  }
  h2 { font: 400 20px var(--f-display); margin: 0; }
  .head { display: flex; justify-content: space-between; align-items: center; gap: 12px; }
  input:not([type="checkbox"]), select {
    font: 13px var(--f-body); color: var(--ink); background: var(--sheet); border: 1px solid var(--rule);
    border-radius: 5px; padding: 7px 10px;
  }
  #cite-search { font-size: 14px; }
  ul { list-style: none; margin: 0; padding: 0; overflow: auto; min-height: 80px; flex: 1; border: 1px solid var(--rule); border-radius: 6px; }
  li.muted { padding: 10px 12px; }
  li button { width: 100%; text-align: left; border: none; border-radius: 0; border-bottom: 1px solid var(--rule); background: none; padding: 7px 12px; display: flex; flex-direction: column; gap: 2px; font-weight: 400; }
  li button.on { background: var(--blue-soft); }
  .top { display: flex; gap: 8px; align-items: center; }
  .year { color: var(--muted); }
  code { margin-left: auto; font-size: 11px; color: var(--muted); }
  .title { color: var(--muted); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hint { margin: 0; color: var(--muted); font-size: 12px; }
  kbd { font-size: 11px; border: 1px solid var(--rule); border-radius: 3px; padding: 0 4px; }
  .link { border: none; background: none; color: var(--blue); padding: 0; text-decoration: underline; font-weight: 400; }
  .add { border-top: 1px solid var(--rule); padding-top: 10px; display: flex; flex-direction: column; gap: 6px; }
  .add label { font: 500 11px var(--f-mono); letter-spacing: 0.08em; text-transform: uppercase; color: var(--muted); }
  .row { display: flex; gap: 8px; }
  .row input { flex: 1; min-width: 0; }
  .msg { margin: 0; color: var(--ok); }
  .msg.error { color: var(--mark); }
  .muted { color: var(--muted); margin: 0; }
  .small { font-size: 11px; }
</style>
