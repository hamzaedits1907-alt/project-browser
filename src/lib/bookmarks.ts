import type { Bookmark } from "./types";

export const BAR_ID = 1;
export const OTHER_ID = 2;

export function isRoot(b: Bookmark): boolean {
  return b.id === BAR_ID || b.id === OTHER_ID;
}

export function childrenOf(all: Bookmark[], parentId: number): Bookmark[] {
  return all.filter((b) => b.parentId === parentId).sort((a, b) => a.position - b.position || a.id - b.id);
}

export function folders(all: Bookmark[]): Bookmark[] {
  return all.filter((b) => b.kind === "folder");
}

/** Folders in tree order with their nesting depth, for "move to" pickers. */
export function folderOptions(all: Bookmark[], excludeId?: number): { id: number; label: string; depth: number }[] {
  const out: { id: number; label: string; depth: number }[] = [];
  const walk = (parentId: number, depth: number) => {
    for (const f of childrenOf(all, parentId)) {
      if (f.kind !== "folder" || f.id === excludeId) continue;
      out.push({ id: f.id, label: f.title, depth });
      walk(f.id, depth + 1);
    }
  };
  for (const root of [BAR_ID, OTHER_ID]) {
    const r = all.find((b) => b.id === root);
    if (!r) continue;
    out.push({ id: r.id, label: r.title, depth: 0 });
    walk(root, 1);
  }
  return out;
}

export function countUrls(all: Bookmark[], folderId: number): number {
  let n = 0;
  for (const c of childrenOf(all, folderId)) n += c.kind === "url" ? 1 : countUrls(all, c.id);
  return n;
}

/** Case-insensitive search across titles and URLs. */
export function searchBookmarks(all: Bookmark[], query: string): Bookmark[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  return all.filter((b) => b.kind === "url" && (b.title.toLowerCase().includes(q) || (b.url ?? "").toLowerCase().includes(q)));
}

/** Rough pixel width of a bookmark-bar chip, used to decide what overflows into the "»" menu. */
export function estimateChipWidth(title: string): number {
  return Math.min(180, 38 + title.length * 6.6);
}

export function splitOverflow(items: Bookmark[], available: number, overflowButton = 36): { visible: Bookmark[]; hidden: Bookmark[] } {
  let used = 0;
  const visible: Bookmark[] = [];
  for (let i = 0; i < items.length; i++) {
    const item = items[i];
    if (!item) continue;
    const w = estimateChipWidth(item.title) + 4;
    const reserve = i < items.length - 1 ? overflowButton : 0;
    if (used + w + reserve > available) break;
    used += w;
    visible.push(item);
  }
  return { visible, hidden: items.slice(visible.length) };
}
