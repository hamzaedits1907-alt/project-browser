import { describe, expect, it } from "vitest";

import type { Tab } from "../../lib/types";
import { comparableUrl, duplicateTabs, linksAsMarkdown, siteGroups, sortedBySite, tabsToTheRight } from "./tabtools";

let next = 1;
function tab(url: string, extra: Partial<Tab> = {}): Tab {
  return {
    id: next++,
    url,
    title: url,
    favicon: null,
    pinned: false,
    private: false,
    loading: false,
    canGoBack: false,
    canGoForward: false,
    zoom: 1,
    audible: false,
    muted: false,
    lifecycle: "live",
    security: "secure",
    error: null,
    heavy: false,
    bookmarked: false,
    lastActive: 0,
    blockedPopup: null,
    group: null,
    ...extra,
  };
}

describe("tab tools", () => {
  it("finds duplicates, keeping the active and pinned copies", () => {
    const a = tab("https://a.example/");
    const a2 = tab("https://a.example#top");
    const b = tab("https://b.example/x");
    const b2 = tab("https://b.example/x", { pinned: true });
    const s = tab("browser://settings");
    const s2 = tab("browser://settings");
    expect(duplicateTabs([a, a2, b, b2, s, s2], a2.id)).toEqual([a.id, b.id]);
    expect(comparableUrl("https://x.example/a/#h")).toBe("https://x.example/a");
  });

  it("sorts by site with pinned tabs first and browser pages last", () => {
    const p = tab("https://zzz.example/", { pinned: true });
    const n = tab("browser://newtab");
    const y = tab("https://www.youtube.com/watch?v=1");
    const g = tab("https://github.com/");
    const y2 = tab("https://youtube.com/");
    expect(sortedBySite([n, y, p, g, y2])).toEqual([p.id, g.id, y.id, y2.id, n.id]);
  });

  it("groups only sites with two or more loose tabs", () => {
    const a = tab("https://a.example/1");
    const a2 = tab("https://a.example/2");
    const b = tab("https://b.example/");
    const grouped = tab("https://a.example/3", { group: 7 });
    expect(siteGroups([a, b, a2, grouped])).toEqual([{ site: "a.example", tabs: [a.id, a2.id] }]);
  });

  it("closes unpinned tabs to the right and exports links", () => {
    const a = tab("https://a.example/");
    const b = tab("https://b.example/", { pinned: true });
    const c = tab("https://c.example/", { title: "C [beta]" });
    expect(tabsToTheRight([a, b, c], a.id)).toEqual([c.id]);
    expect(tabsToTheRight([a, b, c], 999)).toEqual([]);
    expect(linksAsMarkdown([c, tab("browser://newtab")])).toBe("- [C beta](https://c.example/)");
  });
});
