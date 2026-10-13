import { describe, it, expect } from "vitest";
import { Text } from "@codemirror/state";
import { mathContextAt, inMath, closingOf } from "./mathContext";

const doc = (s: string) => Text.of(s.split("\n"));
/** The cursor sits at `|`. */
function at(src: string) {
  const pos = src.indexOf("|");
  return { d: doc(src.replace("|", "")), pos };
}
const ctx = (src: string) => {
  const { d, pos } = at(src);
  return mathContextAt(d, pos);
};

describe("mathContextAt", () => {
  it("knows inline maths, including right after the opening $", () => {
    expect(ctx("Let $x |+ y$ be")?.kind).toBe("inline");
    expect(ctx("Let $|")?.kind).toBe("inline");
    expect(ctx("Let $x$| be")).toBeNull();
    expect(ctx("Let |$x$ be")).toBeNull();
  });

  it("handles $$ … $$ and \\( \\) and \\[ \\]", () => {
    expect(ctx("a $$ x |$$")).toMatchObject({ kind: "display", open: "$$" });
    expect(ctx("a \\( x| \\) b")).toMatchObject({ kind: "inline", open: "\\(" });
    expect(ctx("\\[\n x^2 |\n\\]")).toMatchObject({ kind: "display", open: "\\[" });
    expect(ctx("\\[ x \\] a|")).toBeNull();
  });

  it("knows math environments, closed or not", () => {
    expect(ctx("\\begin{align}\n a &= b\\\\\n c |= d")).toMatchObject({ kind: "env", env: "align" });
    expect(ctx("\\begin{align}\n a\n\\end{align}\n|")).toBeNull();
    expect(ctx("\\begin{itemize}\n \\item |x")).toBeNull();
  });

  it("ignores escaped dollars and comments", () => {
    expect(ctx("costs \\$5 and |more")).toBeNull();
    expect(ctx("% $ not maths\n text |")).toBeNull();
    expect(ctx("x % $ \n y |")).toBeNull();
  });

  it("is not in maths inside \\text{…} within maths", () => {
    expect(ctx("$x \\text{if |it} y$")).toBeNull();
    expect(ctx("$x \\text{if it} y |$")?.kind).toBe("inline");
  });

  it("does not look past a blank line", () => {
    expect(ctx("$ unclosed\n\nnew paragraph |")).toBeNull();
  });

  it("inMath is the boolean form", () => {
    const { d, pos } = at("$a|$");
    expect(inMath(d, pos)).toBe(true);
  });
});

describe("closingOf", () => {
  it("finds the closing delimiter", () => {
    const { d, pos } = at("$a |+ b$ rest");
    const c = mathContextAt(d, pos)!;
    expect(closingOf(d, c, pos)).toEqual({ start: 6, end: 7 });
  });
  it("finds \\end{env} and returns null when unclosed", () => {
    const a = at("\\begin{align}\n a |= b\n\\end{align}");
    expect(closingOf(a.d, mathContextAt(a.d, a.pos)!, a.pos)).not.toBeNull();
    const b = at("\\begin{align}\n a |= b");
    expect(closingOf(b.d, mathContextAt(b.d, b.pos)!, b.pos)).toBeNull();
  });
});
