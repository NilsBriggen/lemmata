// Grammar checking through a LanguageTool server (see lemmata-grammar). Text is
// sent only to the address the user chose in Preferences. This side remembers
// the findings per paragraph text, so an unchanged paragraph is asked about once.
import { invoke } from "@tauri-apps/api/core";

export interface GrammarMatch {
  offset: number;
  length: number;
  message: string;
  shortMessage: string;
  replacements: string[];
  ruleId: string;
  category: string;
}

export type GrammarMode = "off" | "local" | "custom";
export const LOCAL_URL = "http://localhost:8081";

/** LanguageTool language code for a spell-check language ("off" has none). */
export function ltLanguage(spellLang: string): string | null {
  const map: Record<string, string> = { en: "en-US", "en-GB": "en-GB", de: "de-DE", "de-CH": "de-CH", fr: "fr" };
  return map[spellLang] ?? null;
}

/** Rules that misfire on text reduced from LaTeX (removed commands leave stray spaces and capitals). */
const NOISY_RULES = new Set([
  "WHITESPACE_RULE",
  "COMMA_PARENTHESIS_WHITESPACE",
  "UPPERCASE_SENTENCE_START",
  "EN_QUOTES",
  "UNPAIRED_BRACKETS",
  "EN_UNPAIRED_BRACKETS",
  "DOUBLE_PUNCTUATION",
]);
const noisy = (m: GrammarMatch) => NOISY_RULES.has(m.ruleId) || m.category === "TYPOS" || m.ruleId.startsWith("MORFOLOGIK");

const MAX_IN_FLIGHT = 2;
const MAX_CACHED = 2000;

type Listener = () => void;

class GrammarService {
  private mode: GrammarMode = "off";
  private url = LOCAL_URL;
  private lang: string | null = null;
  private epoch = 0;
  private cache = new Map<string, GrammarMatch[]>();
  private inFlight = new Map<string, Promise<void>>();
  private listeners = new Set<Listener>();
  private ignored = new Set<string>();
  /** Why the last request failed; null when it worked. */
  error: string | null = null;

  /** True when paragraphs should be checked. */
  get active() {
    return this.mode !== "off" && this.lang !== null;
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

  /** Applies the settings; a different server or language forgets what was found. */
  configure(mode: GrammarMode, customUrl: string, spellLang: string) {
    const url = mode === "custom" ? customUrl.trim() : LOCAL_URL;
    const lang = mode === "off" ? null : ltLanguage(spellLang);
    if (mode === this.mode && url === this.url && lang === this.lang) return;
    this.mode = mode;
    this.url = url;
    this.lang = lang;
    this.epoch++;
    this.cache.clear();
    this.inFlight.clear();
    this.error = null;
    this.emit();
  }

  /** Findings for a paragraph text, or undefined when it has not been checked. */
  found(text: string): GrammarMatch[] | undefined {
    return this.cache.get(text)?.filter((m) => !noisy(m) && !this.ignored.has(m.ruleId));
  }

  has(text: string) {
    return this.cache.has(text);
  }

  /**
   * Checks the texts not seen before, at most two requests at a time, and calls
   * `done` after each. Stops at the first failure (see `error`); a later call retries.
   */
  async check(texts: string[], done: () => void = () => {}): Promise<void> {
    if (!this.active) return;
    const queue = [...new Set(texts)].filter((t) => !this.cache.has(t));
    const { epoch, url, lang } = this;
    let failed = false;
    const worker = async () => {
      for (let t = queue.shift(); t !== undefined && !failed; t = queue.shift()) {
        let p = this.inFlight.get(t);
        if (!p) {
          p = invoke<GrammarMatch[]>("grammar_check", { url, language: lang, text: t })
            .then((m) => {
              if (epoch !== this.epoch) return;
              if (this.cache.size >= MAX_CACHED) this.cache.delete(this.cache.keys().next().value!);
              this.cache.set(t, m);
              this.error = null;
            })
            .finally(() => this.inFlight.delete(t));
          this.inFlight.set(t, p);
        }
        try {
          await p;
        } catch (e) {
          failed = true;
          if (epoch === this.epoch) {
            this.error = String(e);
            this.emit();
          }
        }
        if (epoch === this.epoch) done();
      }
    };
    await Promise.all(Array.from({ length: MAX_IN_FLIGHT }, worker));
  }

  /** Called when the author ignores a rule, so the app can keep it in the project's settings. */
  onIgnore: ((id: string) => void) | null = null;

  /** Hides a rule in this project. */
  ignoreRule(id: string) {
    this.ignored.add(id);
    this.onIgnore?.(id);
    this.emit();
  }

  /** The rules the open project ignores (from its settings); replaces the previous project's. */
  setIgnored(ids: string[]) {
    if (ids.length === this.ignored.size && ids.every((id) => this.ignored.has(id))) return;
    this.ignored = new Set(ids);
    this.emit();
  }

  /** Quick ping for Preferences: null when a server answers, else the reason it doesn't. */
  async ping(mode: GrammarMode, customUrl: string): Promise<string | null> {
    try {
      await invoke("grammar_status", { url: mode === "custom" ? customUrl.trim() : LOCAL_URL });
      return null;
    } catch (e) {
      return String(e);
    }
  }
}

export const grammar = new GrammarService();
