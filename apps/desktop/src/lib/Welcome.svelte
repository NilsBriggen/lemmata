<script lang="ts">
  // First run: confirm TeX works, a few optional choices, then a way in.
  // Nothing is created unless the user asks; "Skip" is always there.
  import { open } from "@tauri-apps/plugin-dialog";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import Circle from "@lucide/svelte/icons/circle";
  import Plus from "@lucide/svelte/icons/plus";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import FileArchive from "@lucide/svelte/icons/file-archive";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import ArrowRight from "@lucide/svelte/icons/arrow-right";
  import Sun from "@lucide/svelte/icons/sun";
  import Moon from "@lucide/svelte/icons/moon";
  import Monitor from "@lucide/svelte/icons/monitor";
  import Onboarding from "./Onboarding.svelte";
  import NewProjectDialog from "./NewProjectDialog.svelte";
  import ImportDialog from "./ImportDialog.svelte";
  import { settings, updateSettings, DEFAULTS, type Settings } from "./settings.svelte";
  import { LANGUAGES } from "./spell/service";
  import type { TexStatus } from "./api";

  interface Props {
    status: TexStatus;
    checking: boolean;
    onRecheck: (dir?: string) => void;
    onFinish: (how: "example" | "done") => void;
    onOpen: (path: string) => void;
    onImported: (paths: string[]) => void;
  }
  let p: Props = $props();

  let step = $state(0);
  let dialog = $state<null | "new" | { zip: string }>(null);

  const d = $derived(p.status.detection);
  const distribution = $derived(
    d.distribution.name === "texlive" ? `TeX Live ${d.distribution.year ?? ""}` : d.distribution.name === "miktex" ? `MiKTeX ${d.distribution.version ?? ""}` : "TeX",
  );
  const extras: [string, string][] = [
    ["synctex", "Jump between PDF and source"],
    ["biber", "biblatex bibliographies"],
    ["xelatex", "XeLaTeX, for system fonts"],
    ["lualatex", "LuaLaTeX"],
  ];
  const found = (name: string) => d.tools.some((t) => t.name === name && t.path);
  /** Only listed for MiKTeX on Windows, which doesn't ship the Perl latexmk runs on. */
  const needsPerl = $derived(d.tools.some((t) => t.name === "perl") && !found("perl"));

  function set<K extends keyof Settings>(k: K, v: Settings[K]) {
    updateSettings({ [k]: v } as Partial<Settings>);
  }

  async function openFolder() {
    const dir = await open({ directory: true, title: "Open a LaTeX project folder" });
    if (typeof dir === "string") p.onOpen(dir);
  }

  async function importZip() {
    const zip = await open({ title: "Choose a zip downloaded from Overleaf", filters: [{ name: "Zip", extensions: ["zip"] }] });
    if (typeof zip === "string") dialog = { zip };
  }
</script>

{#if !d.ready}
  <!-- TeX is missing: the install guide is the first step. -->
  <Onboarding status={p.status} checking={p.checking} onRecheck={p.onRecheck} />
{:else}
  <div class="welcome">
    <div class="top">
      <ol class="dots" aria-label="Steps">
        {#each ["Welcome", "Setup", "Start"] as label, i (label)}
          <li class:on={i === step} class:done={i < step} aria-current={i === step ? "step" : undefined}>{label}</li>
        {/each}
      </ol>
      <button class="btn ghost skip" onclick={() => p.onFinish("done")}>Skip</button>
    </div>

    <div class="card">
      {#if step === 0}
        <h1>Welcome to Lemmata</h1>
        <p class="lede">Write LaTeX on the left, read the PDF on the right, all on your own computer. Your projects stay ordinary folders.</p>
        <div class="tex">
          <div class="ok"><CircleCheck size={20} /> <span><b>{distribution}</b> is ready{#if d.bin_dir}{" "}<span class="where">in {d.bin_dir}</span>{/if}</span></div>
          {#if needsPerl}
            <p class="perl">
              <b>Perl is missing.</b> latexmk needs it and MiKTeX doesn't include it. Run
              <code>winget install StrawberryPerl.StrawberryPerl</code> in a terminal, then restart Lemmata.
            </p>
          {/if}
          <ul>
            {#each extras as [tool, what] (tool)}
              <li class:missing={!found(tool)}>
                {#if found(tool)}<CircleCheck size={15} />{:else}<Circle size={15} />{/if}
                <span>{what}</span>
                {#if !found(tool)}<i>not installed, optional</i>{/if}
              </li>
            {/each}
          </ul>
        </div>
        <div class="actions"><button class="btn primary lg" onclick={() => (step = 1)}>Continue <ArrowRight size={16} /></button></div>
      {:else if step === 1}
        <h1>Make it yours</h1>
        <p class="lede">All optional, and you can change them later in Preferences.</p>
        <div class="settings">
          <div class="setting">
            <span class="label">Appearance</span>
            <div class="segmented" role="group" aria-label="Theme">
              <button aria-pressed={settings.theme === "system"} onclick={() => set("theme", "system")}><Monitor size={15} /> System</button>
              <button aria-pressed={settings.theme === "light"} onclick={() => set("theme", "light")}><Sun size={15} /> Light</button>
              <button aria-pressed={settings.theme === "dark"} onclick={() => set("theme", "dark")}><Moon size={15} /> Dark</button>
            </div>
          </div>
          <div class="setting">
            <span class="label">Compile</span>
            <div class="segmented" role="group" aria-label="When to compile">
              <button aria-pressed={settings.autosaveMs > 0} onclick={() => set("autosaveMs", DEFAULTS.autosaveMs)}>As I type</button>
              <button aria-pressed={settings.autosaveMs === 0} onclick={() => set("autosaveMs", 0)}>With Ctrl+S</button>
            </div>
          </div>
          <div class="setting">
            <label class="label" for="w-spell">Spell check</label>
            <select id="w-spell" class="input" value={settings.spellDefault} onchange={(e) => set("spellDefault", e.currentTarget.value)}>
              <option value="auto">Match the document's language</option>
              {#each Object.entries(LANGUAGES) as [code, l] (code)}<option value={code}>{l.label}</option>{/each}
              <option value="off">Off</option>
            </select>
          </div>
          <div class="setting">
            <span class="label">Keyboard</span>
            <div class="segmented" role="group" aria-label="Keyboard">
              <button aria-pressed={settings.keymap === "default"} onclick={() => set("keymap", "default")}>Standard</button>
              <button aria-pressed={settings.keymap === "vim"} onclick={() => set("keymap", "vim")}>Vim</button>
            </div>
          </div>
        </div>
        <div class="actions">
          <button class="btn ghost" onclick={() => (step = 0)}>Back</button>
          <button class="btn primary lg" onclick={() => (step = 2)}>Continue <ArrowRight size={16} /></button>
        </div>
      {:else}
        <h1>Start writing</h1>
        <p class="lede">Pick up an existing project, or begin a new one.</p>
        <div class="starts">
          <button class="start" onclick={() => (dialog = "new")}>
            <span class="ico"><Plus size={20} /></span><b>New project</b><span>From a template: article, thesis, slides, CV, letter, homework, or your own.</span>
          </button>
          <button class="start" onclick={openFolder}>
            <span class="ico"><FolderOpen size={20} /></span><b>Open folder</b><span>Any folder with a .tex file.</span>
          </button>
          <button class="start" onclick={importZip}>
            <span class="ico"><FileArchive size={20} /></span><b>Import from Overleaf</b><span>One project, or several at once.</span>
          </button>
        </div>
        <button class="example" onclick={() => p.onFinish("example")}>
          <Sparkles size={15} /> New to LaTeX or Lemmata? <u>Open an example document</u>
        </button>
        <div class="actions">
          <button class="btn ghost" onclick={() => (step = 1)}>Back</button>
          <button class="btn" onclick={() => p.onFinish("done")}>Go to my projects</button>
        </div>
      {/if}
    </div>
  </div>
{/if}

{#if dialog === "new"}
  <NewProjectDialog onCreated={(path) => ((dialog = null), p.onOpen(path))} onClose={() => (dialog = null)} />
{:else if dialog}
  <ImportDialog zip={dialog.zip} onImported={(paths) => ((dialog = null), p.onImported(paths))} onClose={() => (dialog = null)} />
{/if}

<style>
  .welcome {
    height: 100%; overflow: auto; display: flex; flex-direction: column; align-items: center; padding: 28px 24px 40px;
    background:
      radial-gradient(1200px 500px at 50% -10%, color-mix(in srgb, var(--accent) 12%, transparent), transparent 70%),
      var(--app);
  }
  .top { width: min(720px, 100%); display: flex; align-items: center; justify-content: space-between; margin-bottom: 8vh; }
  .dots { list-style: none; margin: 0; padding: 0; display: flex; gap: 22px; counter-reset: s; }
  .dots li { counter-increment: s; display: flex; align-items: center; gap: 8px; color: var(--faint); font-weight: 500; }
  .dots li::before {
    content: counter(s); width: 22px; height: 22px; border-radius: 50%; display: grid; place-items: center;
    border: 1.5px solid currentColor; font: 600 11px var(--f-body);
  }
  .dots li.on { color: var(--accent); }
  .dots li.done { color: var(--ok); }
  .dots li.done::before { content: "✓"; }
  .skip { color: var(--muted); }

  .card {
    width: min(720px, 100%); background: var(--surface); border: 1px solid var(--border); border-radius: 14px;
    box-shadow: var(--shadow); padding: 36px 40px 28px; display: flex; flex-direction: column; gap: 14px;
    animation: pop 0.18s var(--ease);
  }
  h1 { font: 400 32px var(--f-display); margin: 0; letter-spacing: -0.01em; }
  .lede { color: var(--muted); font-size: 15px; margin: 0 0 6px; max-width: 54ch; line-height: 1.55; }

  .tex { border: 1px solid var(--border); border-radius: var(--r-lg); padding: 14px 16px; display: flex; flex-direction: column; gap: 10px; background: var(--surface-2); }
  .ok { display: flex; align-items: center; gap: 10px; color: var(--ok); }
  .ok span { color: var(--ink); }
  .where { color: var(--muted); font: 12px var(--f-mono); }
  .tex ul { list-style: none; margin: 0; padding: 0 0 0 30px; display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 6px 16px; }
  .tex li { display: flex; align-items: center; gap: 8px; color: var(--ok); font-size: 12.5px; }
  .tex li span { color: var(--ink-2); }
  .tex li.missing { color: var(--faint); }
  .tex li i { color: var(--faint); font-style: normal; font-size: 11.5px; }

  .settings { display: flex; flex-direction: column; gap: 4px; }
  .setting { display: grid; grid-template-columns: 140px 1fr; align-items: center; gap: 12px; padding: 10px 0; border-top: 1px solid var(--border); }
  .setting:first-child { border-top: none; }
  .setting .input { max-width: 300px; }
  .setting .segmented { justify-self: start; }

  .starts { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 10px; }
  .start {
    display: flex; flex-direction: column; align-items: flex-start; gap: 4px; padding: 16px; text-align: left;
    background: var(--surface); border: 1px solid var(--border); border-radius: var(--r-lg); color: var(--ink);
    transition: border-color 0.12s, box-shadow 0.12s, transform 0.12s;
  }
  .start:hover { border-color: var(--accent); box-shadow: var(--shadow); transform: translateY(-1px); }
  .start .ico { display: inline-flex; width: 36px; height: 36px; border-radius: var(--r); place-items: center; justify-content: center; align-items: center; background: var(--accent-soft); color: var(--accent); margin-bottom: 6px; }
  .start span:last-child { color: var(--muted); font-size: 12.5px; line-height: 1.45; }
  .example { align-self: flex-start; display: inline-flex; align-items: center; gap: 8px; border: none; background: none; color: var(--muted); padding: 4px 0; }
  .example u { color: var(--accent); text-underline-offset: 3px; }
  .example :global(svg) { color: var(--accent); }

  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 10px; }
  @media (max-width: 720px) {
    .starts { grid-template-columns: 1fr; }
    .card { padding: 24px; }
  }
.perl { margin: 8px 0 4px; padding: 8px 10px; border-radius: var(--r-sm); background: color-mix(in srgb, var(--warn, #c77d00) 14%, transparent); font-size: 13px; line-height: 1.45; }
  .perl code { font-family: var(--f-mono); user-select: all; }
</style>
