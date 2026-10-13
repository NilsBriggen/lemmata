// Live preview of the maths under the cursor, rendered with KaTeX.
import { StateField, type EditorState, type Text } from "@codemirror/state";
import { showTooltip, type Tooltip } from "@codemirror/view";
import katex from "katex";
import "katex/dist/katex.min.css";

const DISPLAY_ENVS = [
  "equation", "equation*", "align", "align*", "gather", "gather*", "multline", "multline*",
  "flalign", "flalign*", "displaymath",
];
/** Environments KaTeX renders with their own name; others become `aligned`/`gathered`. */
const KATEX_ENVS: Record<string, string> = {
  align: "aligned", "align*": "aligned", flalign: "aligned", "flalign*": "aligned",
  gather: "gathered", "gather*": "gathered", multline: "gathered", "multline*": "gathered",
};

export interface MathSpan {
  from: number;
  to: number;
  tex: string;
  display: boolean;
}

/** Inline math on the cursor's line: `$…$` or `\(…\)`. */
function inlineAt(line: string, offset: number, base: number): MathSpan | null {
  const re = /(?<!\\)\$([^$]+?)(?<!\\)\$|\\\((.+?)\\\)/g;
  for (let m; (m = re.exec(line)); ) {
    if (m.index < offset && offset < m.index + m[0].length) {
      return { from: base + m.index, to: base + m.index + m[0].length, tex: m[1] ?? m[2], display: false };
    }
  }
  return null;
}

/** Display math around `pos`: `\[…\]` or a math environment, up to 60 lines away. */
function displayAt(doc: Text, pos: number): MathSpan | null {
  const line = doc.lineAt(pos);
  const startLine = Math.max(1, line.number - 60);
  const endLine = Math.min(doc.lines, line.number + 60);
  const before = doc.sliceString(doc.line(startLine).from, pos);
  const after = doc.sliceString(pos, doc.line(endLine).to);

  const envOpen = [...before.matchAll(/\\begin\{([a-z]+\*?)\}/g)].filter((m) => DISPLAY_ENVS.includes(m[1])).pop();
  if (envOpen && envOpen.index !== undefined) {
    const env = envOpen[1];
    const opened = before.slice(envOpen.index + envOpen[0].length);
    const end = `\\end{${env}}`;
    if (!opened.includes(end)) {
      const close = after.indexOf(end);
      if (close >= 0) {
        const from = doc.line(startLine).from + envOpen.index;
        let tex = opened + after.slice(0, close);
        tex = tex.replace(/\\(label|nonumber|notag)\b(\{[^}]*\})?/g, "");
        const wrap = KATEX_ENVS[env];
        if (wrap) tex = `\\begin{${wrap}}${tex}\\end{${wrap}}`;
        return { from, to: pos + close + end.length, tex, display: true };
      }
    }
  }
  const sq = before.lastIndexOf("\\[");
  if (sq >= 0 && !before.slice(sq).includes("\\]")) {
    const close = after.indexOf("\\]");
    if (close >= 0) {
      const from = doc.line(startLine).from + sq;
      return { from, to: pos + close + 2, tex: before.slice(sq + 2) + after.slice(0, close), display: true };
    }
  }
  return null;
}

export function mathAt(doc: Text, pos: number): MathSpan | null {
  const line = doc.lineAt(pos);
  return inlineAt(line.text, pos - line.from, line.from) ?? displayAt(doc, pos);
}

/** \newcommand, \renewcommand, \def and \DeclareMathOperator from the preamble, as KaTeX macros. */
export function collectMacros(preamble: string): Record<string, string> {
  const macros: Record<string, string> = {};
  const braced = (s: string, i: number): [string, number] | null => {
    if (s[i] !== "{") return null;
    let depth = 0;
    for (let p = i; p < s.length; p++) {
      if (s[p] === "{") depth++;
      else if (s[p] === "}" && --depth === 0) return [s.slice(i + 1, p), p + 1];
    }
    return null;
  };
  const re = /\\(?:re)?newcommand\*?\s*\{?(\\[A-Za-z]+)\}?\s*(?:\[(\d)\])?(?:\[[^\]]*\])?\s*|\\DeclareMathOperator(\*?)\s*\{(\\[A-Za-z]+)\}\s*/g;
  for (let m; (m = re.exec(preamble)); ) {
    const body = braced(preamble, re.lastIndex);
    if (!body) continue;
    if (m[1]) macros[m[1]] = body[0];
    else if (m[4]) macros[m[4]] = m[3] ? `\\operatorname*{${body[0]}}` : `\\operatorname{${body[0]}}`;
    re.lastIndex = body[1];
  }
  return macros;
}

let macros: Record<string, string> = {};

/** Called by the app whenever the main file's preamble may have changed. */
export function setMathMacros(preamble: string) {
  macros = collectMacros(preamble.split("\\begin{document}")[0]);
}

function tooltipFor(state: EditorState): Tooltip | null {
  const sel = state.selection.main;
  if (!sel.empty) return null;
  const span = mathAt(state.doc, sel.head);
  if (!span || !span.tex.trim()) return null;
  return {
    pos: span.from,
    above: true,
    strictSide: false,
    arrow: false,
    create() {
      const dom = document.createElement("div");
      dom.className = "cm-math-preview";
      try {
        katex.render(span.tex, dom, { displayMode: span.display, throwOnError: true, macros: { ...macros }, trust: false });
      } catch (e) {
        dom.classList.add("error");
        dom.textContent = e instanceof Error ? e.message.replace(/^KaTeX parse error: /, "") : String(e);
      }
      return { dom };
    },
  };
}

export const mathPreview = StateField.define<Tooltip | null>({
  create: tooltipFor,
  update(value, tr) {
    if (!tr.docChanged && !tr.selection) return value;
    return tooltipFor(tr.state);
  },
  provide: (f) => showTooltip.from(f),
});
