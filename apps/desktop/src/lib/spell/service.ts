// The app's spell checker. Checking runs in Rust (lemmata-spell); this side
// remembers which words were already checked so each is asked about once.
import { invoke } from "@tauri-apps/api/core";

export const LANGUAGES: Record<string, { label: string }> = {
  en: { label: "English (US)" },
  "en-GB": { label: "English (UK)" },
  de: { label: "Deutsch" },
  "de-CH": { label: "Deutsch (Schweiz)" },
  fr: { label: "Français" },
};

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
  return "en";
}

type Listener = () => void;

class SpellService {
  private lang = "off";
  /** true = correct, false = misspelled, for the current language. */
  private known = new Map<string, boolean>();
  private listeners = new Set<Listener>();
  private loading: Promise<void> = Promise.resolve();

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
    this.known.clear();
    this.loading = invoke("spell_set_language", { lang: target, words })
      .then(() => {
        this.lang = target;
      })
      .catch((e) => {
        console.warn("spell check unavailable:", e);
        this.lang = "off";
      })
      .finally(() => this.emit());
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
    invoke("spell_add", { word }).catch(() => {});
    this.emit();
  }
}

export const spell = new SpellService();
