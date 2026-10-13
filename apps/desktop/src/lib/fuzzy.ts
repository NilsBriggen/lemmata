// Small fuzzy matcher for the palette and quick open: every query character
// must appear in order; consecutive and word-start hits score higher.
export function fuzzyScore(query: string, text: string): number | null {
  const q = query.toLowerCase().replace(/\s+/g, "");
  if (!q) return 0;
  const t = text.toLowerCase();
  let score = 0;
  let ti = 0;
  let last = -2;
  for (const ch of q) {
    const found = t.indexOf(ch, ti);
    if (found < 0) return null;
    const wordStart = found === 0 || /[\s/_.\-]/.test(t[found - 1]);
    score += (found === last + 1 ? 5 : 1) + (wordStart ? 3 : 0);
    last = found;
    ti = found + 1;
  }
  // Prefer shorter texts for equal matches.
  return score - t.length * 0.01;
}

export function fuzzyFilter<T>(items: T[], query: string, text: (item: T) => string): T[] {
  return items
    .map((item) => ({ item, score: fuzzyScore(query, text(item)) }))
    .filter((x) => x.score !== null)
    .sort((a, b) => b.score! - a.score!)
    .map((x) => x.item);
}
