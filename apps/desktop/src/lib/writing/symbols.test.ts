import { describe, it, expect } from "vitest";
import katex from "katex";
import { SYMBOLS, GROUP_NAMES } from "./symbols";

describe("symbols", () => {
  it("has a few hundred distinct entries", () => {
    expect(SYMBOLS.length).toBeGreaterThan(250);
    expect(new Set(SYMBOLS.map((s) => s.tex + s.group)).size).toBe(SYMBOLS.length);
    expect(GROUP_NAMES).toContain("Greek");
  });
  it("commands start with a backslash", () => {
    expect(SYMBOLS.filter((s) => s.group !== "Scripts").every((s) => s.tex.startsWith("\\"))).toBe(true);
  });
  it("renders in KaTeX (or is a known unsupported one)", () => {
    const bad = SYMBOLS.filter((s) => {
      try {
        katex.renderToString(s.tex, { throwOnError: true, strict: "ignore" });
        return false;
      } catch {
        return true;
      }
    }).map((s) => s.tex);
    // The palette falls back to plain text for these.
    expect(bad.length).toBeLessThan(SYMBOLS.length * 0.08);
  });
});

describe("structure entries", () => {
  it("keeps multi-word templates whole", () => {
    const t = SYMBOLS.map((s) => s.tex);
    expect(t).toContain("\\left( x \\right)");
    expect(t).toContain("x^{2}");
    expect(t).toContain("\\,");
    expect(t).toContain("\\quad");
    expect(t).not.toContain("\\x");
  });
});
