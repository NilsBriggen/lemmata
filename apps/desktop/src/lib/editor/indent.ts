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

/**
 * Where a block of lines goes at a cursor. With only blanks before the cursor, the block starts there and
 * its other lines take that indentation; after text, it starts on a new line indented like the cursor's line.
 */
export function placeBlock(lineText: string, col: number): { prefix: string; indent: string } {
  const before = lineText.slice(0, col);
  if (!before.trim()) return { prefix: "", indent: before };
  const indent = /^\s*/.exec(lineText)![0];
  return { prefix: "\n" + indent, indent };
}

/** Indents every line after the first (empty lines stay empty). */
export const indentLines = (text: string, indent: string) => (indent ? text.replace(/\n(?=[^\n])/g, "\n" + indent) : text);
