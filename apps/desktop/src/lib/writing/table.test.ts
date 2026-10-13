import { describe, expect, it } from "vitest";
import { escapeLatex, figureSnippet, tsvToTabular } from "./table";

describe("paste as table", () => {
  it("builds a booktabs tabular and right-aligns numbers", () => {
    const t = tsvToTabular("Method\tScore\tNotes\nBaseline\t0.71\told\nOurs\t0.84\t5% better\n")!;
    expect(t).toContain("\\begin{tabular}{lrl}");
    expect(t).toContain("    Method & Score & Notes \\\\");
    expect(t).toContain("    Ours & 0.84 & 5\\% better \\\\");
    expect(t.indexOf("\\midrule")).toBeLessThan(t.indexOf("Baseline"));
  });

  it("ignores ordinary text", () => {
    expect(tsvToTabular("just one line\twith a tab")).toBeNull();
    expect(tsvToTabular("two\nplain lines")).toBeNull();
  });

  it("escapes LaTeX specials", () => {
    expect(escapeLatex("a_b & {c} ~ 100$ #1 \\")).toBe("a\\_b \\& \\{c\\} \\textasciitilde{} 100\\$ \\#1 \\textbackslash{}");
  });

  it("puts the cursor in the caption", () => {
    const f = figureSnippet("figures/my-plot.png");
    expect(f.text).toContain("\\includegraphics[width=0.8\\linewidth]{figures/my-plot.png}");
    expect(f.text).toContain("\\label{fig:my-plot}");
    expect(f.text.slice(f.cursor - 9, f.cursor)).toBe("\\caption{");
  });
});
