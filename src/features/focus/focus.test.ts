import { describe, expect, it } from "vitest";

import { addUsage, blockReason, dayKey, formatDuration, matchesSite, siteOf, topSites, totalFor } from "./focus";
import type { FocusConfig } from "./focus";

const NOW = new Date(2026, 9, 12, 15, 0, 0).getTime();

describe("screen time", () => {
  it("adds seconds per site and day and forgets old days", () => {
    let u = addUsage({}, "youtube.com", 30, NOW);
    u = addUsage(u, "youtube.com", 15, NOW);
    u = addUsage(u, "github.com", 60, NOW);
    expect(u[dayKey(NOW)]).toEqual({ "youtube.com": 45, "github.com": 60 });
    expect(topSites(u, dayKey(NOW))[0]).toEqual({ site: "github.com", seconds: 60 });
    expect(totalFor(u, dayKey(NOW))).toBe(105);
    const old = { "2020-01-01": { "a.example": 5 } };
    expect(Object.keys(addUsage(old, "b.example", 1, NOW))).toEqual([dayKey(NOW)]);
  });

  it("normalises sites and matches subdomains", () => {
    expect(siteOf("www.YouTube.com")).toBe("youtube.com");
    expect(siteOf("m.facebook.com")).toBe("facebook.com");
    expect(matchesSite("music.youtube.com", "youtube.com")).toBe(true);
    expect(matchesSite("notyoutube.com", "youtube.com")).toBe(false);
    expect(matchesSite("a.example", " ")).toBe(false);
  });

  it("formats durations", () => {
    expect(formatDuration(42)).toBe("42 s");
    expect(formatDuration(600)).toBe("10 min");
    expect(formatDuration(3720)).toBe("1 h 2 min");
  });
});

describe("focus mode", () => {
  const cfg: FocusConfig = { blocked: ["reddit.com"], limits: { "youtube.com": 30 }, session: { startedAt: NOW - 1000, until: NOW + 60_000 } };

  it("blocks listed sites only while a session runs", () => {
    expect(blockReason(cfg, {}, "www.reddit.com", NOW)).toBe("focus");
    expect(blockReason(cfg, {}, "old.reddit.com", NOW)).toBe("focus");
    expect(blockReason(cfg, {}, "github.com", NOW)).toBeNull();
    expect(blockReason(cfg, {}, "reddit.com", NOW + 120_000)).toBeNull();
  });

  it("blocks a site once its daily limit is used up", () => {
    const used = addUsage({}, "youtube.com", 29 * 60, NOW);
    expect(blockReason(cfg, used, "www.youtube.com", NOW)).toBeNull();
    const over = addUsage(used, "youtube.com", 60, NOW);
    expect(blockReason(cfg, over, "www.youtube.com", NOW)).toBe("limit");
    expect(blockReason(cfg, over, "music.youtube.com", NOW)).toBe("limit");
  });
});
