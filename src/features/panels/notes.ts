// Notes kept by the Notes panel: one per site ("pb:notes.site.<host>") and a scratchpad ("pb:notes.all"), in this
// profile's shell storage. Pure helpers for the "All notes" list (search, export).

export interface NoteEntry {
  /** Storage key without the "pb:" prefix. */
  key: string;
  /** The site, or null for the scratchpad. */
  site: string | null;
  text: string;
}

export function noteEntries(store: Record<string, string>): NoteEntry[] {
  const out: NoteEntry[] = [];
  for (const [raw, value] of Object.entries(store)) {
    if (!raw.startsWith("pb:notes.")) continue;
    let text: unknown;
    try {
      text = JSON.parse(value);
    } catch {
      continue;
    }
    if (typeof text !== "string" || !text.trim()) continue;
    const key = raw.slice(3);
    out.push({ key, site: key.startsWith("notes.site.") ? key.slice("notes.site.".length) : null, text });
  }
  return out.sort((a, b) => (a.site === null ? -1 : b.site === null ? 1 : a.site.localeCompare(b.site)));
}

export function searchNotes(entries: NoteEntry[], query: string): NoteEntry[] {
  const q = query.trim().toLowerCase();
  if (!q) return entries;
  return entries.filter((e) => (e.site ?? "scratchpad").includes(q) || e.text.toLowerCase().includes(q));
}

export function exportText(entries: NoteEntry[]): string {
  return entries.map((e) => `# ${e.site ?? "Scratchpad"}\n\n${e.text.trim()}\n`).join("\n");
}
