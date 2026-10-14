import { describe, it, expect } from "vitest";
import { EditorState, type TransactionSpec, type Transaction } from "@codemirror/state";
import { indentUnit } from "@codemirror/language";
import { environmentSync, tokens, closeEnvironment } from "./environments";

/** A minimal stand-in for EditorView, so tests run without a DOM. */
function mk(state: EditorState) {
  const v = {
    state,
    dispatch(spec: TransactionSpec) {
      v.state = v.state.update(spec).state;
    },
  };
  return v;
}
function make(doc: string) {
  return mk(EditorState.create({ doc, extensions: [environmentSync] }));
}

describe("environment pairing", () => {
  it("pairs nested environments", () => {
    const t = tokens(EditorState.create({ doc: "\\begin{a}\\begin{b}\\end{b}\\end{a}" }).doc);
    expect(t.map((x) => x.partner)).toEqual([3, 2, 1, 0]);
  });

  it("ignores commented lines", () => {
    const t = tokens(EditorState.create({ doc: "% \\begin{a}\n\\begin{b}\n\\end{b}" }).doc);
    expect(t.length).toBe(2);
  });

  it("renames \\end when \\begin is edited", () => {
    const v = make("\\begin{itemize}\n\\item x\n\\end{itemize}");
    v.dispatch({ changes: { from: 14, insert: "s" }, userEvent: "input.type" });
    expect(v.state.doc.toString()).toBe("\\begin{itemizes}\n\\item x\n\\end{itemizes}");
  });

  it("renames \\begin when \\end is edited", () => {
    const v = make("\\begin{a}\nx\n\\end{a}");
    const pos = v.state.doc.toString().lastIndexOf("a}");
    v.dispatch({ changes: { from: pos, to: pos + 1, insert: "equation" }, userEvent: "input.type" });
    expect(v.state.doc.toString()).toBe("\\begin{equation}\nx\n\\end{equation}");
  });

  it("leaves mismatched pairs alone", () => {
    const v = make("\\begin{a}\n\\end{b}");
    v.dispatch({ changes: { from: 7, insert: "z" }, userEvent: "input.type" });
    expect(v.state.doc.toString()).toBe("\\begin{za}\n\\end{b}");
  });

  it("undo reverts both halves together", async () => {
    const { history, undo } = await import("@codemirror/commands");
    const state = EditorState.create({ doc: "\\begin{a}\n\\end{a}", extensions: [history(), environmentSync] });
    const v = mk(state);
    v.dispatch({ changes: { from: 8, insert: "b" }, userEvent: "input.type" });
    expect(v.state.doc.toString()).toBe("\\begin{ab}\n\\end{ab}");
    undo({ state: v.state, dispatch: (tr: Transaction) => (v.state = tr.state) } as never);
    expect(v.state.doc.toString()).toBe("\\begin{a}\n\\end{a}");
  });

  it("adds an \\end on Enter after an unclosed \\begin", () => {
    const v = make("\\begin{itemize}");
    v.dispatch({ selection: { anchor: 15 } });
    expect(closeEnvironment(v)).toBe(true);
    expect(v.state.doc.toString()).toBe("\\begin{itemize}\n  \n\\end{itemize}");
    expect(v.state.selection.main.head).toBe(18);
  });

  it("indents the body by the chosen indentation", () => {
    const v = mk(EditorState.create({ doc: "\\begin{itemize}", extensions: [environmentSync, indentUnit.of("\t")] }));
    v.dispatch({ selection: { anchor: 15 } });
    expect(closeEnvironment(v)).toBe(true);
    expect(v.state.doc.toString()).toBe("\\begin{itemize}\n\t\n\\end{itemize}");
  });

  it("does not add a second \\end", () => {
    const v = make("\\begin{itemize}\n\\end{itemize}");
    v.dispatch({ selection: { anchor: 15 } });
    expect(closeEnvironment(v)).toBe(false);
  });
});
