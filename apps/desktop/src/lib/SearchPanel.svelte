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
  .search { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .bar { display: flex; align-items: center; padding: 10px 12px 4px 14px; }
  .bar b { font: 500 10px var(--f-mono); letter-spacing: 0.12em; text-transform: uppercase; color: var(--muted); }
  .x { margin-left: auto; border: none; background: none; padding: 0 4px; }
  .fields { padding: 4px 10px; display: flex; flex-direction: column; gap: 6px; }
  .row { display: flex; gap: 4px; }
  .row input { flex: 1; min-width: 0; font: 12px var(--f-mono); padding: 5px 7px; border: 1px solid var(--rule); border-radius: 4px; background: var(--sheet); color: var(--ink); }
  .toggle { border: none; background: none; padding: 0 2px; }
  .opts { display: flex; gap: 4px; padding-left: 18px; }
  .opts button { font: 11px var(--f-mono); padding: 2px 6px; }
  .opts button.on { border-color: var(--blue); color: var(--blue); background: var(--blue-soft); }
  .error { color: var(--mark); padding: 0 14px; margin: 4px 0; font-size: 12px; }
  .msg { color: var(--ok); padding: 0 14px; margin: 4px 0; font-size: 12px; }
  .count { color: var(--muted); font-size: 11px; padding: 0 14px; margin: 4px 0; }
  ul { list-style: none; margin: 0; padding: 0 0 12px; overflow: auto; flex: 1; }
  .file { display: flex; gap: 6px; align-items: center; padding: 6px 12px 2px; font: 500 12px var(--f-mono); }
  .hit { display: flex; gap: 8px; width: 100%; border: none; border-radius: 0; background: none; text-align: left; padding: 2px 12px 2px 22px; font: 11.5px var(--f-mono); font-weight: 400; }
  .hit:hover { background: color-mix(in srgb, var(--blue) 8%, transparent); }
  .hit i { font-style: normal; color: var(--muted); min-width: 2.5em; text-align: right; }
  .hit span { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  mark { background: color-mix(in srgb, var(--warn) 30%, transparent); color: inherit; border-radius: 2px; }
</style>
