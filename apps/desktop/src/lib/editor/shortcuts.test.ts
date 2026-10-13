import { describe, it, expect } from "vitest";
import { EditorState } from "@codemirror/state";
import { toggleWrap, shiftHeading } from "./shortcuts";

function run(cmd: ReturnType<typeof toggleWrap>, doc: string, from: number, to = from) {
  let state = EditorState.create({ doc, selection: { anchor: from, head: to } });
  cmd({ state, dispatch: (tr) => (state = tr.state) });
  return { text: state.doc.toString(), sel: [state.selection.main.from, state.selection.main.to] };
}

describe("shortcuts", () => {
  it("wraps a selection", () => {
    expect(run(toggleWrap("textbf"), "a word b", 2, 6).text).toBe("a \\textbf{word} b");
  });
  it("unwraps when the selection is inside the braces", () => {
    const r = run(toggleWrap("textbf"), "\\textbf{word}", 8, 12);
    expect(r.text).toBe("word");
  });
  it("unwraps when the whole command is selected", () => {
    expect(run(toggleWrap("textit"), "\\textit{x}", 0, 10).text).toBe("x");
  });
  it("puts the cursor inside when empty", () => {
    const r = run(toggleWrap("emph"), "", 0);
    expect(r.text).toBe("\\emph{}");
    expect(r.sel).toEqual([6, 6]);
  });
  it("shifts heading levels", () => {
    const doc = "\\section{A}";
    let state = EditorState.create({ doc });
    shiftHeading(1)({ state, dispatch: (tr) => (state = tr.state) });
    expect(state.doc.toString()).toBe("\\subsection{A}");
    shiftHeading(-1)({ state, dispatch: (tr) => (state = tr.state) });
    shiftHeading(-1)({ state, dispatch: (tr) => (state = tr.state) });
    expect(state.doc.toString()).toBe("\\chapter{A}");
  });
});
