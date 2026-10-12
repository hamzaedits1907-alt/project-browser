import { describe, expect, it } from "vitest";

import { clampSlot, filterTabs, moveTargetIndex, slotForPointer, summarizeTabs } from "./tabs";
import type { Tab } from "./types";

const t = (id: number, over: Partial<Tab> = {}): Tab => ({
  id, url: `https://t${id}.example/`, title: `Tab ${id}`, favicon: null, pinned: false, private: false, loading: false, canGoBack: false, canGoForward: false,
  zoom: 1, audible: false, muted: false, lifecycle: "live", security: "secure", error: null, heavy: false, bookmarked: false, lastActive: 0, blockedPopup: null, group: null, ...over,
});

describe("drag and drop slots", () => {
  it("accounts for the removal the core does before inserting", () => {
    expect(moveTargetIndex(0, 3)).toBe(2); // [a b c d]: drop a before d -> b c a d
    expect(moveTargetIndex(3, 0)).toBe(0);
    expect(moveTargetIndex(2, 2)).toBe(2);
    expect(moveTargetIndex(2, 3)).toBe(2); // dropping right after itself is a no-op
  });
  it("finds the nearest gap from tab midpoints", () => {
    const rects = [{ left: 0, width: 100 }, { left: 100, width: 100 }, { left: 200, width: 100 }];
    expect(slotForPointer(10, rects)).toBe(0);
    expect(slotForPointer(60, rects)).toBe(1);
    expect(slotForPointer(160, rects)).toBe(2);
    expect(slotForPointer(290, rects)).toBe(3);
    expect(slotForPointer(5, [])).toBe(0);
  });
  it("keeps pinned and unpinned tabs on their side of the boundary", () => {
    const tabs = [t(1, { pinned: true }), t(2, { pinned: true }), t(3), t(4)];
    expect(clampSlot(4, tabs[0] as Tab, tabs)).toBe(2);
    expect(clampSlot(0, tabs[3] as Tab, tabs)).toBe(2);
    expect(clampSlot(3, tabs[3] as Tab, tabs)).toBe(3);
  });
});

describe("tab helpers", () => {
  it("summarises lifecycles", () => {
    expect(summarizeTabs([t(1), t(2, { lifecycle: "sleeping" }), t(3, { lifecycle: "unloaded" }), t(4)])).toEqual({ live: 2, sleeping: 1, unloaded: 1 });
  });
  it("filters by title or url, case-insensitively", () => {
    const tabs = [t(1, { title: "Steam Store" }), t(2, { url: "https://github.com/" }), t(3)];
    expect(filterTabs(tabs, "STEAM").map((x) => x.id)).toEqual([1]);
    expect(filterTabs(tabs, "github").map((x) => x.id)).toEqual([2]);
    expect(filterTabs(tabs, "  ").length).toBe(3);
  });
});
