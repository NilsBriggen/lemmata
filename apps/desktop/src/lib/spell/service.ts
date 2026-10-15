// The app's spell checker. Checking runs in Rust (lemmata-spell); this side
// remembers which words were already checked so each is asked about once.
import { invoke } from "@tauri-apps/api/core";

export const LANGUAGES: Record<string, { label: string }> = {
  en: { label: "English (US)" },
  "en-GB": { label: "English (UK)" },
  de: { label: "Deutsch" },
  "de-CH": { label: "Deutsch (Schweiz)" },
  fr: { label: "Français" },
  // Downloaded on demand (see lemmata-spell's manifest).
  es: { label: "Español" },
  it: { label: "Italiano" },
  "pt-PT": { label: "Português (Portugal)" },
  "pt-BR": { label: "Português (Brasil)" },
  nl: { label: "Nederlands" },
  pl: { label: "Polski" },
  sv: { label: "Svenska" },
  da: { label: "Dansk" },
  nb: { label: "Norsk bokmål" },
  cs: { label: "Čeština" },
  ru: { label: "Русский" },
  uk: { label: "Українська" },
  tr: { label: "Türkçe" },
  el: { label: "Ελληνικά" },
};

/** What spell_available reports for one language. */
export interface Dictionary {
  code: string;
  label: string;
  installed: boolean;
  bundled: boolean;
  size_kb: number;
}

/** "(downloads 5 MB)" for a language that still has to be fetched. */
export function downloadNote(d: Dictionary): string {
  return d.installed ? "" : ` (downloads ${Math.max(1, Math.round(d.size_kb / 1024))} MB)`;
}

/** Guesses the language from babel or polyglossia in the preamble. */
export function detectLanguage(source: string): string {
  const babel = /\\usepackage\[([^\]]*)\]\{babel\}/.exec(source)?.[1] ?? "";
  const poly = /\\set(?:default|main)language(?:\[[^\]]*\])?\{([^}]*)\}/.exec(source)?.[1] ?? "";
  // babel's last option is the main language.
  const main = (poly || babel.split(",").map((s) => s.trim()).filter(Boolean).pop() || "").toLowerCase();
  if (/swiss/.test(main)) return "de-CH";
  if (/german|deutsch/.test(main)) return "de";
  if (/french|francais/.test(main)) return "fr";
  if (/british|ukenglish/.test(main)) return "en-GB";
  const other: [RegExp, string][] = [
    [/spanish|espanol/, "es"],
    [/italian/, "it"],
    [/brazil/, "pt-BR"],
    [/portuguese/, "pt-PT"],
    [/dutch/, "nl"],
    [/polish/, "pl"],
    [/swedish/, "sv"],
    [/danish/, "da"],
    [/norsk|norwegian|bokmal/, "nb"],
    [/czech/, "cs"],
    [/russian/, "ru"],
    [/ukrainian/, "uk"],
    [/turkish/, "tr"],
    [/greek/, "el"],
  ];
  return other.find(([re]) => re.test(main))?.[1] ?? "en";
}

type Listener = () => void;

class SpellService {
  private lang = "off";
  /** true = correct, false = misspelled, for the current language. */
  private known = new Map<string, boolean>();
  private listeners = new Set<Listener>();
  private loading: Promise<void> = Promise.resolve();
  /** What the backend was last asked to load; setLanguage skips an identical request. */
  private target = "";
  private personal = new Set<string>();

  get language() {
    return this.lang;
  }

  onChange(l: Listener) {
    this.listeners.add(l);
    return () => {
      this.listeners.delete(l);
    };
  }

  private emit() {
    this.listeners.forEach((l) => l());
  }

  /** Switches language ("off" to disable) and loads personal words. */
  setLanguage(lang: string, words: string[]) {
    const target = lang in LANGUAGES ? lang : "off";
    const personal = new Set(words);
    if (target === this.target && personal.size === this.personal.size && words.every((w) => this.personal.has(w))) return;
    this.target = target;
    this.personal = personal;
    this.known.clear();
    this.loading = invoke("spell_set_language", { lang: target, words })
      .then(() => {
        this.lang = target;
      })
      .catch((e) => {
        console.warn("spell check unavailable:", e);
        this.target = ""; // allow a retry
        this.lang = "off";
      })
      .finally(() => this.emit());
  }

  /** All languages and whether each is installed; empty when the backend can't say. */
  async dictionaries(): Promise<Dictionary[]> {
    try {
      const list = await invoke<Dictionary[]>("spell_available");
      return Array.isArray(list) ? list : [];
    } catch {
      return [];
    }
  }

  /** Downloads a dictionary; rejects with a message fit to show. */
  async download(code: string): Promise<void> {
    try {
      await invoke("spell_download", { code });
    } catch (e) {
      throw new Error(String(e instanceof Error ? e.message : e));
    }
  }

  status(word: string): boolean | undefined {
    return this.known.get(word);
  }

  /** Checks words not seen before; resolves once their status is known. */
  async check(words: string[]): Promise<void> {
    await this.loading;
    if (this.lang === "off") return;
    const todo = [...new Set(words)].filter((w) => !this.known.has(w));
    if (!todo.length) return;
    const lang = this.lang;
    const bad = new Set(await invoke<string[]>("spell_check", { words: todo }));
    if (lang !== this.lang) return;
    for (const w of todo) this.known.set(w, !bad.has(w));
  }

  async suggest(word: string): Promise<string[]> {
    await this.loading;
    return this.lang === "off" ? [] : invoke<string[]>("spell_suggest", { word });
  }

  /** Accepts a word for this session; the app persists it in lemmata.toml. */
  add(word: string) {
    this.known.set(word, true);
    this.personal.add(word); // the app then reloads with it in the list: not a change
    invoke("spell_add", { word }).catch(() => {});
    this.emit();
  }
}

export const spell = new SpellService();
