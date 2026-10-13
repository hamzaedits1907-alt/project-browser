import { describe, expect, it } from "vitest";

import { IDLE, applyOrder, autoScrollSpeed, cancel, move, orderSettled, press, release, reordered } from "./tabDragMachine";
import type { DragState, MoveInput } from "./tabDragMachine";
import type { TabBox } from "./tabDrag";
import type { Tab } from "./types";

const t = (id: number, over: Partial<Tab> = {}): Tab => ({
  id, url: `https://t${id}.example/`, title: `Tab ${id}`, favicon: null, pinned: false, private: false, loading: false, canGoBack: false, canGoForward: false,
  zoom: 1, audible: false, muted: false, lifecycle: "live", security: "secure", error: null, heavy: false, bookmarked: false, lastActive: 0, blockedPopup: null, group: null, ...over,
});
const boxes = (ids: number[]): TabBox[] => ids.map((id, i) => ({ id, start: i * 100, size: 100 }));
const tabs = [t(1), t(2), t(3), t(4)];
const EXTENT = 400;

/** Presses tab `id` at its centre (pointer 7) and moves the pointer to content position `along` (y = 10 is in the strip). */
function at(state: DragState, along: number, opts: Partial<MoveInput> = {}): DragState {
  return move(state, { pointerId: 7, client: { x: along, y: opts.outside ? 200 : 10 }, along, outside: false, extent: EXTENT, tabs, ...opts });
}
function pressed(id: number): DragState {
  const centre = (id - 1) * 100 + 50;
  return press(id, 7, { x: centre, y: 10 }, centre, boxes([1, 2, 3, 4]), tabs);
}

describe("tab drag state machine", () => {
  it("a press that never passes the threshold is a click (activation happens on release, not on press)", () => {
    let s = pressed(2);
    s = at(s, 152);
    expect(s.phase).toBe("pressed");
    expect(release(s, 7)).toEqual({ state: IDLE, outcome: { kind: "click", id: 2 } });
  });

  it("reorders right and left, including to the first and last positions", () => {
    expect(release(at(pressed(1), 390), 7).outcome).toEqual({ kind: "reorder", id: 1, from: 0, to: 3 });
    expect(release(at(pressed(4), 5), 7).outcome).toEqual({ kind: "reorder", id: 4, from: 3, to: 0 });
    expect(release(at(pressed(2), 270), 7).outcome).toEqual({ kind: "reorder", id: 2, from: 1, to: 2 });
    expect(release(at(pressed(3), 120), 7).outcome).toEqual({ kind: "reorder", id: 3, from: 2, to: 1 });
  });

  it("dropping where it started is a harmless no-op", () => {
    const s = at(at(pressed(2), 230), 160);
    expect(release(s, 7).outcome).toEqual({ kind: "none", abortTear: false, id: 2 });
  });

  it("keeps the dragged tab inside the strip however far the pointer goes", () => {
    const right = at(pressed(1), 5000);
    expect(right.phase === "dragging" && right.offset).toBe(300);
    const left = at(pressed(4), -5000);
    expect(left.phase === "dragging" && left.offset).toBe(-300);
  });

  it("pulled out of the strip it tears off; back in, it reorders again", () => {
    const torn = at(pressed(2), 160, { outside: true });
    expect(torn.phase === "dragging" && torn.torn).toBe(true);
    expect(release(torn, 7).outcome).toEqual({ kind: "tear", id: 2 });
    const back = at(torn, 390);
    expect(back.phase === "dragging" && back.torn).toBe(false);
    // Released back in the strip: the core is told to drop the marker it showed in other windows.
    expect(release(back, 7).outcome).toEqual({ kind: "reorder", id: 2, from: 1, to: 3 });
    expect(release(at(back, 150), 7).outcome).toEqual({ kind: "none", abortTear: true, id: 2 });
  });

  it("Escape, pointer cancel or blur cancels at any point; only a tab that was torn off tells the core", () => {
    expect(cancel(pressed(1)).outcome).toEqual({ kind: "none", abortTear: false, id: 1 });
    expect(cancel(at(pressed(1), 300)).outcome).toEqual({ kind: "none", abortTear: false, id: 1 });
    expect(cancel(at(pressed(1), 300, { outside: true })).outcome).toEqual({ kind: "none", abortTear: true, id: 1 });
    expect(cancel(IDLE).outcome).toBeNull();
  });

  it("ignores other pointers and ends the drag when the dragged tab is closed", () => {
    const s = pressed(1);
    expect(move(s, { pointerId: 9, client: { x: 300, y: 10 }, along: 300, outside: false, extent: EXTENT, tabs })).toBe(s);
    expect(release(s, 9).outcome).toBeNull();
    expect(move(s, { pointerId: 7, client: { x: 300, y: 10 }, along: 300, outside: false, extent: EXTENT, tabs: [t(2), t(3)] })).toEqual(IDLE);
  });

  it("fast moves across several tabs land on the tab under the pointer", () => {
    let s = pressed(1);
    for (const x of [60, 140, 260, 330, 380]) s = at(s, x);
    expect(release(s, 7).outcome).toEqual({ kind: "reorder", id: 1, from: 0, to: 3 });
  });

  it("pinned tabs stay among pinned tabs", () => {
    const mixed = [t(1, { pinned: true }), t(2), t(3), t(4)];
    let s = press(4, 7, { x: 350, y: 10 }, 350, boxes([1, 2, 3, 4]), mixed);
    s = move(s, { pointerId: 7, client: { x: 0, y: 10 }, along: 0, outside: false, extent: EXTENT, tabs: mixed });
    expect(release(s, 7).outcome).toEqual({ kind: "reorder", id: 4, from: 3, to: 1 });
  });

  it("a press on a tab that is not in the strip does nothing", () => {
    expect(press(9, 7, { x: 0, y: 0 }, 0, boxes([1, 2]), tabs)).toEqual(IDLE);
  });
});

describe("optimistic order after a drop", () => {
  it("shows the new order at once and keeps every tab", () => {
    expect(reordered(tabs, 1, 3)).toEqual([2, 3, 4, 1]);
    expect(reordered(tabs, 4, 0)).toEqual([4, 1, 2, 3]);
    expect(applyOrder(tabs, [2, 3, 4, 1]).map((x) => x.id)).toEqual([2, 3, 4, 1]);
    // A tab that appeared meanwhile is kept; one that closed is dropped.
    expect(applyOrder([t(1), t(2), t(5)], [2, 1, 3]).map((x) => x.id)).toEqual([2, 1, 5]);
  });
  it("is settled once the core agrees, or when the set of tabs changed", () => {
    expect(orderSettled(tabs, [1, 2, 3, 4])).toBe(true);
    expect(orderSettled(tabs, [2, 1, 3, 4])).toBe(false);
    expect(orderSettled([t(1), t(2)], [2, 1, 3])).toBe(true);
  });
});

describe("auto-scroll near the strip ends", () => {
  it("scrolls faster the closer the pointer is to an edge, and not at all in the middle", () => {
    expect(autoScrollSpeed(200, 0, 400)).toBe(0);
    expect(autoScrollSpeed(390, 0, 400)).toBeGreaterThan(0);
    expect(autoScrollSpeed(399, 0, 400)).toBeGreaterThan(autoScrollSpeed(370, 0, 400));
    expect(autoScrollSpeed(5, 0, 400)).toBeLessThan(0);
    expect(autoScrollSpeed(-50, 0, 400)).toBe(-14);
  });
});
