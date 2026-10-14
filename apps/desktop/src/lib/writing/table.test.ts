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

import { figureBlock, tableBlock, slug } from "./table";

describe("insert dialogs", () => {
  it("builds a figure with caption and a label from it", () => {
    const f = figureBlock({ path: "figures/a b.png", caption: "Learning curves", label: "", width: 80, placement: "htbp" });
    expect(f.text).toBe(
      "\\begin{figure}[htbp]\n  \\centering\n  \\includegraphics[width=0.8\\linewidth]{figures/a b.png}\n  \\caption{Learning curves}\n  \\label{fig:learning-curves}\n\\end{figure}",
    );
    expect(f.cursor).toBe(f.text.length);
  });
  it("uses full width and keeps an explicit label", () => {
    const f = figureBlock({ path: "x.png", caption: "", label: "fig:mine", width: 100, placement: "" });
    expect(f.text).toContain("\\begin{figure}\n");
    expect(f.text).toContain("width=\\linewidth");
    expect(f.text).toContain("\\label{fig:mine}");
    expect(f.text).not.toContain("\\caption");
  });
  it("builds a booktabs table", () => {
    const t = tableBlock({ rows: 3, cols: 2, header: true, booktabs: true, caption: "Results", label: "", });
    expect(t).toContain("\\begin{tabular}{ll}");
    expect(t).toContain("Header 1 & Header 2 \\\\");
    expect(t).toContain("Cell 1.1 & Cell 1.2 \\\\");
    expect(t).toContain("Cell 2.1 & Cell 2.2 \\\\");
    expect(t).toContain("\\midrule");
    expect(t).toContain("\\label{tab:results}");
  });
  it("fills in placeholders when there is no header, and for a missing caption", () => {
    const t = tableBlock({ rows: 2, cols: 2, header: false, booktabs: true, caption: "", label: "" });
    expect(t).toContain("Cell 1.1 & Cell 1.2 \\\\");
    expect(t).not.toContain("Header");
    expect(t).toContain("\\caption{Caption}");
    expect(t).toContain("\\label{tab:label}");
  });
  it("turns placeholders into tab stops for the editor", () => {
    const t = tableBlock({ rows: 2, cols: 2, header: true, booktabs: true, caption: "", label: "" }, true);
    expect(t).toContain("${Header 1} & ${Header 2} \\\\");
    expect(t).toContain("${Cell 1.1} & ${Cell 1.2} \\\\");
    expect(t).toContain("\\caption{${Caption}}");
    // A caption that itself looks like a field stays plain text.
    const odd = tableBlock({ rows: 1, cols: 1, header: false, booktabs: true, caption: "Cost ${x}", label: "" }, true);
    expect(odd).not.toContain("${Cell");
  });
  it("builds a ruled table without booktabs", () => {
    const t = tableBlock({ rows: 1, cols: 3, header: false, booktabs: false, caption: "", label: "" });
    expect(t).toContain("{|l|l|l|}");
    expect(t).toContain("\\hline");
    expect(t).toContain("\\caption{Caption}");
  });
  it("slugs", () => {
    expect(slug("Hello, World!")).toBe("hello-world");
  });
});
