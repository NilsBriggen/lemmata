<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { api, errorMessage, type Target, type Entry, type Config, type Diagnostic, type Engine, type Heading, type Outcome, type ProjectInfo, type TexStatus } from "./lib/api";
  import Onboarding from "./lib/Onboarding.svelte";
  import Library from "./lib/Library.svelte";
  import Editor from "./lib/Editor.svelte";
  import PdfPane from "./lib/PdfPane.svelte";
  import Toolbar, { type MenuAction } from "./lib/Toolbar.svelte";
  import Sidebar from "./lib/Sidebar.svelte";
  import Splitter from "./lib/ui/Splitter.svelte";
  import Modal from "./lib/ui/Modal.svelte";
  import Welcome from "./lib/Welcome.svelte";
  import X from "@lucide/svelte/icons/x";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import Info from "@lucide/svelte/icons/info";
  import Package from "@lucide/svelte/icons/package";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import ProjectSettings from "./lib/ProjectSettings.svelte";
  import HistoryPanel from "./lib/HistoryPanel.svelte";
  import Preferences from "./lib/Preferences.svelte";
  import CitePicker from "./lib/CitePicker.svelte";
  import FigureDialog from "./lib/FigureDialog.svelte";
  import TableDialog from "./lib/TableDialog.svelte";
  import SymbolPalette from "./lib/SymbolPalette.svelte";
  import WordCount from "./lib/WordCount.svelte";
  import Shortcuts from "./lib/Shortcuts.svelte";
  import EditorToolbar from "./lib/EditorToolbar.svelte";
  import TabStrip from "./lib/TabStrip.svelte";
  import { templateOf } from "./lib/editor/mathCommands";
  import DropMenu from "./lib/ui/DropMenu.svelte";
  import Bell from "@lucide/svelte/icons/bell";
  import FileViewer from "./lib/FileViewer.svelte";
  import { display, keysOf, matches } from "./lib/shortcuts";
  import Palette, { type PaletteItem } from "./lib/Palette.svelte";
  import { ENVIRONMENTS } from "./lib/editor/data";
  import type { Providers } from "./lib/editor/completions";
  import SearchPanel from "./lib/SearchPanel.svelte";
  import RenameSymbol from "./lib/RenameSymbol.svelte";
  import SaveTemplateDialog from "./lib/SaveTemplateDialog.svelte";
  import FileActions from "./lib/FileActions.svelte";
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { loadSession, saveSession } from "./lib/session";
  import { magicLanguage } from "./lib/spell/magic";
  import { loadTexCache, saveTexCache } from "./lib/texCache";
  import { clearSelection, prune, reveal, setGitStatus, tree as treeView } from "./lib/treeSelection.svelte";
  import { dragState } from "./lib/treeDrag.svelte";
  import { findEntry } from "./lib/treeModel";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { getVersion } from "@tauri-apps/api/app";
  import { errorSummary, issueUrl, systemSummary } from "./lib/report";
  import { listen } from "@tauri-apps/api/event";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { loadSettings, settings, updateSettings, DEFAULTS } from "./lib/settings.svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { setMathMacros } from "./lib/writing/math";
  import { figureSnippet } from "./lib/writing/table";
  import { startLanguageServer, type LanguageServer } from "./lib/lsp";
  import { detectLanguage, spell } from "./lib/spell/service";
  import { forgetProject, recentEntries, rememberProject, rememberProjects, type Recent } from "./lib/recent";

  const UPDATE_INTERVAL_MS = 6 * 60 * 60 * 1000;

  let tex = $state<TexStatus | null>(null);
  let checking = $state(false);
  let project = $state<ProjectInfo | null>(null);
  let openError = $state<string | null>(null);
  let recent = $state<Recent[]>(recentEntries());
  /** First-run welcome: shown once, or when asked for from the library. */
  let welcome = $state(false);

  let tabs = $state<string[]>([]);
  /** Pinned tabs (always first in `tabs`), recently closed files (newest first) and folds per file. */
  let pinned = $state<string[]>([]);
  let closedTabs = $state<string[]>([]);
  let folds: Record<string, number[]> = {};
  let foldsVersion = $state(0);
  let active = $state<string | null>(null);
  let dirty = $state<Record<string, boolean>>({});
  const contents = new Map<string, string>();
  /** What each file held when we last read or wrote it: disk content equal to this is not an outside change. */
  const lastSaved = new Map<string, string>();
  const saveTimers = new Map<string, ReturnType<typeof setTimeout>>();

  /** Shown when several files could be the main file and none is chosen. */
  let choosingMain = $state(false);
  let settingsOpen = $state(false);
  let prefsOpen = $state(false);
  let citeOpen = $state(false);
  let dialog = $state<null | "figure" | "table" | "symbols" | "words" | "shortcuts">(null);
  const hint = (id: string) => display(keysOf(id));
  let selectionWords = $state(0);
  let palette = $state<null | "commands" | "files" | "environments" | "headings" | "symbols">(null);
  let savingTemplate = $state(false);
  let renaming = $state<{ kind: "label" | "cite"; name: string } | null>(null);
  let symbolItems = $state<PaletteItem[]>([]);
  let searchOpen = $state(false);
  let focusMode = $state(false);
  let report = $state<{ system: string; log: string } | null>(null);
  let fileActions: FileActions | undefined = $state();
  /** A package a just-inserted snippet needs; offered once per package. */
  let packageOffer = $state<string | null>(null);
  const offered = new Set<string>();
  let notice = $state<{ text: string; tone?: "ok" | "info"; action?: { label: string; run: () => void } } | null>(null);
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;
  let update = $state<{ version: string; progress: number | null; error: string | null } | null>(null);
  let historyOpen = $state(false);
  let historyVersion = $state(0);
  /** Label input for a manual snapshot (Ctrl+Shift+S); null when closed. */
  let snapshotLabel = $state<string | null>(null);
  let lastSnapshot = Date.now();
  let editedSinceSnapshot = false;
  const AUTO_SNAPSHOT_MS = 10 * 60 * 1000;
  let lastThumbnail = 0;

  let building = $state(false);
  let outcome = $state<Outcome | null>(null);
  let lastCompiled = $state(0);
  let diagnostics = $state<Diagnostic[]>([]);
  /** Logs replace the PDF in the right pane, like Overleaf's logs view. */
  let logsOpen = $state(false);
  let buildSeq = $state(0);

  let headings = $state<Heading[]>([]);
  let totalWords = $state(0);
  let cursor = $state<{ file: string; line: number }>({ file: "", line: 1 });
  let lsp: LanguageServer | null = null;
  let lspAvailable = $state(true);
  let lspStarting = $state(false);
  /** Recent notices, so a toast that disappeared can still be read. */
  let notices = $state<{ text: string; time: number }[]>([]);
  let noticesOpen = $state(false);
  let bellBtn = $state<HTMLButtonElement>();
  let seenNotices = $state(0);
  /** Mirrors spell.language, which isn't reactive by itself. */
  let spellLang = $state("off");
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
  let pdf: PdfPane | undefined = $state();

  const errors = $derived(diagnostics.filter((d) => d.severity === "error").length);
  const warnings = $derived(diagnostics.filter((d) => d.severity === "warning").length);

  async function recheck(extraDir?: string) {
    checking = true;
    try {
      tex = await api.texStatus(extraDir);
      saveTexCache(tex);
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
    welcome = false;
    tabs = [];
    contents.clear();
    dirty = {};
    clearSelection();
    pinned = [];
    closedTabs = [];
    folds = {};
    treeView.clipboard = null;
    treeView.collapsed = {};
    setGitStatus([]);
    refreshGit();
    fileActions?.forgetHistory();
    api.purgeTrash(7).catch(() => {});
    diagnostics = [];
    outcome = null;
    headings = [];
    // The file first: texlab can take a moment to start, and nothing here needs it.
    await openFile(project.main);
    await startLsp(project.root);
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

  // Completion data for \ref, \cite, file names and packages. Labels and
  // citation keys change as the user types, so they're cached only briefly.
  function cached<T>(load: () => Promise<T>, ttl: number): () => Promise<T> {
    let at = 0;
    let value: Promise<T> | null = null;
    return () => {
      if (!value || Date.now() - at > ttl) {
        at = Date.now();
        value = load().catch((e) => {
          // Don't remember the failure: the next request tries again.
          value = null;
          at = 0;
          reportOnce("completions", `Completion lists for \\ref and \\cite couldn't be loaded: ${errorMessage(e)}`);
          return [] as unknown as T;
        });
      }
      return value;
    };
  }
  const providers: Providers = {
    labels: cached(() => api.labels(), 1500),
    cites: cached(async () => (await api.bibEntries()).entries, 3000),
    files: () => (project ? allFiles(project.tree) : []),
    packages: cached(() => api.packageNames(), 3_600_000),
    needsPackage: (pkg) => void needsPackage(pkg),
  };
  $effect(() => {
    editor?.setProviders(project ? providers : null);
  });

  const environmentItems: PaletteItem[] = ENVIRONMENTS.map((e) => ({
    id: e.name,
    label: e.name,
    detail: e.detail,
    run: () => editor?.wrapInEnvironment(e.name),
  }));

  async function startLsp(root: string) {
    lsp?.stop();
    lspStarting = true;
    lsp = await startLanguageServer(root, async (rel) => {
      await openFile(rel);
      return editor?.getView() ?? null;
    }, (message) => reportOnce("lsp", message));
    lspStarting = false;
    lspAvailable = lsp !== null;
    editor?.setLanguageServer(lsp ? lsp.extensions : null);
  }

  let fileLanguage: string | null = null;

  function applySpellLanguage() {
    if (!project) return;
    const fallback = settings.spellDefault === "auto" ? detectLanguage(contents.get(project.main) ?? "") : settings.spellDefault;
    // A "% !TeX spellcheck = de_DE" comment in the open file wins over the project's choice.
    fileLanguage = active && /\.(tex|sty|cls)$/i.test(active) ? magicLanguage(contents.get(active) ?? "") : null;
    const lang = fileLanguage ?? project.config.spellcheck ?? fallback;
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

  /** A list with one empty item; `${}` is the tab stop CodeMirror's snippets use. */
  const listTemplate = (ordered: boolean) => {
    const env = ordered ? "enumerate" : "itemize";
    return "\\begin{" + env + "}\n\t\\item ${}\n\\end{" + env + "}";
  };

  const isViewer = (p: string) => /\.(png|jpe?g|gif|webp|svg|pdf)$/i.test(p);
  const viewerActive = $derived(!!active && isViewer(active));

  async function openFile(path: string) {
    if (isViewer(path)) {
      if (!tabs.includes(path)) tabs = [...tabs, path];
      active = path;
      return;
    }
    let text = contents.get(path);
    if (text === undefined) {
      try {
        text = await api.readText(path);
      } catch (e) {
        openError = errorMessage(e);
        return;
      }
      contents.set(path, text);
      lastSaved.set(path, text);
    }
    if (!tabs.includes(path)) tabs = [...tabs, path];
    active = path;
    editor?.open(path, text);
    editor?.showDiagnostics(diagnostics);
    if (project && magicLanguage(text) !== fileLanguage) applySpellLanguage();
  }

  function closeTab(path: string, saveFirst = true) {
    if (saveFirst && dirty[path]) save(path);
    clearTimeout(saveTimers.get(path));
    if (tabs.includes(path)) closedTabs = [path, ...closedTabs.filter((c) => c !== path)].slice(0, 10);
    pinned = pinned.filter((p) => p !== path);
    delete folds[path];
    tabs = tabs.filter((t) => t !== path);
    contents.delete(path);
    lastSaved.delete(path);
    dirty[path] = false;
    editor?.forget(path);
    if (active === path) {
      const next = tabs.at(-1);
      active = null;
      if (next) openFile(next);
      else editor?.close();
    }
  }

  /** Pinned tabs always come first, in the order they were pinned. */
  function pinFirst(list: string[]) {
    return [...pinned.filter((p) => list.includes(p)), ...list.filter((t) => !pinned.includes(t))];
  }

  function togglePin(path: string) {
    pinned = pinned.includes(path) ? pinned.filter((p) => p !== path) : [...pinned, path];
    tabs = pinFirst(tabs);
  }

  async function reopenClosed() {
    while (closedTabs.length) {
      const [path, ...rest] = closedTabs;
      closedTabs = rest;
      // A file that was deleted since can't come back (undoing the delete makes it reopenable).
      if (project && findEntry(project.tree, path)) {
        await openFile(path);
        return;
      }
    }
    showNotice("No closed tab to reopen.");
  }

  function onChange(path: string, text: string) {
    contents.set(path, text);
    dirty[path] = true;
    if (path === active && project && magicLanguage(text) !== fileLanguage) applySpellLanguage();
    clearTimeout(saveTimers.get(path));
    // A delay of 0 means: only save on Ctrl+S.
    if (settings.autosaveMs > 0) saveTimers.set(path, setTimeout(() => save(path, settings.autoCompile, true), settings.autosaveMs));
  }

  /** Automatic builds wait this long after the last edit, so a build isn't started and cancelled again while typing. */
  const AUTO_COMPILE_DELAY_MS = 1500;
  let buildTimer: ReturnType<typeof setTimeout> | undefined;

  /** `auto` marks a save made by the autosave timer: its build is debounced. An explicit save builds at once. */
  async function save(path: string, build = settings.autoCompile, auto = false) {
    clearTimeout(saveTimers.get(path));
    const text = contents.get(path);
    if (text === undefined || !dirty[path]) return;
    try {
      await api.writeText(path, text);
      lastSaved.set(path, text);
      refreshGit();
      // Typing continued while the write was in flight: those edits aren't saved yet.
      if (contents.get(path) === text) dirty[path] = false;
      else if (settings.autosaveMs > 0) saveTimers.set(path, setTimeout(() => save(path, settings.autoCompile, true), settings.autosaveMs));
      if (build) {
        clearTimeout(buildTimer);
        if (auto) buildTimer = setTimeout(runBuild, AUTO_COMPILE_DELAY_MS);
        else runBuild();
      }
      if (path.endsWith(".tex")) refreshOutline();
      if (path === project?.main) setMathMacros(text);
      editedSinceSnapshot = true;
      // The first save of a session takes a snapshot; later ones follow the timer.
      if (lastSnapshot === 0) takeSnapshot("Automatic snapshot");
    } catch (e) {
      openError = errorMessage(e);
    }
  }

  async function saveAll(build = settings.autoCompile) {
    await Promise.all(Object.keys(dirty).filter((p) => dirty[p]).map((p) => save(p, build)));
  }

  async function runBuild() {
    clearTimeout(buildTimer);
    const seq = ++buildSeq;
    building = true;
    try {
      const result = await api.build();
      if (seq !== buildSeq || result.status === "cancelled") return;
      outcome = result;
      lastCompiled = Date.now();
      diagnostics = result.diagnostics;
      editor?.showDiagnostics(diagnostics);
      // Like Overleaf: a failed build with no PDF opens the logs.
      if ((result.status === "failed" || result.status === "timeout") && !result.pdf && errors > 0) logsOpen = true;
      if (result.pdf) {
        await pdf?.load(await api.readPdf());
        saveThumbnail();
      } else {
        // The build wrote no new PDF (up to date, or it failed), but an older one may exist.
        await showExistingPdf();
      }
    } catch (e) {
      if (seq === buildSeq) {
        outcome = null;
        diagnostics = [{ severity: "error", kind: { type: "other" }, file: null, line: null, message: errorMessage(e), hint: null }];
        logsOpen = true;
      }
    } finally {
      if (seq === buildSeq) building = false;
    }
  }

  /** Loads the PDF on disk if the viewer has none, so the pane is never empty when a PDF exists. */
  async function showExistingPdf() {
    if (!pdf || pdf.hasPdf()) return;
    try {
      await pdf.load(await api.readPdf());
    } catch {
      // No PDF has been built yet: the pane explains that.
    }
  }

  async function stopBuild() {
    buildSeq++;
    building = false;
    await api.stopBuild().catch((e) => reportOnce("stop", `The build could not be stopped: ${errorMessage(e)}`));
  }

  async function uploadFiles(folder = "") {
    const picked = await openDialog({ multiple: true, title: "Add files to the project" });
    const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
    if (!paths.length) return;
    try {
      const added = await api.addFiles(paths, folder);
      await refreshTree();
      showNotice(`Added ${added.length} file${added.length === 1 ? "" : "s"}.`);
    } catch (e) {
      showNotice(errorMessage(e));
    }
  }

  /** Shows a problem notice once per kind per minute, so a failing background task doesn't flood the screen. */
  const reported = new Map<string, number>();
  function reportOnce(kind: string, text: string) {
    const last = reported.get(kind) ?? 0;
    if (Date.now() - last < 60_000) return;
    reported.set(kind, Date.now());
    showNotice(text);
  }

  /** `extra` is either an action button or just a tone. */
  function showNotice(text: string, extra?: { label: string; run: () => void } | { tone: "ok" | "info" }) {
    clearTimeout(noticeTimer);
    notices = [{ text, time: Date.now() }, ...notices].slice(0, 30);
    notice = extra && "label" in extra ? { text, action: extra } : { text, tone: extra?.tone };
    noticeTimer = setTimeout(() => (notice = null), 8000);
  }

  async function needsPackage(pkg: string) {
    if (!project || offered.has(pkg)) return;
    if (await api.hasPackage(pkg).catch(() => true)) return;
    offered.add(pkg);
    packageOffer = pkg;
  }

  async function setBuildOption(patch: Partial<Config>) {
    if (!project) return;
    project = await api.setConfig({ ...project.config, main: project.main, ...patch });
    saveAll(false).then(runBuild);
  }

  async function cleanCache() {
    try {
      const n = await api.cleanBuild();
      showNotice(n ? "Cleared the cached files." : "Nothing to clear.", { tone: "ok" });
      saveAll(false).then(runBuild);
    } catch (e) {
      showNotice(errorMessage(e));
    }
  }

  /** The source around a problem, for the Problems cards. */
  async function sourceOf(file: string): Promise<string | null> {
    const rel = file.replace(/^\.\//, "");
    const open = contents.get(rel);
    if (open !== undefined) return open;
    try {
      return await api.readText(rel);
    } catch {
      return null;
    }
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
    pinned = session.pinned ?? [];
    closedTabs = [];
    folds = session.folds ?? {};
    for (const [path, lines] of Object.entries(folds)) editor?.setFolds(path, lines);
    treeView.collapsed = Object.fromEntries((session.collapsed ?? []).map((p) => [p, true]));
    const known = new Set(allFiles(project?.tree ?? []));
    for (const t of session.tabs) if (known.has(t) && t !== active) await openFile(t);
    if (session.active && known.has(session.active)) {
      await openFile(session.active);
      editor?.gotoLine(session.line);
    }
    if (session.pdf && "fit" in session.pdf) pdf?.setPosition(session.pdf);
  }

  /** Remembers the open file's folds and returns them for every open tab. */
  function currentFolds() {
    if (active && /\.(tex|sty|cls)$/i.test(active) && editor) {
      const lines = editor.foldedLines();
      if (lines.length) folds[active] = lines;
      else delete folds[active];
    }
    return Object.fromEntries(Object.entries(folds).filter(([p]) => tabs.includes(p)));
  }

  let sessionTimer: ReturnType<typeof setTimeout> | undefined;
  function rememberSession() {
    clearTimeout(sessionTimer);
    sessionTimer = setTimeout(() => {
      if (!project) return;
      saveSession(project.root, {
        tabs,
        active,
        line: cursor.line,
        collapsed: Object.keys(treeView.collapsed).filter((k) => treeView.collapsed[k]),
        pinned: pinned.filter((p) => tabs.includes(p)),
        folds: currentFolds(),
        pdf: pdf?.getPosition() ?? null,
      });
    }, 1000);
  }

  $effect(() => {
    // Re-run when any of these change.
    void [tabs, active, cursor.line, Object.keys(treeView.collapsed).length, pinned.length, foldsVersion];
    rememberSession();
  });

  async function revealActive() {
    if (!active) return;
    reveal(active);
    treeView.selected = [active];
    treeView.cursor = active;
    if (!settings.sidebarOpen) await updateSettings({ sidebarOpen: true });
  }

  let gitTimer: ReturnType<typeof setTimeout> | undefined;
  /** Marks changed files in the tree when the project is in a git repository. */
  function refreshGit() {
    clearTimeout(gitTimer);
    gitTimer = setTimeout(() => {
      if (project) api.gitStatus().then(setGitStatus).catch(() => setGitStatus([]));
    }, 600);
  }

  async function refreshTree() {
    if (!project) return;
    refreshGit();
    project = await api.projectInfo();
    const tree = project.tree;
    prune((path) => !!findEntry(tree, path));
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
    if (!project) return;
    // Dropped on a folder (or file) in the tree: add the files there.
    const row = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-drop]");
    if (row) {
      try {
        const added = await api.addFiles(paths, row.dataset.drop ?? "");
        await refreshTree();
        showNotice(`Added ${added.length} file${added.length === 1 ? "" : "s"}.`, { tone: "ok" });
      } catch (e) {
        showNotice(errorMessage(e));
      }
      return;
    }
    if (!active?.endsWith(".tex")) return;
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

  /** After a whole-project restore: drop every cached file and reopen the tabs. */
  async function onRestoredAll() {
    const reopen = [...tabs];
    const wasActive = active;
    for (const p of reopen) {
      contents.delete(p);
      editor?.forget(p);
      dirty[p] = false;
    }
    tabs = [];
    active = null;
    await refreshTree();
    for (const p of reopen) {
      if (allFiles(project?.tree ?? []).includes(p)) await openFile(p);
    }
    if (wasActive && tabs.includes(wasActive)) await openFile(wasActive);
    refreshOutline();
    runBuild();
  }

  async function takeSnapshot(label: string) {
    lastSnapshot = Date.now();
    editedSinceSnapshot = false;
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

  /** Switches to a build preset from the project settings. */
  async function setTarget(t: Target) {
    if (!project) return;
    project = await api.setConfig({ ...project.config, main: t.main, engine: t.engine ?? project.config.engine });
    await openFile(project.main);
    applySpellLanguage();
    setMathMacros(contents.get(project.main) ?? "");
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
      showNotice(`Couldn't jump to the source: ${errorMessage(e)}`);
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

  /** Rough word count of a selection: counts words outside comments and commands. */
  function countWords(text: string): number {
    if (!text) return 0;
    return text
      .split("\n")
      .map((l) => l.replace(/(^|[^\\])%.*$/, "$1"))
      .join(" ")
      .replace(/\\[a-zA-Z]+\*?/g, " ")
      .replace(/[{}$&_^~\\[\]]/g, " ")
      .split(/\s+/)
      .filter((w) => /[\p{L}\p{N}]/u.test(w)).length;
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

  const headingItems = $derived<PaletteItem[]>(
    headings.map((h, i) => ({
      id: `${h.file}:${h.line}:${i}`,
      label: `${"\u2003".repeat(Math.max(0, h.level - 1))}${h.title || "(untitled)"}`,
      detail: `${h.file.split("/").pop()}:${h.line}`,
      run: () => gotoProblem(h.file, h.line),
    })),
  );

  async function openSymbols() {
    try {
      const defs = await api.symbolDefs();
      symbolItems = defs.map((d) => ({
        id: `${d.kind}:${d.file}:${d.line}:${d.name}`,
        label: d.name,
        detail: `${d.kind === "label" ? "label" : "citation"} · ${d.file}:${d.line}`,
        run: () => gotoProblem(d.file, d.line),
      }));
      palette = "symbols";
    } catch (e) {
      showNotice(errorMessage(e));
    }
  }

  const commandItems = $derived<PaletteItem[]>(
    [
      { id: "recompile", label: "Recompile", detail: hint("recompile"), run: () => saveAll(false).then(runBuild) },
      { id: "open-file", label: "Open file…", detail: hint("quick-open"), run: () => (palette = "files") },
      { id: "search", label: "Search in project", detail: hint("search"), run: openSearch },
      { id: "cite", label: "Insert citation", detail: hint("cite"), run: () => (citeOpen = true) },
      { id: "figure", label: "Insert figure…", run: () => (dialog = "figure") },
      { id: "table", label: "Insert table…", run: () => (dialog = "table") },
      { id: "symbols", label: "Insert symbol…", detail: hint("symbols"), run: () => (dialog = "symbols") },
      { id: "wrap-env", label: "Wrap selection in an environment…", detail: hint("wrap-env"), run: () => (palette = "environments") },
      { id: "words", label: "Word count", run: () => (dialog = "words") },
      { id: "shortcuts", label: "Keyboard shortcuts", detail: hint("shortcuts"), run: () => (dialog = "shortcuts") },
      { id: "snapshot", label: "Save a snapshot", detail: hint("snapshot"), run: () => (snapshotLabel = "") },
      { id: "history", label: "Show history", run: () => (historyOpen = true) },
      { id: "sync", label: "Show this line in the PDF", detail: hint("sync"), run: () => active && syncForward(active, editor?.currentLine() ?? 1, 0) },
      { id: "focus", label: focusMode ? "Leave focus mode" : "Focus mode", detail: hint("focus"), run: () => (focusMode = !focusMode) },
      { id: "new-file", label: "New file…", run: () => fileActions?.openMenu(null, 240, 120) },
      { id: "goto-heading", label: "Go to a section…", detail: hint("goto-heading"), run: () => (palette = "headings") },
      { id: "goto-symbol", label: "Go to a label or citation…", run: openSymbols },
      { id: "rename-symbol", label: "Rename label or citation key…", run: () => (renaming = editor?.symbolAtCursor() ?? { kind: "label", name: "" }) },
      { id: "reopen-tab", label: "Reopen the last closed tab", detail: hint("reopen-tab"), run: reopenClosed },
      { id: "reveal", label: "Show the open file in the file tree", run: revealActive },
      { id: "unused", label: "Find unused files…", run: () => fileActions?.showUnused() },
      { id: "undo-files", label: "Undo the last file operation", run: () => fileActions?.undo() },
      { id: "zotero-cite", label: "Cite from Zotero…", run: citeFromZotero },
      { id: "zotero-sync", label: "Sync zotero.bib from Zotero", run: () => syncZotero(true) },
      { id: "export-pdf", label: "Export PDF…", run: exportPdf },
      { id: "save-template", label: "Save this project as a template…", run: () => (savingTemplate = true) },
      { id: "export-zip", label: "Export the whole project (.zip)…", run: exportZip },
      { id: "export-arxiv", label: "Export for arXiv (.zip)…", run: exportArxiv },
      { id: "project-settings", label: "Project settings", run: () => (settingsOpen = true) },
      { id: "preferences", label: "Preferences", run: () => (prefsOpen = true) },
      { id: "guide", label: "Open the user guide", run: () => openUrl("https://github.com/NilsBriggen/lemmata/blob/main/docs/guide.md") },
      { id: "report", label: "Report a problem…", run: startReport },
      { id: "close", label: "Close project", run: closeProject },
    ].filter(() => project),
  );

  async function citeFromZotero() {
    citeOpen = false;
    showNotice("Choose references in Zotero's picker…");
    try {
      const keys = await api.zoteroPick();
      notice = null;
      if (!keys.length) return;
      editor?.insertCitation(keys);
      await saveAll();
      await syncZotero(false);
    } catch (e) {
      showNotice(errorMessage(e));
    }
  }

  /** Writes zotero.bib for the cited keys; `report` shows a summary even when all went well. */
  async function syncZotero(report = true) {
    try {
      const r = await api.zoteroSync();
      contents.delete("zotero.bib");
      editor?.forget("zotero.bib");
      await refreshTree();
      if (!r.linked) {
        showNotice("Your document doesn't load zotero.bib yet.", {
          label: "Add it",
          run: async () => {
            try {
              await save(project!.main);
              await api.zoteroLink();
              await reloadFromDisk(project!.main);
              runBuild();
            } catch (e) {
              showNotice(errorMessage(e));
            }
          },
        });
      } else if (r.missing.length) {
        showNotice(`Not in Zotero: ${r.missing.join(", ")}.`);
      } else if (report) {
        showNotice(`zotero.bib has ${r.written} reference${r.written === 1 ? "" : "s"}.`);
      }
      runBuild();
    } catch (e) {
      showNotice(errorMessage(e));
    }
  }

  async function startReport() {
    const version = await getVersion().catch(() => "?");
    report = { system: systemSummary(version, tex), log: errorSummary(diagnostics) };
  }

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
      const name = dest.split(/[\\/]/).pop();
      if (project.config.draft) {
        showNotice(`Saved ${name}, but it was built in Fast (draft) mode, so figures are empty boxes.`, {
          label: "Switch to Normal",
          run: () => setBuildOption({ draft: false }),
        });
      } else showNotice(`Saved ${name}.`);
    } catch (e) {
      showNotice(errorMessage(e));
    }
  }

  async function exportZip() {
    if (!project) return;
    await saveAll(false);
    const dest = await saveDialog({ title: "Export the project as a zip", defaultPath: `${project.name}.zip`, filters: [{ name: "Zip", extensions: ["zip"] }] });
    if (!dest) return;
    try {
      const n = await api.exportZip(dest, true);
      showNotice(`Saved ${n} files, with the latest PDF, to ${dest.split(/[\\/]/).pop()}.`, { tone: "ok" });
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
  async function onFilesChanged(change: { changed: string[]; removed: string[] }) {
    if (!project) return;
    await refreshTree();
    if (change.removed.length) await onRemovedOutside(change.removed);
    for (const path of change.changed) {
      if (!contents.has(path)) continue;
      const disk = await api.readText(path).catch((e) => {
        reportOnce("read-changed", `${path} changed outside Lemmata but could not be read: ${errorMessage(e)}`);
        return null;
      });
      if (disk === null) continue;
      // What we last read or wrote (our own autosave), or what's in the editor, isn't an outside change.
      if (disk === lastSaved.get(path)) continue;
      if (disk === contents.get(path)) {
        lastSaved.set(path, disk);
        continue;
      }
      if (dirty[path]) {
        showNotice(`${path} changed on disk.`, { label: "Load the new version", run: () => reloadFromDisk(path) });
      } else {
        await reloadFromDisk(path);
      }
    }
    refreshOutline();
  }

  /** Files or folders deleted by another program: close their tabs quietly, without saving. */
  async function onRemovedOutside(removed: string[]) {
    const gone = (p: string) => removed.some((r) => p === r || p.startsWith(r + "/"));
    const closing = tabs.filter(gone);
    const cached = [...contents.keys()].filter(gone);
    for (const p of cached) {
      clearTimeout(saveTimers.get(p));
      contents.delete(p);
      lastSaved.delete(p);
      dirty[p] = false;
      editor?.forget(p);
    }
    const wasActive = active;
    if (closing.length) {
      tabs = tabs.filter((t) => !gone(t));
      if (wasActive && gone(wasActive)) {
        active = null;
        const next = tabs.at(-1);
        if (next) await openFile(next);
      }
    }
    // Deleting or moving files inside Lemmata reports here too, after the tabs are already gone.
    if (closing.length || cached.length) {
      showNotice(removed.length === 1 ? `${removed[0]} was removed outside Lemmata.` : `${removed.length} files or folders were removed outside Lemmata.`, { tone: "info" });
    }
    if (project && gone(project.main)) {
      try {
        project = await api.openProject(project.root);
      } catch (e) {
        showNotice(`The main document was removed. ${errorMessage(e)}`);
      }
    }
    openError = null;
  }

  async function reloadFromDisk(path: string) {
    contents.delete(path);
    lastSaved.delete(path);
    editor?.forget(path);
    dirty[path] = false;
    if (active === path) await openFile(path);
  }

  /** Which tab to reopen once the tree has refreshed; set by the first rename of a batch. */
  let reopenAfterMove: string | null | undefined;
  let moveTimer: ReturnType<typeof setTimeout> | undefined;

  function onRenamed(from: string, to: string) {
    for (const p of [...contents.keys()]) {
      if (p === from || p.startsWith(from + "/")) {
        contents.delete(p);
        editor?.forget(p);
      }
    }
    const moved = (p: string) => (p === from ? to : p.startsWith(from + "/") ? to + p.slice(from.length) : p);
    tabs = tabs.map(moved);
    // Several renames can arrive together (moving a selection): follow the active file through all of them.
    if (reopenAfterMove === undefined) reopenAfterMove = active;
    if (reopenAfterMove) reopenAfterMove = moved(reopenAfterMove);
    active = null;
    clearTimeout(moveTimer);
    moveTimer = setTimeout(async () => {
      await refreshTree();
      const reopen = reopenAfterMove;
      reopenAfterMove = undefined;
      if (reopen) openFile(reopen);
      refreshOutline();
    }, 0);
  }

  function onTrashed(path: string) {
    for (const p of [...tabs]) if (p === path || p.startsWith(path + "/")) closeTab(p);
    refreshTree();
    refreshOutline();
  }

  function onKey(e: KeyboardEvent) {
    if (!project && !matches(e, "save")) return;
    const hit = (id: string) => {
      if (!matches(e, id)) return false;
      e.preventDefault();
      return true;
    };
    if (hit("focus")) focusMode = !focusMode;
    else if (hit("palette")) palette = "commands";
    else if (hit("quick-open")) palette = "files";
    else if (hit("goto-heading")) palette = "headings";
    else if (hit("search")) openSearch();
    else if (active && hit("cite")) citeOpen = true;
    else if (active && hit("symbols")) dialog = "symbols";
    else if (hit("shortcuts")) dialog = "shortcuts";
    else if (hit("snapshot")) snapshotLabel = "";
    else if (hit("save")) saveAll();
    else if (hit("recompile")) saveAll(false).then(runBuild);
    else if (active && hit("close-tab")) closeTab(active);
    else if (hit("reopen-tab")) reopenClosed();
    else if (hit("next-tab")) cycleTab(1);
    else if (hit("prev-tab")) cycleTab(-1);
  }

  function cycleTab(dir: 1 | -1) {
    if (tabs.length < 2 || !active) return;
    openFile(tabs[(tabs.indexOf(active) + dir + tabs.length) % tabs.length]);
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

  // Editor options that change the extension list rebuild the editors.
  let appliedEditor = "";
  $effect(() => {
    const key = [settings.keymap, settings.autoClose, settings.autocomplete, settings.envSync, settings.folding, settings.wordWrap, settings.indent].join();
    if (appliedEditor && key !== appliedEditor) editor?.refreshExtensions();
    appliedEditor = key;
  });

  // Pane widths change continuously while dragging; save once it settles.
  let widthTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    void [settings.sidebarWidth, settings.pdfWidth];
    clearTimeout(widthTimer);
    widthTimer = setTimeout(() => updateSettings({}), 400);
  });

  onMount(async () => {
    // Test automation exists only in development and test builds; the flag is replaced at build time, so a release build drops it.
    if (import.meta.env.DEV || import.meta.env.VITE_LEMMATA_TEST === "1") {
      import("./lib/automation").then((m) =>
        m.install({
          state: () => ({
            project: project?.root ?? null,
            main: project?.main ?? null,
            tabs: [...tabs],
            active,
            dirty: Object.keys(dirty).filter((p) => dirty[p]),
            building,
            status: outcome?.status ?? null,
            diagnostics: diagnostics.map(({ severity, message, file, line }) => ({ severity, message, file, line })),
            layout: settings.layout,
            showPdf,
            hasPdf: pdf?.hasPdf() ?? false,
            logsOpen,
            notice: notice?.text ?? null,
            openError,
            dialogs: Object.entries({ settingsOpen, prefsOpen, citeOpen, historyOpen, searchOpen, choosingMain, savingTemplate, palette, dialog, renaming, welcome })
              .filter(([, v]) => !!v)
              .map(([k, v]) => (typeof v === "string" ? `${k}:${v}` : k)),
          }),
          buffers: () => Object.fromEntries(tabs.filter((t) => contents.has(t)).map((t) => [t, contents.get(t)!])),
          commands: () => commandItems,
          openProject,
          openFile,
        }),
      );
    }
    spell.onChange(() => (spellLang = spell.language));
    // Tauri delivers file drops with real paths (DOM drop events don't have them).
    getCurrentWebview()
      .onDragDropEvent((e) => {
        const scale = window.devicePixelRatio || 1;
        if (e.payload.type === "drop") {
          dragState.external = null;
          dropFiles(e.payload.paths, e.payload.position.x / scale, e.payload.position.y / scale);
        } else if (e.payload.type === "enter" || e.payload.type === "over") {
          const el = document.elementFromPoint(e.payload.position.x / scale, e.payload.position.y / scale)?.closest<HTMLElement>("[data-drop]");
          dragState.external = el?.dataset.drop ?? null;
        } else {
          dragState.external = null;
        }
      })
      .catch((e) => reportOnce("dragdrop", `Dropping files onto Lemmata won't work: ${errorMessage(e)}`));
    // Snapshots on a timer while there are unsnapshotted edits.
    setInterval(() => {
      if (project && editedSinceSnapshot && Date.now() - lastSnapshot > AUTO_SNAPSHOT_MS) takeSnapshot("Automatic snapshot");
    }, 60_000);
    listen<{ changed: string[]; removed: string[] }>("files-changed", (e) => onFilesChanged(e.payload)).catch((e) =>
      reportOnce("watch", `Lemmata can't watch for changes made by other programs: ${errorMessage(e)}`),
    );
    await loadSettings();
    appliedEditor = "";
    // Detection can take seconds (it runs pdflatex). With a remembered working result the app starts at
    // once and checks again in the background; without one it has to wait for the answer.
    const remembered = loadTexCache();
    if (remembered) {
      tex = remembered;
      recheck(settings.texDir ?? undefined).catch((e) => reportOnce("tex", `Could not check the TeX installation: ${errorMessage(e)}`));
    } else {
      await recheck(settings.texDir ?? undefined);
    }
    if (settings.checkForUpdates) {
      checkForUpdates();
      setInterval(() => settings.checkForUpdates && !update && checkForUpdates(), UPDATE_INTERVAL_MS);
    }
    // Existing users (with projects) never see the welcome.
    if (!settings.firstRunDone && recent.length === 0) {
      welcome = true;
    } else {
      if (!settings.firstRunDone) updateSettings({ firstRunDone: true });
      if (tex?.detection.ready && recent[0]) openProject(recent[0].path);
    }
  });

  const menuItems = $derived<MenuAction[]>([
    { label: "All projects", run: closeProject },
    { label: "Command palette", hint: hint("palette"), run: () => (palette = "commands"), separator: true },
    { label: "Open file…", hint: hint("quick-open"), run: () => (palette = "files") },
    { label: "Search in project", hint: hint("search"), run: openSearch },
    { label: "Insert citation", hint: hint("cite"), run: () => (citeOpen = true) },
    { label: "Word count", run: () => (dialog = "words") },
    { label: "Save a snapshot", hint: hint("snapshot"), run: () => (snapshotLabel = "") },
    { label: "Undo last file operation", run: () => fileActions?.undo() },
    { label: "Export PDF…", run: exportPdf, separator: true },
    { label: "Export project (.zip)…", run: exportZip },
    { label: "Export for arXiv…", run: exportArxiv },
    { label: "Preferences", run: () => (prefsOpen = true), separator: true },
    { label: "Keyboard shortcuts", hint: hint("shortcuts"), run: () => (dialog = "shortcuts") },
    { label: "User guide", run: () => openUrl("https://github.com/NilsBriggen/lemmata/blob/main/docs/guide.md") },
    { label: "Report a problem…", run: startReport },
  ]);

  const showEditor = $derived(settings.layout !== "pdf" || focusMode);
  const showPdf = $derived(settings.layout !== "editor" && !focusMode);

  /** Where the PDF was scrolled and zoomed when its pane was last hidden. */
  let hiddenPdfPosition: { fit: "width" | "page" | number; scroll: number } | null = null;

  // Switching to "Editor only" (or focus mode) removes the PDF pane, and its
  // viewer starts empty when it comes back: remember its place, then load the
  // PDF on disk again as soon as the new viewer exists.
  $effect.pre(() => {
    if (!showPdf && pdf) hiddenPdfPosition = untrack(() => pdf?.getPosition() ?? null);
  });
  $effect(() => {
    if (!pdf || !project) return;
    untrack(async () => {
      await showExistingPdf();
      if (hiddenPdfPosition) {
        pdf?.setPosition(hiddenPdfPosition);
        hiddenPdfPosition = null;
      }
    });
  });

  const showSidebar = $derived(settings.sidebarOpen && !focusMode && settings.layout !== "pdf");
  const words = new Intl.NumberFormat();

  async function startFromWelcome(kind: "example" | "done") {
    welcome = false;
    await updateSettings({ firstRunDone: true });
    if (kind === "example") {
      try {
        openProject(await api.createExample());
      } catch (e) {
        openError = errorMessage(e);
      }
    }
  }
</script>

<svelte:window onkeydown={onKey} />

{#if !tex}
  <div class="loading"><span class="wordmark">Lemmata</span></div>
{:else if welcome}
  <Welcome status={tex} {checking} onRecheck={recheck} onFinish={startFromWelcome} onOpen={(path) => { updateSettings({ firstRunDone: true }); openProject(path); }} onImported={(paths) => {
      updateSettings({ firstRunDone: true });
      welcome = false;
      recent = rememberProjects(paths);
      if (paths.length === 1) openProject(paths[0]);
      else showNotice(`Imported ${paths.length} projects.`, { tone: "ok" });
    }} />
{:else if !tex.detection.ready}
  <Onboarding status={tex} {checking} onRecheck={recheck} />
{:else if !project}
  <Library
    {recent}
    error={openError}
    onOpen={openProject}
    onForget={(p) => (recent = forgetProject(p))}
    onImported={(paths) => {
      recent = rememberProjects(paths);
      if (paths.length === 1) openProject(paths[0]);
      else showNotice(`Imported ${paths.length} projects.`, { tone: "ok" });
    }}
    onExample={() => startFromWelcome("example")}
    onWelcome={() => (welcome = true)}
    onPreferences={() => (prefsOpen = true)}
  />
{:else}
  <div class="shell">
    <Toolbar
      projectName={project.name}
      {building}
      layout={settings.layout}
      sidebarOpen={settings.sidebarOpen}
      {historyOpen}
      autoCompile={settings.autoCompile}
      engine={project.config.engine}
      menu={menuItems}
      onLayout={(l) => {
        focusMode = false;
        updateSettings({ layout: l });
      }}
      onToggleSidebar={() => updateSettings({ sidebarOpen: !settings.sidebarOpen })}
      onHistory={() => (historyOpen = !historyOpen)}
      onProjectSettings={() => (settingsOpen = true)}
      onRecompile={() => saveAll(false).then(runBuild)}
      onStop={stopBuild}
      onAutoCompile={(on) => updateSettings({ autoCompile: on })}
      draft={project.config.draft ?? false}
      stopOnError={project.config.stop_on_error ?? false}
      onDraft={(draft) => setBuildOption({ draft })}
      onStopOnError={(stop_on_error) => setBuildOption({ stop_on_error })}
      onClean={cleanCache}
      onEngine={setEngine}
      onLibrary={closeProject}
      targets={project.config.targets ?? []}
      mainFile={project.main}
      onTarget={setTarget}
    />

    <div class="panes" class:focus={focusMode} style:--side-w="{showSidebar ? settings.sidebarWidth : 0}px">
      {#if showSidebar}
        <div class="side" style:width="{settings.sidebarWidth}px">
          {#if searchOpen}
            <div class="search-wrap chrome">
              <SearchPanel
                onOpen={gotoProblem}
                beforeReplace={async () => {
                  await saveAll();
                  await takeSnapshot("Before replace");
                }}
                onReplaced={async (paths) => {
                  for (const p of paths) await reloadFromDisk(p);
                  runBuild();
                }}
                onClose={() => (searchOpen = false)}
              />
            </div>
          {:else}
            <Sidebar
              tree={project.tree}
              {active}
              main={project.main}
              {headings}
              {currentHeading}
              onOpen={openFile}
              onJump={gotoProblem}
              onContext={(entry, x, y) => fileActions?.openMenu(entry, x, y)}
              onNewFile={() => fileActions?.newItem(false)}
              onNewFolder={() => fileActions?.newItem(true)}
              onUpload={() => uploadFiles()}
              onSearch={openSearch}
              actions={fileActions}
            />
          {/if}
        </div>
        <Splitter
          size={settings.sidebarWidth}
          min={180}
          max={480}
          label="Resize the file panel"
          onResize={(w) => (settings.sidebarWidth = w)}
          onReset={() => updateSettings({ sidebarWidth: DEFAULTS.sidebarWidth })}
        />
      {/if}

      <section class="code" class:hidden={!showEditor}>
        <div class="tabstrip">
          <TabStrip
            {tabs}
            {active}
            {dirty}
            {pinned}
            closed={closedTabs}
            onPin={togglePin}
            onReopen={reopenClosed}
            onSelect={openFile}
            onClose={closeTab}
            onReorder={(next) => (tabs = pinFirst(next))}
          />
        </div>
        <EditorToolbar
          disabled={!active || !/\.(tex|sty|cls)$/.test(active)}
          onBold={() => editor?.toggleCommand("textbf")}
          onItalic={() => editor?.toggleCommand("textit")}
          onHeading={(c) => editor?.heading(c)}
          onMath={() => editor?.wrapWith("$", "$")}
          onSymbols={() => (dialog = "symbols")}
          onLink={() => editor?.insertTemplate("\\href{${https://}}{${}}")}
          onFigure={() => (dialog = "figure")}
          onTable={() => (dialog = "table")}
          onCite={() => (citeOpen = true)}
          onList={(ordered) => editor?.insertTemplate(listTemplate(ordered))}
        />
        {#if openError}
          <div class="err-bar" role="alert">
            <span>{openError}</span>
            <button class="btn ghost icon sm" aria-label="Dismiss" onclick={() => (openError = null)}><X size={14} /></button>
          </div>
        {/if}
        {#if viewerActive && active}
          <div class="cm">{#key active}<FileViewer path={active} />{/key}</div>
        {/if}
        {#if !active}
          <div class="no-file">
            <p>No file is open.</p>
            <p class="muted">Pick one in the file tree, or press {hint("quick-open")} to find it.</p>
          </div>
        {/if}
        <div class="cm" class:hidden={viewerActive || !active}>
          <Editor
            bind:this={editor}
            {onChange}
            onSyncRequest={syncForward}
            onCursor={(file, line) => (cursor = { file, line })}
            onAddWord={addWord}
            onPasteImage={pasteImage}
            onNeedsPackage={needsPackage}
            onWrapEnvironment={() => (palette = "environments")}
            onSelection={(t) => (selectionWords = countWords(t))}
            onFoldsChange={() => foldsVersion++}
            onPastedTable={(undo) => showNotice("Pasted as a table.", { label: "Paste as text instead", run: undo })}
          />
        </div>
        <footer class="status">
          <span>Line {cursor.line}</span>
          {#if selectionWords}<span title="Words in the selection">{selectionWords} selected</span>{/if}
          {#if active}<span>{dirty[active] ? "Unsaved" : "Saved"}</span>{/if}
          <span class="grow"></span>
          {#if lspStarting}<span class="starting" title="The language server is starting"><span class="spin-dot"></span>Starting completion</span>{:else if !lspAvailable}<span title="texlab wasn't found, so completion and hover are off">No completion</span>{/if}
          {#if tex.detection.outdated}<span class="warn" title="Some packages may lack newer features">Old TeX Live</span>{/if}
          {#if project.config.draft}<span class="warn" title="Figures are drawn as boxes. Switch to Normal in the Recompile menu for the final PDF.">Draft mode</span>{/if}
          {#if spellLang !== "off"}<span title="Spell-check language">{spellLang}</span>{/if}
          {#if totalWords}<span title="Words in the document body, without commands, maths or comments">{words.format(totalWords)} words</span>{/if}
          <span>{project.config.engine === "pdflatex" ? "pdfLaTeX" : project.config.engine === "xelatex" ? "XeLaTeX" : "LuaLaTeX"}</span>
          {#if outcome && !building}
            <span class={outcome.status === "success" ? "ok" : "bad"} title={lastCompiled ? `Finished at ${new Date(lastCompiled).toLocaleTimeString()}` : undefined}>
              {outcome.status === "success" ? `Compiled in ${(outcome.duration_ms / 1000).toFixed(1)} s` : outcome.status === "timeout" ? "Stopped: took too long" : errors ? `${errors} error${errors === 1 ? "" : "s"}` : "Compile failed"}
            </span>
          {/if}
          <button class="bell" bind:this={bellBtn} aria-label="Recent notifications" title="Recent notifications" onclick={() => ((noticesOpen = !noticesOpen), (seenNotices = notices.length))}>
            <Bell size={13} />{#if notices.length > seenNotices}<span class="pip"></span>{/if}
          </button>
        </footer>
      </section>

      {#if showPdf}
        {#if showEditor}
          <Splitter
            size={settings.pdfWidth}
            side="after"
            min={300}
            max={1400}
            label="Resize the PDF panel"
            onResize={(w) => (settings.pdfWidth = w)}
            onReset={() => updateSettings({ pdfWidth: DEFAULTS.pdfWidth })}
          />
        {/if}
        <div class="preview" style:width={showEditor ? `${settings.pdfWidth}px` : undefined} class:wide={!showEditor}>
          <div class="fill" class:hidden={historyOpen}>
            <PdfPane
              bind:this={pdf}
              {diagnostics}
              {building}
              failed={!!outcome && outcome.status !== "success" && !outcome.pdf}
              buildId={buildSeq}
              {logsOpen}
              onLogs={(o) => {
                logsOpen = o;
                if (!o) showExistingPdf();
              }}
              onSourceClick={syncBackward}
              onGoto={gotoProblem}
              onAddPackage={addPackage}
              getSource={sourceOf}
              onExport={exportPdf}
            />
          </div>
          {#if historyOpen}
            <div class="fill"><HistoryPanel version={historyVersion} {currentText} {onRestored} {onRestoredAll} onClose={() => (historyOpen = false)} /></div>
          {/if}
        </div>
      {/if}
    </div>
  </div>
{/if}

{#if noticesOpen && bellBtn}
  <DropMenu anchor={bellBtn} align="end" onClose={() => (noticesOpen = false)}>
    <div class="notices">
      {#each notices as n (n.time + n.text)}
        <p><time>{new Date(n.time).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</time> {n.text}</p>
      {:else}
        <p class="none">Nothing yet.</p>
      {/each}
    </div>
  </DropMenu>
{/if}

{#if packageOffer || notice}
  <div class="toasts">
    {#if packageOffer}
      <div class="toast" role="status">
        <span class="t-ico"><Package size={16} /></span>
        <span class="t-text">This needs the <code>{packageOffer}</code> package, which your document doesn't load yet.</span>
        <button class="btn sm ghost" onclick={() => (packageOffer = null)}>Not now</button>
        <button class="btn sm primary" onclick={() => addPackage(packageOffer!)}>Add it</button>
      </div>
    {/if}
    {#if notice}
      <div class="toast" role="status">
        <span class="t-ico" class:ok={notice.tone === "ok"}>{#if notice.tone === "ok"}<CircleCheck size={16} />{:else}<Info size={16} />{/if}</span>
        <span class="t-text">{notice.text}</span>
        {#if notice.action}<button class="btn sm" onclick={() => { notice?.action?.run(); notice = null; }}>{notice.action.label}</button>{/if}
        <button class="btn ghost icon sm" aria-label="Dismiss" onclick={() => (notice = null)}><X size={14} /></button>
      </div>
    {/if}
  </div>
{/if}

<FileActions
  root={project?.root ?? ""}
  entries={project?.tree ?? []}
  main={project?.main ?? ""}
  bind:this={fileActions}
  beforeChange={saveAll}
  onChanged={refreshTree}
  onSetMain={chooseMain}
  onNotice={(text, action) => showNotice(text, action)}
  onCreated={async (path, dir) => {
    await refreshTree();
    if (!dir && /\.(tex|bib|sty|cls|txt|md|toml)$/i.test(path)) openFile(path);
  }}
  {onRenamed}
  {onTrashed}
  onUpload={uploadFiles}
/>

{#if report}
  <Modal title="Report a problem" subtitle="Opens a new GitHub issue with these details. You can edit everything before submitting; file paths are left out." width={560} onClose={() => (report = null)}>
    <pre class="report">{report.system}{report.log ? `\n\n${report.log}` : ""}</pre>
    {#snippet footer()}
      <button class="btn" onclick={() => (report = null)}>Cancel</button>
      <button
        class="btn primary"
        onclick={() => {
          openUrl(issueUrl(report!.system, report!.log)).catch((e) => showNotice(errorMessage(e)));
          report = null;
        }}>Open GitHub</button
      >
    {/snippet}
  </Modal>
{/if}

{#if palette}
  <Palette
    items={palette === "files" ? fileItems : palette === "environments" ? environmentItems : palette === "headings" ? headingItems : palette === "symbols" ? symbolItems : commandItems}
    placeholder={palette === "files" ? "Open a file by name" : palette === "environments" ? "Wrap the selection in…" : palette === "headings" ? "Go to a section" : palette === "symbols" ? "Go to a label or citation key" : "Type a command"}
    onClose={() => {
      palette = null;
      editor?.getView()?.focus();
    }}
  />
{/if}

{#if savingTemplate && project}
  <SaveTemplateDialog
    projectName={project.name}
    onSaved={(name) => {
      savingTemplate = false;
      showNotice(`Saved the template "${name}". It appears when you create a new project.`, { tone: "ok" });
    }}
    onClose={() => (savingTemplate = false)}
  />
{/if}

{#if renaming && project}
  <RenameSymbol
    kind={renaming.kind}
    name={renaming.name}
    beforeApply={async () => {
      await saveAll();
      await takeSnapshot("Before rename");
    }}
    onApplied={async (files, text) => {
      for (const f of files) await reloadFromDisk(f);
      showNotice(text, { tone: "ok" });
      runBuild();
    }}
    onClose={() => (renaming = null)}
  />
{/if}

{#if dialog === "figure" && project}
  <FigureDialog
    tree={project.tree}
    onInsert={(snippet) => editor?.insertSnippet(snippet, "graphicx")}
    onClose={() => ((dialog = null), editor?.getView()?.focus())}
  />
{:else if dialog === "table"}
  <TableDialog
    onInsert={(template, needs) => editor?.insertBlock(template, needs)}
    onClose={() => ((dialog = null), editor?.getView()?.focus())}
  />
{:else if dialog === "symbols"}
  <SymbolPalette
    onInsert={(tex, needs) => {
      editor?.insertTemplate(templateOf(tex));
      if (needs) needsPackage(needs);
    }}
    onClose={() => ((dialog = null), editor?.getView()?.focus())}
  />
{:else if dialog === "words"}
  <WordCount onClose={() => (dialog = null)} />
{:else if dialog === "shortcuts"}
  <Shortcuts onClose={() => (dialog = null)} />
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
    onZotero={citeFromZotero}
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
  <div class="toasts right">
    <div class="toast" role="status">
      <span class="t-ico ok"><Sparkles size={16} /></span>
      <span class="t-text">
        {#if update.error}The update failed: {update.error}{:else if update.progress !== null}Downloading Lemmata {update.version}… {Math.round(update.progress * 100)}%{:else}Lemmata {update.version} is available.{/if}
      </span>
      {#if update.progress === null}
        <button class="btn sm ghost" onclick={() => (update = null)}>Later</button>
        <button class="btn sm primary" onclick={installUpdate}>Install and restart</button>
      {/if}
    </div>
  </div>
{/if}

{#if settingsOpen && project}
  <ProjectSettings {project} onSave={saveSettings} onClose={() => (settingsOpen = false)} />
{/if}

{#if snapshotLabel !== null}
  <Modal title="Save a snapshot" subtitle="Give it a name you'll recognise later, like “Sent to supervisor”." width={440} onClose={() => (snapshotLabel = null)}>
    <form id="snapshot-form" onsubmit={confirmSnapshot}>
      <!-- svelte-ignore a11y_autofocus -->
      <input id="snapshot-label" class="input" bind:value={snapshotLabel} placeholder="Snapshot" autofocus />
    </form>
    {#snippet footer()}
      <button class="btn" onclick={() => (snapshotLabel = null)}>Cancel</button>
      <button class="btn primary" type="submit" form="snapshot-form">Save snapshot</button>
    {/snippet}
  </Modal>
{/if}

{#if choosingMain && project}
  <Modal title="Which file is the main document?" subtitle="Several files start with \documentclass. Lemmata builds the one you choose and remembers it in lemmata.toml." width={480} onClose={() => (choosingMain = false)}>
    <div class="choices">
      {#each project.main_candidates as c (c)}
        <button class="btn choice" onclick={() => chooseMain(c)}>{c}</button>
      {/each}
    </div>
  </Modal>
{/if}

<style>
  .loading { height: 100%; display: grid; place-items: center; background: var(--chrome); }
  .wordmark { font: 28px var(--f-display); color: var(--chrome-ink); opacity: 0.7; }
  .shell { display: flex; flex-direction: column; height: 100%; background: var(--surface); }
  .panes { flex: 1; display: flex; min-height: 0; }
  .side { flex: none; min-width: 0; }
  .search-wrap { height: 100%; background: var(--chrome-2); border-right: 1px solid var(--chrome-border); color: var(--chrome-ink); }

  .code { flex: 1; display: flex; flex-direction: column; min-width: 280px; background: var(--code-bg); }
  .code.hidden { display: none; }
  .cm { flex: 1; min-height: 0; }
  .err-bar { display: flex; align-items: center; gap: 10px; padding: 6px 8px 6px 14px; background: var(--error-soft); color: var(--error); border-bottom: 1px solid var(--border); }
  .err-bar span { flex: 1; }
  .status {
    display: flex; align-items: center; gap: 16px; height: 26px; padding: 0 12px; flex: none;
    border-top: 1px solid var(--border); background: var(--surface-2); color: var(--muted); font-size: 12px;
    white-space: nowrap; overflow: hidden;
  }
  .status .grow { flex: 1; }
  .status .ok { color: var(--ok); }
  .status .bad, .status .warn { color: var(--error); }

  /* The editor always keeps at least 320px, however wide the PDF was dragged. */
  .preview { flex: none; display: flex; min-width: 0; max-width: calc(100% - var(--side-w) - 320px); position: relative; border-left: 1px solid var(--border); }
  .preview.wide { flex: 1; border-left: none; }
  .fill { position: absolute; inset: 0; }
  .fill.hidden { visibility: hidden; }

  /* Focus mode: only the editor, as a centred column. */
  .panes.focus .tabstrip { display: none; }
  .cm.hidden { display: none; }
  .no-file { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 4px; color: var(--ink); }
  .no-file p { margin: 0; }
  .no-file .muted { color: var(--muted); }
  .bell { position: relative; border: none; background: none; color: var(--muted); padding: 0 4px; display: inline-flex; align-items: center; }
  .bell:hover { color: var(--ink); }
  .bell .pip { position: absolute; top: -1px; right: 1px; width: 6px; height: 6px; border-radius: 50%; background: var(--accent); }
  .notices { max-width: 360px; max-height: 280px; overflow: auto; padding: 4px 10px; }
  .notices p { margin: 6px 0; font-size: 12.5px; line-height: 1.4; }
  .notices time { color: var(--muted); font-variant-numeric: tabular-nums; margin-right: 6px; }
  .notices .none { color: var(--muted); }
  .starting { display: inline-flex; align-items: center; gap: 6px; }
  .spin-dot { width: 8px; height: 8px; border: 1.5px solid var(--muted); border-top-color: transparent; border-radius: 50%; animation: spin 0.8s linear infinite; }
  .panes.focus .code :global(.cm-content) { max-width: 88ch; margin: 0 auto; }

  .toasts { position: fixed; left: 50%; bottom: 20px; transform: translateX(-50%); z-index: 45; display: flex; flex-direction: column; gap: 8px; }
  .toasts.right { left: auto; right: 20px; transform: none; }
  .toast {
    display: flex; gap: 10px; align-items: center; background: var(--surface); border: 1px solid var(--border);
    border-radius: var(--r-lg); padding: 8px 8px 8px 12px; box-shadow: var(--shadow); max-width: min(720px, 90vw);
    animation: pop 0.16s var(--ease);
  }
  .t-ico { display: inline-flex; color: var(--accent); }
  .t-ico.ok { color: var(--ok); }
  .t-text { flex: 1; min-width: 0; }
  .report { background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--r); padding: 10px 12px; font: 12px var(--f-mono); white-space: pre-wrap; max-height: 260px; overflow: auto; margin: 0; }
  .choices { display: flex; flex-direction: column; gap: 6px; }
  .choice { justify-content: flex-start; font-family: var(--f-mono); height: 36px; }
</style>
