<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage, type Config, type Diagnostic, type Engine, type Heading, type Outcome, type ProjectInfo, type TexStatus } from "./lib/api";
  import Onboarding from "./lib/Onboarding.svelte";
  import Library from "./lib/Library.svelte";
  import Tree from "./lib/Tree.svelte";
  import Editor from "./lib/Editor.svelte";
  import PdfView from "./lib/PdfView.svelte";
  import Problems from "./lib/Problems.svelte";
  import Outline from "./lib/Outline.svelte";
  import ProjectSettings from "./lib/ProjectSettings.svelte";
  import HistoryPanel from "./lib/HistoryPanel.svelte";
  import { startLanguageServer, type LanguageServer } from "./lib/lsp";
  import { forgetProject, recentProjects, rememberProject } from "./lib/recent";

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

  /** Shown when several files could be the main file and none is chosen. */
  let choosingMain = $state(false);
  let settingsOpen = $state(false);
  let historyOpen = $state(false);
  let historyVersion = $state(0);
  /** Label input for a manual snapshot (Ctrl+Shift+S); null when closed. */
  let snapshotLabel = $state<string | null>(null);
  let lastSnapshot = Date.now();
  const AUTO_SNAPSHOT_MS = 10 * 60 * 1000;
  let lastThumbnail = 0;

  let building = $state(false);
  let outcome = $state<Outcome | null>(null);
  let diagnostics = $state<Diagnostic[]>([]);
  let showInfo = $state(false);
  let problemsOpen = $state(true);
  let buildSeq = 0;

  let headings = $state<Heading[]>([]);
  let cursor = $state<{ file: string; line: number }>({ file: "", line: 1 });
  let lsp: LanguageServer | null = null;
  let lspAvailable = $state(true);
  let outlineTimer: ReturnType<typeof setTimeout> | undefined;

  /** Index of the heading the cursor is under, or -1. */
  const currentHeading = $derived.by(() => {
    let found = -1;
    headings.forEach((h, i) => {
      if (h.file === cursor.file && h.line <= cursor.line) found = i;
    });
    return found;
  });

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
    headings = [];
    await startLsp(project.root);
    await openFile(project.main);
    refreshOutline();
    lastThumbnail = 0;
    lastSnapshot = 0; // the first save of a session takes a snapshot
    choosingMain = project.main_candidates.length > 1 && !project.config.main;
    // Show the existing PDF right away, then rebuild.
    api.readPdf().then((b) => pdf?.load(b)).catch(() => {});
    runBuild();
  }

  async function startLsp(root: string) {
    lsp?.stop();
    lsp = await startLanguageServer(root, async (rel) => {
      await openFile(rel);
      return editor?.getView() ?? null;
    });
    lspAvailable = lsp !== null;
    editor?.setLanguageServer(lsp ? lsp.extensions : null);
  }

  function refreshOutline() {
    clearTimeout(outlineTimer);
    outlineTimer = setTimeout(async () => {
      try {
        headings = await api.outline();
      } catch {
        headings = [];
      }
    }, 150);
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
      if (path.endsWith(".tex")) refreshOutline();
      if (Date.now() - lastSnapshot > AUTO_SNAPSHOT_MS) takeSnapshot("Automatic snapshot");
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
      if (result.pdf) {
        await pdf?.load(await api.readPdf());
        saveThumbnail();
      }
    } catch (e) {
      if (seq === buildSeq) {
        outcome = null;
        diagnostics = [{ severity: "error", kind: { type: "other" }, file: null, line: null, message: errorMessage(e), hint: null }];
      }
    } finally {
      if (seq === buildSeq) building = false;
    }
  }

  async function takeSnapshot(label: string) {
    lastSnapshot = Date.now();
    try {
      if (await api.historySnapshot(label)) historyVersion++;
    } catch {
      // History is off for this project (not a repo, or no git): nothing to do.
    }
  }

  async function confirmSnapshot(e: SubmitEvent) {
    e.preventDefault();
    const label = snapshotLabel?.trim() || "Snapshot";
    snapshotLabel = null;
    await saveAll();
    await takeSnapshot(label);
    historyOpen = true;
  }

  async function currentText(path: string) {
    return contents.get(path) ?? api.readText(path).catch(() => null);
  }

  async function onRestored(path: string) {
    contents.delete(path);
    editor?.forget(path);
    dirty[path] = false;
    if (active === path || tabs.includes(path)) await openFile(path);
    historyVersion++;
    runBuild();
  }

  /** Refreshes the library thumbnail at most every two minutes. */
  async function saveThumbnail() {
    if (!project || Date.now() - lastThumbnail < 120_000) return;
    lastThumbnail = Date.now();
    const png = await pdf?.thumbnail().catch(() => null);
    if (png && project) api.saveThumbnail(project.root, png).catch(() => {});
  }

  async function saveSettings(config: Config) {
    if (!project) return;
    settingsOpen = false;
    const mainChanged = config.main !== project.main;
    project = await api.setConfig(config);
    if (mainChanged) await openFile(project.main);
    refreshOutline();
    runBuild();
  }

  async function chooseMain(main: string) {
    if (!project) return;
    choosingMain = false;
    project = await api.setConfig({ ...project.config, main });
    await openFile(main);
    refreshOutline();
    runBuild();
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

  function closeProject() {
    saveAll();
    lsp?.stop();
    lsp = null;
    editor?.setLanguageServer(null);
    project = null;
  }

  function onKey(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.shiftKey && e.key.toLowerCase() === "s") {
      e.preventDefault();
      if (project) snapshotLabel = "";
    } else if (mod && e.key === "s") {
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
  <Library {recent} error={openError} onOpen={openProject} onForget={(p) => (recent = forgetProject(p))} />
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
        {#if !lspAvailable}<span class="pill warn" title="texlab wasn't found, so completion and hover are off">no completion</span>{/if}
        <button class:on={historyOpen} onclick={() => (historyOpen = !historyOpen)}>History</button>
        <button onclick={() => (settingsOpen = true)}>Settings</button>
        <button onclick={closeProject}>Close project</button>
        <button class="primary" onclick={() => saveAll().then(runBuild)} title="Ctrl+Enter">Recompile</button>
      </div>
    </header>

    <div class="panes">
      <nav class="side">
        <div class="files">
          <div class="h">Project</div>
          <Tree entries={project.tree} {active} main={project.main} onOpen={openFile} />
        </div>
        <div class="outline">
          <div class="h">Outline</div>
          <Outline {headings} current={currentHeading} onJump={gotoProblem} />
        </div>
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
        <div class="cm"><Editor bind:this={editor} {onChange} onSyncRequest={syncForward} onCursor={(file, line) => (cursor = { file, line })} /></div>
        {#if openError}<div class="err-bar">{openError} <button onclick={() => (openError = null)}>Dismiss</button></div>{/if}
      </section>

      <section class="preview">
        <div class="pdfwrap" class:hidden={historyOpen}><PdfView bind:this={pdf} onSourceClick={syncBackward} /></div>
        {#if historyOpen}
          <div class="pdfwrap"><HistoryPanel version={historyVersion} {currentText} {onRestored} onClose={() => (historyOpen = false)} /></div>
        {/if}
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

{#if settingsOpen && project}
  <ProjectSettings {project} onSave={saveSettings} onClose={() => (settingsOpen = false)} />
{/if}

{#if snapshotLabel !== null}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && (snapshotLabel = null)}>
    <form class="dialog" onsubmit={confirmSnapshot}>
      <h2>Save a snapshot</h2>
      <p>Give it a name you'll recognise later, like "Sent to supervisor".</p>
      <!-- svelte-ignore a11y_autofocus -->
      <input id="snapshot-label" class="label-input" bind:value={snapshotLabel} placeholder="Snapshot" autofocus />
      <div class="actions">
        <button type="button" onclick={() => (snapshotLabel = null)}>Cancel</button>
        <button class="primary" type="submit">Save snapshot</button>
      </div>
    </form>
  </div>
{/if}

{#if choosingMain && project}
  <div class="backdrop" role="presentation">
    <div class="dialog" role="dialog" aria-labelledby="main-title">
      <h2 id="main-title">Which file is the main document?</h2>
      <p>Several files start with <code>\documentclass</code>. Lemmata will build the one you choose and remember it in <code>lemmata.toml</code>.</p>
      <ul>
        {#each project.main_candidates as c (c)}
          <li><button onclick={() => chooseMain(c)}>{c}</button></li>
        {/each}
      </ul>
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
  .side { background: var(--chrome); border-right: 1px solid var(--rule); display: flex; flex-direction: column; min-height: 0; }
  .files, .outline { overflow: auto; min-height: 0; }
  .files { flex: 1 1 55%; padding-bottom: 8px; }
  .outline { flex: 1 1 45%; border-top: 1px solid var(--rule); }
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
  .pdfwrap.hidden { display: none; }
  header button.on { border-color: var(--blue); color: var(--blue); }
  .label-input { width: 100%; font: 13px var(--f-body); padding: 7px 10px; border: 1px solid var(--rule); border-radius: 5px; background: var(--sheet); color: var(--ink); }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 14px; }
  .problems { border-top: 1px solid var(--rule); background: var(--sheet); flex: none; max-height: 40%; display: flex; flex-direction: column; }
  .backdrop { position: fixed; inset: 0; background: rgba(10, 15, 20, 0.45); display: grid; place-items: center; padding: 24px; z-index: 10; }
  .dialog { width: min(520px, 100%); background: var(--sheet); border: 1px solid var(--rule); border-radius: 8px; padding: 22px 24px; }
  .dialog h2 { font: 400 20px var(--f-display); margin: 0 0 8px; }
  .dialog p { color: var(--muted); margin: 0 0 12px; }
  .dialog ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  .dialog li button { width: 100%; text-align: left; font: 13px var(--f-mono); padding: 8px 12px; }
  .ph { display: flex; gap: 8px; align-items: center; border: none; border-radius: 0; background: var(--chrome); padding: 5px 12px; font: 500 12px var(--f-body); color: var(--muted); text-align: left; }
</style>
