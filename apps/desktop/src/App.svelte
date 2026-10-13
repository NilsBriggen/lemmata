<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage, type Entry, type Config, type Diagnostic, type Engine, type Heading, type Outcome, type ProjectInfo, type TexStatus } from "./lib/api";
  import Onboarding from "./lib/Onboarding.svelte";
  import Library from "./lib/Library.svelte";
  import Tree from "./lib/Tree.svelte";
  import Editor from "./lib/Editor.svelte";
  import PdfView from "./lib/PdfView.svelte";
  import Problems from "./lib/Problems.svelte";
  import Outline from "./lib/Outline.svelte";
  import ProjectSettings from "./lib/ProjectSettings.svelte";
  import HistoryPanel from "./lib/HistoryPanel.svelte";
  import Preferences from "./lib/Preferences.svelte";
  import CitePicker from "./lib/CitePicker.svelte";
  import Palette, { type PaletteItem } from "./lib/Palette.svelte";
  import SearchPanel from "./lib/SearchPanel.svelte";
  import FileActions from "./lib/FileActions.svelte";
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { loadSession, saveSession } from "./lib/session";
  import { listen } from "@tauri-apps/api/event";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { loadSettings, settings } from "./lib/settings.svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { setMathMacros } from "./lib/writing/math";
  import { figureSnippet } from "./lib/writing/table";
  import { startLanguageServer, type LanguageServer } from "./lib/lsp";
  import { detectLanguage, spell } from "./lib/spell/service";
  import { forgetProject, recentProjects, rememberProject } from "./lib/recent";

  const UPDATE_INTERVAL_MS = 6 * 60 * 60 * 1000;

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
  let prefsOpen = $state(false);
  let citeOpen = $state(false);
  let palette = $state<null | "commands" | "files">(null);
  let searchOpen = $state(false);
  let focusMode = $state(false);
  let fileActions: FileActions | undefined = $state();
  /** A package a just-inserted snippet needs; offered once per package. */
  let packageOffer = $state<string | null>(null);
  const offered = new Set<string>();
  let notice = $state<{ text: string; action?: { label: string; run: () => void } } | null>(null);
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;
  let update = $state<{ version: string; progress: number | null; error: string | null } | null>(null);
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
  let totalWords = $state(0);
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
    applySpellLanguage();
    setMathMacros(contents.get(project.main) ?? "");
    refreshOutline();
    await restoreSession(project.root);
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

  function applySpellLanguage() {
    if (!project) return;
    const lang = project.config.spellcheck ?? detectLanguage(contents.get(project.main) ?? "");
    spell.setLanguage(lang, project.config.words ?? []);
  }

  async function addWord(word: string) {
    if (!project) return;
    const words = Array.from(new Set([...(project.config.words ?? []), word])).sort();
    project = await api.setConfig({ ...project.config, main: project.main, words });
  }

  function refreshOutline() {
    clearTimeout(outlineTimer);
    outlineTimer = setTimeout(async () => {
      try {
        const o = await api.outline();
        headings = o.headings;
        totalWords = o.total_words;
      } catch {
        headings = [];
        totalWords = 0;
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
    // A delay of 0 means: only save on Ctrl+S.
    if (settings.autosaveMs > 0) saveTimers.set(path, setTimeout(() => save(path), settings.autosaveMs));
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
      if (path === project?.main) setMathMacros(text);
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

  function showNotice(text: string, action?: { label: string; run: () => void }) {
    clearTimeout(noticeTimer);
    notice = { text, action };
    noticeTimer = setTimeout(() => (notice = null), 8000);
  }

  async function needsPackage(pkg: string) {
    if (!project || offered.has(pkg)) return;
    if (await api.hasPackage(pkg).catch(() => true)) return;
    offered.add(pkg);
    packageOffer = pkg;
  }

  async function addPackage(pkg: string) {
    if (!project) return;
    packageOffer = null;
    await save(project.main);
    try {
      await api.addPackage(pkg);
    } catch (e) {
      openError = errorMessage(e);
      return;
    }
    contents.delete(project.main);
    editor?.forget(project.main);
    if (active === project.main) await openFile(project.main);
    runBuild();
  }

  async function restoreSession(root: string) {
    const session = await loadSession(root);
    if (!session) return;
    const known = new Set(allFiles(project?.tree ?? []));
    for (const t of session.tabs) if (known.has(t) && t !== active) await openFile(t);
    if (session.active && known.has(session.active)) {
      await openFile(session.active);
      editor?.gotoLine(session.line);
    }
    if (session.pdf) pdf?.setPosition(session.pdf);
  }

  let sessionTimer: ReturnType<typeof setTimeout> | undefined;
  function rememberSession() {
    clearTimeout(sessionTimer);
    sessionTimer = setTimeout(() => {
      if (!project) return;
      saveSession(project.root, { tabs, active, line: cursor.line, pdf: pdf?.getPosition() ?? null });
    }, 1000);
  }

  $effect(() => {
    // Re-run when any of these change.
    void [tabs, active, cursor.line];
    rememberSession();
  });

  async function refreshTree() {
    if (project) project = await api.projectInfo();
  }

  async function pasteImage(png: Uint8Array) {
    try {
      const path = await api.savePastedImage(png);
      refreshTree();
      return path;
    } catch (e) {
      openError = errorMessage(e);
      return null;
    }
  }

  async function dropFiles(paths: string[], x: number, y: number) {
    if (!project || !active?.endsWith(".tex")) return;
    const pos = editor?.posAtPoint(x, y) ?? undefined;
    for (const p of paths) {
      try {
        const rel = await api.importFigure(p);
        editor?.insertSnippet(figureSnippet(rel), "graphicx", pos);
      } catch (e) {
        showNotice(errorMessage(e));
      }
    }
    refreshTree();
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
    applySpellLanguage();
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
    if (project) saveSession(project.root, { tabs, active, line: cursor.line, pdf: pdf?.getPosition() ?? null });
    lsp?.stop();
    lsp = null;
    editor?.setLanguageServer(null);
    project = null;
  }

  function allFiles(entries: Entry[]): string[] {
    return entries.flatMap((e) => (e.is_dir ? allFiles(e.children) : [e.path]));
  }

  const fileItems = $derived<PaletteItem[]>(
    project
      ? allFiles(project.tree)
          .filter((p) => /\.(tex|bib|sty|cls|txt|md|toml|bst|tikz)$/i.test(p))
          .map((p) => ({ id: p, label: p.split("/").pop()!, detail: p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "", run: () => openFile(p) }))
      : [],
  );

  const commandItems = $derived<PaletteItem[]>(
    [
      { id: "recompile", label: "Recompile", detail: "Ctrl+Enter", run: () => saveAll().then(runBuild) },
      { id: "open-file", label: "Open file…", detail: "Ctrl+P", run: () => (palette = "files") },
      { id: "search", label: "Search in project", detail: "Ctrl+Shift+F", run: openSearch },
      { id: "cite", label: "Insert citation", detail: "Ctrl+Shift+C", run: () => (citeOpen = true) },
      { id: "snapshot", label: "Save a snapshot", detail: "Ctrl+Shift+S", run: () => (snapshotLabel = "") },
      { id: "history", label: "Show history", run: () => (historyOpen = true) },
      { id: "sync", label: "Show this line in the PDF", detail: "Ctrl+J", run: () => active && syncForward(active, editor?.currentLine() ?? 1, 0) },
      { id: "focus", label: focusMode ? "Leave focus mode" : "Focus mode", detail: "F11", run: () => (focusMode = !focusMode) },
      { id: "new-file", label: "New file…", run: () => fileActions?.openMenu(null, 240, 120) },
      { id: "export-pdf", label: "Export PDF…", run: exportPdf },
      { id: "export-arxiv", label: "Export for arXiv (.zip)…", run: exportArxiv },
      { id: "project-settings", label: "Project settings", run: () => (settingsOpen = true) },
      { id: "preferences", label: "Preferences", run: () => (prefsOpen = true) },
      { id: "close", label: "Close project", run: closeProject },
    ].filter(() => project),
  );

  function openSearch() {
    focusMode = false;
    searchOpen = true;
  }

  async function exportPdf() {
    if (!project) return;
    const dest = await saveDialog({ title: "Export PDF", defaultPath: `${project.name}.pdf`, filters: [{ name: "PDF", extensions: ["pdf"] }] });
    if (!dest) return;
    try {
      await api.exportPdf(dest);
      showNotice(`Saved ${dest.split(/[\\/]/).pop()}.`);
    } catch (e) {
      showNotice(errorMessage(e));
    }
  }

  async function exportArxiv() {
    if (!project) return;
    await saveAll();
    await runBuild();
    const dest = await saveDialog({ title: "Export for arXiv", defaultPath: `${project.name}-arxiv.zip`, filters: [{ name: "Zip", extensions: ["zip"] }] });
    if (!dest) return;
    try {
      const n = await api.exportArxiv(dest, true);
      showNotice(`Exported ${n} files with comments removed, including the compiled bibliography.`);
    } catch (e) {
      showNotice(errorMessage(e));
    }
  }

  /** Reloads files changed by another program, unless they have unsaved edits here. */
  async function onFilesChanged(paths: string[]) {
    if (!project) return;
    await refreshTree();
    for (const path of paths) {
      if (!contents.has(path)) continue;
      const disk = await api.readText(path).catch(() => null);
      if (disk === null || disk === contents.get(path)) continue; // our own save, or deleted
      if (dirty[path]) {
        showNotice(`${path} changed on disk.`, { label: "Load the new version", run: () => reloadFromDisk(path) });
      } else {
        await reloadFromDisk(path);
      }
    }
    refreshOutline();
  }

  async function reloadFromDisk(path: string) {
    contents.delete(path);
    editor?.forget(path);
    dirty[path] = false;
    if (active === path) await openFile(path);
  }

  function onRenamed(from: string, to: string) {
    for (const p of [...contents.keys()]) {
      if (p === from || p.startsWith(from + "/")) {
        contents.delete(p);
        editor?.forget(p);
      }
    }
    const moved = (p: string) => (p === from ? to : p.startsWith(from + "/") ? to + p.slice(from.length) : p);
    tabs = tabs.map(moved);
    const wasActive = active;
    active = null;
    refreshTree().then(() => {
      if (wasActive) openFile(moved(wasActive));
    });
    refreshOutline();
  }

  function onTrashed(path: string) {
    for (const p of [...tabs]) if (p === path || p.startsWith(path + "/")) closeTab(p);
    refreshTree();
    refreshOutline();
  }

  function onKey(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (e.key === "F11" && project) {
      e.preventDefault();
      focusMode = !focusMode;
    } else if (mod && e.shiftKey && e.key.toLowerCase() === "p" && project) {
      e.preventDefault();
      palette = "commands";
    } else if (mod && !e.shiftKey && e.key.toLowerCase() === "p" && project) {
      e.preventDefault();
      palette = "files";
    } else if (mod && e.shiftKey && e.key.toLowerCase() === "f" && project) {
      e.preventDefault();
      openSearch();
    } else if (mod && e.shiftKey && e.key.toLowerCase() === "c" && project && active) {
      e.preventDefault();
      citeOpen = true;
    } else if (mod && e.shiftKey && e.key.toLowerCase() === "s") {
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

  /** Returns a short message for the Preferences dialog. */
  async function checkForUpdates(): Promise<string> {
    try {
      const found = await api.updateCheck(settings.updateChannel);
      if (!found) return "Lemmata is up to date.";
      update = { version: found.version, progress: null, error: null };
      return `Lemmata ${found.version} is available.`;
    } catch (e) {
      return `Couldn't check for updates: ${errorMessage(e)}`;
    }
  }

  async function installUpdate() {
    if (!update) return;
    update.progress = 0;
    update.error = null;
    const unlisten = await listen<number>("update-progress", (e) => update && (update.progress = e.payload));
    try {
      await saveAll();
      await api.updateInstall();
      await relaunch();
    } catch (e) {
      if (update) {
        update.error = errorMessage(e);
        update.progress = null;
      }
    } finally {
      unlisten();
    }
  }

  // Switching to or from Vim rebuilds the editors.
  let appliedKeymap = settings.keymap;
  $effect(() => {
    if (settings.keymap !== appliedKeymap) {
      appliedKeymap = settings.keymap;
      editor?.refreshExtensions();
    }
  });

  onMount(async () => {
    // Tauri delivers file drops with real paths (DOM drop events don't have them).
    getCurrentWebview()
      .onDragDropEvent((e) => {
        if (e.payload.type === "drop") {
          const scale = window.devicePixelRatio || 1;
          dropFiles(e.payload.paths, e.payload.position.x / scale, e.payload.position.y / scale);
        }
      })
      .catch(() => {});
    listen<string[]>("files-changed", (e) => onFilesChanged(e.payload)).catch(() => {});
    await loadSettings();
    appliedKeymap = settings.keymap;
    await recheck(settings.texDir ?? undefined);
    if (settings.checkForUpdates) {
      checkForUpdates();
      setInterval(() => settings.checkForUpdates && !update && checkForUpdates(), UPDATE_INTERVAL_MS);
    }
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
  <Library {recent} error={openError} onOpen={openProject} onForget={(p) => (recent = forgetProject(p))} onPreferences={() => (prefsOpen = true)} />
{:else}
  <div class="shell">
    <header>
      <span class="brand">Lemmata</span>
      <span class="crumbs">{project.name}{active ? ` / ${active}` : ""}</span>
      <div class="right">
        {#if tex.detection.outdated}<span class="pill warn" title="Some packages may lack newer features">old TeX Live</span>{/if}
        {#if totalWords}<span class="words" title="Words in the document body, without commands, maths or comments">{totalWords.toLocaleString()} words</span>{/if}
        {#if statusPill}<span class="pill {statusPill.cls}">{statusPill.text}</span>{/if}
        <select aria-label="Engine" value={project.config.engine} onchange={(e) => setEngine(e.currentTarget.value as Engine)}>
          <option value="pdflatex">pdfLaTeX</option>
          <option value="xelatex">XeLaTeX</option>
          <option value="lualatex">LuaLaTeX</option>
        </select>
        {#if !lspAvailable}<span class="pill warn" title="texlab wasn't found, so completion and hover are off">no completion</span>{/if}
        <button class:on={historyOpen} onclick={() => (historyOpen = !historyOpen)}>History</button>
        <button onclick={() => (settingsOpen = true)}>Project</button>
        <button onclick={() => (prefsOpen = true)} aria-label="Preferences" title="Preferences">Preferences</button>
        <button onclick={closeProject}>Close project</button>
        <button class="primary" onclick={() => saveAll().then(runBuild)} title="Ctrl+Enter">Recompile</button>
      </div>
    </header>

    <div class="panes" class:focus={focusMode}>
      <nav class="side">
        {#if searchOpen}
          <SearchPanel
            onOpen={gotoProblem}
            beforeReplace={saveAll}
            onReplaced={async (paths) => {
              for (const p of paths) await reloadFromDisk(p);
              runBuild();
            }}
            onClose={() => (searchOpen = false)}
          />
        {:else}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="files" oncontextmenu={(e) => { e.preventDefault(); fileActions?.openMenu(null, e.clientX, e.clientY); }}>
            <div class="h">Project</div>
            <Tree entries={project.tree} {active} main={project.main} onOpen={openFile} onContext={(entry, x, y) => fileActions?.openMenu(entry, x, y)} />
          </div>
          <div class="outline">
            <div class="h">Outline</div>
            <Outline {headings} current={currentHeading} onJump={gotoProblem} />
          </div>
        {/if}
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
        <div class="cm"><Editor bind:this={editor} {onChange} onSyncRequest={syncForward} onCursor={(file, line) => (cursor = { file, line })}
            onAddWord={addWord}
            onPasteImage={pasteImage}
            onNeedsPackage={needsPackage}
            onPastedTable={(undo) => showNotice("Pasted as a table.", { label: "Paste as text instead", run: undo })}
          /></div>
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

{#if packageOffer || notice}
  <div class="toasts">
    {#if packageOffer}
      <div class="toast" role="status">
        <span>This needs the <code>{packageOffer}</code> package, which your document doesn't load yet.</span>
        <button onclick={() => (packageOffer = null)}>Not now</button>
        <button class="primary" onclick={() => addPackage(packageOffer!)}>Add \usepackage{"{"}{packageOffer}{"}"}</button>
      </div>
    {/if}
    {#if notice}
      <div class="toast" role="status">
        <span>{notice.text}</span>
        {#if notice.action}<button onclick={() => { notice?.action?.run(); notice = null; }}>{notice.action.label}</button>{/if}
        <button class="x" aria-label="Dismiss" onclick={() => (notice = null)}>×</button>
      </div>
    {/if}
  </div>
{/if}

<FileActions
  bind:this={fileActions}
  beforeChange={saveAll}
  onCreated={async (path, dir) => {
    await refreshTree();
    if (!dir && /\.(tex|bib|sty|cls|txt|md|toml)$/i.test(path)) openFile(path);
  }}
  {onRenamed}
  {onTrashed}
/>

{#if palette}
  <Palette
    items={palette === "files" ? fileItems : commandItems}
    placeholder={palette === "files" ? "Open a file by name" : "Type a command"}
    onClose={() => {
      palette = null;
      editor?.getView()?.focus();
    }}
  />
{/if}

{#if citeOpen}
  <CitePicker
    onInsert={(keys) => {
      citeOpen = false;
      editor?.insertCitation(keys);
    }}
    onBibChanged={(file) => {
      contents.delete(file);
      editor?.forget(file);
      if (tabs.includes(file)) openFile(file);
    }}
    onClose={() => {
      citeOpen = false;
      editor?.getView()?.focus();
    }}
  />
{/if}

{#if prefsOpen}
  <Preferences
    detection={tex?.detection ?? null}
    onTexDir={(dir) => recheck(dir ?? undefined)}
    onCheckUpdates={checkForUpdates}
    onClose={() => (prefsOpen = false)}
  />
{/if}

{#if update}
  <div class="update" role="status">
    {#if update.error}
      <span>The update failed: {update.error}</span>
    {:else if update.progress !== null}
      <span>Downloading Lemmata {update.version}… {Math.round(update.progress * 100)}%</span>
    {:else}
      <span>Lemmata {update.version} is available.</span>
    {/if}
    {#if update.progress === null}
      <button onclick={() => (update = null)}>Later</button>
      <button class="primary" onclick={installUpdate}>Install and restart</button>
    {/if}
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
  /* Focus mode: only the editor, as a centred column. */
  .panes.focus { grid-template-columns: minmax(0, 1fr); }
  .panes.focus .side, .panes.focus .preview, .panes.focus .tabs { display: none; }
  .panes.focus .code { border-right: none; padding-inline: max(24px, calc((100% - 88ch) / 2)); }
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
  .toasts { position: fixed; left: 50%; bottom: 16px; transform: translateX(-50%); z-index: 20; display: flex; flex-direction: column; gap: 8px; }
  .toast {
    display: flex; gap: 10px; align-items: center; background: var(--sheet); border: 1px solid var(--rule);
    border-radius: 8px; padding: 9px 12px; box-shadow: 0 10px 30px -12px rgba(0, 0, 0, 0.4); max-width: min(720px, 90vw);
  }
  .toast .x { border: none; background: none; padding: 0 4px; }
  .update {
    position: fixed; right: 16px; bottom: 16px; z-index: 20; display: flex; gap: 10px; align-items: center;
    background: var(--sheet); border: 1px solid var(--rule); border-radius: 8px; padding: 10px 12px;
    box-shadow: 0 10px 30px -12px rgba(0, 0, 0, 0.4);
  }
  .words { font: 11px var(--f-mono); color: var(--muted); white-space: nowrap; }
  .ph { display: flex; gap: 8px; align-items: center; border: none; border-radius: 0; background: var(--chrome); padding: 5px 12px; font: 500 12px var(--f-body); color: var(--muted); text-align: left; }
</style>
