// Formatting shortcuts that toggle: pressing Ctrl+B on bold text removes it.
import { EditorSelection, type StateCommand } from "@codemirror/state";
import { keymap, type EditorView } from "@codemirror/view";
import { toggleComment } from "@codemirror/commands";

/** Wraps the selection in \cmd{…}, or unwraps it if already wrapped. */
export function toggleWrap(cmd: string): StateCommand {
  const open = `\\${cmd}{`;
  return ({ state, dispatch }) => {
    const tr = state.changeByRange((r) => {
      const before = state.sliceDoc(Math.max(0, r.from - open.length), r.from);
      const after = state.sliceDoc(r.to, r.to + 1);
      if (before === open && after === "}") {
        return {
          changes: [
            { from: r.from - open.length, to: r.from },
            { from: r.to, to: r.to + 1 },
          ],
          range: EditorSelection.range(r.from - open.length, r.to - open.length),
        };
      }
      const text = state.sliceDoc(r.from, r.to);
      if (text.startsWith(open) && text.endsWith("}")) {
        const inner = text.slice(open.length, -1);
        return { changes: { from: r.from, to: r.to, insert: inner }, range: EditorSelection.range(r.from, r.from + inner.length) };
      }
      return {
        changes: [{ from: r.from, insert: open }, { from: r.to, insert: "}" }],
        range: r.empty
          ? EditorSelection.cursor(r.from + open.length)
          : EditorSelection.range(r.from + open.length, r.to + open.length),
      };
    });
    dispatch(state.update(tr, { scrollIntoView: true, userEvent: "input" }));
    return true;
  };
}

const HEADINGS = ["part", "chapter", "section", "subsection", "subsubsection", "paragraph"];

/** Moves the heading on the current line one level up or down. */
export function shiftHeading(delta: 1 | -1): StateCommand {
  return ({ state, dispatch }) => {
    const line = state.doc.lineAt(state.selection.main.head);
    const m = /^(\s*\\)(part|chapter|section|subsection|subsubsection|paragraph)(\*?[\[{])/.exec(line.text);
    if (!m) return false;
    const i = HEADINGS.indexOf(m[2]) + delta;
    if (i < 0 || i >= HEADINGS.length) return true;
    const from = line.from + m[1].length;
    dispatch(state.update({ changes: { from, to: from + m[2].length, insert: HEADINGS[i] }, userEvent: "input" }));
    return true;
  };
}

export interface ShortcutHooks {
  nextProblem: (dir: 1 | -1) => boolean;
  wrapInEnvironment: () => boolean;
}

export function formattingKeymap(h: ShortcutHooks) {
  const run = (cmd: StateCommand) => (v: EditorView) => cmd({ state: v.state, dispatch: v.dispatch });
  return keymap.of([
    { key: "Mod-b", run: run(toggleWrap("textbf")) },
    { key: "Mod-i", run: run(toggleWrap("textit")) },
    { key: "Mod-/", run: toggleComment },
    { key: "Mod-]", run: run(shiftHeading(1)) },
    { key: "Mod-[", run: run(shiftHeading(-1)) },
    { key: "Mod-Shift-e", run: () => h.wrapInEnvironment() },
    { key: "F8", run: () => h.nextProblem(1) },
    { key: "Shift-F8", run: () => h.nextProblem(-1) },
  ]);
}
