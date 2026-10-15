<script lang="ts">
  // Edits lemmata.toml for the open project.
  import { untrack } from "svelte";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import Modal from "./ui/Modal.svelte";
  import Plus from "@lucide/svelte/icons/plus";
  import Trash2 from "@lucide/svelte/icons/trash";
  import X from "@lucide/svelte/icons/x";
  import type { Config, ProjectInfo } from "./api";
  import { LANGUAGES } from "./spell/service";

  interface Props {
    project: ProjectInfo;
    /** Shell escape was allowed for this project on this computer. */
    trusted: boolean;
    onSave: (config: Config) => void;
    onClose: () => void;
  }
  let { project, trusted, onSave, onClose }: Props = $props();

  // A working copy; nothing changes until Save.
  let config = $state<Config>(untrack(() => ({ ...project.config, main: project.config.main ?? project.main })));
  const candidates = $derived(Array.from(new Set([project.main, ...project.main_candidates])));

  // Mirrors valid_outdir in crates/lemmata-project: the build folder must stay inside the project.
  const outdirError = $derived.by(() => {
    const d = config.outdir ?? "";
    const parts = d.split(/[\\/]/);
    if (!d.trim()) return "Enter a folder name.";
    if (/^([\\/]|[A-Za-z]:)/.test(d)) return "Use a folder inside the project, not a full path.";
    if (parts.includes("..")) return "The folder must stay inside the project, so remove the \"..\".";
    if (!parts.some((s) => s && s !== ".")) return "Name a folder, not \".\".";
    return null;
  });
  const timeoutValid = $derived(Number.isInteger(config.timeout_secs) && config.timeout_secs! >= 10 && config.timeout_secs! <= 3600);
  // An empty box gives null, which Number.isInteger rejects.
  const invalid = $derived(!!outdirError || !timeoutValid);
</script>

<Modal title="Project settings" subtitle="Saved in lemmata.toml in the project folder, so they travel with it." width={540} {onClose}>
  <form id="project-settings" onsubmit={(e) => (e.preventDefault(), !invalid && onSave(config))}>
    <div class="field">
      <label for="ps-main">Main document</label>
      <select id="ps-main" class="input mono" bind:value={config.main}>
        {#each candidates as c (c)}<option value={c}>{c}</option>{/each}
      </select>
    </div>

    <div class="grid">
      <div class="field">
        <label for="ps-engine">Compiler</label>
        <select id="ps-engine" class="input" bind:value={config.engine}>
          <option value="pdflatex">pdfLaTeX</option>
          <option value="xelatex">XeLaTeX</option>
          <option value="lualatex">LuaLaTeX</option>
          <option value="tectonic">Tectonic (no TeX install)</option>
        </select>
      </div>
      <div class="field">
        <label for="ps-bib">Bibliography</label>
        <select id="ps-bib" class="input" bind:value={config.bib}>
          <option value="biber">Biber (biblatex)</option>
          <option value="bibtex">BibTeX (natbib, .bst)</option>
        </select>
      </div>
    </div>
    <p class="hint">pdfLaTeX is fastest. XeLaTeX and LuaLaTeX can use the fonts installed on your computer.</p>
    {#if config.engine === "tectonic"}
      <p class="hint">Tectonic downloads the packages a document needs on first use, so it needs an internet connection then. It runs BibTeX itself; biblatex with biber is not supported, so choose BibTeX above.</p>
    {/if}

    <div class="grid">
      <div class="field">
        <label for="ps-timeout">Stop a build after N seconds</label>
        <input id="ps-timeout" class="input" type="number" min="10" max="3600" step="1" aria-invalid={!timeoutValid} bind:value={config.timeout_secs} />
        {#if !timeoutValid}<p class="err">Enter a whole number from 10 to 3600.</p>{/if}
      </div>
      <div class="field">
        <label for="ps-outdir">Build folder</label>
        <input id="ps-outdir" class="input mono" aria-invalid={!!outdirError} aria-describedby="ps-outdir-hint" bind:value={config.outdir} />
        {#if outdirError}<p class="err">{outdirError}</p>{/if}
        <p class="hint" id="ps-outdir-hint">Relative to the project, e.g. .lemmata/build. Must stay inside the project.</p>
      </div>
    </div>

    <div class="field">
      <label for="ps-spell">Spell check</label>
      <select id="ps-spell" class="input" bind:value={config.spellcheck}>
        <option value={null}>Use my default</option>
        {#each Object.entries(LANGUAGES) as [code, l] (code)}<option value={code}>{l.label}</option>{/each}
        <option value="off">Off</option>
      </select>
      {#if config.words?.length}
        <details class="list">
          <summary>{config.words.length} word{config.words.length === 1 ? "" : "s"} added to this project's dictionary</summary>
          <ul>
            {#each config.words as w (w)}
              <li>{w}<button type="button" class="btn ghost icon sm" aria-label="Remove {w}" title="Remove from the dictionary" onclick={() => (config.words = config.words!.filter((x) => x !== w))}><X size={13} /></button></li>
            {/each}
          </ul>
        </details>
      {/if}
      {#if config.grammar_ignore?.length}
        <details class="list">
          <summary>{config.grammar_ignore.length} grammar rule{config.grammar_ignore.length === 1 ? "" : "s"} ignored</summary>
          <ul>
            {#each config.grammar_ignore as r (r)}
              <li><code>{r}</code><button type="button" class="btn ghost sm" title="Underline mistakes of this kind again" onclick={() => (config.grammar_ignore = config.grammar_ignore!.filter((x) => x !== r))}>Check again</button></li>
            {/each}
          </ul>
        </details>
      {/if}
    </div>

    <div class="field">
      <span class="label">Build targets</span>
      <p class="hint">Optional. Name more than one main file (a paper and its slides, say) to get a picker in the toolbar.</p>
      {#each config.targets ?? [] as t, i (i)}
        <div class="target">
          <input class="input" bind:value={t.name} placeholder="Name" aria-label="Target name" />
          <select class="input mono" bind:value={t.main} aria-label="Main file of {t.name}">
            {#each candidates as c (c)}<option value={c}>{c}</option>{/each}
          </select>
          <select class="input" value={t.engine ?? ""} onchange={(e) => (t.engine = (e.currentTarget.value || null) as typeof t.engine)} aria-label="Compiler of {t.name}">
            <option value="">Keep compiler</option>
            <option value="pdflatex">pdfLaTeX</option>
            <option value="xelatex">XeLaTeX</option>
            <option value="lualatex">LuaLaTeX</option>
            <option value="tectonic">Tectonic</option>
          </select>
          <button type="button" class="btn icon" aria-label="Remove {t.name}" onclick={() => (config.targets = (config.targets ?? []).filter((_, j) => j !== i))}><Trash2 size={15} /></button>
        </div>
      {/each}
      <button type="button" class="btn sm add" onclick={() => (config.targets = [...(config.targets ?? []), { name: `Target ${(config.targets?.length ?? 0) + 1}`, main: config.main ?? project.main, engine: null }])}><Plus size={14} /> Add target</button>
    </div>

    <label class="check" for="ps-shell">
      <input id="ps-shell" type="checkbox" bind:checked={config.shell_escape} />
      Allow the document to run programs (shell escape)
    </label>
    {#if config.shell_escape && project.config.shell_escape && !trusted}
      <p class="hint">The project's lemmata.toml turns this on, but it isn't allowed on this computer yet, so builds run without it. Save to allow it.</p>
    {/if}
    {#if config.shell_escape}
      <div class="warn">
        <TriangleAlert size={16} />
        <p>Only for projects you trust: with shell escape, the document can run any command on your computer. Packages like <code>minted</code> and <code>svg</code> need it.</p>
      </div>
    {/if}
  </form>
  {#snippet footer()}
    <button class="btn" onclick={onClose}>Cancel</button>
    <button class="btn primary" type="submit" form="project-settings" disabled={invalid}>Save</button>
  {/snippet}
</Modal>

<style>
  form { display: flex; flex-direction: column; gap: 14px; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .target { display: grid; grid-template-columns: 1fr 1.3fr 1fr auto; gap: 6px; margin-bottom: 6px; }
  .add { align-self: flex-start; }
  .hint { margin: -6px 0 0; color: var(--muted); font-size: 12px; }
  .field .hint { margin: 2px 0 0; }
  .list summary { cursor: pointer; color: var(--muted); font-size: 12px; margin-top: 4px; }
  .list ul { list-style: none; margin: 6px 0 0; padding: 0; display: flex; flex-wrap: wrap; gap: 4px; max-height: 140px; overflow: auto; }
  .list li { display: inline-flex; align-items: center; gap: 2px; padding: 0 0 0 8px; border: 1px solid var(--border); border-radius: var(--r-sm); font-size: 12.5px; }
  .err { margin: 2px 0 0; color: var(--error); font-size: 12px; }
  .warn { display: flex; gap: 10px; align-items: flex-start; padding: 10px 12px; border-radius: var(--r); color: var(--warn); background: var(--warn-soft); }
  .warn p { margin: 0; color: var(--ink-2); }
</style>
