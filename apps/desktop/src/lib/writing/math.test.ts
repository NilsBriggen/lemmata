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
