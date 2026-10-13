<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage, type Entry, type Config, type Diagnostic, type Engine, type Heading, type Outcome, type ProjectInfo, type TexStatus } from "./lib/api";
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
  import { display, keysOf, matches } from "./lib/shortcuts";
  import Palette, { type PaletteItem } from "./lib/Palette.svelte";
  import { ENVIRONMENTS } from "./lib/editor/data";
  import type { Providers } from "./lib/editor/completions";
  import SearchPanel from "./lib/SearchPanel.svelte";
  import FileActions from "./lib/FileActions.svelte";
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { loadSession, saveSession } from "./lib/session";
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
  let active = $state<string | null>(null);
  let dirty = $state<Record<string, boolean>>({});
  const contents = new Map<string, string>();
  const saveTimers = new Map<string, ReturnType<typeof setTimeout>>();

  /** Shown when several files could be the main file and none is chosen. */
  let choosingMain = $state(false);
  let settingsOpen = $state(false);
  let prefsOpen = $state(false);
  let citeOpen = $state(false);
  let dialog = $state<null | "figure" | "table" | "symbols" | "words" | "shortcuts">(null);
  const hint = (id: string) => display(keysOf(id));
  let selectionWords = $state(0);
  let palette = $state<null | "commands" | "files" | "environments">(null);
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
  const AUTO_SNAPSHOT_MS = 10 * 60 * 1000;
  let lastThumbnail = 0;

  let building = $state(false);
  let outcome = $state<Outcome | null>(null);
  let diagnostics = $state<Diagnostic[]>([]);
  /** Logs replace the PDF in the right pane, like Overleaf's logs view. */
  let logsOpen = $state(false);
  let buildSeq = $state(0);

  let headings = $state<Heading[]>([]);
  let totalWords = $state(0);
  let cursor = $state<{ file: string; line: number }>({ file: "", line: 1 });
  let lsp: LanguageServer | null = null;
  let lspAvailable = $state(true);
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

  // Completion data for \ref, \cite, file names and packages. Labels and
  // citation keys change as the user types, so they're cached only briefly.
  function cached<T>(load: () => Promise<T>, ttl: number): () => Promise<T> {
    let at = 0;
    let value: Promise<T> | null = null;
    return () => {
      if (!value || Date.now() - at > ttl) {
        at = Date.now();
        value = load().catch(() => [] as unknown as T);
      }
      return value;
    };
  }
  const providers: Providers = {
    labels: cached(() => api.labels(), 1500),
    cites: cached(async () => (await api.bibEntries()).entries, 3000),
    files: () => (project ? allFiles(project.tree) : []),
    packages: cached(() => api.packageNames(), 3_600_000),
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
    lsp = await startLanguageServer(root, async (rel) => {
      await openFile(rel);
      return editor?.getView() ?? null;
    });
    lspAvailable = lsp !== null;
    editor?.setLanguageServer(lsp ? lsp.extensions : null);
  }

  function applySpellLanguage() {
    if (!project) return;
    const fallback = settings.spellDefault === "auto" ? detectLanguage(contents.get(project.main) ?? "") : settings.spellDefault;
    const lang = project.config.spellcheck ?? fallback;
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

  async function save(path: string, build = settings.autoCompile) {
    clearTimeout(saveTimers.get(path));
    const text = contents.get(path);
    if (text === undefined || !dirty[path]) return;
    try {
      await api.writeText(path, text);
      dirty[path] = false;
      if (build) runBuild();
      if (path.endsWith(".tex")) refreshOutline();
      if (path === project?.main) setMathMacros(text);
      if (Date.now() - lastSnapshot > AUTO_SNAPSHOT_MS) takeSnapshot("Automatic snapshot");
    } catch (e) {
      openError = errorMessage(e);
    }
  }

  async function saveAll(build = settings.autoCompile) {
    await Promise.all(Object.keys(dirty).filter((p) => dirty[p]).map((p) => save(p, build)));
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
      // Like Overleaf: a failed build with no PDF opens the logs.
      if ((result.status === "failed" || result.status === "timeout") && !result.pdf && errors > 0) logsOpen = true;
      if (result.pdf) {
        await pdf?.load(await api.readPdf());
        saveThumbnail();
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

  async function stopBuild() {
    buildSeq++;
    building = false;
    await api.stopBuild().catch(() => {});
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

  /** `extra` is either an action button or just a tone. */
  function showNotice(text: string, extra?: { label: string; run: () => void } | { tone: "ok" | "info" }) {
    clearTimeout(noticeTimer);
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
    const known = new Set(allFiles(project?.tree ?? []));
    for (const t of session.tabs) if (known.has(t) && t !== active) await openFile(t);
    if (session.active && known.has(session.active)) {
      await openFile(session.active);
      editor?.gotoLine(session.line);
    }
    if (session.pdf && "fit" in session.pdf) pdf?.setPosition(session.pdf);
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
      { id: "zotero-cite", label: "Cite from Zotero…", run: citeFromZotero },
      { id: "zotero-sync", label: "Sync zotero.bib from Zotero", run: () => syncZotero(true) },
      { id: "export-pdf", label: "Export PDF…", run: exportPdf },
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
    if (!project && !matches(e, "save")) return;
    const hit = (id: string) => {
      if (!matches(e, id)) return false;
      e.preventDefault();
      return true;
    };
    if (hit("focus")) focusMode = !focusMode;
    else if (hit("palette")) palette = "commands";
    else if (hit("quick-open")) palette = "files";
    else if (hit("search")) openSearch();
    else if (active && hit("cite")) citeOpen = true;
    else if (active && hit("symbols")) dialog = "symbols";
    else if (hit("shortcuts")) dialog = "shortcuts";
    else if (hit("snapshot")) snapshotLabel = "";
    else if (hit("save")) saveAll();
    else if (hit("recompile")) saveAll(false).then(runBuild);
    else if (active && hit("close-tab")) closeTab(active);
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
    const key = [settings.keymap, settings.autoClose, settings.autocomplete, settings.envSync, settings.folding, settings.wordWrap].join();
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
    spell.onChange(() => (spellLang = spell.language));
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
    appliedEditor = "";
    await recheck(settings.texDir ?? undefined);
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
    { label: "Export PDF…", run: exportPdf, separator: true },
    { label: "Export for arXiv…", run: exportArxiv },
    { label: "Preferences", run: () => (prefsOpen = true), separator: true },
    { label: "Keyboard shortcuts", hint: hint("shortcuts"), run: () => (dialog = "shortcuts") },
    { label: "User guide", run: () => openUrl("https://github.com/NilsBriggen/lemmata/blob/main/docs/guide.md") },
    { label: "Report a problem…", run: startReport },
  ]);

  const showEditor = $derived(settings.layout !== "pdf" || focusMode);
  const showPdf = $derived(settings.layout !== "editor" && !focusMode);
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
    />

    <div class="panes" class:focus={focusMode} style:--side-w="{showSidebar ? settings.sidebarWidth : 0}px">
      {#if showSidebar}
        <div class="side" style:width="{settings.sidebarWidth}px">
          {#if searchOpen}
            <div class="search-wrap chrome">
              <SearchPanel
                onOpen={gotoProblem}
                beforeReplace={saveAll}
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
        <div class="tabs">
          {#each tabs as t (t)}
            <div class="tab" class:on={t === active}>
              <button class="name" onclick={() => openFile(t)} title={t}>
                {t.split("/").pop()}
              </button>
              <button class="x" class:dirty={dirty[t]} aria-label={dirty[t] ? `${t} has unsaved changes; close` : `Close ${t}`} onclick={() => closeTab(t)}>
                <span class="dot"></span><X size={13} />
              </button>
            </div>
          {/each}
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
          onList={(ordered) => editor?.insertTemplate(`\\begin{${ordered ? "enumerate" : "itemize"}}\n\t\\item \${"{}"}\n\\end{${ordered ? "enumerate" : "itemize"}}`)}
        />
        {#if openError}
          <div class="err-bar" role="alert">
            <span>{openError}</span>
            <button class="btn ghost icon sm" aria-label="Dismiss" onclick={() => (openError = null)}><X size={14} /></button>
          </div>
        {/if}
        <div class="cm">
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
            onPastedTable={(undo) => showNotice("Pasted as a table.", { label: "Paste as text instead", run: undo })}
          />
        </div>
        <footer class="status">
          <span>Line {cursor.line}</span>
          {#if selectionWords}<span title="Words in the selection">{selectionWords} selected</span>{/if}
          {#if active}<span>{dirty[active] ? "Unsaved" : "Saved"}</span>{/if}
          <span class="grow"></span>
          {#if !lspAvailable}<span title="texlab wasn't found, so completion and hover are off">No completion</span>{/if}
          {#if tex.detection.outdated}<span class="warn" title="Some packages may lack newer features">Old TeX Live</span>{/if}
          {#if spellLang !== "off"}<span title="Spell-check language">{spellLang}</span>{/if}
          {#if totalWords}<span title="Words in the document body, without commands, maths or comments">{words.format(totalWords)} words</span>{/if}
          <span>{project.config.engine === "pdflatex" ? "pdfLaTeX" : project.config.engine === "xelatex" ? "XeLaTeX" : "LuaLaTeX"}</span>
          {#if outcome && !building}
            <span class={outcome.status === "success" ? "ok" : "bad"}>
              {outcome.status === "success" ? `Compiled in ${(outcome.duration_ms / 1000).toFixed(1)} s` : outcome.status === "timeout" ? "Stopped: took too long" : errors ? `${errors} error${errors === 1 ? "" : "s"}` : "Compile failed"}
            </span>
          {/if}
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
              buildId={buildSeq}
              {logsOpen}
              onLogs={(o) => (logsOpen = o)}
              onSourceClick={syncBackward}
              onGoto={gotoProblem}
              onAddPackage={addPackage}
              getSource={sourceOf}
              onExport={exportPdf}
            />
          </div>
          {#if historyOpen}
            <div class="fill"><HistoryPanel version={historyVersion} {currentText} {onRestored} onClose={() => (historyOpen = false)} /></div>
          {/if}
        </div>
      {/if}
    </div>
  </div>
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
  bind:this={fileActions}
  beforeChange={saveAll}
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
    items={palette === "files" ? fileItems : palette === "environments" ? environmentItems : commandItems}
    placeholder={palette === "files" ? "Open a file by name" : palette === "environments" ? "Wrap the selection in…" : "Type a command"}
    onClose={() => {
      palette = null;
      editor?.getView()?.focus();
    }}
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
    onInsert={(text, needs) => editor?.insertSnippet({ text, cursor: text.length }, needs)}
    onClose={() => ((dialog = null), editor?.getView()?.focus())}
  />
{:else if dialog === "symbols"}
  <SymbolPalette
    onInsert={(tex, needs) => {
      editor?.insertText(tex);
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
  .tabs { display: flex; height: 36px; background: var(--surface-2); border-bottom: 1px solid var(--border); overflow-x: auto; flex: none; scrollbar-width: none; }
  .tab { display: flex; align-items: center; border-right: 1px solid var(--border); position: relative; max-width: 220px; }
  .tab.on { background: var(--code-bg); }
  .tab.on::after { content: ""; position: absolute; left: 0; right: 0; bottom: -1px; height: 1px; background: var(--code-bg); }
  .tab.on::before { content: ""; position: absolute; left: 0; right: 0; top: 0; height: 2px; background: var(--accent); }
  .tab button { border: none; background: none; }
  .tab .name { font: 13px var(--f-body); color: var(--muted); padding: 0 4px 0 14px; height: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tab.on .name { color: var(--ink); }
  .tab .x { width: 26px; height: 26px; margin-right: 4px; border-radius: var(--r-sm); display: grid; place-items: center; color: var(--faint); position: relative; }
  .tab .x :global(svg) { opacity: 0; grid-area: 1 / 1; }
  .tab .x .dot { grid-area: 1 / 1; width: 7px; height: 7px; border-radius: 50%; background: transparent; }
  .tab .x.dirty .dot { background: var(--accent); }
  .tab:hover .x :global(svg), .tab.on .x:not(.dirty) :global(svg), .tab .x:focus-visible :global(svg) { opacity: 1; }
  .tab:hover .x .dot, .tab .x:focus-visible .dot { opacity: 0; }
  .tab .x:hover { background: var(--surface-3); color: var(--ink); }
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
  .panes.focus .tabs { display: none; }
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
