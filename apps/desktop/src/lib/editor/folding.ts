// Code folding for sections, environments and comment blocks.
import { foldService, foldGutter, foldKeymap } from "@codemirror/language";
import type { EditorState } from "@codemirror/state";

const LEVELS: Record<string, number> = { part: 0, chapter: 1, section: 2, subsection: 3, subsubsection: 4, paragraph: 5 };
const HEADING = /^\s*\\(part|chapter|section|subsection|subsubsection|paragraph)\*?\s*[\[{]/;
const BEGIN = /^\s*\\begin\{([^{}]+)\}/;

export function foldRange(state: EditorState, lineStart: number, lineEnd: number): { from: number; to: number } | null {
  const doc = state.doc;
  const line = doc.lineAt(lineStart);
  const text = line.text;

  const h = HEADING.exec(text);
  if (h) {
    const level = LEVELS[h[1]];
    let end = doc.length;
    for (let n = line.number + 1; n <= doc.lines; n++) {
      const t = doc.line(n).text;
      const x = HEADING.exec(t);
      if (x && LEVELS[x[1]] <= level) {
        end = doc.line(n - 1).to;
        break;
      }
      if (/^\s*\\end\{document\}/.test(t)) {
        end = doc.line(n - 1).to;
        break;
      }
    }
    // Trim trailing blank lines so the fold ends at real content.
    let last = doc.lineAt(end);
    while (last.number > line.number && !last.text.trim()) last = doc.line(last.number - 1);
    return last.number > line.number ? { from: line.to, to: last.to } : null;
  }

  const b = BEGIN.exec(text);
  if (b && b[1] !== "document") {
    let depth = 0;
    for (let n = line.number; n <= doc.lines; n++) {
      const t = doc.line(n).text;
      for (const m of t.matchAll(/\\(begin|end)\{([^{}]+)\}/g)) {
        if (m[2] !== b[1]) continue;
        depth += m[1] === "begin" ? 1 : -1;
        if (depth === 0) return n - 1 > line.number - 1 && n > line.number ? { from: line.to, to: doc.line(n - 1).to } : null;
      }
    }
    return null;
  }

  if (/^\s*%/.test(text) && !/^\s*%/.test(line.number > 1 ? doc.line(line.number - 1).text : "")) {
    let last = line.number;
    while (last < doc.lines && /^\s*%/.test(doc.line(last + 1).text)) last++;
    return last > line.number ? { from: line.to, to: doc.line(last).to } : null;
  }
  return null;
}

export const folding = [
  foldService.of(foldRange),
  foldGutter({
    markerDOM(open) {
      const el = document.createElement("span");
      el.className = "cm-fold-chevron";
      el.textContent = open ? "▾" : "▸";
      el.title = open ? "Fold" : "Unfold";
      return el;
    },
  }),
];
export { foldKeymap };
