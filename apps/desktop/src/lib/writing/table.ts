// Turns tab-separated text (copied from LibreOffice, Excel or Google Sheets)
// into a LaTeX tabular with booktabs rules.

const SPECIAL: Record<string, string> = {
  "\\": "\\textbackslash{}", "&": "\\&", "%": "\\%", "$": "\\$", "#": "\\#", "_": "\\_",
  "{": "\\{", "}": "\\}", "~": "\\textasciitilde{}", "^": "\\textasciicircum{}",
};

export function escapeLatex(s: string): string {
  return s.replace(/[\\&%$#_{}~^]/g, (c) => SPECIAL[c]);
}

/** Null unless `text` looks like a table: 2+ rows, 2+ columns, all rows tab-separated. */
export function tsvToTabular(text: string): string | null {
  const rows = text.replace(/\r\n?/g, "\n").replace(/\n+$/, "").split("\n");
  if (rows.length < 2 || !rows.every((r) => r.includes("\t"))) return null;
  const cells = rows.map((r) => r.split("\t").map((c) => c.trim()));
  const cols = Math.max(...cells.map((r) => r.length));
  if (cols < 2) return null;
  // Right-align columns that are numbers below the header.
  const numeric = (v: string) => /^[-+]?[\d\s'.,]*\d[%]?$/.test(v);
  const align = Array.from({ length: cols }, (_, c) =>
    cells.slice(1).every((r) => !r[c] || numeric(r[c])) ? "r" : "l",
  ).join("");
  const line = (r: string[]) =>
    "    " + Array.from({ length: cols }, (_, c) => escapeLatex(r[c] ?? "")).join(" & ") + " \\\\";
  return [
    "\\begin{table}[htbp]",
    "  \\centering",
    `  \\begin{tabular}{${align}}`,
    "    \\toprule",
    line(cells[0]),
    "    \\midrule",
    ...cells.slice(1).map(line),
    "    \\bottomrule",
    "  \\end{tabular}",
    "  \\caption{}",
    "  \\label{tab:}",
    "\\end{table}",
  ].join("\n");
}

export function figureSnippet(path: string): { text: string; cursor: number } {
  const name = path.replace(/^.*\//, "").replace(/\.[^.]+$/, "");
  const text = [
    "\\begin{figure}[htbp]",
    "  \\centering",
    `  \\includegraphics[width=0.8\\linewidth]{${path}}`,
    "  \\caption{",
  ].join("\n");
  const rest = ["}", `  \\label{fig:${name}}`, "\\end{figure}"].join("\n");
  return { text: text + rest, cursor: text.length };
}
