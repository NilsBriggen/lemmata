// Filtering, sorting and duplicate lookup for the bibliography manager.
import type { Duplicate, ManagedEntry } from "./api";

export type SortBy = "key" | "author" | "year";

/** Entries containing every search word in their key, authors, title, year or DOI. */
export function filterEntries(entries: ManagedEntry[], query: string): ManagedEntry[] {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!terms.length) return entries;
  return entries.filter((e) => {
    const hay = `${e.key} ${e.authors} ${e.title} ${e.year} ${e.doi}`.toLowerCase();
    return terms.every((t) => hay.includes(t));
  });
}

/** A sorted copy. Entries without an author or year go last; newest year first. */
export function sortEntries(entries: ManagedEntry[], by: SortBy): ManagedEntry[] {
  const text = (a: string, b: string) => (a && !b ? -1 : !a && b ? 1 : a.localeCompare(b, undefined, { sensitivity: "base" }));
  const cmp: Record<SortBy, (a: ManagedEntry, b: ManagedEntry) => number> = {
    key: (a, b) => text(a.key, b.key),
    author: (a, b) => text(a.authors, b.authors) || text(a.key, b.key),
    year: (a, b) => (a.year && !b.year ? -1 : !a.year && b.year ? 1 : b.year.localeCompare(a.year)) || text(a.key, b.key),
  };
  return [...entries].sort(cmp[by]);
}

export type Warning = { kind: Duplicate["kind"]; others: number[] };

/** For each entry index, the duplicate groups it belongs to (with the other members). */
export function warningsByEntry(duplicates: Duplicate[]): Map<number, Warning[]> {
  const out = new Map<number, Warning[]>();
  for (const d of duplicates) {
    for (const i of d.entries) {
      const list = out.get(i) ?? [];
      list.push({ kind: d.kind, others: d.entries.filter((j) => j !== i) });
      out.set(i, list);
    }
  }
  return out;
}

/** A key not in `taken` (compared without case): `key`, `keya`, `keyb`, … */
export function freeKey(base: string, taken: string[]): string {
  const lower = new Set(taken.map((k) => k.toLowerCase()));
  let key = base;
  for (let n = 0; lower.has(key.toLowerCase()) && n < 26; n++) key = base + String.fromCharCode(97 + n);
  return key;
}

/** Whether BibTeX would accept the key. Mirrors `valid_key` in lemmata-bib. */
export function validKey(key: string): boolean {
  return key.length > 0 && !/[\s,{}"#%'()=\\]/.test(key);
}
