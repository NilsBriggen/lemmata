import { describe, expect, it } from "vitest";
import { indentLines, placeBlock, reindent, tabSizeOf, unitOf, validIndent } from "./indent";

describe("indentation", () => {
  it("names one level", () => {
    expect(unitOf(4)).toBe("    ");
    expect(unitOf("tab")).toBe("\t");
    expect(tabSizeOf("tab")).toBe(4);
    expect(tabSizeOf(8)).toBe(8);
  });
  it("falls back to 4 for unknown values", () => {
    expect(validIndent(2)).toBe(2);
    expect(validIndent("tab")).toBe("tab");
    expect(validIndent(3)).toBe(4);
    expect(validIndent(undefined)).toBe(4);
  });
  it("rewrites leading two-space levels only", () => {
    const block = "\\begin{figure}\n  \\centering\n    \\toprule a  b\n\\end{figure}";
    expect(reindent(block, "    ")).toBe("\\begin{figure}\n    \\centering\n        \\toprule a  b\n\\end{figure}");
    expect(reindent(block, "\t")).toBe("\\begin{figure}\n\t\\centering\n\t\t\\toprule a  b\n\\end{figure}");
    expect(reindent(block, "  ")).toBe(block);
  });

  it("places a block at an indented blank line: every line takes the cursor's indentation", () => {
    const { prefix, indent } = placeBlock("\t\t", 2);
    expect(prefix).toBe("");
    expect(prefix + indentLines("\\begin{figure}\n\t\\centering\n\\end{figure}", indent)).toBe("\\begin{figure}\n\t\t\t\\centering\n\t\t\\end{figure}");
  });
  it("starts a new line, indented like the cursor's line, after text", () => {
    expect(placeBlock("    \\item done", 15)).toEqual({ prefix: "\n    ", indent: "    " });
  });
  it("leaves a block at column 0 and empty lines alone", () => {
    expect(placeBlock("", 0)).toEqual({ prefix: "", indent: "" });
    expect(indentLines("a\n\nb", "  ")).toBe("a\n\n  b");
  });
});
