import { describe, expect, it } from "vitest";
import { computeHunks, isTextPath, rejectHunks } from "./hunks";

const old = "one\ntwo\nthree\nfour\nfive\nsix\nseven\n";

describe("hunks", () => {
  it("finds nothing in equal texts", () => {
    expect(computeHunks(old, old)).toEqual([]);
  });

  it("splits separate changes and describes each", () => {
    const now = "one\nTWO\nthree\nfour\nfive\nsix\nseven\neight\n";
    const hunks = computeHunks(old, now);
    expect(hunks.map((h) => [h.oldText, h.newText, h.line])).toEqual([
      ["two\n", "TWO\n", 2],
      ["", "eight\n", 8],
    ]);
  });

  it("rejects one hunk and leaves the others", () => {
    const now = "one\nTWO\nthree\nfour\nfive\nsix\nseven\neight\n";
    const [first] = computeHunks(old, now);
    expect(rejectHunks(now, [first])).toBe("one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n");
  });

  it("rejecting every hunk gives the snapshot back", () => {
    const cases: [string, string][] = [
      [old, "ONE\ntwo\nfour\nfive\nSIX\nseven\nnew tail"],
      ["a\nb", "a\nb\nc"],
      ["a\nb\n", "b\n"],
      ["", "all new\n"],
      ["all gone\n", ""],
      ["x\ny\nx\ny\n", "x\nz\nx\ny\nx\ny\n"],
      ["no newline at end", "no newline at end\nnow there is more"],
    ];
    for (const [a, b] of cases) expect(rejectHunks(b, computeHunks(a, b))).toBe(a);
  });

  it("round-trips random edits", () => {
    let seed = 7;
    const rnd = (n: number) => ((seed = (seed * 1103515245 + 12345) & 0x7fffffff), seed % n);
    const words = ["a", "b", "c", "", "d d", "e"];
    const doc = () => Array.from({ length: rnd(12) }, () => words[rnd(words.length)]).join("\n");
    for (let i = 0; i < 300; i++) {
      const [a, b] = [doc(), doc()];
      const hunks = computeHunks(a, b);
      expect(rejectHunks(b, hunks)).toBe(a);
      // Keeping a subset never loses the rest.
      expect(computeHunks(a, rejectHunks(b, hunks.slice(1))).length).toBeLessThanOrEqual(1);
    }
  });

  it("rejecting a hunk shrinks the list by one", () => {
    const now = "ONE\ntwo\nthree\nfour\nfive\nsix\nSEVEN\n";
    const hunks = computeHunks(old, now);
    expect(hunks).toHaveLength(2);
    const after = rejectHunks(now, [hunks[1]]);
    expect(computeHunks(old, after).map((h) => h.newText)).toEqual(["ONE\n"]);
  });

  it("keys identical changes apart but keep them stable", () => {
    const a = "x\ny\nx\ny\nx\n";
    const b = "X\ny\nx\ny\nX\n";
    const hunks = computeHunks(a, b);
    expect(new Set(hunks.map((h) => h.key)).size).toBe(hunks.length);
    expect(computeHunks(a, b).map((h) => h.key)).toEqual(hunks.map((h) => h.key));
  });

  it("recognises text files", () => {
    expect(isTextPath("chapters/a.tex")).toBe(true);
    expect(isTextPath("refs.bib")).toBe(true);
    expect(isTextPath("fig/plot.png")).toBe(false);
    expect(isTextPath("main.pdf")).toBe(false);
    expect(isTextPath("Makefile")).toBe(true);
  });
});
