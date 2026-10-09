import { describe, expect, it } from "vitest";

import { BAR_ID, OTHER_ID, childrenOf, countUrls, folderOptions, searchBookmarks, splitOverflow } from "./bookmarks";
import type { Bookmark } from "./types";

const b = (id: number, parentId: number | null, kind: "url" | "folder", title: string, position = 0, url: string | null = null): Bookmark => ({
  id, parentId, kind, title, url, position, createdAt: 0,
});

const all: Bookmark[] = [
  b(1, null, "folder", "Bookmarks bar"),
  b(2, null, "folder", "Other bookmarks", 1),
  b(3, 1, "url", "Steam", 1, "https://store.steampowered.com/"),
  b(4, 1, "url", "GitHub", 0, "https://github.com/"),
  b(5, 1, "folder", "Dev", 2),
  b(6, 5, "url", "MDN", 0, "https://developer.mozilla.org/"),
  b(7, 2, "folder", "Old", 0),
];

describe("bookmark tree helpers", () => {
  it("orders children by position", () => {
    expect(childrenOf(all, BAR_ID).map((x) => x.title)).toEqual(["GitHub", "Steam", "Dev"]);
  });
  it("counts urls recursively", () => {
    expect(countUrls(all, BAR_ID)).toBe(3);
    expect(countUrls(all, OTHER_ID)).toBe(0);
  });
  it("lists folders with depth and can exclude one", () => {
    expect(folderOptions(all).map((f) => [f.label, f.depth])).toEqual([["Bookmarks bar", 0], ["Dev", 1], ["Other bookmarks", 0], ["Old", 1]]);
    expect(folderOptions(all, 5).map((f) => f.label)).not.toContain("Dev");
  });
  it("searches titles and urls", () => {
    expect(searchBookmarks(all, "mdn").map((x) => x.id)).toEqual([6]);
    expect(searchBookmarks(all, "steampowered").map((x) => x.id)).toEqual([3]);
    expect(searchBookmarks(all, "")).toEqual([]);
  });
  it("moves what does not fit into the overflow menu", () => {
    const items = Array.from({ length: 30 }, (_, i) => b(100 + i, 1, "url", `Bookmark number ${i}`));
    const { visible, hidden } = splitOverflow(items, 600);
    expect(visible.length).toBeGreaterThan(0);
    expect(visible.length + hidden.length).toBe(30);
    expect(splitOverflow(items.slice(0, 2), 600).hidden).toEqual([]);
  });
});
