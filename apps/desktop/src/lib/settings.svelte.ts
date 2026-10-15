// App-wide settings, saved with tauri-plugin-store in the app's config
// folder. Project-specific options live in lemmata.toml instead.
import { load, type Store } from "@tauri-apps/plugin-store";
import type { Snippet } from "./editor/snippets";
import { validIndent, type Indent } from "./editor/indent";

export interface Settings {
  theme: "system" | "light" | "dark";
  fontSize: number;
  /** Delay after the last keystroke before saving; 0 saves only on Ctrl+S. */
  autosaveMs: number;
  /** Compile after every save. Off means only the Recompile button builds. */
  autoCompile: boolean;
  /** Editor behaviour. */
  autoClose: boolean;
  autocomplete: boolean;
  envSync: boolean;
  folding: boolean;
  wordWrap: boolean;
  /** One indentation level: that many spaces, or a tab character. */
  indent: Indent;
  /** "plex" (bundled look), "system" or a font family name. */
  fontFamily: string;
  lineHeight: number;
  keymap: "default" | "vim";
  updateChannel: "stable" | "beta";
  checkForUpdates: boolean;
  /** A TeX bin folder chosen during onboarding. */
  texDir: string | null;
  pdfDark: boolean;
  /** Which panes the editor screen shows, like Overleaf's layout menu. */
  layout: "split" | "editor" | "pdf";
  sidebarOpen: boolean;
  sidebarWidth: number;
  /** Width of the PDF pane in split layout. */
  pdfWidth: number;
  /** Spell-check language for projects that don't choose one. */
  spellDefault: string;
  libraryView: "list" | "grid";
  /** How the file tree is ordered, and whether folders stay above files. */
  treeSort: "name" | "type" | "modified" | "size";
  treeFoldersFirst: boolean;
  /** "project" keeps deleted files in .lemmata/trash so Undo works; "system" uses the OS trash. */
  trashMode: "project" | "system";
  /** Text the editor offers after typing a short trigger. */
  snippets: Snippet[];
  firstRunDone: boolean;
  /** Projects (by folder) whose lemmata.toml may turn on shell escape: the user allowed it. */
  shellEscapeProjects: string[];
  /** Grammar checking with LanguageTool: off, a server on this computer, or an address the user gives. */
  grammar: "off" | "local" | "custom";
  grammarUrl: string;
  /** PDF pages one by one, or side by side like a book. */
  pdfLayout: "single" | "spread" | "spread-first";
  /** The app version last started, for the "What's new" window after an update; "" before 2.0. */
  lastSeenVersion: string;
}

export const DEFAULTS: Settings = {
  theme: "system",
  fontSize: 13,
  autosaveMs: 600,
  autoCompile: true,
  autoClose: true,
  autocomplete: true,
  envSync: true,
  folding: true,
  wordWrap: true,
  indent: 4,
  fontFamily: "plex",
  lineHeight: 1.65,
  keymap: "default",
  updateChannel: "stable",
  checkForUpdates: true,
  texDir: null,
  pdfDark: false,
  layout: "split",
  sidebarOpen: true,
  sidebarWidth: 240,
  pdfWidth: 560,
  spellDefault: "auto",
  libraryView: "list",
  treeSort: "name",
  treeFoldersFirst: true,
  trashMode: "project",
  snippets: [],
  firstRunDone: false,
  shellEscapeProjects: [],
  grammar: "off",
  grammarUrl: "http://localhost:8081",
  pdfLayout: "single",
  lastSeenVersion: "",
};

/** Saved values of the wrong type (a damaged or hand-edited file) fall back to the defaults. */
export function validSettings(saved: Record<string, unknown>): Partial<Settings> {
  return Object.fromEntries(
    Object.entries(saved).filter(([k, v]) => {
      if (!(k in DEFAULTS)) return false;
      const d = DEFAULTS[k as keyof Settings];
      if (d === null) return v === null || typeof v === "string";
      return typeof v === typeof d && Array.isArray(v) === Array.isArray(d);
    }),
  ) as Partial<Settings>;
}

export const settings: Settings = $state({ ...DEFAULTS });
let store: Store | null = null;

/** Loads saved settings; keeps defaults if the store can't be read. */
export async function loadSettings() {
  try {
    store = await load("settings.json", { autoSave: 300, defaults: {} });
    const saved = validSettings((await store.get<Record<string, unknown>>("settings")) ?? {});
    // Before 1.0, "compile as I type" and "save as I type" were one switch.
    if (saved.autoCompile === undefined && saved.autosaveMs === 0) saved.autoCompile = false;
    Object.assign(settings, { ...DEFAULTS, ...saved, indent: validIndent(saved.indent ?? DEFAULTS.indent) });
  } catch (e) {
    console.warn("settings unavailable:", e);
  }
  applyTheme();
}

export async function updateSettings(patch: Partial<Settings>) {
  Object.assign(settings, patch);
  applyTheme();
  await store?.set("settings", { ...settings });
}

export function applyTheme() {
  const root = document.documentElement;
  if (settings.theme === "system") delete root.dataset.theme;
  else root.dataset.theme = settings.theme;
  root.style.setProperty("--editor-font-size", `${settings.fontSize}px`);
  root.style.setProperty("--editor-line-height", String(settings.lineHeight));
  const family = settings.fontFamily;
  if (family === "plex") root.style.removeProperty("--editor-font");
  else if (family === "system") root.style.setProperty("--editor-font", "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace");
  else root.style.setProperty("--editor-font", `"${family.replace(/"/g, "")}", ui-monospace, monospace`);
}
