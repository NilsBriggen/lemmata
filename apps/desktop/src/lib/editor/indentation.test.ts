import { describe, it, expect } from "vitest";
import { EditorSelection, EditorState, type Transaction } from "@codemirror/state";
import { getIndentation, indentUnit } from "@codemirror/language";
import { indentSelection, insertNewlineAndIndent } from "@codemirror/commands";
import { environmentIndent } from "./environments";
import { levelOf, lineStyle } from "./indentGuides";

const state = (doc: string, anchor = doc.length) =>
  EditorState.create({ doc, selection: { anchor }, extensions: [environmentIndent, indentUnit.of("    ")] });

/** Runs a state command and returns the new document. */
function run(cmd: typeof indentSelection, s: EditorState): string {
  let next = s;
  cmd({ state: s, dispatch: (tr: Transaction) => (next = tr.state) });
  return next.doc.toString();
}

describe("indentation from environments", () => {
  it("indents one level inside an environment that is already closed", () => {
    const doc = "\\begin{solution}\n    \\begin{align*}\n    \\end{align*}\n\\end{solution}";
    const s = state(doc, doc.indexOf("\n    \\end{align*}"));
    expect(run(insertNewlineAndIndent, s)).toContain("\\begin{align*}\n        \n    \\end{align*}");
  });

  it("lines \\end up with its \\begin", () => {
    const s = state("\\begin{solution}\n    \\begin{itemize}\n        \\item a\n        \\end{itemize}");
    expect(getIndentation(s, s.doc.line(4).from)).toBe(4);
  });

  it("has no opinion at the top level, directly in document, or in verbatim", () => {
    const s = state("text\n\\begin{document}\n  body\n\\begin{verbatim}\n   keep\n\\end{verbatim}\n\\end{document}");
    expect(getIndentation(s, s.doc.line(1).from)).toBeNull();
    expect(getIndentation(s, s.doc.line(3).from)).toBeNull();
    expect(getIndentation(s, s.doc.line(5).from)).toBeNull();
    expect(getIndentation(s, s.doc.line(7).from)).toBe(0); // \end{document} still lines up
  });

  it("reindents a messy block into clean levels", () => {
    const messy = [
      "\\begin{document}",
      "    \\begin{solution}",
      "\\begin{align*}",
      "x &= y \\\\",
      "  &= z",
      "      \\end{align*}",
      "    \\end{solution}",
      "\\end{document}",
    ].join("\n");
    const s = state(messy).update({ selection: EditorSelection.single(0, messy.length) }).state;
    expect(run(indentSelection, s)).toBe(
      [
        "\\begin{document}",
        "    \\begin{solution}",
        "        \\begin{align*}",
        "            x &= y \\\\",
        "            &= z",
        "        \\end{align*}",
        "    \\end{solution}",
        "\\end{document}",
      ].join("\n"),
    );
  });
});

describe("indent guides", () => {
  it("gives blank lines the smaller level of the text around them", () => {
    const doc = EditorState.create({ doc: "a\n        b\n\n    c\n\n\nd" }).doc;
    expect([1, 2, 3, 4, 5, 6, 7].map((n) => levelOf(doc, n, 4, 4))).toEqual([0, 2, 1, 1, 0, 0, 0]);
  });

  it("draws a guide under each enclosing level and keeps the line's background colour", () => {
    expect(lineStyle(0, 1, 32)).toBe("");
    const style = lineStyle(24, 3, 32);
    expect(style).toContain("padding-left:34px;text-indent:-24px");
    expect(style).toContain("background-position:42px 0,74px 0");
    expect(style).not.toMatch(/background:/);
  });
});
