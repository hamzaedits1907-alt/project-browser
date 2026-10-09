import { describe, expect, it } from "vitest";

import type { DetectedProfile, ImportReport } from "../../lib/types";
import { importPlan, profileKey, summarizeReport } from "./importPlan";

const profile: DetectedProfile = {
  browser: "chrome",
  browserLabel: "Google Chrome",
  profileDir: "Default",
  name: "Person 1",
  items: [
    { kind: "bookmarks", label: "Bookmarks", supported: true, present: true, detail: "10 bookmarks found" },
    { kind: "history", label: "Browsing history", supported: true, present: false, detail: "No history file" },
    { kind: "searchEngines", label: "Default search engine", supported: true, present: true, detail: "Default: DuckDuckGo" },
    { kind: "passwords", label: "Saved passwords", supported: false, present: true, detail: "Not imported." },
    { kind: "autofill", label: "Autofill", supported: false, present: true, detail: "Not imported." },
    { kind: "cookies", label: "Cookies", supported: false, present: true, detail: "Not imported." },
  ],
};

describe("import plan", () => {
  it("only offers data that is both supported and present", () => {
    const plan = importPlan(profile);
    expect(plan.bookmarks.available).toBe(true);
    expect(plan.history.available).toBe(false);
    expect(plan.searchEngine.available).toBe(true);
  });
  it("never offers passwords, autofill or cookies for import", () => {
    const plan = importPlan(profile);
    expect(plan.unsupported.map((u) => u.kind)).toEqual(["passwords", "autofill", "cookies"]);
    expect(Object.keys(plan).sort()).toEqual(["bookmarks", "history", "searchEngine", "unsupported"]);
  });
  it("does not offer a search engine the profile does not have", () => {
    const none = { ...profile, items: profile.items.map((i) => (i.kind === "searchEngines" ? { ...i, present: false } : i)) };
    expect(importPlan(none).searchEngine.available).toBe(false);
  });
  it("keys profiles uniquely per browser", () => {
    expect(profileKey(profile)).toBe("chrome/Default");
    expect(profileKey({ ...profile, browser: "edge" })).toBe("edge/Default");
  });
});

describe("import report wording", () => {
  const base: ImportReport = { bookmarksImported: 0, bookmarksSkipped: 0, historyImported: 0, historySkipped: 0, searchEngine: null, errors: [] };
  it("summarises successes", () => {
    const s = summarizeReport({ ...base, bookmarksImported: 1, historyImported: 2000 });
    expect(s.tone).toBe("good");
    expect(s.lines.join(" ")).toContain("1 bookmark ");
    expect(s.lines.join(" ")).toContain("history entries");
  });
  it("mentions an adopted search engine and previously imported items", () => {
    const s = summarizeReport({ ...base, bookmarksImported: 2, bookmarksSkipped: 5, searchEngine: "DuckDuckGo" });
    expect(s.lines.join(" ")).toContain("Default search engine set to DuckDuckGo");
    expect(s.lines.join(" ")).toContain("5 already imported earlier");
  });
  it("is honest about partial and failed imports", () => {
    expect(summarizeReport({ ...base, bookmarksImported: 3, errors: ["History: locked"] }).tone).toBe("warn");
    expect(summarizeReport({ ...base, errors: ["Bookmarks: x"] }).tone).toBe("bad");
    expect(summarizeReport(base).title).toBe("Nothing new to import");
  });
  it("a repeat import says it was already up to date instead of claiming new data", () => {
    const s = summarizeReport({ ...base, bookmarksSkipped: 3, historySkipped: 10 });
    expect(s.title).toBe("Already up to date");
    expect(s.lines.join(" ")).toContain("13 items were already imported");
  });
  it("never reports success when errors left nothing imported, even if items were skipped", () => {
    expect(summarizeReport({ ...base, historySkipped: 4, errors: ["Bookmarks: unreadable"] }).tone).toBe("bad");
  });
});
