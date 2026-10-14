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

export interface FigureOptions { path: string; caption: string; label: string; width: number; placement: string }

/** The slug used for a default label: letters, digits and dashes. */
export function slug(s: string): string {
  return s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
}

export function figureBlock(o: FigureOptions): { text: string; cursor: number } {
  const w = o.width >= 100 ? "\\linewidth" : `${(o.width / 100).toFixed(2).replace(/0$/, "")}\\linewidth`;
  const lines = [
    `\\begin{figure}${o.placement ? `[${o.placement}]` : ""}`,
    "  \\centering",
    `  \\includegraphics[width=${w}]{${o.path}}`,
  ];
  if (o.caption.trim()) lines.push(`  \\caption{${o.caption.trim()}}`);
  const label = o.label.trim() || (o.caption.trim() ? slug(o.caption).slice(0, 30) : slug(o.path.replace(/^.*\//, "").replace(/\.[^.]+$/, "")));
  if (label) lines.push(`  \\label{fig:${label.replace(/^fig:/, "")}}`);
  lines.push("\\end{figure}");
  const text = lines.join("\n");
  return { text, cursor: text.length };
}

export interface TableOptions { rows: number; cols: number; header: boolean; booktabs: boolean; caption: string; label: string }

/**
 * A table to insert. The cells are filled with placeholders ("Header 1",
 * "Cell 2.3", "Caption") so the result is visible right away; with `fields`,
 * each placeholder is a tab stop, so Tab selects the next one to type over.
 */
export function tableBlock(o: TableOptions, fields = false): string {
  // Text that looks like a snippet field would be misread as one.
  const useFields = fields && !/[#$]\{/.test(o.caption + o.label);
  const ph = (text: string) => (useFields ? `\${${text}}` : text);
  const cell = (r: number, c: number) => (o.header && r === 0 ? ph(`Header ${c + 1}`) : ph(`Cell ${o.header ? r : r + 1}.${c + 1}`));
  const row = (r: number) => "    " + Array.from({ length: o.cols }, (_, c) => cell(r, c)).join(" & ") + " \\\\";
  const [top, mid, bottom] = o.booktabs ? ["\\toprule", "\\midrule", "\\bottomrule"] : ["\\hline", "\\hline", "\\hline"];
  const lines = ["\\begin{table}[htbp]", "  \\centering"];
  const label = o.label.trim() || (o.caption.trim() ? slug(o.caption).slice(0, 30) : "");
  lines.push(`  \\begin{tabular}{${o.booktabs ? "l".repeat(o.cols) : "|" + "l|".repeat(o.cols)}}`, `    ${top}`);
  for (let r = 0; r < o.rows; r++) {
    lines.push(row(r));
    if (r === 0 && o.header && o.rows > 1) lines.push(`    ${mid}`);
  }
  // Caption and label go below the table, as is standard for floats.
  lines.push(`    ${bottom}`, "  \\end{tabular}", `  \\caption{${o.caption.trim() || ph("Caption")}}`, `  \\label{tab:${label ? label.replace(/^tab:/, "") : ph("label")}}`, "\\end{table}");
  return lines.join("\n");
}
