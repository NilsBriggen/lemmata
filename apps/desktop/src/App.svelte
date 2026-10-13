<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage, type Diagnostic, type Engine, type Outcome, type ProjectInfo, type TexStatus } from "./lib/api";
  import Onboarding from "./lib/Onboarding.svelte";
  import Welcome from "./lib/Welcome.svelte";
  import Tree from "./lib/Tree.svelte";
  import Editor from "./lib/Editor.svelte";
  import PdfView from "./lib/PdfView.svelte";
  import Problems from "./lib/Problems.svelte";
  import { recentProjects, rememberProject } from "./lib/recent";

  /** Edits are saved this long after the last keystroke, which starts a build. */
  const SAVE_DELAY_MS = 600;

  let tex = $state<TexStatus | null>(null);
  let checking = $state(false);
  let project = $state<ProjectInfo | null>(null);
  let openError = $state<string | null>(null);
  let recent = $state<string[]>(recentProjects());

  let tabs = $state<string[]>([]);
  let active = $state<string | null>(null);
  let dirty = $state<Record<string, boolean>>({});
  const contents = new Map<string, string>();
  const saveTimers = new Map<string, ReturnType<typeof setTimeout>>();

  let building = $state(false);
  let outcome = $state<Outcome | null>(null);
  let diagnostics = $state<Diagnostic[]>([]);
  let showInfo = $state(false);
  let problemsOpen = $state(true);
  let buildSeq = 0;

  let editor: Editor | undefined = $state();
  let pdf: PdfView | undefined = $state();

  const errors = $derived(diagnostics.filter((d) => d.severity === "error").length);
  const warnings = $derived(diagnostics.filter((d) => d.severity === "warning").length);

  async function recheck(extraDir?: string) {
    checking = true;
    try {
      tex = await api.texStatus(extraDir);
    } finally {
      checking = false;
    }
  }

  async function openProject(path: string) {
    openError = null;
    try {
      project = await api.openProject(path);
    } catch (e) {
      openError = errorMessage(e);
      return;
    }
    recent = rememberProject(project.root);
    tabs = [];
    contents.clear();
    dirty = {};
    diagnostics = [];
    outcome = null;
    await openFile(project.main);
    // Show the existing PDF right away, then rebuild.
    api.readPdf().then((b) => pdf?.load(b)).catch(() => {});
    runBuild();
  }

  async function openFile(path: string) {
    let text = contents.get(path);
    if (text === undefined) {
      try {
        text = await api.readText(path);
      } catch (e) {
        openError = errorMessage(e);
        return;
      }
      contents.set(path, text);
    }
    if (!tabs.includes(path)) tabs = [...tabs, path];
    active = path;
    editor?.open(path, text);
    editor?.showDiagnostics(diagnostics);
  }

  function closeTab(path: string) {
    if (dirty[path]) save(path);
    tabs = tabs.filter((t) => t !== path);
    contents.delete(path);
    editor?.forget(path);
    if (active === path) {
      const next = tabs.at(-1);
      active = null;
      if (next) openFile(next);
    }
  }

  function onChange(path: string, text: string) {
    contents.set(path, text);
    dirty[path] = true;
    clearTimeout(saveTimers.get(path));
    saveTimers.set(path, setTimeout(() => save(path), SAVE_DELAY_MS));
  }

  async function save(path: string) {
    clearTimeout(saveTimers.get(path));
    const text = contents.get(path);
    if (text === undefined || !dirty[path]) return;
    try {
      await api.writeText(path, text);
      dirty[path] = false;
      runBuild();
    } catch (e) {
      openError = errorMessage(e);
    }
  }

  async function saveAll() {
    await Promise.all(Object.keys(dirty).filter((p) => dirty[p]).map(save));
  }

  async function runBuild() {
    const seq = ++buildSeq;
    building = true;
    try {
      const result = await api.build();
      if (seq !== buildSeq || result.status === "cancelled") return;
      outcome = result;
      diagnostics = result.diagnostics;
      editor?.showDiagnostics(diagnostics);
      if (result.status === "failed" && errors > 0) problemsOpen = true;
      if (result.pdf) await pdf?.load(await api.readPdf());
    } catch (e) {
      if (seq === buildSeq) {
        outcome = null;
        diagnostics = [{ severity: "error", kind: { type: "other" }, file: null, line: null, message: errorMessage(e), hint: null }];
      }
    } finally {
      if (seq === buildSeq) building = false;
    }
  }

  async function syncForward(path: string, line: number, column: number) {
    try {
      pdf?.showBoxes(await api.synctexForward(path, line, column));
    } catch (e) {
      console.warn("synctex forward", e);
    }
  }

  async function syncBackward(page: number, x: number, y: number) {
    try {
      const hit = await api.synctexBackward(page, x, y);
      if (!hit || !hit.inside_project) return;
      await openFile(hit.path);
      editor?.gotoLine(hit.line);
    } catch (e) {
      console.warn("synctex backward", e);
    }
  }

  async function gotoProblem(file: string, line: number) {
    await openFile(file);
    editor?.gotoLine(line);
  }

  async function setEngine(engine: Engine) {
    if (!project) return;
    project = await api.setConfig({ ...project.config, engine, main: project.main });
    runBuild();
  }

  function onKey(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key === "s") {
      e.preventDefault();
      saveAll();
    } else if (mod && e.key === "Enter") {
      e.preventDefault();
      saveAll().then(runBuild);
    }
  }

  onMount(async () => {
    await recheck();
    if (tex?.detection.ready && recent[0]) openProject(recent[0]);
  });

  const statusPill = $derived.by(() => {
    if (building) return { cls: "", text: "building…" };
    if (!outcome) return null;
    if (outcome.status === "success") return { cls: "ok", text: `built in ${(outcome.duration_ms / 1000).toFixed(1)} s` };
    return { cls: "err", text: errors ? `${errors} error${errors > 1 ? "s" : ""}` : "build failed" };
  });
</script>

<svelte:window onkeydown={onKey} />

{#if !tex}
  <div class="loading">Looking for TeX…</div>
{:else if !tex.detection.ready}
  <Onboarding status={tex} {checking} onRecheck={recheck} />
{:else if !project}
  <Welcome {recent} error={openError} onOpen={openProject} />
{:else}
  <div class="shell">
    <header>
      <span class="brand">Lemmata</span>
      <span class="crumbs">{project.name}{active ? ` / ${active}` : ""}</span>
      <div class="right">
        {#if tex.detection.outdated}<span class="pill warn" title="Some packages may lack newer features">old TeX Live</span>{/if}
        {#if statusPill}<span class="pill {statusPill.cls}">{statusPill.text}</span>{/if}
        <select aria-label="Engine" value={project.config.engine} onchange={(e) => setEngine(e.currentTarget.value as Engine)}>
          <option value="pdflatex">pdfLaTeX</option>
          <option value="xelatex">XeLaTeX</option>
          <option value="lualatex">LuaLaTeX</option>
        </select>
        <button onclick={() => (project = null)}>Close project</button>
        <button class="primary" onclick={() => saveAll().then(runBuild)} title="Ctrl+Enter">Recompile</button>
      </div>
    </header>

    <div class="panes">
      <nav class="side">
        <div class="h">Project</div>
        <Tree entries={project.tree} {active} main={project.main} onOpen={openFile} />
      </nav>

      <section class="code">
        <div class="tabs">
          {#each tabs as t (t)}
            <div class="tab" class:on={t === active}>
              <button class="name" onclick={() => openFile(t)}>{t.split("/").pop()}{#if dirty[t]}<i>•</i>{/if}</button>
              <button class="x" aria-label="Close {t}" onclick={() => closeTab(t)}>×</button>
            </div>
          {/each}
        </div>
        <div class="cm"><Editor bind:this={editor} {onChange} onSyncRequest={syncForward} /></div>
        {#if openError}<div class="err-bar">{openError} <button onclick={() => (openError = null)}>Dismiss</button></div>{/if}
      </section>

      <section class="preview">
        <div class="pdfwrap"><PdfView bind:this={pdf} onSourceClick={syncBackward} /></div>
        <div class="problems" class:open={problemsOpen}>
          <button class="ph" onclick={() => (problemsOpen = !problemsOpen)}>
            <span>{problemsOpen ? "▾" : "▸"} Problems</span>
            {#if errors}<span class="pill err">{errors}</span>{/if}
            {#if warnings}<span class="pill warn">{warnings}</span>{/if}
          </button>
          {#if problemsOpen}<Problems {diagnostics} bind:showInfo onGoto={gotoProblem} />{/if}
        </div>
      </section>
    </div>
  </div>
{/if}

<style>
  .loading { height: 100%; display: grid; place-items: center; color: var(--muted); }
  .shell { display: flex; flex-direction: column; height: 100%; }
  header {
    display: flex; align-items: center; gap: 14px; padding: 7px 14px;
    background: var(--chrome); border-bottom: 1px solid var(--rule);
  }
  .brand { font: 16px var(--f-display); }
  .crumbs { font: 12px var(--f-mono); color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .right { margin-left: auto; display: flex; gap: 8px; align-items: center; }
  .panes { flex: 1; display: grid; grid-template-columns: 220px minmax(0, 1fr) minmax(0, 1fr); min-height: 0; }
  .side { background: var(--chrome); border-right: 1px solid var(--rule); overflow: auto; padding-bottom: 12px; }
  .h { font: 500 10px var(--f-mono); letter-spacing: 0.12em; text-transform: uppercase; color: var(--muted); padding: 12px 14px 6px; }
  .code { display: flex; flex-direction: column; min-width: 0; border-right: 1px solid var(--rule); background: var(--code-bg); }
  .tabs { display: flex; background: var(--chrome); border-bottom: 1px solid var(--rule); overflow-x: auto; flex: none; }
  .tab { display: flex; align-items: center; border-right: 1px solid var(--rule); }
  .tab.on { background: var(--code-bg); }
  .tab button { border: none; background: none; border-radius: 0; font: 12px var(--f-mono); color: var(--muted); padding: 7px 4px 7px 12px; }
  .tab.on .name { color: var(--ink); }
  .tab .x { padding: 7px 10px 7px 4px; opacity: 0.6; }
  .tab .x:hover { opacity: 1; color: var(--mark); }
  .tab i { font-style: normal; color: var(--blue); margin-left: 4px; }
  .cm { flex: 1; min-height: 0; }
  .err-bar { background: color-mix(in srgb, var(--mark) 14%, var(--sheet)); color: var(--mark); padding: 6px 12px; display: flex; gap: 10px; align-items: center; }
  .err-bar button { margin-left: auto; }
  .preview { display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  .pdfwrap { flex: 1; min-height: 0; }
  .problems { border-top: 1px solid var(--rule); background: var(--sheet); flex: none; max-height: 40%; display: flex; flex-direction: column; }
  .ph { display: flex; gap: 8px; align-items: center; border: none; border-radius: 0; background: var(--chrome); padding: 5px 12px; font: 500 12px var(--f-body); color: var(--muted); text-align: left; }
</style>
