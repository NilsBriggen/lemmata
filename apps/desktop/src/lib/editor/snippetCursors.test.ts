import { describe, expect, it } from "vitest";
import { snippet, clearSnippet, type Completion } from "@codemirror/autocomplete";
import { EditorSelection, EditorState, type TransactionSpec } from "@codemirror/state";
import { indentUnit } from "@codemirror/language";
import { endSnippetCursors } from "./snippetCursors";

/** A minimal stand-in for EditorView: snippets only use state and dispatch. */
function mk(doc = "") {
  const v = {
    // The app's editor allows several cursors; a bare state doesn't.
    state: EditorState.create({ doc, extensions: [endSnippetCursors, EditorState.allowMultipleSelections.of(true), indentUnit.of("\t")] }),
    dispatch(spec: TransactionSpec) {
      v.state = v.state.update(spec).state;
    },
  };
  return v;
}

const BEGIN = "\\begin{${name}}\n\t${}\n\\end{${name}}";
const right = (v: ReturnType<typeof mk>) =>
  v.dispatch({
    selection: EditorSelection.create(
      v.state.selection.ranges.map((r) => EditorSelection.cursor(r.head + 1)),
      v.state.selection.mainIndex,
    ),
  });

describe("snippet cursors", () => {
  it("types the environment name in both places, then leaves one cursor", () => {
    const v = mk();
    snippet(BEGIN)(v as never, null as unknown as Completion, 0, 0);
    expect(v.state.selection.ranges.length).toBe(2);
    v.dispatch(v.state.replaceSelection("itemize"));
    expect(v.state.doc.toString()).toBe("\\begin{itemize}\n\t\n\\end{itemize}");
    expect(v.state.selection.ranges.length).toBe(2);
    // The arrow key out of the braces ends the snippet: one cursor, after \begin{itemize}.
    right(v);
    expect(v.state.selection.ranges.length).toBe(1);
    expect(v.state.selection.main.head).toBe("\\begin{itemize}".length);
    v.dispatch(v.state.replaceSelection("x"));
    expect(v.state.doc.toString()).toBe("\\begin{itemize}x\n\t\n\\end{itemize}");
  });

  it("also leaves one cursor when the snippet is ended with Esc", () => {
    const v = mk();
    snippet(BEGIN)(v as never, null as unknown as Completion, 0, 0);
    clearSnippet(v as never);
    expect(v.state.selection.ranges.length).toBe(1);
  });

  it("keeps cursors the user made", () => {
    const v = mk("abc\nabc");
    v.dispatch({ selection: EditorSelection.create([EditorSelection.cursor(0), EditorSelection.cursor(4)]) });
    right(v);
    expect(v.state.selection.ranges.length).toBe(2);
  });
});
