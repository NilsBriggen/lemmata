import { describe, expect, it, vi } from "vitest";
import { Text } from "@codemirror/state";

vi.mock("katex/dist/katex.min.css", () => ({}));
const { collectMacros, mathAt } = await import("./math");

const doc = (s: string) => Text.of(s.split("\n"));

describe("math under the cursor", () => {
  it("finds inline maths", () => {
    const d = doc("Energy $E = mc^2$ here, \\(a+b\\) too.");
    expect(mathAt(d, 10)).toMatchObject({ tex: "E = mc^2", display: false });
    expect(mathAt(d, 27)).toMatchObject({ tex: "a+b" });
    expect(mathAt(d, 2)).toBeNull();
  });

  it("finds environments across lines and drops labels", () => {
    const d = doc("Text\n\\begin{align}\\label{eq:x}\n  a &= b \\\\\n  c &= d\n\\end{align}\nafter");
    const m = mathAt(d, d.line(3).from + 3)!;
    expect(m.display).toBe(true);
    expect(m.tex).toContain("\\begin{aligned}");
    expect(m.tex).not.toContain("\\label");
    expect(mathAt(d, d.line(6).from + 2)).toBeNull();
  });

  it("finds \\[ … \\]", () => {
    const d = doc("\\[\n x^2\n\\]");
    expect(mathAt(d, d.line(2).from + 1)).toMatchObject({ tex: "\n x^2\n", display: true });
  });
});

describe("math while typing", () => {
  it("previews $$ … $$ and multi-line inline maths", () => {
    const d = doc("a $$ x^2 $$ b");
    expect(mathAt(d, 6)).toMatchObject({ tex: " x^2 ", display: true });
    const m = doc("so $a +\n b$ ok".replace("\\n", "\n"));
    expect(mathAt(m, 6)).toMatchObject({ display: false });
  });

  it("previews what is there while a span is still open", () => {
    expect(mathAt(doc("Let $x^{2} + "), 13)).toMatchObject({ tex: "x^{2} + ", display: false });
    const d = doc("\\begin{align}\n a &= b\\\\\n c &= ");
    const m = mathAt(d, d.length)!;
    expect(m.display).toBe(true);
    expect(m.tex).toContain("\\begin{aligned}");
  });

  it("leaves out a command that is still being typed", () => {
    expect(mathAt(doc("Let $x = \\al"), 12)).toMatchObject({ tex: "x = " });
    // The cursor in the middle of a finished command leaves it alone.
    expect(mathAt(doc("$x = \\alpha$"), 9)).toMatchObject({ tex: "x = \\alpha" });
  });

  it("ignores comments and \\text", () => {
    expect(mathAt(doc("$a % $ oops\n$"), 8)).toBeNull(); // in the comment
    expect(mathAt(doc("$a % $ oops\n$"), 3)).toMatchObject({ tex: "a \n" }); // the $ in the comment is not a closer
    expect(mathAt(doc("$x \\text{hello} y$"), 12)).toBeNull();
  });

  it("knows eqnarray and removes comments from the source", () => {
    const d = doc("\\begin{eqnarray}\n a &=& b % note\n\\end{eqnarray}");
    const m = mathAt(d, d.line(2).from + 2)!;
    expect(m.tex).toContain("aligned");
    expect(m.tex).not.toContain("note");
  });
});

describe("preamble macros", () => {
  it("collects newcommand and DeclareMathOperator", () => {
    const m = collectMacros(
      "\\newcommand{\\R}{\\mathbb{R}}\n\\newcommand\\norm[1]{\\left\\lVert #1 \\right\\rVert}\n\\DeclareMathOperator*{\\argmax}{arg\\,max}",
    );
    expect(m["\\R"]).toBe("\\mathbb{R}");
    expect(m["\\norm"]).toBe("\\left\\lVert #1 \\right\\rVert");
    expect(m["\\argmax"]).toBe("\\operatorname*{arg\\,max}");
  });
});
