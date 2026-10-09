import type { Tab } from "./types";

/**
 * Converts a drop "slot" (the gap before tab `slot` in the current order,
 * 0..tabs.length) into the index `tab_move` expects. The core removes the tab
 * first and then inserts, so dropping to the right of the original position
 * shifts by one.
 */
export function moveTargetIndex(from: number, slot: number): number {
  return slot > from ? slot - 1 : slot;
}

/** Which gap the pointer is nearest to, given the horizontal midpoints of the tab elements. */
export function slotForPointer(x: number, rects: { left: number; width: number }[]): number {
  for (let i = 0; i < rects.length; i++) {
    const r = rects[i];
    if (r && x < r.left + r.width / 2) return i;
  }
  return rects.length;
}

/** Pinned tabs live at the front; a drag may not cross that boundary. */
export function clampSlot(slot: number, dragged: Tab, tabs: Tab[]): number {
  const pinned = tabs.filter((t) => t.pinned).length;
  return dragged.pinned ? Math.min(slot, pinned) : Math.max(slot, pinned);
}

export function summarizeTabs(tabs: Tab[]): { live: number; sleeping: number; unloaded: number } {
  let live = 0;
  let sleeping = 0;
  let unloaded = 0;
  for (const t of tabs) {
    if (t.lifecycle === "live") live++;
    else if (t.lifecycle === "sleeping") sleeping++;
    else unloaded++;
  }
  return { live, sleeping, unloaded };
}

/** Case-insensitive match on title or URL, for the tab search popover. */
export function filterTabs(tabs: Tab[], query: string): Tab[] {
  const q = query.trim().toLowerCase();
  if (!q) return tabs;
  return tabs.filter((t) => t.title.toLowerCase().includes(q) || t.url.toLowerCase().includes(q));
}
