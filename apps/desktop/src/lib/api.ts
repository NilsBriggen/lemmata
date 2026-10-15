// Typed wrappers around the Tauri commands in src-tauri/src/lib.rs.
import { invoke } from "@tauri-apps/api/core";

export type Severity = "error" | "warning" | "info";
export type DiagnosticKind =
  | { type: "missing-file"; name: string }
  | { type: "undefined-citation"; key: string }
  | { type: "undefined-reference"; key: string }
  | { type: "undefined-control-sequence" }
  | { type: "bad-box" }
  | { type: "other" };

export interface Diagnostic {
  severity: Severity;
  kind: DiagnosticKind;
  file: string | null;
  line: number | null;
  message: string;
  hint: string | null;
  fix?: { explanation: string; command: string | null; package?: string };
}

export type InstallRequest =
  | { what: "tex"; platform: string; full: boolean }
  | { what: "package"; file: string };

export interface Outcome {
  status: "success" | "failed" | "cancelled" | "timeout";
  duration_ms: number;
  pdf: string | null;
  diagnostics: Diagnostic[];
}

export interface Tool { name: string; path: string | null; required: boolean }
export type Distribution =
  | { name: "texlive"; year: number | null }
  | { name: "miktex"; version: string | null }
  | { name: "unknown" };

export interface Detection {
  bin_dir: string | null;
  distribution: Distribution;
  tools: Tool[];
  ready: boolean;
  outdated: boolean;
}

export interface Instructions {
  label: string;
  recommended: string;
  full: string;
  recommended_size: string;
  full_size: string;
  link: string | null;
  note: string | null;
}

export type Family = "arch" | "debian" | "fedora" | "suse" | "macos" | "windows" | "other";

export interface TexStatus {
  detection: Detection;
  os: { family: Family; name: string };
  instructions: Record<string, Instructions>;
}

export type Engine = "pdflatex" | "xelatex" | "lualatex";

export interface Target { name: string; main: string; engine?: Engine | null }

export interface Config {
  main: string | null;
  engine: Engine;
  bib: "biber" | "bibtex";
  shell_escape: boolean;
  outdir: string;
  spellcheck?: string | null;
  words?: string[];
  /** Fast mode: figures are drawn as boxes. */
  draft?: boolean;
  stop_on_error?: boolean;
  timeout_secs?: number;
  /** Named build presets: another main file, and maybe another compiler. */
  targets?: Target[];
}

export interface Entry { name: string; path: string; is_dir: boolean; size: number; modified: number; children: Entry[] }
export type SymbolKind = "label" | "cite";
export interface Occurrence { kind: SymbolKind; role: "def" | "use"; name: string; file: string; line: number; col: number; text: string }
export interface Moved { from: string; to: string }
export interface TrashBatch { id: string; paths: string[] }

export interface ProjectInfo {
  root: string;
  name: string;
  main: string;
  main_candidates: string[];
  config: Config;
  tree: Entry[];
}

export interface Heading { level: number; title: string; file: string; line: number; words: number }
export interface OutlineData { headings: Heading[]; total_words: number }

export type HistoryStatus = { state: "on" } | { state: "not-a-repo" } | { state: "no-git"; message: string };
export interface Snapshot {
  id: string;
  time: number;
  label: string;
  changes: { status: "added" | "modified" | "deleted"; path: string }[];
}

export interface LabelInfo { label: string; file: string; line: number; context: string }
export interface WordCounts {
  total_words: number;
  files: { file: string; words: number }[];
  headings: number;
  inline_math: number;
  display_math: number;
  floats: number;
  citations: number;
}

export interface BibEntry { key: string; kind: string; title: string; authors: string; year: string; file: string }

export interface SearchOptions { query: string; regex: boolean; case_sensitive: boolean; whole_word: boolean }
export interface FileMatches { path: string; matches: { line: number; text: string; ranges: [number, number][]; replaced?: string }[] }
export interface ReplaceTarget { path: string; lines: number[] }

export interface Template { id: string; name: string; description: string; /** Saved by the user, so it can be deleted. */ user?: boolean }

export interface PdfBox { page: number; x: number; y: number; h: number; v: number; width: number; height: number }
export interface SourceHit { path: string; line: number; inside_project: boolean }

export const api = {
  texStatus: (extraDir?: string) => invoke<TexStatus>("tex_status", { extraDir }),
  openProject: (path: string) => invoke<ProjectInfo>("open_project", { path }),
  projectInfo: () => invoke<ProjectInfo>("project_info_cmd"),
  setConfig: (config: Config) => invoke<ProjectInfo>("set_config", { config }),
  createMain: (dir: string) => invoke<void>("create_main", { dir }),
  watching: () => invoke<boolean>("watching"),
  readText: (path: string) => invoke<string>("read_text", { path }),
  /** `expected`: hash (contentHash) of the text last read or written; the save is refused if the disk differs. */
  writeText: (path: string, content: string, expected?: number) => invoke<void>("write_text", { path, content, expected: expected ?? null }),
  /** `shellEscape`: the user allowed it for this project; lemmata.toml alone can't turn it on. */
  build: (shellEscape: boolean) => invoke<Outcome>("build", { shellEscape }),
  readPdf: async () => new Uint8Array(await invoke<ArrayBuffer>("read_pdf")),
  synctexForward: (path: string, line: number, column: number) =>
    invoke<PdfBox[]>("synctex_forward", { path, line, column }),
  synctexBackward: (page: number, x: number, y: number) =>
    invoke<SourceHit | null>("synctex_backward", { page, x, y }),
  outline: () => invoke<OutlineData>("outline"),
  labels: () => invoke<LabelInfo[]>("labels"),
  wordCount: () => invoke<WordCounts>("word_count"),
  packageNames: () => invoke<string[]>("package_names"),
  gitStatus: () => invoke<{ path: string; status: string }[]>("git_status"),
  historyStatus: () => invoke<HistoryStatus>("history_status"),
  historyEnable: () => invoke<void>("history_enable"),
  historySnapshot: (label: string) => invoke<string | null>("history_snapshot", { label }),
  historyList: () => invoke<Snapshot[]>("history_list"),
  historyFile: (id: string, path: string) => invoke<string | null>("history_file", { id, path }),
  historyRestoreAll: (id: string) => invoke<number>("history_restore_all", { id }),
  historyCompare: (a: string, b: string) => invoke<Snapshot["changes"]>("history_compare", { a, b }),
  historyRelabel: (id: string, label: string) => invoke<void>("history_relabel", { id, label }),
  historyRestore: (id: string, path: string) => invoke<void>("history_restore", { id, path }),
  saveAsTemplate: (name: string, description: string) => invoke<string>("save_as_template", { name, description }),
  deleteTemplate: (id: string) => invoke<void>("delete_template", { id }),
  listTemplates: () => invoke<Template[]>("list_templates"),
  createProject: (template: string, parent: string, name: string) =>
    invoke<string>("create_project", { template, parent, name }),
  importOverleafZip: (zip: string, parent: string, name: string) =>
    invoke<string[]>("import_overleaf_zip", { zip, parent, name }),
  inspectZip: (zip: string) => invoke<{ bulk: boolean; projects: string[] }>("inspect_zip", { zip }),
  createExample: () => invoke<string>("create_example"),
  stopBuild: () => invoke<void>("stop_build"),
  cleanBuild: () => invoke<number>("clean_build"),
  readLog: () => invoke<string>("read_log"),
  addFiles: (paths: string[], folder: string) => invoke<string[]>("add_files", { paths, folder }),
  saveThumbnail: (root: string, png: Uint8Array) =>
    invoke<void>("save_thumbnail", png, { headers: { "x-project-root": encodeURIComponent(root) } }),
  /** Moves a project folder to the system trash. */
  trashProject: (path: string) => invoke<void>("trash_project", { path }),
  loadThumbnail: async (root: string) => new Uint8Array(await invoke<ArrayBuffer>("load_thumbnail", { root })),
  updateCheck: (channel: string) =>
    invoke<{ version: string; current: string; notes: string | null } | null>("update_check", { channel }),
  updateInstall: () => invoke<void>("update_install"),
  bibEntries: () => invoke<{ files: string[]; entries: BibEntry[] }>("bib_entries"),
  bibAddReference: (id: string, file: string) => invoke<{ key: string; file: string }>("bib_add_reference", { id, file }),
  importFigure: (path: string) => invoke<string>("import_figure", { path }),
  savePastedImage: (png: Uint8Array) => invoke<string>("save_pasted_image", png),
  hasPackage: (pkg: string) => invoke<boolean>("has_package", { package: pkg }),
  addPackage: (pkg: string) => invoke<void>("add_package", { package: pkg }),
  searchProject: (options: SearchOptions, replacement?: string) => invoke<FileMatches[]>("search_project", { options, replacement: replacement ?? null }),
  replaceInFiles: (options: SearchOptions, replacement: string, targets: ReplaceTarget[]) =>
    invoke<number>("replace_in_files", { options, replacement, targets }),
  createPath: (path: string, dir: boolean) => invoke<void>("create_path", { path, dir }),
  copyPath: (from: string, to: string) => invoke<void>("copy_path", { from, to }),
  readBinary: async (path: string) => new Uint8Array(await invoke<ArrayBuffer>("read_binary", { path })),
  trashPath: (path: string) => invoke<void>("trash_path", { path }),
  findReferences: (path: string) => invoke<{ file: string; line: number; text: string }[]>("find_references", { path }),
  renamePath: (from: string, to: string, updateReferences: boolean) =>
    invoke<number>("rename_path", { from, to, updateReferences }),
  movePaths: (paths: string[], folder: string, updateReferences: boolean) =>
    invoke<Moved[]>("move_paths", { paths, folder, updateReferences }),
  trashPaths: (paths: string[]) => invoke<TrashBatch>("trash_paths", { paths }),
  restoreTrash: (id: string) => invoke<string[]>("restore_trash", { id }),
  purgeTrash: (days: number) => invoke<number>("purge_trash", { days }),
  unusedFiles: () => invoke<string[]>("unused_files"),
  symbolDefs: () => invoke<Occurrence[]>("symbol_defs"),
  symbolUses: (kind: SymbolKind, name: string) => invoke<Occurrence[]>("symbol_uses", { kind, name }),
  renameSymbol: (kind: SymbolKind, old: string, newName: string) => invoke<string[]>("rename_symbol", { kind, old, new: newName }),
  exportPdf: (dest: string) => invoke<void>("export_pdf", { dest }),
  exportZip: (dest: string, withPdf: boolean) => invoke<number>("export_zip", { dest, withPdf }),
  exportArxiv: (dest: string, strip: boolean) => invoke<number>("export_arxiv", { dest, strip }),
  zoteroStatus: () => invoke<"ready" | "starting" | "unavailable">("zotero_status"),
  zoteroPick: () => invoke<string[]>("zotero_pick"),
  zoteroSync: () => invoke<{ written: number; missing: string[]; linked: boolean }>("zotero_sync"),
  zoteroLink: () => invoke<void>("zotero_link"),
  openInstallTerminal: (request: InstallRequest) => invoke<void>("open_install_terminal", { request }),
};

export function errorMessage(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}
