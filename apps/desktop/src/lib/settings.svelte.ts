// App-wide settings, saved with tauri-plugin-store in the app's config
// folder. Project-specific options live in lemmata.toml instead.
import { load, type Store } from "@tauri-apps/plugin-store";

export interface Settings {
  theme: "system" | "light" | "dark";
  fontSize: number;
  /** Delay after the last keystroke before saving and building. */
  autosaveMs: number;
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
  firstRunDone: boolean;
}

export const DEFAULTS: Settings = {
  theme: "system",
  fontSize: 13,
  autosaveMs: 600,
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
  firstRunDone: false,
};

export const settings: Settings = $state({ ...DEFAULTS });
let store: Store | null = null;

/** Loads saved settings; keeps defaults if the store can't be read. */
export async function loadSettings() {
  try {
    store = await load("settings.json", { autoSave: 300, defaults: {} });
    const saved = (await store.get<Partial<Settings>>("settings")) ?? {};
    Object.assign(settings, { ...DEFAULTS, ...saved });
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
}
