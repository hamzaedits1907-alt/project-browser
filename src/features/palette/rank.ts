// Fuzzy ranking for the Command Center. Pure and tested.

export interface Rankable {
  title: string;
  /** Extra words that should find the item (synonyms, the address of a page). */
  keywords?: string;
  /** Added to the score: commands and open tabs rank above history for the same match. */
  boost?: number;
}

/**
 * How well `query` matches `text` (0 = no match). Every query character must appear in order. Matches at the start
 * of the text or of a word, and consecutive characters, score higher; shorter texts win ties.
 */
export function fuzzyScore(query: string, text: string): number {
  const q = query.toLowerCase().trim();
  const t = text.toLowerCase();
  if (!q) return 1;
  if (!t) return 0;
  const direct = t.indexOf(q);
  if (direct >= 0) {
    const atWord = direct === 0 || /[\s\-_./:]/.test(t[direct - 1] ?? "");
    return 1000 - direct * 2 + (atWord ? 300 : 0) - t.length * 0.5;
  }
  let score = 0;
  let ti = 0;
  let run = 0;
  for (const ch of q) {
    if (ch === " ") continue;
    const found = t.indexOf(ch, ti);
    if (found < 0) return 0;
    const atWord = found === 0 || /[\s\-_./:]/.test(t[found - 1] ?? "");
    run = found === ti ? run + 1 : 0;
    score += 10 + (atWord ? 25 : 0) + run * 8 - Math.min(found - ti, 20);
    ti = found + 1;
  }
  return Math.max(1, score - t.length * 0.2);
}

export function rank<T extends Rankable>(items: T[], query: string, limit = 50): T[] {
  if (!query.trim()) return items.slice(0, limit);
  return items
    .map((item) => {
      const s = Math.max(fuzzyScore(query, item.title), item.keywords ? fuzzyScore(query, item.keywords) * 0.8 : 0);
      return { item, s: s > 0 ? s + (item.boost ?? 0) : 0 };
    })
    .filter((x) => x.s > 0)
    .sort((a, b) => b.s - a.s)
    .slice(0, limit)
    .map((x) => x.item);
}
