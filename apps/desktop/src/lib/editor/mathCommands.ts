// Maths completions, built from the symbol palette's list so both stay in
// step, plus structures with tab stops. Every item can be previewed with KaTeX.
import { snippet, type Completion } from "@codemirror/autocomplete";
import { SYMBOLS } from "../writing/symbols";
import { renderTex } from "../writing/katexRender";

/** What to render for a completion, by object: a small glyph in the list, a larger one in the info panel. */
export const previews = new WeakMap<Completion, string>();

interface Item {
  label: string;
  apply: string;
  preview: string;
  detail?: string;
  group: string;
}

/** Structures that need real tab stops or paired delimiters. */
const STRUCTURES: Item[] = [
  { label: "\\sum", apply: "\\sum_{${}}^{${}} ${}", preview: "\\sum_{i=1}^{n}", group: "Big operators" },
  { label: "\\prod", apply: "\\prod_{${}}^{${}} ${}", preview: "\\prod_{i=1}^{n}", group: "Big operators" },
  { label: "\\int", apply: "\\int_{${}}^{${}} ${} \\,\\mathrm{d}${}", preview: "\\int_{a}^{b} f \\,\\mathrm{d}x", group: "Big operators" },
  { label: "\\lim", apply: "\\lim_{${} \\to ${}} ${}", preview: "\\lim_{x \\to 0}", group: "Big operators" },
  { label: "\\binom", apply: "\\binom{${}}{${}}", preview: "\\binom{n}{k}", group: "Structures" },
  { label: "\\left(", apply: "\\left( ${} \\right)", preview: "\\left( x \\right)", group: "Structures" },
  { label: "\\left[", apply: "\\left[ ${} \\right]", preview: "\\left[ x \\right]", group: "Structures" },
  { label: "\\left\\{", apply: "\\left\\{ ${} \\right\\}", preview: "\\left\\{ x \\right\\}", group: "Structures" },
  { label: "\\left|", apply: "\\left| ${} \\right|", preview: "\\left| x \\right|", group: "Structures" },
  { label: "\\sqrt[", apply: "\\sqrt[${}]{${}}", preview: "\\sqrt[n]{x}", detail: "n-th root", group: "Structures" },
  { label: "\\xrightarrow", apply: "\\xrightarrow{${}}", preview: "\\xrightarrow{f}", group: "Arrows" },
  { label: "\\text", apply: "\\text{${}}", preview: "\\text{if } x", group: "Structures" },
];

/** Short forms people type from habit. */
const ALIASES: [string, string][] = [
  ["\\to", "\\to"], ["\\gets", "\\gets"], ["\\le", "\\le"], ["\\ge", "\\ge"], ["\\ne", "\\ne"],
  ["\\iff", "\\iff"], ["\\implies", "\\implies"], ["\\impliedby", "\\impliedby"],
  ["\\land", "\\land"], ["\\lor", "\\lor"], ["\\lnot", "\\lnot"], ["\\cdot", "\\cdot"],
  ["\\dots", "\\dots"], ["\\ldots", "\\ldots"], ["\\cdots", "\\cdots"], ["\\ell", "\\ell"],
];

/** `\name` or `\name[opt]{arg}…` with plain arguments (no scripts, spaces or nesting). */
const SIMPLE = /^\\[A-Za-z]+(\[[^\]]*\])?(\{[^{}]*\})*$/;

/** A palette entry as a snippet: each plain argument becomes a tab stop, anything else is inserted as is. */
export function templateOf(tex: string): string {
  if (!SIMPLE.test(tex)) return tex;
  return tex.replace(/\[[^\]]*\]/, "[${}]").replace(/\{[^{}]*\}/g, "{${}}");
}

function fromSymbols(): Item[] {
  const out: Item[] = [];
  const seen = new Set<string>();
  for (const s of SYMBOLS) {
    if (!SIMPLE.test(s.tex)) continue;
    const label = /^\\[A-Za-z]+/.exec(s.tex)![0];
    if (seen.has(label)) continue;
    seen.add(label);
    // Each braced argument becomes a tab stop; an optional [..] one too.
    const apply = templateOf(s.tex);
    out.push({ label, apply, preview: s.tex, detail: s.pkg ?? undefined, group: s.group });
  }
  return out;
}

const ITEMS: Item[] = (() => {
  const items = [...STRUCTURES, ...fromSymbols()];
  const have = new Set(items.map((i) => i.label));
  for (const [label, preview] of ALIASES) if (!have.has(label)) items.push({ label, apply: label, preview, group: "Misc" });
  return items;
})();

/** Math completions; `boost` decides whether they come before or after text commands. */
export function mathCompletions(boost: number): Completion[] {
  return ITEMS.map((i) => {
    const c: Completion = {
      label: i.label,
      detail: i.detail ?? i.group.toLowerCase(),
      type: "keyword",
      boost,
      apply: snippet(i.apply),
      info: () => {
        const html = renderTex(i.preview, true);
        if (!html) return null;
        const dom = document.createElement("div");
        dom.className = "cm-math-info";
        dom.innerHTML = html;
        const code = document.createElement("code");
        code.textContent = i.apply.replace(/\$\{\}/g, "▢");
        dom.append(code);
        return dom;
      },
    };
    previews.set(c, i.preview);
    return c;
  });
}

export const mathLabels = new Set(ITEMS.map((i) => i.label));
