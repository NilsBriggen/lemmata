import { describe, expect, it } from "vitest";
import { reindent, tabSizeOf, unitOf, validIndent } from "./indent";

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
});
