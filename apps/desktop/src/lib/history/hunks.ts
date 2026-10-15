// Hunks of a file against an earlier version, and putting one hunk back.
// Pure functions on strings: the same diff @codemirror/merge shows in the diff view.
import { Text } from "@codemirror/state";
import { Chunk } from "@codemirror/merge";

export interface Hunk {
  /** Stable while the file is unchanged: the same change gets the same key, even when two are identical. */
  key: string;
  /** Character offsets into the old (snapshot) and the current text; whole lines. */
  oldFrom: number;
  oldTo: number;
  newFrom: number;
  newTo: number;
  oldText: string;
  newText: string;
  /** 1-based line of the hunk in the current text. */
  line: number;
}

/** Differences between a snapshot's text and the current text, in document order. */
export function computeHunks(oldText: string, newText: string): Hunk[] {
  if (oldText === newText) return [];
  const a = Text.of(oldText.split("\n"));
  const b = Text.of(newText.split("\n"));
  const seen = new Map<string, number>();
  return Chunk.build(a, b).map((c) => {
    const o = oldText.slice(c.fromA, c.toA);
    const n = newText.slice(c.fromB, c.toB);
    const base = `${o}\u0000${n}`;
    const nth = seen.get(base) ?? 0;
    seen.set(base, nth + 1);
    return {
      key: `${nth}\u0000${base}`,
      oldFrom: c.fromA,
      oldTo: c.toA,
      newFrom: c.fromB,
      newTo: c.toB,
      oldText: o,
      newText: n,
      line: b.lineAt(Math.min(c.fromB, newText.length)).number,
    };
  });
}

/** The current text with these hunks put back as they were in the snapshot. */
export function rejectHunks(newText: string, hunks: Hunk[]): string {
  let out = newText;
  // From the end, so earlier offsets stay valid.
  for (const h of [...hunks].sort((x, y) => y.newFrom - x.newFrom)) {
    out = out.slice(0, h.newFrom) + h.oldText + out.slice(h.newTo);
  }
  return out;
}

/** Files that are plain text; binaries come back from git as garbage and can't be reviewed. */
export const isTextPath = (path: string) =>
  /\.(tex|bib|sty|cls|bst|bbx|cbx|dbx|def|cfg|clo|ltx|dtx|ins|ist|md|txt|csv|tsv|toml|json|ya?ml|tikz|latexmkrc)$/i.test(path) ||
  !/\.[^/.]+$/.test(path);
