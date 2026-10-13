// KaTeX rendering shared by the live preview, the symbol palette and the
// completion list. Results are cached; the cache is dropped when the
// preamble's macros change.
import katex from "katex";

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
