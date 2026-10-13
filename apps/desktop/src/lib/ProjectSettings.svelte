<script lang="ts">
  // Edits lemmata.toml for the open project.
  import { untrack } from "svelte";
  import type { Config, ProjectInfo } from "./api";

  interface Props {
    project: ProjectInfo;
    onSave: (config: Config) => void;
    onClose: () => void;
  }
  let { project, onSave, onClose }: Props = $props();

  // A working copy; nothing changes until Save.
  let config = $state<Config>(untrack(() => ({ ...project.config, main: project.config.main ?? project.main })));
  const candidates = $derived(Array.from(new Set([project.main, ...project.main_candidates])));

  function save(e: SubmitEvent) {
    e.preventDefault();
    onSave(config);
  }
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onClose()}>
  <form class="dialog" onsubmit={save}>
    <h2>Project settings</h2>
    <p class="muted">Saved in <code>lemmata.toml</code> in the project folder, so they travel with it.</p>

    <label for="ps-main">Main file</label>
    <select id="ps-main" bind:value={config.main}>
      {#each candidates as c (c)}<option value={c}>{c}</option>{/each}
    </select>

    <label for="ps-engine">Engine</label>
    <select id="ps-engine" bind:value={config.engine}>
      <option value="pdflatex">pdfLaTeX: fastest, the usual choice</option>
      <option value="xelatex">XeLaTeX: system fonts and Unicode</option>
      <option value="lualatex">LuaLaTeX: system fonts, Unicode and Lua scripting</option>
    </select>

    <label for="ps-bib">Bibliography tool</label>
    <select id="ps-bib" bind:value={config.bib}>
      <option value="biber">Biber (for biblatex)</option>
      <option value="bibtex">BibTeX (for natbib and classic .bst styles)</option>
    </select>

    <label class="check" for="ps-shell">
      <input id="ps-shell" type="checkbox" bind:checked={config.shell_escape} />
      Allow the document to run programs (shell escape)
    </label>
    {#if config.shell_escape}
      <p class="warn">
        Only turn this on for projects you trust. With shell escape, the document can run any command on your computer.
        Packages like <code>minted</code> and <code>svg</code> need it.
      </p>
    {/if}

    <div class="actions">
      <button type="button" onclick={onClose}>Cancel</button>
      <button class="primary" type="submit">Save</button>
    </div>
  </form>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(10, 15, 20, 0.45); display: grid; place-items: center; padding: 24px; z-index: 10; }
  .dialog {
    width: min(520px, 100%); background: var(--sheet); border: 1px solid var(--rule); border-radius: 8px;
    padding: 22px 24px; display: flex; flex-direction: column; gap: 6px;
  }
  h2 { font: 400 22px var(--f-display); margin: 0; }
  .muted { color: var(--muted); margin: 0 0 6px; }
  label { font: 500 11px var(--f-mono); letter-spacing: 0.08em; text-transform: uppercase; color: var(--muted); margin-top: 8px; }
  label.check { display: flex; gap: 8px; align-items: center; text-transform: none; letter-spacing: 0; font: 13px var(--f-body); color: var(--ink); margin-top: 14px; }
  select { padding: 6px 8px; font-size: 13px; }
  .warn { margin: 4px 0 0; padding: 8px 10px; border-radius: 5px; color: var(--warn); background: color-mix(in srgb, var(--warn) 12%, transparent); }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }
</style>
