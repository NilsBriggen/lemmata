// Highlights commented text in the editor. The app hands over the ranges
// (from comments/model.ts locate); edits move them along with the text.
import { RangeSet, StateEffect, StateField, type Extension } from "@codemirror/state";
import { Decoration, EditorView, type DecorationSet } from "@codemirror/view";

export const setCommentRanges = StateEffect.define<{ id: string; from: number; to: number; active: boolean }[]>();

const field = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(set, tr) {
    set = set.map(tr.changes);
    for (const e of tr.effects) {
      if (!e.is(setCommentRanges)) continue;
      const len = tr.state.doc.length;
      set = RangeSet.of(
        e.value
          .filter((r) => r.from < r.to && r.to <= len)
          .sort((a, b) => a.from - b.from)
          .map((r) =>
            Decoration.mark({ class: r.active ? "cm-comment cm-comment-active" : "cm-comment", attributes: { "data-comment": r.id } }).range(r.from, r.to),
          ),
      );
    }
    return set;
  },
  provide: (f) => EditorView.decorations.from(f),
});

/** `onClick` gets the thread id when commented text is clicked. */
export function comments(onClick: (id: string) => void): Extension {
  return [
    field,
    EditorView.domEventHandlers({
      mousedown(e) {
        const id = (e.target as HTMLElement | null)?.closest<HTMLElement>("[data-comment]")?.dataset.comment;
        if (id) onClick(id);
        return false;
      },
    }),
    EditorView.baseTheme({
      ".cm-comment": { backgroundColor: "rgba(240, 190, 40, 0.22)", borderBottom: "2px solid rgba(220, 160, 20, 0.7)" },
      ".cm-comment-active": { backgroundColor: "rgba(240, 190, 40, 0.42)" },
    }),
  ];
}
