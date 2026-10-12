// Tab dragging, the pure part: where a dragged tab would land and how its neighbours make room. The pointer handling
// lives in components/chrome/useTabDrag.ts; moving between windows is decided by the core (controller/drag.rs).

import type { Tab, TabId } from "./types";

/** A rendered tab (collapsed groups render none of their tabs). `start`/`size` along the strip's axis. */
export interface TabBox {
  id: TabId;
  start: number;
  size: number;
}

/** Pixels the pointer must travel before a press becomes a drag. */
export const DRAG_THRESHOLD = 5;
/** How far the pointer may leave the strip (across it) before the tab is torn off. */
export const TEAR_DISTANCE = 34;

/**
 * The index `tab_move` should receive (the core removes the tab, then inserts it at this index) for a dragged tab whose
 * centre is at `center`. Pinned tabs stay among pinned tabs and unpinned tabs among unpinned ones.
 */
export function dropIndex(center: number, boxes: TabBox[], draggedId: TabId, tabs: Tab[]): number {
  const dragged = tabs.find((t) => t.id === draggedId);
  const without = tabs.filter((t) => t.id !== draggedId);
  const visible = boxes.filter((b) => b.id !== draggedId);
  const k = visible.findIndex((b) => center < b.start + b.size / 2);
  let index = k < 0 ? without.length : without.findIndex((t) => t.id === visible[k]?.id);
  if (index < 0) index = without.length;
  const pinned = without.filter((t) => t.pinned).length;
  if (dragged?.pinned) return Math.min(index, pinned);
  return Math.max(index, pinned);
}

/** The position in the full strip order (`tabs`) that a `tab_move` index corresponds to, for drawing a drop marker. */
export function insertionBefore(index: number, draggedId: TabId | null, tabs: Tab[]): TabId | null {
  const without = draggedId === null ? tabs : tabs.filter((t) => t.id !== draggedId);
  return without[index]?.id ?? null;
}

/**
 * How far the tab at `index` (strip order) slides while the tab at `from` is dragged towards final index `to`: the tabs
 * it passes make room for it by its own size.
 */
export function shiftFor(index: number, from: number, to: number, size: number): number {
  if (index === from) return 0;
  if (to > from && index > from && index <= to) return -size;
  if (to < from && index >= to && index < from) return size;
  return 0;
}

/** True when the pointer has left the strip far enough (or left the window) to tear the tab off. */
export function isTornOff(
  pointer: { x: number; y: number },
  strip: { top: number; bottom: number; left: number; right: number },
  viewport: { width: number; height: number },
  axis: "x" | "y",
): boolean {
  if (pointer.x < 0 || pointer.y < 0 || pointer.x > viewport.width || pointer.y > viewport.height) return true;
  if (axis === "x") return pointer.y < strip.top - TEAR_DISTANCE || pointer.y > strip.bottom + TEAR_DISTANCE;
  return pointer.x < strip.left - TEAR_DISTANCE || pointer.x > strip.right + TEAR_DISTANCE;
}

/** Where a tab dropped in from another window goes: the gap nearest the pointer (`tab_to_window` index). */
export function foreignDropIndex(pos: number, boxes: TabBox[], tabs: Tab[]): number {
  const k = boxes.findIndex((b) => pos < b.start + b.size / 2);
  if (k < 0) return tabs.length;
  const i = tabs.findIndex((t) => t.id === boxes[k]?.id);
  const pinned = tabs.filter((t) => t.pinned).length;
  return Math.max(i < 0 ? tabs.length : i, pinned);
}
