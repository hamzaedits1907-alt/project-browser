import { describe, expect, it } from "vitest";

import type { ShieldList } from "../../lib/types";
import { ageText, badgeText, formatCount, listStatus, ruleLines, updateMessage, withException } from "./shieldModel";

function list(over: Partial<ShieldList> = {}): ShieldList {
  return {
    id: "easylist",
    name: "EasyList",
    description: "Advertisements.",
    homepage: "https://easylist.to/",
    license: "CC BY-SA 3.0",
    enabled: true,
    state: "ready",
    error: null,
    rules: 53618,
    fetchedAt: 1_000_000,
    version: "202610121017",
    ...over,
  };
}

describe("the toolbar badge", () => {
  it("shows nothing for zero, the number below 100 and 99+ above", () => {
    expect(badgeText(0)).toBe("");
    expect(badgeText(-3)).toBe("");
    expect(badgeText(NaN)).toBe("");
    expect(badgeText(1)).toBe("1");
    expect(badgeText(99)).toBe("99");
    expect(badgeText(100)).toBe("99+");
    expect(badgeText(12345)).toBe("99+");
  });
  it("formats counts with thousands separators", () => {
    expect(formatCount(1234567)).toBe("1,234,567");
    expect(formatCount(-5)).toBe("0");
  });
});

describe("list status", () => {
  const now = 1_000_000 + 3 * 60 * 60_000;
  it("describes a ready list with its rule count and age", () => {
    expect(listStatus(list(), now)).toEqual({ text: "53,618 rules, checked 3 h ago", tone: "good" });
  });
  it("says a list is off, updating, missing or failed, and keeps the old copy visible on failure", () => {
    expect(listStatus(list({ enabled: false }), now).text).toBe("Off");
    expect(listStatus(list({ state: "updating" }), now).text).toBe("Updating…");
    expect(listStatus(list({ state: "missing", rules: 0, fetchedAt: null }), now)).toEqual({ text: "Not downloaded yet", tone: "warn" });
    expect(listStatus(list({ state: "missing", rules: 0, error: "Couldn't reach the server." }), now).tone).toBe("bad");
    const failed = listStatus(list({ state: "failed", error: "The server answered with HTTP 500." }), now);
    expect(failed.tone).toBe("warn");
    expect(failed.text).toContain("HTTP 500");
    expect(failed.text).toContain("earlier copy is still in use");
    expect(listStatus(list({ state: "failed", rules: 0, error: "x" }), now).text).not.toContain("earlier copy");
  });
  it("words ages plainly", () => {
    expect(ageText(10_000)).toBe("just now");
    expect(ageText(-1)).toBe("just now");
    expect(ageText(5 * 60_000)).toBe("5 min ago");
    expect(ageText(5 * 3_600_000)).toBe("5 h ago");
    expect(ageText(72 * 3_600_000)).toBe("3 days ago");
  });
});

describe("update messages", () => {
  it("summarises the outcome", () => {
    expect(updateMessage({ updated: 2, unchanged: 0, failed: 0, alreadyRunning: false })).toBe("2 lists updated.");
    expect(updateMessage({ updated: 1, unchanged: 1, failed: 0, alreadyRunning: false })).toBe("1 list updated, 1 already up to date.");
    expect(updateMessage({ updated: 0, unchanged: 0, failed: 2, alreadyRunning: false })).toBe("2 could not be updated.");
    expect(updateMessage({ updated: 0, unchanged: 0, failed: 0, alreadyRunning: false })).toBe("Nothing to update.");
    expect(updateMessage({ updated: 0, unchanged: 0, failed: 0, alreadyRunning: true })).toBe("An update is already running.");
  });
});

describe("custom rules", () => {
  it("counts only lines that are rules", () => {
    expect(ruleLines("! my rules\n\n||a.example^\r\n  @@||b.example^  \n")).toEqual(["||a.example^", "@@||b.example^"]);
  });
  it("adds an exception once", () => {
    const once = withException("", "news.example");
    expect(once).toBe("@@||news.example^\n");
    expect(withException(once, "news.example")).toBe(once);
    expect(withException("||x.example^\n\n", "news.example")).toBe("||x.example^\n@@||news.example^\n");
  });
});
