// The tab drag as one explicit state machine (pure, unit-tested): idle -> pressed -> dragging -> idle.
//
// Every pointer sequence ends in exactly one outcome:
//   click   - released before moving past the threshold: activate the tab (a press alone never activates, so a
//             background tab can be dragged without switching to it);
//   reorder - released inside the strip at a new index (`tab_move`);
//   tear    - released after being pulled out of the strip: the core moves it to the window under the pointer or
//             into a new window (`tab_drag_end`);
//   none    - released where it started, or cancelled (Escape, pointer cancel, lost capture, window blur, the tab
//             closing); `abortTear` tells the core to drop its drag-over marker if the tab had been pulled out.
// Positions along the strip are in "content" coordinates (client position + scroll offset), so auto-scrolling the
// strip while dragging never makes the drop target jump.

import { DRAG_THRESHOLD, dropIndex } from "./tabDrag";
import type { TabBox } from "./tabDrag";
import type { Tab, TabId } from "./types";

export interface Point {
  x: number;
  y: number;
}

export interface Pressed {
  phase: "pressed";
  id: TabId;
  pointerId: number;
  /** Client coordinates of the press. */
  start: Point;
  /** Content coordinate (along the axis) of the press. */
  startAlong: number;
  boxes: TabBox[];
  box: TabBox;
  from: number;
}

export interface Dragging extends Omit<Pressed, "phase"> {
  phase: "dragging";
  /** Offset of the dragged tab from where it started (clamped to the strip). */
  offset: number;
  to: number;
  torn: boolean;
  everTorn: boolean;
}

export type DragState = { phase: "idle" } | Pressed | Dragging;

export type DragOutcome =
  | { kind: "click"; id: TabId }
  | { kind: "reorder"; id: TabId; from: number; to: number }
  | { kind: "tear"; id: TabId }
  | { kind: "none"; abortTear: boolean; id: TabId | null };

export const IDLE: DragState = { phase: "idle" };

export function press(id: TabId, pointerId: number, start: Point, startAlong: number, boxes: TabBox[], tabs: Tab[]): DragState {
  const box = boxes.find((b) => b.id === id);
  const from = tabs.findIndex((t) => t.id === id);
  if (!box || from < 0) return IDLE;
  return { phase: "pressed", id, pointerId, start, startAlong, boxes, box, from };
}

export interface MoveInput {
  pointerId: number;
  client: Point;
  /** Content coordinate of the pointer along the axis. */
  along: number;
  /** The pointer is far enough outside the strip to tear the tab off. */
  outside: boolean;
  /** Length of the strip's content along the axis (the dragged tab stays inside it). */
  extent: number;
  tabs: Tab[];
}

export function move(state: DragState, m: MoveInput): DragState {
  if (state.phase === "idle" || m.pointerId !== state.pointerId) return state;
  if (!m.tabs.some((t) => t.id === state.id)) return IDLE;
  if (state.phase === "pressed" && Math.hypot(m.client.x - state.start.x, m.client.y - state.start.y) < DRAG_THRESHOLD) return state;
  const base = state.phase === "dragging" ? state : { ...state, everTorn: false };
  const raw = m.along - state.startAlong;
  const offset = Math.max(-state.box.start, Math.min(raw, m.extent - state.box.start - state.box.size));
  const torn = m.outside;
  // The landing slot follows the pointer itself; only the drawn tab is kept inside the strip (so the ends stay reachable).
  const center = state.box.start + raw + state.box.size / 2;
  const to = torn ? state.from : dropIndex(center, state.boxes, state.id, m.tabs);
  return { ...base, phase: "dragging", offset: torn ? 0 : offset, to, torn, everTorn: base.everTorn || torn };
}

/** The pointer was released. */
export function release(state: DragState, pointerId: number): { state: DragState; outcome: DragOutcome | null } {
  if (state.phase === "idle" || pointerId !== state.pointerId) return { state, outcome: null };
  if (state.phase === "pressed") return { state: IDLE, outcome: { kind: "click", id: state.id } };
  if (state.torn) return { state: IDLE, outcome: { kind: "tear", id: state.id } };
  if (state.to !== state.from) return { state: IDLE, outcome: { kind: "reorder", id: state.id, from: state.from, to: state.to } };
  return { state: IDLE, outcome: { kind: "none", abortTear: state.everTorn, id: state.id } };
}

/** Escape, pointer cancel, lost capture, window blur, unmount, or the tab went away. */
export function cancel(state: DragState): { state: DragState; outcome: DragOutcome | null } {
  if (state.phase === "idle") return { state, outcome: null };
  return { state: IDLE, outcome: { kind: "none", abortTear: state.phase === "dragging" && state.everTorn, id: state.id } };
}

/** The strip order right after a `tab_move(id, to)`, shown until the core's state confirms it. */
export function reordered(tabs: Tab[], id: TabId, to: number): TabId[] {
  const ids = tabs.map((t) => t.id).filter((x) => x !== id);
  ids.splice(Math.max(0, Math.min(to, ids.length)), 0, id);
  return ids;
}

/** `tabs` in the optimistic `order` (ids missing from it keep their place at the end, unknown ids are dropped). */
export function applyOrder(tabs: Tab[], order: TabId[] | null): Tab[] {
  if (!order) return tabs;
  const byId = new Map(tabs.map((t) => [t.id, t]));
  const out = order.map((id) => byId.get(id)).filter((t): t is Tab => !!t);
  for (const t of tabs) if (!order.includes(t.id)) out.push(t);
  return out;
}

/** True once the core's tab order matches the optimistic one (or the set of tabs changed, which makes it moot). */
export function orderSettled(tabs: Tab[], order: TabId[]): boolean {
  if (tabs.length !== order.length) return true;
  const set = new Set(order);
  if (tabs.some((t) => !set.has(t.id))) return true;
  return tabs.every((t, i) => t.id === order[i]);
}

/** Auto-scroll speed (px per frame) when the pointer is within `edge` px of the strip's visible ends. */
export function autoScrollSpeed(pos: number, start: number, end: number, edge = 36, max = 14): number {
  if (pos < start + edge) return -Math.ceil(max * Math.min(1, (start + edge - pos) / edge));
  if (pos > end - edge) return Math.ceil(max * Math.min(1, (pos - (end - edge)) / edge));
  return 0;
}
