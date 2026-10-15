// KaTeX rendering shared by the live preview, the symbol palette and the
// completion list. Results are cached; the cache is dropped when the
// preamble's macros change. KaTeX and its CSS (~270 KB) load on first use, so
// until `loadKatex()` resolves a render returns `html: null` and starts the load.
type Katex = typeof import("katex").default;
let katex: Katex | null = null;
let loading: Promise<void> | null = null;

export function loadKatex(): Promise<void> {
  return (loading ??= Promise.all([import("katex"), import("katex/dist/katex.min.css")]).then(([m]) => {
    katex = m.default;
    cache.clear();
  }));
}

export const katexReady = () => katex !== null;

let macros: Record<string, string> = {};
const cache = new Map<string, { html: string | null; error: string | null }>();

export function setMacros(next: Record<string, string>) {
  macros = next;
  cache.clear();
}

export function getMacros() {
  return macros;
}

/** Renders `tex`; `html` is null (and `error` set) when KaTeX can't parse it. */
export function renderTexResult(tex: string, display = false): { html: string | null; error: string | null } {
  if (!katex) {
    loadKatex().catch(() => {});
    return { html: null, error: null };
  }
  const key = `${display ? 1 : 0}:${tex}`;
  let hit = cache.get(key);
  if (!hit) {
    try {
      hit = { html: katex.renderToString(tex, { displayMode: display, throwOnError: true, strict: "ignore", trust: false, macros: { ...macros } }), error: null };
    } catch (e) {
      hit = { html: null, error: e instanceof Error ? e.message.replace(/^KaTeX parse error: /, "") : String(e) };
    }
    if (cache.size > 800) cache.clear();
    cache.set(key, hit);
  }
  return hit;
}

export const renderTex = (tex: string, display = false) => renderTexResult(tex, display).html;
