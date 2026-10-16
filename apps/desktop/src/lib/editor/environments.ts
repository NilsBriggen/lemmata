// Keeps \begin{X} and \end{X} in step, like Overleaf: rename one and the
// other follows, and pressing Enter after an unclosed \begin{X} adds the \end.
import { EditorState, type TransactionSpec, type Transaction } from "@codemirror/state";
import { EditorView, Decoration, type DecorationSet, ViewPlugin, type ViewUpdate } from "@codemirror/view";
import type { Text } from "@codemirror/state";
import { indentService, indentUnit } from "@codemirror/language";

export interface Token {
  kind: "begin" | "end";
  name: string;
  /** Span of the name between the braces. */
  from: number;
  to: number;
  /** Index of the partner token, if paired. */
  partner: number | null;
}

const TOKEN = /\\(begin|end)\{([^{}\n]*)\}/g;

/** All \begin/\end tokens, paired by nesting. Commented lines are skipped. */
export function tokens(doc: Text): Token[] {
  const out: Token[] = [];
  const stack: number[] = [];
  for (let n = 1; n <= doc.lines; n++) {
    const line = doc.line(n);
    if (!line.text.includes("\\")) continue;
    const comment = commentStart(line.text);
    TOKEN.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = TOKEN.exec(line.text))) {
      if (comment >= 0 && m.index >= comment) break;
      const kind = m[1] as "begin" | "end";
      const from = line.from + m.index + m[0].indexOf("{") + 1;
      const t: Token = { kind, name: m[2], from, to: from + m[2].length, partner: null };
      const i = out.push(t) - 1;
      if (kind === "begin") stack.push(i);
      else {
        const b = stack.pop();
        if (b !== undefined) {
          t.partner = b;
          out[b].partner = i;
        }
      }
    }
  }
  return out;
}

/** Index of the first unescaped % on a line, or -1. */
function commentStart(text: string): number {
  for (let i = 0; i < text.length; i++) {
    if (text[i] === "\\") i++;
    else if (text[i] === "%") return i;
  }
  return -1;
}

/** The partner edit for a transaction that changed one \begin or \end name. */
export function pairedEdit(tr: Transaction): TransactionSpec | null {
  if (!tr.docChanged || tr.isUserEvent("undo") || tr.isUserEvent("redo")) return null;
  let single: [number, number, number, number] | null = null;
  let count = 0;
  tr.changes.iterChangedRanges((fa, ta, fb, tb) => {
    count++;
    single = [fa, ta, fb, tb];
  });
  if (count !== 1 || !single) return null;
  const [fa, ta] = single as [number, number, number, number];
  // Most keystrokes are nowhere near a \begin or \end: skip tokenizing the whole document for those.
  const edited = tr.startState.doc.lineAt(fa);
  if (!/\\(?:begin|end)\s*\{/.test(edited.text) || tr.startState.doc.lineAt(ta).number !== edited.number) return null;
  const before = tokens(tr.startState.doc);
  const i = before.findIndex((t) => fa >= t.from && ta <= t.to);
  if (i < 0) return null;
  const tok = before[i];
  if (tok.partner == null) return null;
  const other = before[tok.partner];
  // Only follow when they matched before this edit.
  if (other.name !== tok.name) return null;
  const from = tr.changes.mapPos(tok.from, -1);
  const to = tr.changes.mapPos(tok.to, 1);
  const name = tr.newDoc.sliceString(from, to);
  if (name.includes("}") || name.includes("{") || name.includes("\n")) return null;
  const pFrom = tr.changes.mapPos(other.from, -1);
  const pTo = tr.changes.mapPos(other.to, 1);
  if (tr.newDoc.sliceString(pFrom, pTo) === name) return null;
  return { changes: { from: pFrom, to: pTo, insert: name }, sequential: true };
}

export const environmentSync = EditorState.transactionFilter.of((tr) => {
  const extra = pairedEdit(tr);
  return extra ? [tr, extra] : tr;
});

/** One scan per document: indenting a whole file asks for every line of the same text. */
const tokenCache = new WeakMap<Text, Token[]>();
function cachedTokens(doc: Text): Token[] {
  let t = tokenCache.get(doc);
  if (!t) tokenCache.set(doc, (t = tokens(doc)));
  return t;
}

/** Their lines are content, not code: indenting them would change it. */
const VERBATIM = new Set(["verbatim", "verbatim*", "Verbatim", "lstlisting", "minted", "comment"]);

/**
 * Indentation from environments, like Overleaf: one level inside each, and \end lined up with its \begin.
 * No opinion (the line keeps its own) at the top level, directly in `document` (some indent its body,
 * many don't) and inside verbatim-like environments.
 */
export const environmentIndent = indentService.of((ctx, pos) => {
  const doc = ctx.state.doc;
  const open: Token[] = [];
  // Everything before `pos`: on Enter that's the cursor, so a \begin earlier on the same line counts.
  for (const t of cachedTokens(doc)) {
    if (t.from >= pos) break;
    if (t.kind === "begin") open.push(t);
    else open.pop();
  }
  const env = open.at(-1);
  if (!env || VERBATIM.has(env.name)) return null;
  const closing = /^\s*\\end\b/.test(ctx.textAfterPos(pos));
  if (env.name === "document" && !closing) return null;
  const base = ctx.lineIndent(doc.lineAt(env.from).from);
  return closing ? base : base + ctx.unit;
});

/** Enter right after an unclosed \begin{X}: add the \end{X} below. */
export function closeEnvironment(view: { state: EditorState; dispatch: (spec: TransactionSpec) => void }): boolean {
  const { state } = view;
  const sel = state.selection.main;
  if (!sel.empty) return false;
  const line = state.doc.lineAt(sel.head);
  const m = /^(\s*)\\begin\{([^{}]+)\}$/.exec(line.text);
  if (!m || sel.head !== line.to) return false;
  const toks = tokens(state.doc);
  const here = toks.find((t) => t.kind === "begin" && t.from >= line.from && t.to <= line.to);
  if (!here || here.partner != null) return false;
  const indent = m[1];
  const unit = state.facet(indentUnit);
  const body = `\n${indent}${unit}`;
  const insert = `${body}\n${indent}\\end{${m[2]}}`;
  view.dispatch({
    changes: { from: sel.head, insert },
    selection: { anchor: sel.head + body.length },
    userEvent: "input",
    scrollIntoView: true,
  });
  return true;
}

const unmatched = Decoration.mark({ class: "cm-unmatched-env" });

/** A soft underline on \begin/\end that has no partner. */
export const unmatchedEnvironments = [
  ViewPlugin.fromClass(
    class {
      decorations: DecorationSet = Decoration.none;
      timer: ReturnType<typeof setTimeout> | undefined;
      constructor(readonly view: EditorView) {
        this.compute();
      }
      update(u: ViewUpdate) {
        if (u.docChanged) {
          this.decorations = this.decorations.map(u.changes);
          clearTimeout(this.timer);
          this.timer = setTimeout(() => {
            this.compute();
            this.view.requestMeasure();
          }, 500);
        }
      }
      compute() {
        const ranges = tokens(this.view.state.doc)
          .filter((t) => t.partner == null)
          .map((t) => unmatched.range(t.from - (t.kind === "begin" ? 7 : 5), t.to + 1));
        this.decorations = Decoration.set(ranges, true);
      }
      destroy() {
        clearTimeout(this.timer);
      }
    },
    { decorations: (v) => v.decorations },
  ),
  EditorView.baseTheme({
    ".cm-unmatched-env": { textDecoration: "underline dotted var(--warn)", textUnderlineOffset: "3px" },
  }),
];
