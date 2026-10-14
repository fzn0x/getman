// Fuzzy matching for the command palette and the tree filter: every
// character of the query must appear in order; tighter and earlier
// matches, and matches at word starts, rank higher.

export interface Match {
  score: number;
  /** Indexes of the matched characters, for highlighting. */
  positions: number[];
}

export function fuzzy(query: string, text: string): Match | null {
  const q = query.toLowerCase().replace(/\s+/g, "");
  if (q === "") return { score: 0, positions: [] };
  const t = text.toLowerCase();
  const positions: number[] = [];
  let score = 0;
  let from = 0;
  let previous = -2;
  for (const char of q) {
    const at = t.indexOf(char, from);
    if (at < 0) return null;
    positions.push(at);
    const boundary = at === 0 || /[\s/_\-.:]/.test(t[at - 1] ?? "");
    score += 1;
    if (at === previous + 1) score += 4;
    if (boundary) score += 3;
    score -= Math.min(at - from, 6) * 0.25;
    previous = at;
    from = at + 1;
  }
  if (t.startsWith(q)) score += 5;
  score -= text.length * 0.01;
  return { score, positions };
}

/** Items that match, best first. */
export function rank<T>(query: string, items: T[], text: (item: T) => string): T[] {
  return items
    .map((item) => ({ item, match: fuzzy(query, text(item)) }))
    .filter((entry): entry is { item: T; match: Match } => entry.match !== null)
    .sort((a, b) => b.match.score - a.match.score)
    .map((entry) => entry.item);
}
