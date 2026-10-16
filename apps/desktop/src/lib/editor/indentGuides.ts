// Indentation you can see, like Overleaf: a wrapped line continues under its own indentation
// (a hanging indent), and a faint guide runs down under each enclosing level.
import { countColumn, RangeSetBuilder, type Text } from "@codemirror/state";
import { Decoration, ViewPlugin, type DecorationSet, type EditorView, type ViewUpdate } from "@codemirror/view";
import { getIndentUnit } from "@codemirror/language";

/** .cm-line's left padding in Editor.svelte's theme. */
const PAD = 10;
/** How far a blank line looks for text above and below. */
const REACH = 100;

/** Columns of leading whitespace, or null for a blank line. */
function columns(text: string, tabSize: number): number | null {
  const ws = /^\s*/.exec(text)![0];
  return ws.length === text.length ? null : countColumn(ws, tabSize);
}

/** Indentation level of line `n`. A blank line takes the smaller level of the nearest text above and below. */
export function levelOf(doc: Text, n: number, tabSize: number, unit: number): number {
  const own = columns(doc.line(n).text, tabSize);
  if (own !== null) return Math.floor(own / unit);
  const near = (step: number) => {
    for (let i = n + step, k = 0; i >= 1 && i <= doc.lines && k < REACH; i += step, k++) {
      const c = columns(doc.line(i).text, tabSize);
      if (c !== null) return Math.floor(c / unit);
    }
    return 0;
  };
  return Math.min(near(-1), near(1));
}

/** The line's inline style: a hanging indent `hang` px wide, and a guide under levels 1…level−1. */
export function lineStyle(hang: number, level: number, unitPx: number): string {
  const parts: string[] = [];
  if (hang > 0) parts.push(`padding-left:${PAD + hang}px`, `text-indent:-${hang}px`);
  if (level > 1) {
    const at = Array.from({ length: level - 1 }, (_, k) => `${PAD + (k + 1) * unitPx}px 0`);
    // Longhands only: the `background` shorthand would also clear the active line's colour.
    parts.push(
      `background-image:${at.map(() => "linear-gradient(var(--indent-guide),var(--indent-guide))").join(",")}`,
      `background-position:${at.join(",")}`,
      "background-size:1px 100%",
      "background-repeat:no-repeat",
    );
  }
  return parts.join(";");
}

const cache = new Map<string, Decoration>();
const lineDeco = (style: string) => {
  let d = cache.get(style);
  if (!d) cache.set(style, (d = Decoration.line({ attributes: { style } })));
  return d;
};

function build(view: EditorView): DecorationSet {
  const { state } = view;
  const builder = new RangeSetBuilder<Decoration>();
  const cw = view.defaultCharacterWidth;
  const unit = getIndentUnit(state);
  // A very deep line still keeps half the width for its text.
  const maxHang = Math.max(0, view.contentDOM.clientWidth / 2 - PAD);
  let last = 0;
  for (const { from, to } of view.visibleRanges) {
    for (let n = Math.max(state.doc.lineAt(from).number, last + 1); n <= state.doc.lineAt(to).number; n++) {
      last = n;
      const line = state.doc.line(n);
      const cols = columns(line.text, state.tabSize) ?? 0;
      const hang = view.lineWrapping ? Math.min(cols * cw, maxHang) : 0;
      const style = lineStyle(hang, levelOf(state.doc, n, state.tabSize, unit), unit * cw);
      if (style) builder.add(line.from, line.from, lineDeco(style));
    }
  }
  return builder.finish();
}

export const indentGuides = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = build(view);
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.viewportChanged || u.geometryChanged) this.decorations = build(u.view);
    }
  },
  { decorations: (v) => v.decorations },
);
