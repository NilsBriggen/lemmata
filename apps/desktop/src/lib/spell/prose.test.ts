import { describe, expect, it } from "vitest";
import { lineWords, type ProseState } from "./prose";

const texts = (line: string, s: ProseState) => lineWords(line, s).map((w) => w.text);

// Same cases as crates/lemmata-project/src/prose.rs, so the two stay in step.
describe("prose words", () => {
  it("skips commands, maths, keys and comments", () => {
    const s: ProseState = { env: null, display: null };
    expect(texts("We use \\emph{robust} methods~\\cite[p.~3]{knuth84} with $x^2$ \\% only, see \\cref{sec:a}. % not this", s)).toEqual([
      "We", "use", "robust", "methods", "with", "only", "see",
    ]);
    expect(texts("\\section{Über die Größe} don't stop-words", s)).toEqual(["Über", "die", "Größe", "don't", "stop-words"]);
    expect(texts("\\includegraphics[width=\\linewidth]{figures/plot} Caption", s)).toEqual(["Caption"]);
  });

  it("follows maths and verbatim across lines", () => {
    const s: ProseState = { env: null, display: null };
    expect(texts("Before \\begin{equation}", s)).toEqual(["Before"]);
    expect(texts("  E = mc^2 \\text{energy}", s)).toEqual([]);
    expect(texts("\\end{equation} after", s)).toEqual(["after"]);
    expect(texts("Then \\[", s)).toEqual(["Then"]);
    expect(texts("a + b", s)).toEqual([]);
    expect(texts("\\] done", s)).toEqual(["done"]);
    expect(texts("\\begin{verbatim} code here", s)).toEqual([]);
    expect(texts("more code \\end{verbatim} text", s)).toEqual(["text"]);
  });

  it("reports positions with the offset", () => {
    const w = lineWords("a \\textbf{bold}", { env: null, display: null }, 100);
    expect(w).toEqual([
      { from: 100, to: 101, text: "a" },
      { from: 110, to: 114, text: "bold" },
    ]);
  });
});
