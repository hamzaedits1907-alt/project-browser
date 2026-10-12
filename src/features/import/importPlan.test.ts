import { describe, expect, it } from "vitest";

import type { DetectedProfile, ImportReport, PasswordImportReport } from "../../lib/types";
import { defaultChoices, effectiveChoices, hasAnyChoice, importPlan, profileKey, summarizePasswordReport, summarizeReport } from "./importPlan";

const profile: DetectedProfile = {
  browser: "chrome",
  browserLabel: "Google Chrome",
  profileDir: "Default",
  name: "Person 1",
  items: [
    { kind: "bookmarks", label: "Bookmarks", supported: true, present: true, detail: "10 bookmarks found" },
    { kind: "history", label: "Browsing history", supported: true, present: false, detail: "No history file" },
    { kind: "searchEngines", label: "Default search engine", supported: true, present: true, detail: "Default: DuckDuckGo" },
    { kind: "passwords", label: "Saved passwords", supported: true, present: true, detail: "24 saved logins found." },
    { kind: "autofill", label: "Saved addresses", supported: true, present: true, detail: "1 saved address found." },
    { kind: "session", label: "Open tabs", supported: true, present: true, detail: "The tabs Chrome had open." },
    { kind: "cookies", label: "Cookies", supported: false, present: true, detail: "Not imported." },
    { kind: "cards", label: "Payment cards", supported: false, present: false, detail: "Never imported." },
  ],
};

describe("import plan", () => {
  it("only offers data that is both supported and present", () => {
    const plan = importPlan(profile);
    expect(plan.bookmarks.available).toBe(true);
    expect(plan.history.available).toBe(false);
    expect(plan.searchEngine.available).toBe(true);
    expect(plan.addresses.available).toBe(true);
    expect(plan.session.available).toBe(true);
  });

  it("lists what can never be imported, each with the core's own reason", () => {
    const plan = importPlan(profile);
    expect(plan.unsupported.map((u) => u.kind)).toEqual(["cookies", "cards"]);
    expect(plan.unsupported.every((u) => u.detail.length > 0)).toBe(true);
  });

  it("treats passwords as a separate, user-driven step rather than a tick box", () => {
    const plan = importPlan(profile);
    expect(plan.passwords.available).toBe(true);
    expect(Object.keys(defaultChoices(plan)).sort()).toEqual(["addresses", "bookmarks", "history", "searchEngine", "session"]);
  });

  it("ticks what is available, but leaves open tabs off because the other browser must be closed", () => {
    expect(defaultChoices(importPlan(profile))).toEqual({ bookmarks: true, history: false, searchEngine: true, addresses: true, session: false });
  });

  it("never sends a choice for data that does not exist", () => {
    const plan = importPlan(profile);
    const all = { bookmarks: true, history: true, searchEngine: true, addresses: true, session: true };
    expect(effectiveChoices(plan, all)).toEqual({ bookmarks: true, history: false, searchEngine: true, addresses: true, session: true });
    expect(hasAnyChoice(plan, { bookmarks: false, history: true, searchEngine: false, addresses: false, session: false })).toBe(false);
    expect(hasAnyChoice(plan, all)).toBe(true);
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

const base: ImportReport = {
  bookmarksImported: 0,
  bookmarksSkipped: 0,
  historyImported: 0,
  historySkipped: 0,
  historyVisitsImported: 0,
  searchEngine: null,
  sessionTabs: 0,
  sessionTabsSaved: 0,
  sessionUrls: [],
  addressesImported: 0,
  addressesSkipped: 0,
  errors: [],
};

describe("import report wording", () => {
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

  it("reports visits, open tabs and addresses, each on its own row", () => {
    const s = summarizeReport({ ...base, historyImported: 10, historyVisitsImported: 40, sessionTabs: 5, sessionTabsSaved: 3, addressesImported: 1, addressesSkipped: 1 });
    expect(s.lines.join(" ")).toContain("40 visits added with their original dates");
    expect(s.lines.join(" ")).toContain("3 open tabs saved");
    expect(s.rows.map((r) => r.key)).toEqual(["history", "tabs", "addresses"]);
    expect(s.rows.find((r) => r.key === "tabs")?.text).toContain("2 not saved");
    expect(s.rows.find((r) => r.key === "addresses")?.text).toContain("1 skipped");
  });

  it("shows duplicates as 'already there' instead of hiding them", () => {
    const s = summarizeReport({ ...base, bookmarksImported: 2, bookmarksSkipped: 3 });
    expect(s.rows[0]).toMatchObject({ key: "bookmarks", tone: "good", text: "2 bookmarks added, 3 already there" });
    const none = summarizeReport({ ...base, bookmarksSkipped: 3 });
    expect(none.rows[0]).toMatchObject({ tone: "neutral", text: "3 already there" });
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

describe("password import wording", () => {
  const pw: PasswordImportReport = {
    rows: 0,
    imported: 0,
    updated: 0,
    duplicates: 0,
    duplicatesInFile: 0,
    skippedAppLogins: 0,
    skippedInvalid: 0,
    skippedEmpty: 0,
    fileErased: false,
    fileEraseError: null,
    errors: [],
  };

  it("counts every outcome separately", () => {
    const s = summarizePasswordReport({ ...pw, rows: 30, imported: 20, updated: 2, duplicates: 4, duplicatesInFile: 1, skippedAppLogins: 2, skippedInvalid: 1, skippedEmpty: 1 });
    expect(s.title).toBe("Passwords imported");
    expect(s.tone).toBe("good");
    expect(s.rows.map((r) => r.key)).toEqual(["imported", "updated", "duplicates", "inFile", "apps", "invalid", "empty"]);
    expect(s.rows.find((r) => r.key === "apps")?.text).toContain("Android apps");
  });

  it("says a repeat import changed nothing", () => {
    const s = summarizePasswordReport({ ...pw, rows: 5, duplicates: 5 });
    expect(s.title).toBe("Already up to date");
    expect(s.rows).toHaveLength(1);
  });

  it("does not claim success for a file with nothing usable", () => {
    expect(summarizePasswordReport({ ...pw, rows: 2, skippedEmpty: 2 })).toMatchObject({ tone: "bad", title: "No passwords found in that file" });
  });

  it("warns about the plain-text export file unless it was erased", () => {
    expect(summarizePasswordReport({ ...pw, imported: 1 }).fileNote).toMatchObject({ tone: "warn" });
    expect(summarizePasswordReport({ ...pw, imported: 1 }).fileNote.text).toContain("plain text");
    expect(summarizePasswordReport({ ...pw, imported: 1, fileErased: true }).fileNote).toEqual({ tone: "good", text: "The export file was overwritten and deleted." });
    expect(summarizePasswordReport({ ...pw, imported: 1, fileEraseError: "The file couldn't be deleted (busy)." }).fileNote.text).toContain("couldn't be deleted");
  });

  it("never puts a secret in its wording: the report type has no field that could hold one", () => {
    expect(Object.keys(pw).sort()).toEqual(
      ["duplicates", "duplicatesInFile", "errors", "fileEraseError", "fileErased", "imported", "rows", "skippedAppLogins", "skippedEmpty", "skippedInvalid", "updated"],
    );
  });
});
