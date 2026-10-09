import { describe, expect, it } from "vitest";

import { clamp, dayLabel, downloadProgress, formatBytes, formatDuration, formatPercent, formatRelative, groupByDay, hostOf, permissionLabel, permissionName } from "./format";

describe("formatBytes", () => {
  it("scales units and avoids noise", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1023)).toBe("1023 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(150 * 1024 * 1024)).toBe("150 MB");
    expect(formatBytes(1.5 * 1024 ** 3)).toBe("1.5 GB");
  });
  it("handles missing values", () => {
    expect(formatBytes(null)).toBe("-");
    expect(formatBytes(-1)).toBe("-");
    expect(formatBytes(Number.NaN)).toBe("-");
  });
});

describe("misc formatters", () => {
  it("percent and duration", () => {
    expect(formatPercent(12.34)).toBe("12%");
    expect(formatPercent(12.34, 1)).toBe("12.3%");
    expect(formatPercent(null)).toBe("-");
    expect(formatDuration(412)).toBe("412 ms");
    expect(formatDuration(1380)).toBe("1.38 s");
    expect(formatDuration(12_500)).toBe("12.5 s");
    expect(formatDuration(null)).toBe("-");
  });
  it("progress is clamped and unknown for missing totals", () => {
    expect(downloadProgress(50, 100)).toBe(0.5);
    expect(downloadProgress(200, 100)).toBe(1);
    expect(downloadProgress(5, null)).toBeNull();
    expect(downloadProgress(5, 0)).toBeNull();
  });
  it("clamp and hostOf", () => {
    expect(clamp(5, 0, 3)).toBe(3);
    expect(clamp(-1, 0, 3)).toBe(0);
    expect(hostOf("https://www.example.com/a?b=1")).toBe("example.com");
    expect(hostOf("not a url")).toBe("not a url");
  });
});

describe("relative time and grouping", () => {
  const now = new Date(2026, 5, 15, 14, 30).getTime();
  it("describes recent times", () => {
    expect(formatRelative(now - 10_000, now)).toBe("just now");
    expect(formatRelative(now - 5 * 60_000, now)).toBe("5 min ago");
    expect(formatRelative(now - 3 * 3_600_000, now)).toBe("3 hr ago");
    expect(formatRelative(now - 26 * 3_600_000, now)).toBe("yesterday");
    expect(formatRelative(now - 5 * 86_400_000, now)).toBe("5 days ago");
  });
  it("labels days", () => {
    expect(dayLabel(now - 3_600_000, now)).toBe("Today");
    expect(dayLabel(now - 86_400_000, now)).toBe("Yesterday");
    expect(dayLabel(now - 5 * 86_400_000, now)).not.toBe("Today");
  });
  it("groups consecutive entries by day", () => {
    const items = [now, now - 1000, now - 86_400_000, now - 86_400_000 - 1000, now - 3 * 86_400_000].map((t) => ({ t }));
    const groups = groupByDay(items, (i) => i.t, now);
    expect(groups.map((g) => g.items.length)).toEqual([2, 2, 1]);
    expect(groups[0]?.label).toBe("Today");
    expect(groups[1]?.label).toBe("Yesterday");
  });
});

describe("permission wording", () => {
  it("knows the kinds the core emits and falls back gracefully", () => {
    expect(permissionName("geolocation")).toBe("Location");
    expect(permissionLabel("camera")).toBe("Use your camera");
    expect(permissionName("somethingNew")).toBe("SomethingNew");
    expect(permissionLabel("somethingNew")).toBe("Use somethingNew");
  });
});
