// CodeMirror extension: underlines grammar findings from LanguageTool (dotted
// blue, apart from spelling's red wave). Hovering one shows the message, a
// button per replacement and "Ignore this rule". Only paragraphs near the
// viewport are checked, and only once their text changed.
import { ViewPlugin, Decoration, EditorView, hoverTooltip, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { StateEffect } from "@codemirror/state";
import { grammar, type GrammarMatch } from "./service";
import { paragraphs, toSource, type Paragraph } from "./text";

const refresh = StateEffect.define<null>();
/** Lines checked beyond the visible ones. */
const MARGIN = 40;
const TYPING_DELAY = 1500;

interface Found {
  match: GrammarMatch;
  /** The text the finding was made on, to refuse a stale edit. */
  flagged: string;
  editable: boolean;
}

/** Marks for the findings known for these paragraphs' texts. */
export function buildMarks(source: string, paras: Paragraph[]): DecorationSet {
  const ranges = [];
  for (const p of paras) {
    for (const m of grammar.found(p.text) ?? []) {
      const r = toSource(p, m.offset, m.length, source);
      if (!r) continue;
      const found: Found = { match: m, flagged: source.slice(r.from, r.to), editable: r.editable };
      ranges.push(Decoration.mark({ class: "cm-grammar", found }).range(r.from, r.to));
    }
  }
  return Decoration.set(ranges, true);
}

function visibleParagraphs(view: EditorView, paras: Paragraph[]): Paragraph[] {
  const doc = view.state.doc;
  const spans = view.visibleRanges.map((r) => [
    doc.line(Math.max(1, doc.lineAt(r.from).number - MARGIN)).from,
    doc.line(Math.min(doc.lines, doc.lineAt(r.to).number + MARGIN)).to,
  ]);
  return paras.filter((p) => spans.some(([a, b]) => p.from <= b && p.to >= a));
}

export function grammarcheck() {
  const plugin = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet = Decoration.none;
      timer: ReturnType<typeof setTimeout> | undefined;
      dead = false;
      unsubscribe: () => void;

      constructor(readonly view: EditorView) {
        this.unsubscribe = grammar.onChange(() => this.schedule(0));
        this.schedule(0);
      }

      update(u: ViewUpdate) {
        if (u.docChanged) {
          this.decorations = this.decorations.map(u.changes);
          this.schedule(TYPING_DELAY);
        } else if (u.viewportChanged && this.timer === undefined) this.schedule(300);
      }

      schedule(delay: number) {
        clearTimeout(this.timer);
        this.timer = setTimeout(() => this.run(), delay);
      }

      /** Re-derives the marks from the current text and the findings known so far. */
      rebuild(source: string, paras: Paragraph[]) {
        this.decorations = buildMarks(source, paras);
        this.view.dispatch({ effects: refresh.of(null) });
      }

      async run() {
        this.timer = undefined;
        const source = this.view.state.doc.toString();
        const paras = grammar.active ? paragraphs(source) : [];
        this.rebuild(source, paras);
        if (!grammar.active) return;
        const near = visibleParagraphs(this.view, paras);
        await grammar.check(
          near.map((p) => p.text),
          () => {
            // Findings are keyed by text, so they still apply if the document changed meanwhile.
            if (this.dead) return;
            const now = this.view.state.doc.toString();
            this.rebuild(now, now === source ? paras : paragraphs(now));
          },
        );
      }

      at(pos: number) {
        let hit: { from: number; to: number; found: Found } | undefined;
        this.decorations.between(pos, pos, (from, to, d) => {
          hit = { from, to, found: d.spec.found };
          return false;
        });
        return hit;
      }

      destroy() {
        this.dead = true;
        clearTimeout(this.timer);
        this.unsubscribe();
      }
    },
    { decorations: (v) => v.decorations },
  );

  const tooltip = hoverTooltip((view, pos) => {
    const hit = view.plugin(plugin)?.at(pos);
    if (!hit) return null;
    const { match, flagged, editable } = hit.found;
    return {
      pos: hit.from,
      end: hit.to,
      above: true,
      create() {
        const dom = document.createElement("div");
        dom.className = "cm-grammar-tip";
        const msg = document.createElement("div");
        msg.textContent = match.message;
        dom.appendChild(msg);
        const row = document.createElement("div");
        row.className = "cm-grammar-actions";
        const button = (label: string, action: () => void) => {
          const b = document.createElement("button");
          b.textContent = label;
          b.onclick = () => {
            action();
            view.focus();
          };
          row.appendChild(b);
        };
        if (editable) {
          for (const r of match.replacements) {
            button(r || "(delete)", () => {
              // The text may have moved or changed since the check.
              if (view.state.sliceDoc(hit.from, hit.to) === flagged) {
                view.dispatch({ changes: { from: hit.from, to: hit.to, insert: r } });
              }
            });
          }
        }
        button("Ignore this rule", () => grammar.ignoreRule(match.ruleId));
        dom.appendChild(row);
        return { dom };
      },
    };
  });

  const theme = EditorView.baseTheme({
    ".cm-grammar": { textDecoration: "underline dotted var(--accent)", textDecorationThickness: "2px", textDecorationSkipInk: "none", textUnderlineOffset: "3px" },
    ".cm-grammar-tip": { padding: "8px 10px", maxWidth: "360px", fontSize: "13px", display: "flex", flexDirection: "column", gap: "6px" },
    ".cm-grammar-actions": { display: "flex", flexWrap: "wrap", gap: "4px" },
    ".cm-grammar-actions button": { border: "1px solid var(--border)", background: "var(--surface-2)", color: "var(--ink)", borderRadius: "var(--r-sm)", padding: "3px 8px", fontSize: "12px", cursor: "pointer" },
    ".cm-grammar-actions button:hover": { background: "var(--surface-3)" },
  });

  return [plugin, tooltip, theme];
}
