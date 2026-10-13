<script lang="ts">
  // Edits lemmata.toml for the open project.
  import { untrack } from "svelte";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import Modal from "./ui/Modal.svelte";
  import type { Config, ProjectInfo } from "./api";
  import { LANGUAGES } from "./spell/service";

  interface Props {
    project: ProjectInfo;
    onSave: (config: Config) => void;
    onClose: () => void;
  }
  let { project, onSave, onClose }: Props = $props();

  // A working copy; nothing changes until Save.
  let config = $state<Config>(untrack(() => ({ ...project.config, main: project.config.main ?? project.main })));
  const candidates = $derived(Array.from(new Set([project.main, ...project.main_candidates])));
</script>

<Modal title="Project settings" subtitle="Saved in lemmata.toml in the project folder, so they travel with it." width={540} {onClose}>
  <form id="project-settings" onsubmit={(e) => (e.preventDefault(), onSave(config))}>
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

    <div class="field">
      <label for="ps-spell">Spell check</label>
      <select id="ps-spell" class="input" bind:value={config.spellcheck}>
        <option value={null}>Use my default</option>
        {#each Object.entries(LANGUAGES) as [code, l] (code)}<option value={code}>{l.label}</option>{/each}
        <option value="off">Off</option>
      </select>
      {#if config.words?.length}
        <p class="hint">{config.words.length} word{config.words.length === 1 ? "" : "s"} added to this project's dictionary.</p>
      {/if}
    </div>

    <label class="check" for="ps-shell">
      <input id="ps-shell" type="checkbox" bind:checked={config.shell_escape} />
      Allow the document to run programs (shell escape)
    </label>
    {#if config.shell_escape}
      <div class="warn">
        <TriangleAlert size={16} />
        <p>Only for projects you trust: with shell escape, the document can run any command on your computer. Packages like <code>minted</code> and <code>svg</code> need it.</p>
      </div>
    {/if}
  </form>
  {#snippet footer()}
    <button class="btn" onclick={onClose}>Cancel</button>
    <button class="btn primary" type="submit" form="project-settings">Save</button>
  {/snippet}
</Modal>

<style>
  form { display: flex; flex-direction: column; gap: 14px; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .hint { margin: -6px 0 0; color: var(--muted); font-size: 12px; }
  .field .hint { margin: 2px 0 0; }
  .warn { display: flex; gap: 10px; align-items: flex-start; padding: 10px 12px; border-radius: var(--r); color: var(--warn); background: var(--warn-soft); }
  .warn p { margin: 0; color: var(--ink-2); }
</style>
