// Indentation: how wide one level is, and rewriting generated blocks (figures,
// tables, environments) that are written with two spaces per level.
export type Indent = 2 | 4 | 8 | "tab";

export const INDENTS: [Indent, string][] = [
  [2, "2 spaces"],
  [4, "4 spaces"],
  [8, "8 spaces"],
  ["tab", "Tab"],
];

/** The text of one indentation level. */
export const unitOf = (i: Indent) => (i === "tab" ? "\t" : " ".repeat(i));

/** How wide a tab character is drawn. */
export const tabSizeOf = (i: Indent) => (i === "tab" ? 4 : i);

/** Settings saved by hand or by older versions may hold anything. */
export const validIndent = (v: unknown): Indent => (v === "tab" || v === 2 || v === 4 || v === 8 ? v : 4);

/** Rewrites leading pairs of spaces (the width generated blocks use) into `unit`. */
export function reindent(text: string, unit: string): string {
  if (unit === "  ") return text;
  return text.replace(/^( {2})+/gm, (m) => unit.repeat(m.length / 2));
}
