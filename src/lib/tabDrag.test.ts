import { describe, expect, it } from "vitest";

import { dropIndex, foreignDropIndex, insertionBefore, isTornOff, shiftFor } from "./tabDrag";
import type { TabBox } from "./tabDrag";
import type { Tab } from "./types";

const t = (id: number, over: Partial<Tab> = {}): Tab => ({
  id, url: `https://t${id}.example/`, title: `Tab ${id}`, favicon: null, pinned: false, private: false, loading: false, canGoBack: false, canGoForward: false,
  zoom: 1, audible: false, muted: false, lifecycle: "live", security: "secure", error: null, heavy: false, bookmarked: false, lastActive: 0, blockedPopup: null, group: null, ...over,
});

// Four 100px tabs side by side.
const boxes = (ids: number[]): TabBox[] => ids.map((id, i) => ({ id, start: i * 100, size: 100 }));

describe("drop index", () => {
  const tabs = [t(1), t(2), t(3), t(4)];
  it("follows the dragged tab's centre across its neighbours' midpoints", () => {
    // Dragging tab 1 (centre starts at 50) to the right.
    expect(dropIndex(90, boxes([1, 2, 3, 4]), 1, tabs)).toBe(0);
    expect(dropIndex(160, boxes([1, 2, 3, 4]), 1, tabs)).toBe(1);
    expect(dropIndex(260, boxes([1, 2, 3, 4]), 1, tabs)).toBe(2);
    expect(dropIndex(390, boxes([1, 2, 3, 4]), 1, tabs)).toBe(3);
    // Dragging tab 4 to the far left.
    expect(dropIndex(-20, boxes([1, 2, 3, 4]), 4, tabs)).toBe(0);
  });
  it("never crosses the pinned boundary", () => {
    const mixed = [t(1, { pinned: true }), t(2, { pinned: true }), t(3), t(4)];
    expect(dropIndex(0, boxes([1, 2, 3, 4]), 4, mixed)).toBe(2);
    expect(dropIndex(390, boxes([1, 2, 3, 4]), 1, mixed)).toBe(1);
  });
  it("counts the tabs hidden in a collapsed group when it computes the core's index", () => {
    // Tabs 2 and 3 are in a collapsed group and not drawn; the drawn strip is [1, 4, 5].
    const all = [t(1), t(2, { group: 7 }), t(3, { group: 7 }), t(4), t(5)];
    const drawn: TabBox[] = [{ id: 1, start: 0, size: 100 }, { id: 4, start: 100, size: 100 }, { id: 5, start: 200, size: 100 }];
    // Dropping tab 5 before tab 4 means index 3 in [1, 2, 3, 4].
    expect(dropIndex(120, drawn, 5, all)).toBe(3);
  });
  it("names the tab a marker goes in front of", () => {
    const tabs4 = [t(1), t(2), t(3), t(4)];
    expect(insertionBefore(0, 2, tabs4)).toBe(1);
    expect(insertionBefore(1, 2, tabs4)).toBe(3);
    expect(insertionBefore(3, 2, tabs4)).toBeNull();
    expect(insertionBefore(2, null, tabs4)).toBe(3);
  });
});

describe("making room", () => {
  it("slides the passed tabs by the dragged tab's size, towards where it came from", () => {
    // Dragging index 1 to index 3: tabs 2 and 3 move left.
    expect([0, 1, 2, 3].map((i) => shiftFor(i, 1, 3, 100))).toEqual([0, 0, -100, -100]);
    // Dragging index 3 to index 0: tabs 0..2 move right.
    expect([0, 1, 2, 3].map((i) => shiftFor(i, 3, 0, 80))).toEqual([80, 80, 80, 0]);
    expect([0, 1, 2].map((i) => shiftFor(i, 1, 1, 80))).toEqual([0, 0, 0]);
  });
});

describe("tearing a tab off", () => {
  const strip = { top: 0, bottom: 40, left: 54, right: 1200 };
  const viewport = { width: 1280, height: 800 };
  it("needs a clear move away from the strip or out of the window", () => {
    expect(isTornOff({ x: 300, y: 60 }, strip, viewport, "x")).toBe(false);
    expect(isTornOff({ x: 300, y: 90 }, strip, viewport, "x")).toBe(true);
    expect(isTornOff({ x: -5, y: 20 }, strip, viewport, "x")).toBe(true);
    expect(isTornOff({ x: 1300, y: 20 }, strip, viewport, "x")).toBe(true);
  });
  it("uses the other axis for vertical tabs", () => {
    const column = { top: 40, bottom: 800, left: 54, right: 300 };
    expect(isTornOff({ x: 320, y: 300 }, column, viewport, "y")).toBe(false);
    expect(isTornOff({ x: 400, y: 300 }, column, viewport, "y")).toBe(true);
  });
});

describe("dropping a tab from another window", () => {
  it("lands in the gap nearest the pointer, after the pinned tabs", () => {
    const tabs = [t(1, { pinned: true }), t(2), t(3)];
    expect(foreignDropIndex(10, boxes([1, 2, 3]), tabs)).toBe(1);
    expect(foreignDropIndex(170, boxes([1, 2, 3]), tabs)).toBe(2);
    expect(foreignDropIndex(500, boxes([1, 2, 3]), tabs)).toBe(3);
  });
});
