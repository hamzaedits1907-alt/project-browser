import { describe, expect, it } from "vitest";

import { fuzzyScore, rank } from "./rank";

describe("command center ranking", () => {
  it("matches in order and prefers word starts and direct hits", () => {
    expect(fuzzyScore("spl", "Split view: open")).toBeGreaterThan(fuzzyScore("spl", "Display settings"));
    expect(fuzzyScore("ntab", "New tab")).toBeGreaterThan(0);
    expect(fuzzyScore("xyz", "New tab")).toBe(0);
    expect(fuzzyScore("", "anything")).toBe(1);
    expect(fuzzyScore("tab", "Tabs")).toBeGreaterThan(fuzzyScore("tab", "Close other tabs in this window"));
  });

  it("ranks with keywords and boosts", () => {
    const items = [
      { title: "History", keywords: "past visited" },
      { title: "Screenshot", keywords: "capture image picture" },
      { title: "github.com", boost: 0 },
      { title: "GitHub tab", boost: 200 },
    ];
    expect(rank(items, "capture")[0]?.title).toBe("Screenshot");
    expect(rank(items, "github")[0]?.title).toBe("GitHub tab");
    expect(rank(items, "zzz")).toEqual([]);
    expect(rank(items, "", 2)).toHaveLength(2);
  });
});
