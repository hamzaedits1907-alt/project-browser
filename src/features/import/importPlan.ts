import type { DetectedProfile, ImportChoices, ImportItem, ImportReport, PasswordImportReport } from "../../lib/types";

export function profileKey(p: Pick<DetectedProfile, "browser" | "profileDir">): string {
  return `${p.browser}/${p.profileDir}`;
}

export interface ImportOption {
  available: boolean;
  detail: string;
}

export interface ImportPlan {
  bookmarks: ImportOption;
  history: ImportOption;
  searchEngine: ImportOption;
  addresses: ImportOption;
  session: ImportOption;
  /** Passwords are never read from the other browser: the user exports them and picks the file (see PasswordImport). */
  passwords: ImportOption;
  /** Everything the core reports as unsupported, with its own explanation. */
  unsupported: ImportItem[];
}

function option(items: ImportItem[], kind: string): ImportOption {
  const item = items.find((i) => i.kind === kind);
  if (!item) return { available: false, detail: "Not available" };
  return { available: item.supported && item.present, detail: item.detail };
}

/** Turns the core's per-profile item list into what the wizard can offer. */
export function importPlan(profile: DetectedProfile): ImportPlan {
  return {
    bookmarks: option(profile.items, "bookmarks"),
    history: option(profile.items, "history"),
    searchEngine: option(profile.items, "searchEngines"),
    addresses: option(profile.items, "autofill"),
    session: option(profile.items, "session"),
    passwords: option(profile.items, "passwords"),
    unsupported: profile.items.filter((i) => !i.supported),
  };
}

/** Everything available is ticked, except open tabs: reading them needs the other browser closed. */
export function defaultChoices(plan: ImportPlan): ImportChoices {
  return {
    bookmarks: plan.bookmarks.available,
    history: plan.history.available,
    searchEngine: plan.searchEngine.available,
    addresses: plan.addresses.available,
    session: false,
  };
}

/** What is actually sent to the core: a choice counts only if the data exists. */
export function effectiveChoices(plan: ImportPlan, choices: ImportChoices): ImportChoices {
  return {
    bookmarks: choices.bookmarks && plan.bookmarks.available,
    history: choices.history && plan.history.available,
    searchEngine: choices.searchEngine && plan.searchEngine.available,
    addresses: choices.addresses && plan.addresses.available,
    session: choices.session && plan.session.available,
  };
}

export function hasAnyChoice(plan: ImportPlan, choices: ImportChoices): boolean {
  return Object.values(effectiveChoices(plan, choices)).some(Boolean);
}

export type Tone = "good" | "warn" | "bad" | "neutral";

export interface ReportRow {
  key: string;
  label: string;
  tone: Tone;
  text: string;
}

export interface ReportSummary {
  tone: "good" | "warn" | "bad";
  title: string;
  lines: string[];
  /** One row per kind of data that had something to report: what was added and what was already there. */
  rows: ReportRow[];
}

function plural(n: number, one: string, many: string): string {
  return `${n.toLocaleString()} ${n === 1 ? one : many}`;
}

function counts(added: number, already: number, noun: [string, string]): { tone: Tone; text: string } {
  const parts: string[] = [];
  if (added > 0) parts.push(`${plural(added, noun[0], noun[1])} added`);
  if (already > 0) parts.push(`${already.toLocaleString()} already there`);
  return { tone: added > 0 ? "good" : "neutral", text: parts.join(", ") };
}

/** Wording for the result screen. It only ever claims what the core reports as actually written. */
export function summarizeReport(r: ImportReport): ReportSummary {
  const lines: string[] = [];
  const rows: ReportRow[] = [];
  if (r.bookmarksImported > 0) lines.push(`${plural(r.bookmarksImported, "bookmark", "bookmarks")} added under Other bookmarks`);
  if (r.historyImported > 0) lines.push(`${plural(r.historyImported, "history entry", "history entries")} added`);
  if (r.historyVisitsImported > 0) lines.push(`${plural(r.historyVisitsImported, "visit", "visits")} added with their original dates`);
  if (r.searchEngine) lines.push(`Default search engine set to ${r.searchEngine}`);
  if (r.sessionTabsSaved > 0) lines.push(`${plural(r.sessionTabsSaved, "open tab", "open tabs")} saved in the “Open tabs” bookmark folder`);
  if (r.addressesImported > 0) lines.push(`${plural(r.addressesImported, "address", "addresses")} saved (encrypted)`);

  if (r.bookmarksImported + r.bookmarksSkipped > 0) rows.push({ key: "bookmarks", label: "Bookmarks", ...counts(r.bookmarksImported, r.bookmarksSkipped, ["bookmark", "bookmarks"]) });
  if (r.historyImported + r.historySkipped + r.historyVisitsImported > 0) {
    const c = counts(r.historyImported, r.historySkipped, ["page", "pages"]);
    const visits = r.historyVisitsImported > 0 ? `${c.text ? `${c.text}; ` : ""}${plural(r.historyVisitsImported, "visit", "visits")} with dates` : c.text;
    rows.push({ key: "history", label: "History", tone: r.historyImported + r.historyVisitsImported > 0 ? "good" : "neutral", text: visits });
  }
  if (r.searchEngine) rows.push({ key: "search", label: "Search engine", tone: "good", text: r.searchEngine });
  if (r.sessionTabs > 0) {
    const skipped = r.sessionTabs - r.sessionTabsSaved;
    rows.push({
      key: "tabs",
      label: "Open tabs",
      tone: r.sessionTabsSaved > 0 ? "good" : "neutral",
      text: `${plural(r.sessionTabs, "tab", "tabs")} found${r.sessionTabsSaved > 0 ? `, ${r.sessionTabsSaved.toLocaleString()} saved` : ""}${skipped > 0 ? `, ${skipped.toLocaleString()} not saved (already there or not web pages)` : ""}`,
    });
  }
  if (r.addressesImported + r.addressesSkipped > 0) {
    const c = counts(r.addressesImported, 0, ["address", "addresses"]);
    rows.push({ key: "addresses", label: "Addresses", tone: c.tone, text: [c.text, r.addressesSkipped > 0 ? `${r.addressesSkipped} skipped (already saved or empty)` : ""].filter(Boolean).join(", ") });
  }

  const already = r.bookmarksSkipped + r.historySkipped;
  if (lines.length === 0) {
    if (r.errors.length > 0) return { tone: "bad", title: "Nothing was imported", lines: [], rows };
    if (already > 0) {
      return { tone: "good", title: "Already up to date", lines: [`${already.toLocaleString()} item${already === 1 ? " was" : "s were"} already imported earlier, so nothing was added again.`], rows };
    }
    return { tone: "warn", title: "Nothing new to import", lines: ["The selected data was empty."], rows };
  }
  if (already > 0) lines.push(`${already.toLocaleString()} already imported earlier and left as they were`);
  return { tone: r.errors.length > 0 ? "warn" : "good", title: r.errors.length > 0 ? "Imported with some problems" : "Import complete", lines, rows };
}

export interface PasswordSummary {
  tone: "good" | "warn" | "bad";
  title: string;
  rows: ReportRow[];
  /** What happened to the export file, and what to do about it. */
  fileNote: { tone: Tone; text: string };
}

/** Wording for the password result screen: every kind of outcome gets its own count, none is folded away. */
export function summarizePasswordReport(r: PasswordImportReport): PasswordSummary {
  const rows: ReportRow[] = [];
  const add = (key: string, label: string, n: number, tone: Tone, text: string) => {
    if (n > 0) rows.push({ key, label, tone, text });
  };
  add("imported", "Added", r.imported, "good", plural(r.imported, "login", "logins"));
  add("updated", "Updated", r.updated, "good", `${plural(r.updated, "login", "logins")} had a different password in the file, which replaced the saved one`);
  add("duplicates", "Already saved", r.duplicates, "neutral", `${plural(r.duplicates, "login", "logins")} with the same password, left as they were`);
  add("inFile", "Repeated in the file", r.duplicatesInFile, "neutral", `${plural(r.duplicatesInFile, "entry", "entries")} for a site and user that appeared more than once`);
  add("apps", "App logins", r.skippedAppLogins, "warn", `${plural(r.skippedAppLogins, "entry", "entries")} for Android apps, not websites, so they can't be used here`);
  add("invalid", "Unreadable", r.skippedInvalid, "warn", `${plural(r.skippedInvalid, "entry", "entries")} with a missing or invalid web address`);
  add("empty", "No password", r.skippedEmpty, "warn", `${plural(r.skippedEmpty, "entry", "entries")} had no password`);

  const stored = r.imported + r.updated;
  const title = stored > 0 ? "Passwords imported" : r.duplicates > 0 ? "Already up to date" : r.errors.length > 0 ? "Nothing was imported" : "No passwords found in that file";
  const tone = stored > 0 ? (r.errors.length > 0 ? "warn" : "good") : r.duplicates > 0 ? "good" : "bad";

  const fileNote: PasswordSummary["fileNote"] = r.fileErased
    ? { tone: "good", text: "The export file was overwritten and deleted." }
    : r.fileEraseError
      ? { tone: "warn", text: r.fileEraseError }
      : { tone: "warn", text: "The export file still holds your passwords as plain text. Delete it now, including from the Recycle Bin." };
  return { tone, title, rows, fileNote };
}
