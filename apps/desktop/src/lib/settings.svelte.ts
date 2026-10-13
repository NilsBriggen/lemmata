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
