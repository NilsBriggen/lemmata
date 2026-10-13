<script lang="ts">
  // Project-wide search and replace (Ctrl+Shift+F).
  import { onMount } from "svelte";
  import { api, errorMessage, type FileMatches, type SearchOptions } from "./api";

  interface Props {
    onOpen: (path: string, line: number) => void;
    /** Saves open files before replacing and reloads them after. */
    onReplaced: (paths: string[]) => Promise<void>;
    beforeReplace: () => Promise<void>;
    onClose: () => void;
  }
  let { onOpen, onReplaced, beforeReplace, onClose }: Props = $props();

  let query = $state("");
  let replacement = $state("");
  let showReplace = $state(false);
  let regex = $state(false);
  let caseSensitive = $state(false);
  let wholeWord = $state(false);
  let results = $state<FileMatches[]>([]);
  let excluded = $state<Record<string, boolean>>({});
  let error = $state<string | null>(null);
  let message = $state<string | null>(null);
  let input = $state<HTMLInputElement>();
  let timer: ReturnType<typeof setTimeout> | undefined;

  const options = (): SearchOptions => ({ query, regex, case_sensitive: caseSensitive, whole_word: wholeWord });
  const total = $derived(results.reduce((n, f) => n + f.matches.reduce((m, l) => m + l.ranges.length, 0), 0));

  onMount(() => input?.focus());

  export function focus() {
    input?.focus();
    input?.select();
  }

  function search() {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      error = null;
      message = null;
      try {
        results = await api.searchProject(options());
        excluded = {};
      } catch (e) {
        error = errorMessage(e);
        results = [];
      }
    }, 200);
  }

  async function replaceAll() {
    const paths = results.map((f) => f.path).filter((p) => !excluded[p]);
    if (!paths.length) return;
    try {
      await beforeReplace();
      const n = await api.replaceInFiles(options(), replacement, paths);
      await onReplaced(paths);
      message = `Replaced ${n} occurrence${n === 1 ? "" : "s"} in ${paths.length} file${paths.length === 1 ? "" : "s"}.`;
      results = await api.searchProject(options());
    } catch (e) {
      error = errorMessage(e);
    }
  }

  /** The line split around match ranges, for highlighting. */
  function parts(text: string, ranges: [number, number][]) {
    const chars = Array.from(text);
    const out: { t: string; hit: boolean }[] = [];
    let at = 0;
    for (const [a, b] of ranges) {
      if (a > at) out.push({ t: chars.slice(at, a).join(""), hit: false });
      out.push({ t: chars.slice(a, b).join(""), hit: true });
      at = b;
    }
    out.push({ t: chars.slice(at).join(""), hit: false });
    // Trim long leading context so the hit stays visible.
    if (out[0] && !out[0].hit && out[0].t.length > 40) out[0].t = "…" + out[0].t.slice(-30);
    return out;
  }
</script>

<div class="search">
  <div class="bar">
    <b>Search</b>
    <button class="x" aria-label="Close search" onclick={onClose}>×</button>
  </div>
  <div class="fields">
    <div class="row">
      <button class="toggle" aria-label="Show replace" aria-expanded={showReplace} onclick={() => (showReplace = !showReplace)}>{showReplace ? "▾" : "▸"}</button>
      <input id="search-query" bind:this={input} bind:value={query} oninput={search} placeholder="Search the project" spellcheck="false" />
    </div>
    <div class="opts">
      <button class:on={caseSensitive} aria-pressed={caseSensitive} title="Match case" onclick={() => ((caseSensitive = !caseSensitive), search())}>Aa</button>
      <button class:on={wholeWord} aria-pressed={wholeWord} title="Whole words" onclick={() => ((wholeWord = !wholeWord), search())}>ab</button>
      <button class:on={regex} aria-pressed={regex} title="Regular expression" onclick={() => ((regex = !regex), search())}>.*</button>
    </div>
    {#if showReplace}
      <div class="row">
        <input id="search-replace" bind:value={replacement} placeholder={regex ? "Replace ($1 for groups)" : "Replace with"} spellcheck="false" />
        <button disabled={!total} onclick={replaceAll}>Replace all</button>
      </div>
    {/if}
  </div>
  {#if error}<p class="error">{error}</p>{/if}
  {#if message}<p class="msg">{message}</p>{/if}
  {#if query && !error}
    <p class="count">{total} result{total === 1 ? "" : "s"} in {results.length} file{results.length === 1 ? "" : "s"}</p>
  {/if}
  <ul>
    {#each results as f (f.path)}
      <li>
        <div class="file">
          {#if showReplace}
            <input type="checkbox" checked={!excluded[f.path]} onchange={(e) => (excluded[f.path] = !e.currentTarget.checked)} aria-label="Replace in {f.path}" />
          {/if}
          <span>{f.path}</span>
        </div>
        {#each f.matches as m (m.line)}
          <button class="hit" onclick={() => onOpen(f.path, m.line)}>
            <i>{m.line}</i>
            <span>{#each parts(m.text, m.ranges) as p, i (i)}{#if p.hit}<mark>{p.t}</mark>{:else}{p.t}{/if}{/each}</span>
          </button>
        {/each}
      </li>
    {/each}
  </ul>
</div>

<style>
  /* Sits in the dark sidebar, so it uses the chrome colours. */
  .search { display: flex; flex-direction: column; height: 100%; min-height: 0; color: var(--chrome-ink); }
  .bar { display: flex; align-items: center; height: 36px; padding: 0 6px 0 14px; }
  .bar b { font: 600 11px var(--f-body); letter-spacing: 0.08em; text-transform: uppercase; color: var(--chrome-muted); }
  .x { margin-left: auto; border: none; background: none; padding: 2px 6px; color: var(--chrome-muted); border-radius: var(--r-sm); }
  .x:hover { background: var(--chrome-hover); color: var(--chrome-ink); }
  .fields { padding: 2px 10px 8px; display: flex; flex-direction: column; gap: 6px; }
  .row { display: flex; gap: 4px; align-items: center; }
  .row input {
    flex: 1; min-width: 0; height: 28px; font: 12.5px var(--f-body); padding: 0 8px; border-radius: var(--r-sm);
    border: 1px solid var(--chrome-border); background: var(--chrome-3); color: var(--chrome-ink);
  }
  .row input:focus { outline: none; border-color: var(--accent-on-dark); }
  .row input::placeholder { color: var(--chrome-muted); }
  .row > button:not(.toggle) { height: 28px; padding: 0 8px; border-radius: var(--r-sm); border: 1px solid var(--chrome-border); background: var(--chrome-3); color: var(--chrome-ink); font-size: 12px; white-space: nowrap; }
  .toggle { border: none; background: none; padding: 0 2px; color: var(--chrome-muted); }
  .opts { display: flex; gap: 4px; padding-left: 18px; }
  .opts button { font: 11px var(--f-mono); height: 22px; padding: 0 7px; border-radius: var(--r-sm); border: 1px solid var(--chrome-border); background: transparent; color: var(--chrome-muted); }
  .opts button.on { border-color: var(--accent-on-dark); color: var(--accent-on-dark); background: var(--chrome-active); }
  .error { color: var(--error); padding: 0 14px; margin: 4px 0; font-size: 12px; }
  .msg { color: var(--ok); padding: 0 14px; margin: 4px 0; font-size: 12px; }
  .count { color: var(--chrome-muted); font-size: 11.5px; padding: 0 14px; margin: 2px 0 4px; }
  ul { list-style: none; margin: 0; padding: 0 0 12px; overflow: auto; flex: 1; }
  .file { display: flex; gap: 6px; align-items: center; padding: 8px 12px 2px 14px; font: 600 12px var(--f-body); color: var(--chrome-ink); }
  .hit { display: flex; gap: 8px; width: 100%; border: none; border-radius: 0; background: none; text-align: left; padding: 3px 12px 3px 16px; font: 12px var(--f-mono); color: var(--chrome-muted); }
  .hit:hover { background: var(--chrome-hover); color: var(--chrome-ink); }
  .hit i { font-style: normal; color: var(--chrome-muted); min-width: 2.5em; text-align: right; opacity: 0.7; }
  .hit span { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  mark { background: color-mix(in srgb, var(--warn) 40%, transparent); color: var(--chrome-ink); border-radius: 2px; }
</style>
