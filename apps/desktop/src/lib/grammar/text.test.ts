import { describe, expect, it } from "vitest";
import { paragraphs, toSource } from "./text";

const one = (src: string) => paragraphs(src)[0];

describe("paragraph text", () => {
  it("drops commands, maths, comments, cite and ref", () => {
    const p = one("We use \\emph{robust} methods~\\cite[p.~3]{knuth84} with $x^2$ \\% only, see \\cref{sec:a}. % not this");
    expect(p.text).toBe("We use robust methods with X only, see.");
  });

  it("keeps punctuation after commands and inline maths as a placeholder", () => {
    expect(one("This is shown in~\\cite{a}. Let $x$ be small, and\n\\textbf{bold}.").text).toBe(
      "This is shown in. Let X be small, and bold.",
    );
  });

  it("splits at blank lines and structure; skips maths environments", () => {
    const src = "\\section{Intro}\nFirst line\nsecond line.\n\nNext paragraph.\n\\begin{equation}\na = b.\n\\end{equation}\nAfter it.\n\\item Listed";
    expect(paragraphs(src).map((p) => p.text)).toEqual([
      "Intro", "First line second line.", "Next paragraph.", "After it.", "Listed",
    ]);
  });

  it("maps every real character back to its source offset", () => {
    const src = "We use \\emph{robust} methods~\\cite{k} with $x$ here.\nSecond line.";
    for (const p of paragraphs(src)) {
      p.map.forEach((s, i) => {
        if (s >= 0) expect(src[s]).toBe(p.text[i]);
      });
    }
  });
});

describe("toSource", () => {
  const src = "Intro text.\n\nWe has \\emph{robust} methods~\\cite{k} here.";
  const [, p] = paragraphs(src);

  it("maps a finding to the exact source range, editable when contiguous", () => {
    const r = toSource(p, p.text.indexOf("We has"), 6, src)!;
    expect(src.slice(r.from, r.to)).toBe("We has");
    expect(r.editable).toBe(true);
  });

  it("is not editable across removed commands, and skips made-up characters", () => {
    const i = p.text.indexOf("has robust");
    const r = toSource(p, i, "has robust".length, src)!;
    expect(src.slice(r.from, r.to)).toBe("has \\emph{robust");
    expect(r.editable).toBe(false);
    // A finding on the placeholder alone covers no source.
    const q = paragraphs("Let $x$ be.")[0];
    expect(toSource(q, q.text.indexOf("X"), 1, "Let $x$ be.")).toBeNull();
  });

  it("maps correctly after comments and multiple lines", () => {
    const s = "Aaa bbb. % hidden words\nCcc ddd.";
    const q = one(s);
    const r = toSource(q, q.text.indexOf("Ccc"), 3, s)!;
    expect(s.slice(r.from, r.to)).toBe("Ccc");
  });
});
