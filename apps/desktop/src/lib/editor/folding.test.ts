import { describe, it, expect } from "vitest";
import { EditorState } from "@codemirror/state";
import { foldRange } from "./folding";

const st = (s: string) => EditorState.create({ doc: s });
const fold = (s: string, line: number) => {
  const state = st(s);
  const l = state.doc.line(line);
  return foldRange(state, l.from, l.to);
};

describe("folding", () => {
  const doc = "\\section{A}\ntext\n\\subsection{B}\nmore\n\n\\section{C}\nend";
  it("folds a section up to the next one of the same level", () => {
    const r = fold(doc, 1)!;
    expect(st(doc).sliceDoc(r.from, r.to)).toBe("\ntext\n\\subsection{B}\nmore");
  });
  it("folds a subsection inside a section", () => {
    const r = fold(doc, 3)!;
    expect(st(doc).sliceDoc(r.from, r.to)).toBe("\nmore");
  });
  it("folds environments with nesting", () => {
    const d = "\\begin{a}\n\\begin{a}\nx\n\\end{a}\n\\end{a}";
    const r = fold(d, 1)!;
    expect(st(d).sliceDoc(r.from, r.to)).toBe("\n\\begin{a}\nx\n\\end{a}");
  });
  it("folds runs of comments", () => {
    const d = "% a\n% b\n% c\ntext";
    const r = fold(d, 1)!;
    expect(st(d).sliceDoc(r.from, r.to)).toBe("\n% b\n% c");
  });
  it("does not fold \\begin{document}", () => {
    expect(fold("\\begin{document}\nx\n\\end{document}", 1)).toBeNull();
  });
});
