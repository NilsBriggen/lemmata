<script lang="ts">
  // App-wide preferences (lemmata.toml holds the per-project ones).
  import { open } from "@tauri-apps/plugin-dialog";
  import { getVersion } from "@tauri-apps/api/app";
  import Sun from "@lucide/svelte/icons/sun";
  import Moon from "@lucide/svelte/icons/moon";
  import Monitor from "@lucide/svelte/icons/monitor";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import Modal from "./ui/Modal.svelte";
  import SnippetsDialog from "./SnippetsDialog.svelte";
  import { INDENTS, validIndent } from "./editor/indent";
  import { settings, updateSettings, DEFAULTS, type Settings } from "./settings.svelte";
  import { LANGUAGES, spell, downloadNote, type Dictionary } from "./spell/service";
  import type { Detection } from "./api";

  interface Props {
    detection: Detection | null;
    onTexDir: (dir: string | null) => void;
    onCheckUpdates: () => Promise<string>;
    onClose: () => void;
  }
  let { detection, onTexDir, onCheckUpdates, onClose }: Props = $props();
  let snippetsOpen = $state(false);

  let version = $state("");
  let updateMessage = $state<string | null>(null);
  let checking = $state(false);
  getVersion().then((v) => (version = v)).catch(() => {});

  // Spell-check languages that aren't installed are fetched when chosen.
  let dicts = $state<Dictionary[]>([]);
  let downloading = $state<string | null>(null);
  let spellError = $state<string | null>(null);
  spell.dictionaries().then((d) => (dicts = d));

  async function chooseSpell(select: HTMLSelectElement) {
    const code = select.value;
    spellError = null;
    const d = dicts.find((x) => x.code === code);
    if (!d || d.installed) return set("spellDefault", code);
    downloading = code;
    try {
      await spell.download(code);
      dicts = await spell.dictionaries();
      set("spellDefault", code);
    } catch (e) {
      spellError = e instanceof Error ? e.message : String(e);
      select.value = settings.spellDefault;
    } finally {
      downloading = null;
    }
  }

  function set<K extends keyof Settings>(key: K, value: Settings[K]) {
    updateSettings({ [key]: value } as Partial<Settings>);
  }

  async function chooseTex() {
    const dir = await open({ directory: true, title: "Choose the folder that contains pdflatex and latexmk" });
    if (typeof dir === "string") {
      await updateSettings({ texDir: dir });
      onTexDir(dir);
    }
  }

  async function checkNow() {
    checking = true;
    updateMessage = await onCheckUpdates();
    checking = false;
  }

  const distribution = $derived.by(() => {
    const d = detection?.distribution;
    if (!d || d.name === "unknown") return "No TeX distribution found";
    return d.name === "texlive" ? `TeX Live ${d.year ?? ""}` : `MiKTeX ${d.version ?? ""}`;
  });
  let grammarStatus = $state<string | null>(null);
  async function testGrammar() {
    grammarStatus = "checking…";
    const { grammar } = await import("./grammar/service");
    grammarStatus = (await grammar.ping(settings.grammar, settings.grammarUrl)) ?? "connected";
  }
</script>

<Modal title="Preferences" subtitle="These apply to every project." width={600} {onClose}>
  <section>
    <h3>Appearance</h3>
    <div class="row">
      <span>Theme</span>
      <div class="segmented" role="group" aria-label="Theme">
        <button aria-pressed={settings.theme === "system"} onclick={() => set("theme", "system")}><Monitor size={15} /> System</button>
        <button aria-pressed={settings.theme === "light"} onclick={() => set("theme", "light")}><Sun size={15} /> Light</button>
        <button aria-pressed={settings.theme === "dark"} onclick={() => set("theme", "dark")}><Moon size={15} /> Dark</button>
      </div>
    </div>
    <div class="row">
      <label for="pref-font">Editor text size</label>
      <select id="pref-font" class="input" value={String(settings.fontSize)} onchange={(e) => set("fontSize", Number(e.currentTarget.value))}>
        {#each [11, 12, 13, 14, 15, 16, 18, 20] as n (n)}<option value={String(n)}>{n} px</option>{/each}
      </select>
    </div>
    <label class="check" for="pref-pdfdark">
      <input id="pref-pdfdark" type="checkbox" checked={settings.pdfDark} onchange={(e) => set("pdfDark", e.currentTarget.checked)} />
      Dark PDF pages when the app is dark
    </label>
  </section>

  <section>
    <h3>Editing</h3>
    <div class="row">
      <span>Save</span>
      <div class="segmented" role="group" aria-label="When to save">
        <button aria-pressed={settings.autosaveMs > 0} onclick={() => set("autosaveMs", DEFAULTS.autosaveMs)}>As I type</button>
        <button aria-pressed={settings.autosaveMs === 0} onclick={() => set("autosaveMs", 0)}>With Ctrl+S</button>
      </div>
    </div>
    <label class="check" for="pref-autocompile">
      <input id="pref-autocompile" type="checkbox" checked={settings.autoCompile} onchange={(e) => set("autoCompile", e.currentTarget.checked)} />
      Compile after every save
    </label>
    <div class="row">
      <span>Snippets</span>
      <button class="btn" onclick={() => (snippetsOpen = true)}>{settings.snippets.length ? `Edit ${settings.snippets.length}…` : "Add…"}</button>
    </div>
    <div class="row">
      <span>Deleted files</span>
      <div class="segmented" role="group" aria-label="Where deleted files go">
        <button aria-pressed={settings.trashMode === "project"} title="Kept inside the project for a week, so Undo can bring them back" onclick={() => set("trashMode", "project")}>Keep for Undo</button>
        <button aria-pressed={settings.trashMode === "system"} title="Sent to your computer's trash" onclick={() => set("trashMode", "system")}>System trash</button>
      </div>
    </div>
    <div class="row">
      <span>Keyboard</span>
      <div class="segmented" role="group" aria-label="Keyboard">
        <button aria-pressed={settings.keymap === "default"} onclick={() => set("keymap", "default")}>Standard</button>
        <button aria-pressed={settings.keymap === "vim"} onclick={() => set("keymap", "vim")}>Vim</button>
      </div>
    </div>
    <div class="row">
      <label for="pref-spell">Spell check</label>
      <select id="pref-spell" class="input" value={settings.spellDefault} disabled={downloading !== null} onchange={(e) => chooseSpell(e.currentTarget)}>
        <option value="auto">Match the document's language</option>
        {#each Object.entries(LANGUAGES) as [code, l] (code)}
          {@const d = dicts.find((x) => x.code === code)}
          <option value={code}>{l.label}{d ? downloadNote(d) : ""}</option>
        {/each}
        <option value="off">Off</option>
      </select>
    </div>
    {#if downloading}
      <p class="note" role="status">Downloading the {LANGUAGES[downloading]?.label} dictionary…</p>
    {:else if spellError}
      <p class="note error" role="alert">{spellError}</p>
    {/if}
    <div class="row">
      <label for="pref-grammar">Grammar check</label>
      <select id="pref-grammar" class="input" value={settings.grammar} onchange={(e) => set("grammar", e.currentTarget.value as "off" | "local" | "custom")}>
        <option value="off">Off</option>
        <option value="local">LanguageTool on this computer</option>
        <option value="custom">LanguageTool at an address…</option>
      </select>
    </div>
    {#if settings.grammar === "custom"}
      <div class="row">
        <label for="pref-grammar-url">Server address</label>
        <input id="pref-grammar-url" class="input mono" value={settings.grammarUrl} onchange={(e) => set("grammarUrl", e.currentTarget.value.trim())} />
      </div>
    {/if}
    {#if settings.grammar !== "off"}
      <p class="note">Your text goes to {settings.grammar === "local" ? "the LanguageTool server on this computer (http://localhost:8081)" : "this server"} and nowhere else. <button class="link" onclick={testGrammar}>Test the connection</button>{#if grammarStatus} — {grammarStatus}{/if}</p>
    {/if}
  </section>

  <section>
    <h3>Editor</h3>
    <label class="check" for="pref-complete">
      <input id="pref-complete" type="checkbox" checked={settings.autocomplete} onchange={(e) => set("autocomplete", e.currentTarget.checked)} />
      Suggest commands, environments, labels and citations while typing
    </label>
    <label class="check" for="pref-env">
      <input id="pref-env" type="checkbox" checked={settings.envSync} onchange={(e) => set("envSync", e.currentTarget.checked)} />
      Rename \end{"{…}"} when \begin{"{…}"} changes, and the other way round
    </label>
    <label class="check" for="pref-close">
      <input id="pref-close" type="checkbox" checked={settings.autoClose} onchange={(e) => set("autoClose", e.currentTarget.checked)} />
      Close brackets and dollar signs automatically
    </label>
    <label class="check" for="pref-fold">
      <input id="pref-fold" type="checkbox" checked={settings.folding} onchange={(e) => set("folding", e.currentTarget.checked)} />
      Code folding for sections and environments
    </label>
    <div class="row">
      <label for="pref-indent">Indentation</label>
      <select id="pref-indent" class="input" value={String(settings.indent)} onchange={(e) => set("indent", validIndent(e.currentTarget.value === "tab" ? "tab" : Number(e.currentTarget.value)))}>
        {#each INDENTS as [value, label] (value)}<option value={String(value)}>{label}</option>{/each}
      </select>
    </div>
    <label class="check" for="pref-wrap">
      <input id="pref-wrap" type="checkbox" checked={settings.wordWrap} onchange={(e) => set("wordWrap", e.currentTarget.checked)} />
      Wrap long lines
    </label>
    <div class="row">
      <label for="pref-family">Font</label>
      <select id="pref-family" class="input" value={settings.fontFamily} onchange={(e) => set("fontFamily", e.currentTarget.value)}>
        <option value="plex">IBM Plex Mono</option>
        <option value="system">System monospace</option>
        {#if !["plex", "system"].includes(settings.fontFamily)}<option value={settings.fontFamily}>{settings.fontFamily}</option>{/if}
        {#each ["JetBrains Mono", "Fira Code", "Cascadia Code", "Source Code Pro", "Menlo", "Consolas"] as f (f)}
          <option value={f}>{f} (if installed)</option>
        {/each}
      </select>
    </div>
    <div class="row">
      <label for="pref-lh">Line height</label>
      <select id="pref-lh" class="input" value={String(settings.lineHeight)} onchange={(e) => set("lineHeight", Number(e.currentTarget.value))}>
        {#each [1.4, 1.5, 1.65, 1.8, 2] as n (n)}<option value={String(n)}>{n}</option>{/each}
      </select>
    </div>
  </section>

  <section>
    <h3>TeX</h3>
    <p class="muted">{distribution}{detection?.bin_dir ? ` in ${detection.bin_dir}` : ""}</p>
    <div class="buttons">
      <button class="btn" onclick={chooseTex}><FolderOpen size={15} /> Choose TeX folder…</button>
      {#if settings.texDir}
        <button class="btn ghost" onclick={async () => { await updateSettings({ texDir: null }); onTexDir(null); }}>Find automatically</button>
      {/if}
    </div>
  </section>

  <section>
    <h3>Updates</h3>
    <label class="check" for="pref-updates">
      <input id="pref-updates" type="checkbox" checked={settings.checkForUpdates} onchange={(e) => set("checkForUpdates", e.currentTarget.checked)} />
      Check for updates automatically
    </label>
    <div class="buttons">
      <button class="btn" disabled={checking} onclick={checkNow}>{checking ? "Checking…" : "Check now"}</button>
      {#if updateMessage}<span class="muted">{updateMessage}</span>{/if}
    </div>
  </section>

  {#snippet footer()}
    <span class="version">Lemmata {version}</span>
    <button class="btn primary" onclick={onClose}>Done</button>
  {/snippet}
</Modal>

{#if snippetsOpen}
  <SnippetsDialog onClose={() => (snippetsOpen = false)} />
{/if}

<style>
  section { display: flex; flex-direction: column; gap: 10px; padding-bottom: 14px; border-bottom: 1px solid var(--border); }
  section:last-of-type { border-bottom: none; padding-bottom: 0; }
  h3 { font: 600 11px var(--f-body); letter-spacing: 0.08em; text-transform: uppercase; color: var(--muted); margin: 4px 0 0; }
  .row { display: grid; grid-template-columns: 140px 1fr; align-items: center; gap: 12px; }
  .row .input { max-width: 300px; }
  .row .segmented { justify-self: start; }
  .buttons { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .muted { color: var(--muted); margin: 0; }
  .version { color: var(--faint); font-size: 12px; margin-right: auto; }
  .note { margin: 0; color: var(--muted); font-size: 12.5px; }
  .note.error { color: var(--error); }
  .link { border: none; background: none; padding: 0; color: var(--accent); text-decoration: underline; cursor: pointer; font: inherit; }
</style>
