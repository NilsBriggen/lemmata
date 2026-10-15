<script lang="ts">
  // Every entry of the project's .bib files: find duplicates and unused
  // references, jump to an entry, delete it or fix a shared key.
  import { onMount } from "svelte";
  import { errorMessage } from "./api";
  import { bibApi, type Library, type ManagedEntry } from "./bib/api";
  import { filterEntries, freeKey, sortEntries, validKey, warningsByEntry, type SortBy } from "./bib/manager";
  import Modal from "./ui/Modal.svelte";

  interface Props {
    onClose: () => void;
    /** Show the entry in the editor. The dialog closes after this. */
    onOpen: (file: string, line: number) => void;
    /** Saves unsaved edits first, so changing a .bib file on disk doesn't lose them. */
    beforeChange: () => Promise<void>;
    /** Project-relative paths that changed on disk, so the app can reload them. */
    onChanged: (paths: string[]) => void;
  }
  let { onClose, onOpen, beforeChange, onChanged }: Props = $props();

  // Plain objects, so entries can be looked up by identity.
  let lib = $state.raw<Library | null>(null);
  let loading = $state(true);
  let busy = $state(false);
  let error = $state("");
  let query = $state("");
  let sortBy = $state<SortBy>("key");
  let show = $state<"all" | "duplicates" | "uncited">("all");
  let confirming = $state("");
  let renaming = $state<{ id: string; value: string } | null>(null);
  let expanded = $state<string[]>([]);

  const id = (e: ManagedEntry) => `${e.file}:${e.line}`;
  const warnings = $derived(warningsByEntry(lib?.duplicates ?? []));
  const rows = $derived.by(() => {
    if (!lib) return [];
    const index = new Map(lib.entries.map((e, i) => [e, i]));
    const keep = (e: ManagedEntry) =>
      show === "all" || (show === "duplicates" ? warnings.has(index.get(e)!) : !e.cited);
    return sortEntries(filterEntries(lib.entries, query), sortBy)
      .filter(keep)
      .map((e) => ({ e, i: index.get(e)! }));
  });
  const uncited = $derived(lib?.entries.filter((e) => !e.cited).length ?? 0);

  async function load() {
    try {
      lib = await bibApi.library();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  onMount(async () => {
    // Line numbers must match what is on disk.
    try {
      await beforeChange();
    } catch (e) {
      error = errorMessage(e);
    }
    await load();
  });

  async function change(run: () => Promise<string[]>) {
    busy = true;
    error = "";
    try {
      await beforeChange();
      onChanged(await run());
      confirming = "";
      renaming = null;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      // Lines moved (or the entry was stale): show what is on disk now.
      await load();
      busy = false;
    }
  }

  const remove = (e: ManagedEntry) => change(() => bibApi.removeEntry(e.file, e.key, e.line));

  function startRename(e: ManagedEntry) {
    const taken = lib!.entries.map((x) => x.key);
    renaming = { id: id(e), value: freeKey(e.key, taken) };
    confirming = "";
  }

  function rename(e: ManagedEntry) {
    const value = renaming!.value.trim();
    if (!validKey(value)) {
      error = "A key can't be empty or contain spaces, commas, braces or quotes.";
      return;
    }
    change(() => bibApi.renameKey(e.file, e.key, e.line, value));
  }

  function describe(kind: string, others: number[]) {
    const names = others.map((j) => lib!.entries[j]).map((o) => `${o.key} (${o.file}:${o.line})`).join(", ");
    return kind === "key" ? `Same key as ${names}` : kind === "doi" ? `Same DOI as ${names}` : `Same title as ${names}`;
  }

  function toggleDetails(e: ManagedEntry) {
    expanded = expanded.includes(id(e)) ? expanded.filter((x) => x !== id(e)) : [...expanded, id(e)];
  }
</script>

<Modal title="Bibliography" subtitle="Every reference in your .bib files, with duplicates and unused entries." width={880} {onClose}>
  {#if loading}
    <p class="muted">Reading the bibliography…</p>
  {:else if lib}
    <p class="summary">
      {lib.entries.length} {lib.entries.length === 1 ? "entry" : "entries"} in {lib.files.length} {lib.files.length === 1 ? "file" : "files"}
      · {lib.duplicates.length} possible {lib.duplicates.length === 1 ? "duplicate" : "duplicates"}
      · {uncited} not cited
    </p>
    {#if lib.missing.length}
      <p class="msg error">Cited but not in any .bib file: <code>{lib.missing.join(", ")}</code></p>
    {/if}
    <div class="tools">
      <input bind:value={query} placeholder="Search by author, title, year, key or DOI" aria-label="Search the bibliography" autocomplete="off" />
      <select bind:value={show} aria-label="Show">
        <option value="all">All entries</option>
        <option value="duplicates">Duplicates</option>
        <option value="uncited">Not cited</option>
      </select>
      <select bind:value={sortBy} aria-label="Sort by">
        <option value="key">Sort by key</option>
        <option value="author">Sort by author</option>
        <option value="year">Sort by year</option>
      </select>
    </div>
    {#if error}<p class="msg error">{error}</p>{/if}
    <ul>
      {#if lib.entries.length === 0}
        <li class="muted pad">No entries yet. Add references from the citation picker.</li>
      {:else if rows.length === 0}
        <li class="muted pad">Nothing matches.</li>
      {/if}
      {#each rows as { e, i } (id(e))}
        <li>
          <div class="main">
            <span class="top">
              <b>{e.authors || "Unknown author"}</b>{#if e.year}<span class="year">{e.year}</span>{/if}
              <code>{e.key}</code>
            </span>
            <span class="title">{e.title || "(no title)"}</span>
            <span class="meta">
              {e.file}:{e.line} · {e.kind}
              {#if !e.cited}<span class="tag">Not cited</span>{/if}
              {#each warnings.get(i) ?? [] as w}<span class="tag warn" title={describe(w.kind, w.others)}>{w.kind === "key" ? "Duplicate key" : w.kind === "doi" ? "Same DOI" : "Same title"}</span>{/each}
            </span>
            {#each warnings.get(i) ?? [] as w}<span class="why">{describe(w.kind, w.others)}</span>{/each}
            {#if expanded.includes(id(e))}
              <dl>
                {#each Object.entries(e.fields) as [name, value] (name)}<dt>{name}</dt><dd>{value}</dd>{/each}
              </dl>
            {/if}
            {#if renaming?.id === id(e)}
              <form class="rename" onsubmit={(ev) => { ev.preventDefault(); rename(e); }}>
                <input bind:value={renaming.value} aria-label="New key" />
                <button class="btn sm" type="submit" disabled={busy}>Rename</button>
                <button class="btn sm ghost" type="button" onclick={() => (renaming = null)}>Cancel</button>
                <span class="muted small">Citations in your .tex files are not changed.</span>
              </form>
            {/if}
          </div>
          <div class="actions">
            <button class="btn sm" onclick={() => { onOpen(e.file, e.line); onClose(); }}>Open</button>
            <button class="btn sm ghost" onclick={() => toggleDetails(e)}>{expanded.includes(id(e)) ? "Hide fields" : "Fields"}</button>
            {#if (warnings.get(i) ?? []).some((w) => w.kind === "key")}
              <button class="btn sm" onclick={() => startRename(e)} disabled={busy}>Rename key…</button>
            {/if}
            {#if confirming === id(e)}
              <button class="btn sm danger" onclick={() => remove(e)} disabled={busy}>Delete entry</button>
              <button class="btn sm ghost" onclick={() => (confirming = "")}>Keep</button>
            {:else}
              <button class="btn sm ghost" onclick={() => (confirming = id(e))} disabled={busy}>Delete…</button>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  {:else if error}
    <p class="msg error">{error}</p>
  {/if}
</Modal>

<style>
  input, select {
    font: 13px var(--f-body); color: var(--ink); background: var(--surface); border: 1px solid var(--border-strong);
    border-radius: var(--r); padding: 7px 10px;
  }
  input:focus, select:focus { outline: none; border-color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 22%, transparent); }
  .tools { display: flex; gap: 8px; }
  .tools input { flex: 1; min-width: 0; }
  .summary { margin: 0; color: var(--muted); }
  ul { list-style: none; margin: 0; padding: 0; overflow: auto; min-height: 80px; max-height: 52vh; border: 1px solid var(--border); border-radius: var(--r-lg); }
  li { display: flex; gap: 12px; align-items: flex-start; padding: 8px 12px; border-bottom: 1px solid var(--border); }
  li:last-child { border-bottom: none; }
  .main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .top { display: flex; gap: 8px; align-items: baseline; }
  .year { color: var(--muted); }
  code { margin-left: auto; font-size: 11px; color: var(--muted); }
  .title { color: var(--muted); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { font-size: 11px; color: var(--muted); display: flex; gap: 6px; flex-wrap: wrap; align-items: center; }
  .tag { border: 1px solid var(--border-strong); border-radius: 999px; padding: 0 7px; }
  .tag.warn { color: var(--error); border-color: var(--error); }
  .why { font-size: 11px; color: var(--error); }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: 2px 10px; margin: 4px 0 0; font-size: 12px; }
  dt { color: var(--muted); font-family: var(--f-mono); }
  dd { margin: 0; overflow-wrap: anywhere; }
  .rename { display: flex; gap: 6px; align-items: center; margin-top: 4px; flex-wrap: wrap; }
  .actions { display: flex; gap: 6px; flex-wrap: wrap; justify-content: flex-end; max-width: 270px; }
  .btn.danger { background: var(--error); border-color: var(--error); color: #fff; }
  .msg { margin: 0; color: var(--ok); }
  .msg.error { color: var(--error); }
  .muted { color: var(--muted); margin: 0; }
  .pad { padding: 10px 12px; }
  .small { font-size: 11px; }
</style>
