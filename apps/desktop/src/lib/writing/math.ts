// Live preview of the maths under the cursor, rendered with KaTeX.
import { StateField, type EditorState, type Text } from "@codemirror/state";
import { showTooltip, type Tooltip } from "@codemirror/view";
import { mathContextAt, closingOf } from "../editor/mathContext";
import { katexReady, loadKatex, renderTexResult, setMacros } from "./katexRender";

/** Environments whose content KaTeX renders as plain maths, without the environment around it. */
const MATH_ONLY = new Set(["equation", "equation*", "displaymath", "math"]);
/** Environments KaTeX renders with their own name; others become `aligned`/`gathered`. */
const KATEX_ENVS: Record<string, string> = {
  align: "aligned", "align*": "aligned", flalign: "aligned", "flalign*": "aligned",
  gather: "gathered", "gather*": "gathered", multline: "gathered", "multline*": "gathered",
  alignat: "aligned", "alignat*": "aligned", eqnarray: "aligned", "eqnarray*": "aligned",
};

export interface MathSpan {
  from: number;
  to: number;
  tex: string;
  display: boolean;
}

/** Drops `%` comments, which KaTeX would choke on. */
function stripComments(tex: string): string {
  return tex
    .split("\n")
    .map((line) => {
      for (let i = 0; i < line.length; i++) {
        if (line[i] === "\\") i++;
        else if (line[i] === "%") return line.slice(0, i);
      }
      return line;
    })
    .join("\n");
}

/** The maths around `pos`, closed or still being typed: inline, `$$`, `\[`, or a math environment. */
export function mathAt(doc: Text, pos: number): MathSpan | null {
  const ctx = mathContextAt(doc, pos);
  if (!ctx) return null;
  const closing = closingOf(doc, ctx, pos);
  // Unclosed while typing: preview what's there up to the end of the line (inline) or paragraph.
  const lineEnd = doc.lineAt(pos).to;
  const contentTo = closing ? closing.start : lineEnd;
  const stop = Math.max(contentTo, ctx.contentFrom);
  let before = doc.sliceString(ctx.contentFrom, Math.min(pos, stop));
  const after = doc.sliceString(Math.min(pos, stop), stop);
  // A command still being typed (\alp|) isn't an error yet: leave it out.
  if (!/^[A-Za-z]/.test(after)) before = before.replace(/\\[A-Za-z]*$/, "");
  let tex = stripComments(before + after);
  if (ctx.open === "begin" && ctx.env) {
    tex = tex.replace(/\\(label|nonumber|notag)\b(\{[^}]*\})?/g, "");
    const wrap = KATEX_ENVS[ctx.env];
    if (wrap) tex = `\\begin{${wrap}}${tex}\\end{${wrap}}`;
    else if (!MATH_ONLY.has(ctx.env)) tex = `\\begin{${ctx.env}}${tex}\\end{${ctx.env}}`;
  }
  return { from: ctx.from, to: closing ? closing.end : contentTo, tex, display: ctx.kind !== "inline" };
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

/** Called by the app whenever the main file's preamble may have changed. */
export function setMathMacros(preamble: string) {
  setMacros(collectMacros(preamble.split("\\begin{document}")[0]));
}

/** The last successful render, shown dimmed under an error while the user is mid-expression. */
let lastGood: string | null = null;

function tooltipFor(state: EditorState): Tooltip | null {
  const sel = state.selection.main;
  if (!sel.empty) return null;
  const span = mathAt(state.doc, sel.head);
  if (!span || !span.tex.trim()) return null;
  return {
    // Tall display maths is anchored at the cursor so the preview stays in view.
    pos: span.display ? sel.head : span.from,
    above: true,
    strictSide: false,
    arrow: false,
    create(view) {
      const dom = document.createElement("div");
      dom.className = "cm-math-preview";
      const fill = () => {
        const { html, error } = renderTexResult(span.tex, span.display);
        if (html) {
          lastGood = html;
          dom.innerHTML = html;
        } else {
          dom.classList.add("error");
          if (lastGood) {
            const old = document.createElement("div");
            old.className = "stale";
            old.innerHTML = lastGood;
            dom.append(old);
          }
          const msg = document.createElement("div");
          msg.className = "msg";
          msg.textContent = error ?? "";
          dom.append(msg);
        }
        view.requestMeasure();
      };
      // First use: KaTeX loads lazily, so fill the tooltip once it's there.
      if (katexReady()) fill();
      else loadKatex().then(fill, () => {});
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
