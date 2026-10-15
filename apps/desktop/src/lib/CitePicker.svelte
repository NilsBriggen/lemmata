<script lang="ts">
  // Search the project's bibliography and insert a citation, or add a new
  // reference by DOI or arXiv ID.
  import { onMount, tick } from "svelte";
  import { api, errorMessage, type BibEntry } from "./api";
  import { bibApi, type OnlineHit } from "./bib/api";
  import Modal from "./ui/Modal.svelte";
  import BookOpen from "@lucide/svelte/icons/book-open";

  interface Props {
    onInsert: (keys: string[]) => void;
    onClose: () => void;
    /** Saves unsaved edits to .bib files, so adding an entry on disk doesn't lose them. */
    beforeBibChange: () => Promise<void>;
    onBibChanged: (file: string) => void;
    onZotero: () => void;
    /** Opened from a hover card for a key that isn't in any .bib file: look it up online at once. */
    searchOnlineFor?: string;
  }
  let { onInsert, onClose, beforeBibChange, onBibChanged, onZotero, searchOnlineFor = "" }: Props = $props();
  let zotero = $state<"ready" | "starting" | "unavailable">("unavailable");

  let entries = $state<BibEntry[]>([]);
  let files = $state<string[]>([]);
  // svelte-ignore state_referenced_locally
  let query = $state(searchOnlineFor);
  let selected = $state(0);
  let picked = $state<string[]>([]);
  let loading = $state(true);
  let addId = $state("");
  let addFile = $state("refs.bib");
  let adding = $state(false);
  let addMessage = $state<{ ok: boolean; text: string } | null>(null);
  let online = $state<OnlineHit[] | null>(null);
  let searching = $state(false);
  let onlineError = $state("");
  let addingDoi = $state("");
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
    if (searchOnlineFor) searchOnline();
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
      // Ctrl+Enter also means "Recompile" to the app; here it only toggles the entry.
      e.stopPropagation();
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

  // Goes online only on this click, never while typing.
  async function searchOnline() {
    searching = true;
    onlineError = "";
    online = null;
    try {
      online = await bibApi.searchOnline(query);
    } catch (err) {
      onlineError = errorMessage(err);
    } finally {
      searching = false;
    }
  }

  async function addHit(h: OnlineHit) {
    addingDoi = h.doi;
    onlineError = "";
    try {
      await beforeBibChange();
      const added = await api.bibAddReference(h.doi, addFile);
      onBibChanged(added.file);
      onInsert([added.key]);
    } catch (err) {
      onlineError = errorMessage(err);
    } finally {
      addingDoi = "";
    }
  }

  async function addReference(e: SubmitEvent) {
    e.preventDefault();
    adding = true;
    addMessage = null;
    try {
      await beforeBibChange();
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

<Modal title="Insert citation" subtitle="Search your bibliography by author, title, year or key." width={700} top {onClose}>
    <div class="head">
      {#if zotero !== "unavailable"}
        <button class="btn sm" onclick={onZotero} disabled={zotero === "starting"} title="Opens Zotero's citation picker"><BookOpen size={14} />
          {zotero === "starting" ? "Zotero is starting…" : "Cite from Zotero…"}
        </button>
      {/if}
    </div>
    <input
      id="cite-search"
      bind:this={searchEl}
      bind:value={query}
      oninput={() => { selected = 0; online = null; onlineError = ""; }}
      onkeydown={onKey}
      placeholder="Search by author, title, year or key"
      autocomplete="off"
    />
    <ul class="local" bind:this={listEl}>
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
    {#if query.trim() && !loading}
      <div class="online">
        <div class="row">
          <button class="btn sm" class:primary={results.length < 3} onclick={searchOnline} disabled={searching}>{searching ? "Searching…" : "Search online"}</button>
          <span class="muted small">{results.length < 3 ? "Few matches here. " : ""}Sends “{query.trim()}” to Crossref, only when you click.</span>
        </div>
        {#if onlineError}<p class="msg error">{onlineError}</p>{/if}
        {#if online}
          {#if online.length === 0}
            <p class="muted">Crossref found nothing for “{query.trim()}”.</p>
          {:else}
            <ul class="hits">
              {#each online as h (h.doi)}
                <li>
                  <div class="hit">
                    <span class="top"><b>{h.authors || "Unknown author"}</b>{#if h.year}<span class="year">{h.year}</span>{/if}<code>{h.doi}</code></span>
                    <span class="title wrap">{h.title || "(no title)"}{h.venue ? ` · ${h.venue}` : ""}</span>
                  </div>
                  <button class="btn sm" onclick={() => addHit(h)} disabled={!!addingDoi} title="Adds it to {addFile} and inserts the citation">{addingDoi === h.doi ? "Adding…" : "Add"}</button>
                </li>
              {/each}
            </ul>
            <p class="muted small">Add puts the reference in {addFile} (change the file below) and inserts the citation.</p>
          {/if}
        {/if}
      </div>
    {/if}
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
        <button class="btn" type="submit" disabled={adding || !addId.trim()}>{adding ? "Looking up…" : "Add"}</button>
      </div>
      {#if addMessage}<p class:error={!addMessage.ok} class="msg">{addMessage.text}</p>{/if}
      <p class="muted small">Looks the reference up on doi.org. This is the only time Lemmata goes online for references.</p>
    </form>
  
</Modal>

<style>
  .head { display: flex; justify-content: flex-end; margin-top: -6px; }
  .head:empty { display: none; }
  input:not([type="checkbox"]), select {
    font: 13px var(--f-body); color: var(--ink); background: var(--surface); border: 1px solid var(--border-strong);
    border-radius: var(--r); padding: 7px 10px;
  }
  input:not([type="checkbox"]):focus, select:focus { outline: none; border-color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 22%, transparent); }
  #cite-search { font-size: 14px; }
  ul { list-style: none; margin: 0; padding: 0; overflow: auto; min-height: 80px; max-height: 42vh; border: 1px solid var(--border); border-radius: var(--r-lg); }
  li.muted { padding: 10px 12px; }
  .local li button { width: 100%; text-align: left; border: none; border-radius: 0; border-bottom: 1px solid var(--border); background: none; padding: 7px 12px; display: flex; flex-direction: column; gap: 2px; font-weight: 400; }
  .local li button.on { background: var(--accent-soft); }
  .top { display: flex; gap: 8px; align-items: center; }
  .year { color: var(--muted); }
  code { margin-left: auto; font-size: 11px; color: var(--muted); }
  .title { color: var(--muted); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .online { display: flex; flex-direction: column; gap: 6px; }
  .online .row { align-items: center; }
  .hits { max-height: 28vh; }
  .hits li { display: flex; align-items: center; gap: 10px; padding: 6px 12px; border-bottom: 1px solid var(--border); }
  .hit { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .wrap { white-space: normal; }
  .hint { margin: 0; color: var(--muted); font-size: 12px; }
  kbd { font-size: 11px; border: 1px solid var(--border); border-radius: 3px; padding: 0 4px; }
  .link { border: none; background: none; color: var(--accent); padding: 0; text-decoration: underline; font-weight: 400; }
  .add { border-top: 1px solid var(--border); padding-top: 10px; display: flex; flex-direction: column; gap: 6px; }
  .add label { font: 500 11px var(--f-mono); letter-spacing: 0.08em; text-transform: uppercase; color: var(--muted); }
  .row { display: flex; gap: 8px; }
  .row input { flex: 1; min-width: 0; }
  .msg { margin: 0; color: var(--ok); }
  .msg.error { color: var(--error); }
  .muted { color: var(--muted); margin: 0; }
  .small { font-size: 11px; }
</style>
