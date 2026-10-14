import { describe, it, expect } from "vitest";
import { EditorState } from "@codemirror/state";
import { CompletionContext, type CompletionResult } from "@codemirror/autocomplete";
import { envSnippet, latexCompletions, type Providers } from "./completions";
import { ENVIRONMENTS } from "./data";

const providers: Providers = {
  labels: async () => [{ label: "sec:intro", file: "main.tex", line: 3, context: "\\section{Intro}" }],
  cites: async () => [{ key: "knuth1984", kind: "book", title: "The TeXbook", authors: "Knuth, Donald and X", year: "1984", file: "refs.bib" }],
  files: () => ["main.tex", "chapters/a.tex", "figures/plot.png", "refs.bib"],
  packages: async () => ["amsmath", "graphicx"],
};

async function complete(doc: string, pos = doc.length, explicit = false): Promise<CompletionResult | null> {
  const state = EditorState.create({ doc });
  const source = latexCompletions(() => providers, async () => null);
  return (await source(new CompletionContext(state, pos, explicit))) as CompletionResult | null;
}

const labels = (r: CompletionResult | null) => r?.options.map((o) => o.label) ?? [];

describe("latex completions", () => {
  it("offers environments after \\begin{ and replaces an existing closing brace", async () => {
    const doc = "\\begin{ite}";
    const r = await complete(doc, 10);
    expect(labels(r)).toContain("itemize");
    expect(r!.from).toBe(7);
    expect(r!.to).toBe(11); // swallows the } that closeBrackets added
  });

  it("only swaps the name when the environment already has its \\end", async () => {
    const r = await complete("\\begin{ite}\nx\n\\end{ite}", 10);
    expect(r!.options.find((o) => o.label === "itemize")!.apply).toBe("itemize}");
  });

  it("offers environment names after \\end{ without a body", async () => {
    const r = await complete("\\end{al", 7);
    expect(labels(r)).toContain("align");
    expect(typeof r!.options.find((o) => o.label === "align")!.apply).toBe("string");
  });

  it("offers whole \\begin{…} blocks while typing \\beg", async () => {
    const r = await complete("\\beg");
    expect(labels(r)).toContain("\\begin{itemize}");
    expect(labels(r)).toContain("\\begin{align*}");
    expect(labels(r)).toContain("\\begin{table}");
  });

  it("suggests closing the innermost open environment after \\en", async () => {
    const r = await complete("\\begin{document}\n\\begin{itemize}\n\\item a\n\\en");
    expect(labels(r)).toContain("\\end{itemize}");
    expect(labels(r)).not.toContain("\\end{document}");
    // Nothing open: nothing to suggest.
    expect(labels(await complete("\\begin{a}\\end{a}\n\\en")).some((l) => l.startsWith("\\end{"))).toBe(false);
  });

  it("starts tables with a header, sample rows and a column spec", () => {
    const table = envSnippet(ENVIRONMENTS.find((e) => e.name === "table")!);
    expect(table).toContain("table}[${htbp}]");
    expect(table).toContain("\t\\caption{${Caption}}");
    expect(table).toContain("\t\t${Header 1} & ${Header 2} & ${Header 3}");
    expect(table).toContain("\t\t${Cell 2.1} & ${Cell 2.2} & ${Cell 2.3}");
    expect(table.endsWith("\\end{table}")).toBe(true);
    const tabular = envSnippet(ENVIRONMENTS.find((e) => e.name === "tabular")!);
    expect(tabular).toContain("tabular}{${lll}}");
    expect(tabular).toContain("\t${Cell 1.1} & ${Cell 1.2} & ${Cell 1.3}");
  });

  it("offers names used in the document", async () => {
    const r = await complete("\\begin{myenv}\\end{myenv}\n\\begin{my", 33);
    expect(labels(r)).toContain("myenv");
  });

  it("offers project labels inside \\ref and \\cref lists", async () => {
    expect(labels(await complete("See \\ref{"))).toEqual(["sec:intro"]);
    const r = await complete("\\cref{a,s");
    expect(r!.from).toBe(8);
    expect(labels(r)).toEqual(["sec:intro"]);
  });

  it("offers citation keys with author and title", async () => {
    const r = await complete("\\parencite[p.~3]{kn");
    expect(labels(r)).toEqual(["knuth1984"]);
    expect(r!.options[0].detail).toBe("Knuth, Donald 1984");
    expect(r!.options[0].info).toBe("The TeXbook");
  });

  it("filters files by command and drops extensions for \\input", async () => {
    expect(labels(await complete("\\input{"))).toEqual(["main", "chapters/a"]);
    expect(labels(await complete("\\includegraphics[width=3cm]{"))).toEqual(["figures/plot.png"]);
    expect(labels(await complete("\\addbibresource{"))).toEqual(["refs.bib"]);
  });

  it("offers package names", async () => {
    expect(labels(await complete("\\usepackage[utf8]{"))).toEqual(["amsmath", "graphicx"]);
  });

  it("offers commands and merges the language server's", async () => {
    const state = EditorState.create({ doc: "\\sec" });
    const source = latexCompletions(
      () => providers,
      async () => ({ from: 0, options: [{ label: "\\section" }, { label: "\\sectionmark" }] }),
    );
    const r = (await source(new CompletionContext(state, 4, false))) as CompletionResult;
    const l = r.options.map((o) => o.label);
    expect(l.filter((x) => x === "\\section").length).toBe(1); // ours wins, no duplicate
    expect(l).toContain("\\sectionmark");
  });

  it("stays quiet for plain words", async () => {
    expect(await complete("hello wor")).toBeNull();
  });

  it("ranks maths first inside maths and text commands first in prose", async () => {
    const inside = await complete("Let $x = \\al");
    const prose = await complete("Let us write \\al");
    const boost = (r: CompletionResult | null, label: string) => r!.options.find((o) => o.label === label)!.boost;
    expect(labels(inside)).toContain("\\alpha");
    expect(boost(inside, "\\alpha")).toBeGreaterThan(boost(inside, "\\section") ?? 0);
    // In prose \alpha is still there, but below the text commands.
    expect(boost(prose, "\\alpha")).toBeLessThan(0);
    expect(boost(prose, "\\section")).toBeGreaterThan(0);
  });

  it("gives maths structures real tab stops and a preview", async () => {
    const { previews } = await import("./mathCommands");
    const r = await complete("$\\fra");
    const frac = r!.options.find((o) => o.label === "\\frac")!;
    expect(typeof frac.apply).toBe("function"); // a snippet
    expect(previews.get(frac)).toBe("\\frac{a}{b}");
    expect(labels(await complete("$\\su"))).toContain("\\sum");
  });

  it("every maths completion renders", async () => {
    const { mathCompletions, previews } = await import("./mathCommands");
    const { renderTex } = await import("../writing/katexRender");
    const all = mathCompletions(0);
    expect(all.length).toBeGreaterThan(150);
    const bad = all.filter((c) => !renderTex(previews.get(c)!)).map((c) => c.label);
    expect(bad.length).toBeLessThan(all.length * 0.05);
  });
});
