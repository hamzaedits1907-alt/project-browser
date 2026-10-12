import { describe, expect, it } from "vitest";

import type { DetectedProfile, ImportChoices, ImportReport, PasswordImportReport } from "../../lib/types";
import { buildChecklist, chosenKinds, importPlan, onlyChoice, recordReport } from "./importPlan";
import type { KindReports } from "./importPlan";

const profile: DetectedProfile = {
  browser: "chrome",
  browserLabel: "Google Chrome",
  profileDir: "Default",
  name: "Person 1",
  items: [
    { kind: "bookmarks", label: "Bookmarks", supported: true, present: true, detail: "10 bookmarks found" },
    { kind: "history", label: "Browsing history", supported: true, present: true, detail: "History found" },
    { kind: "searchEngines", label: "Default search engine", supported: true, present: true, detail: "Default: DuckDuckGo" },
    { kind: "passwords", label: "Saved passwords", supported: true, present: true, detail: "24 saved logins found." },
    { kind: "autofill", label: "Saved addresses", supported: true, present: true, detail: "1 saved address found." },
    { kind: "session", label: "Open tabs", supported: true, present: true, detail: "The tabs Chrome had open." },
    { kind: "cookies", label: "Cookies and active logins", supported: false, present: true, detail: "Chrome encrypts these for itself." },
  ],
};
const plan = importPlan(profile);

const none: ImportReport = {
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
const all: ImportChoices = { bookmarks: true, history: true, searchEngine: true, addresses: true, session: true };
const pw = (over: Partial<PasswordImportReport> = {}): PasswordImportReport => ({
  rows: 24,
  imported: 0,
  updated: 0,
  duplicates: 0,
  duplicatesInFile: 0,
  skippedAppLogins: 0,
  skippedInvalid: 0,
  skippedEmpty: 0,
  fileErased: true,
  fileEraseError: null,
  errors: [],
  ...over,
});

function check(report: ImportReport, chosen: ImportChoices = all, passwords: PasswordImportReport | null = null) {
  const reports: KindReports = recordReport({}, chosenKinds(chosen), report);
  const rows = buildChecklist({ plan, chosen, reports, passwords, browserLabel: "Google Chrome" });
  return Object.fromEntries(rows.map((r) => [r.key, r]));
}

describe("the migration checklist", () => {
  it("marks what was added as imported, with counts", () => {
    const rows = check({ ...none, bookmarksImported: 7, bookmarksSkipped: 3, historyImported: 50, historyVisitsImported: 200, searchEngine: "DuckDuckGo", addressesImported: 1, sessionTabs: 4, sessionTabsSaved: 4 });
    expect(rows.bookmarks).toMatchObject({ status: "imported", text: "7 bookmarks added, 3 already there" });
    expect(rows.history!.status).toBe("imported");
    expect(rows.history!.text).toContain("200 visits with their dates");
    expect(rows.searchEngine).toMatchObject({ status: "imported", text: "Default set to DuckDuckGo" });
    expect(rows.addresses!.status).toBe("imported");
    expect(rows.session!.status).toBe("imported");
  });

  it("says 'already imported' for a repeat run instead of pretending something was added", () => {
    const rows = check({ ...none, bookmarksSkipped: 10, historySkipped: 500, addressesSkipped: 1, sessionTabs: 3, sessionTabsSaved: 0 });
    for (const key of ["bookmarks", "history", "addresses", "session"]) expect(rows[key]!.status, key).toBe("already");
  });

  it("asks for the old browser to be closed when no open tabs could be read, and offers to repeat only that item", () => {
    const rows = check(none);
    expect(rows.session).toMatchObject({ status: "needsBrowserClosed", retry: "session" });
    expect(rows.session!.text).toContain("close it completely");
  });

  it("reports an item that was never selected as waiting for the user, with a retry for only that item", () => {
    const rows = check({ ...none, bookmarksImported: 1 }, { ...all, history: false, session: false });
    expect(rows.history).toMatchObject({ status: "needsAction" });
    // History was not part of the run, so it has no report and is not shown as done.
    const reports = recordReport({}, ["bookmarks"], { ...none, bookmarksImported: 1 });
    const direct = buildChecklist({ plan, chosen: { ...all, history: false }, reports, passwords: null, browserLabel: "Google Chrome" });
    expect(direct.find((r) => r.key === "history")).toMatchObject({ status: "needsAction", retry: "history" });
  });

  it("shows a failed item as failed with a retry, not as nothing found", () => {
    const rows = check({ ...none, bookmarksImported: 2, errors: ["The history file is locked."] });
    expect(rows.history).toMatchObject({ status: "failed", retry: "history" });
  });

  it("says nothing was found when a kind was empty and nothing went wrong", () => {
    const rows = check({ ...none, sessionTabs: 1, sessionTabsSaved: 1 });
    expect(rows.bookmarks!.status).toBe("notFound");
  });

  it("keeps passwords a separate step the user performs, and never claims them before they are imported", () => {
    expect(check(none).passwords).toMatchObject({ status: "needsAction", action: "passwords" });
    expect(check(none, all, pw({ imported: 20, updated: 2 })).passwords).toMatchObject({ status: "imported" });
    expect(check(none, all, pw({ imported: 20 })).passwords!.text).toContain("deleted");
    expect(check(none, all, pw({ imported: 20, fileErased: false })).passwords!.text).toContain("still exists");
    expect(check(none, all, pw({ duplicates: 24 })).passwords!.status).toBe("already");
    expect(check(none, all, pw({ errors: ["Could not read the file"] })).passwords).toMatchObject({ status: "failed", action: "passwords" });
  });

  it("lists active logins (cookies) as not supported, with the core's reason", () => {
    const rows = check(none);
    expect(rows.cookies).toMatchObject({ status: "unsupported", text: "Chrome encrypts these for itself." });
  });

  it("keeps Google sign-in and other sites as the user's own steps, and promises no more than that", () => {
    const rows = check(none);
    expect(rows.google).toMatchObject({ status: "needsAction", action: "google" });
    expect(rows.google!.text).toContain("separate from Chrome Sync");
    expect(rows.sites!.text).toContain("never filled or submitted automatically");
  });

  it("repeats exactly one kind of data and replaces only that kind's result", () => {
    expect(onlyChoice("session")).toEqual({ bookmarks: false, history: false, searchEngine: false, addresses: false, session: true });
    const first = recordReport({}, chosenKinds(all), none);
    const second = recordReport(first, ["session"], { ...none, sessionTabs: 2, sessionTabsSaved: 2 });
    expect(second.session!.sessionTabsSaved).toBe(2);
    expect(second.bookmarks).toBe(first.bookmarks);
    expect(chosenKinds({ ...all, history: false })).toEqual(["bookmarks", "searchEngine", "addresses", "session"]);
  });
});
