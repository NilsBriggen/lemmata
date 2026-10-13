// Is the cursor in maths, and where does that maths start? Used by the live
// preview and to rank completions. LaTeX maths can't span a blank line, so
// the scan starts at the paragraph's first line and stays cheap.
import type { Text } from "@codemirror/state";

/** Environments whose content is maths. */
export const MATH_ENVS = new Set([
  "equation", "equation*", "align", "align*", "alignat", "alignat*", "gather", "gather*",
  "multline", "multline*", "flalign", "flalign*", "eqnarray", "eqnarray*", "displaymath", "math",
  "split", "cases", "array", "matrix", "pmatrix", "bmatrix", "vmatrix", "Vmatrix", "Bmatrix", "smallmatrix",
]);

/** Commands whose braced argument is ordinary text even inside maths. */
const TEXT_COMMANDS = /^\\(?:text|textrm|textbf|textit|textsf|texttt|mbox|intertext|label|ref|eqref|cref|cite)\b/;

export interface MathContext {
  kind: "inline" | "display" | "env";
  /** How it was opened: `$`, `$$`, `\(`, `\[` or `\begin`. */
  open: "$" | "$$" | "\\(" | "\\[" | "begin";
  env?: string;
  /** Start of the opening delimiter. */
  from: number;
  /** Where the maths itself starts, after the delimiter. */
  contentFrom: number;
}

const MAX_LINES = 120;

/** `line` with everything from an unescaped % on blanked out (offsets kept). */
function withoutComment(line: string): string {
  for (let i = 0; i < line.length; i++) {
    if (line[i] === "\\") i++;
    else if (line[i] === "%") return line.slice(0, i) + " ".repeat(line.length - i);
  }
  return line;
}

export function mathContextAt(doc: Text, pos: number): MathContext | null {
  const at = doc.lineAt(pos);
  // A cursor inside a comment is not in maths, whatever the line above says.
  const typed = at.text.slice(0, pos - at.from);
  if (withoutComment(typed) !== typed) return null;
  // Back to the start of the paragraph.
  let first = at.number;
  while (first > 1 && at.number - first < MAX_LINES && doc.line(first - 1).text.trim() !== "") first--;
  const base = doc.line(first).from;
  let src = "";
  for (let n = first; n <= at.number; n++) {
    const line = doc.line(n);
    const text = n === at.number ? line.text.slice(0, pos - line.from) : line.text;
    src += withoutComment(text) + (n < at.number ? "\n" : "");
  }

  let cur = null as MathContext | null;
  // Brace depth inside a \text{…} within maths; > 0 means "in text".
  let textDepth = 0;
  let pendingText = false;
  const open = (o: MathContext["open"], i: number, len: number, env?: string) => {
    cur = {
      kind: o === "$" || o === "\\(" ? "inline" : o === "begin" ? "env" : "display",
      open: o,
      env,
      from: base + i,
      contentFrom: base + i + len,
    };
  };

  for (let i = 0; i < src.length; ) {
    const c = src[i];
    if (c === "\\") {
      const rest = src.slice(i, i + 40);
      const next = src[i + 1];
      if (next === "(" && !cur) open("\\(", i, 2);
      else if (next === ")" && cur?.open === "\\(") cur = null;
      else if (next === "[" && !cur) open("\\[", i, 2);
      else if (next === "]" && cur?.open === "\\[") cur = null;
      else {
        const begin = /^\\begin\{([A-Za-z]+\*?)\}/.exec(rest);
        const end = /^\\end\{([A-Za-z]+\*?)\}/.exec(rest);
        if (begin && MATH_ENVS.has(begin[1]) && !cur) {
          open("begin", i, begin[0].length, begin[1]);
          i += begin[0].length;
          continue;
        }
        if (end && cur?.open === "begin" && cur.env === end[1]) {
          cur = null;
          i += end[0].length;
          continue;
        }
        if (cur && TEXT_COMMANDS.test(rest)) {
          pendingText = true;
          i += rest.match(TEXT_COMMANDS)![0].length;
          continue;
        }
      }
      i += 2; // also skips \$, \%, \\ and friends
      continue;
    }
    if (cur && c === "{") {
      if (textDepth > 0) textDepth++;
      else if (pendingText) {
        textDepth = 1;
        pendingText = false;
      }
    } else if (cur && c === "}" && textDepth > 0) {
      textDepth--;
    } else if (c !== " " && c !== "[" && pendingText) {
      pendingText = false; // an optional argument may sit between, anything else cancels
    }
    if (c === "$") {
      if (src[i + 1] === "$") {
        if (cur?.open === "$$") cur = null;
        else if (!cur) open("$$", i, 2);
        i += 2;
        continue;
      }
      if (cur?.open === "$") cur = null;
      else if (!cur) open("$", i, 1);
    }
    i++;
  }
  return cur && textDepth === 0 ? cur : null;
}

/** True when `pos` is in maths (and not in a `\text{…}` inside it). */
export const inMath = (doc: Text, pos: number) => mathContextAt(doc, pos) !== null;

/** Where the maths that `ctx` opened ends: the start of its closing delimiter and the end of it. */
export function closingOf(doc: Text, ctx: MathContext, pos: number): { start: number; end: number } | null {
  const closer = ctx.open === "$" ? "$" : ctx.open === "$$" ? "$$" : ctx.open === "\\(" ? "\\)" : ctx.open === "\\[" ? "\\]" : `\\end{${ctx.env}}`;
  const at = doc.lineAt(pos);
  let last = at.number;
  while (last < doc.lines && last - at.number < MAX_LINES && doc.line(last + 1).text.trim() !== "") last++;
  const tail = doc.sliceString(pos, doc.line(last).to);
  for (let i = 0; i < tail.length; i++) {
    if (tail.startsWith(closer, i) && !(closer === "$" && tail[i + 1] === "$")) {
      return { start: pos + i, end: pos + i + closer.length };
    }
    if (tail[i] === "\\") i++; // \$, \% and other escapes
    else if (tail[i] === "%") {
      const nl = tail.indexOf("\n", i);
      if (nl < 0) break;
      i = nl;
    }
  }
  return null;
}
