import type { DetectedProfile, ImportItem, ImportReport } from "../../lib/types";

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
    unsupported: profile.items.filter((i) => !i.supported),
  };
}

export interface ReportSummary {
  tone: "good" | "warn" | "bad";
  title: string;
  lines: string[];
}

function plural(n: number, one: string, many: string): string {
  return `${n.toLocaleString()} ${n === 1 ? one : many}`;
}

/** Wording for the result screen. It only ever claims what the core reports as actually written. */
export function summarizeReport(r: ImportReport): ReportSummary {
  const lines: string[] = [];
  if (r.bookmarksImported > 0) lines.push(`${plural(r.bookmarksImported, "bookmark", "bookmarks")} added under Other bookmarks`);
  if (r.historyImported > 0) lines.push(`${plural(r.historyImported, "history entry", "history entries")} added`);
  if (r.searchEngine) lines.push(`Default search engine set to ${r.searchEngine}`);
  const already = r.bookmarksSkipped + r.historySkipped;
  if (lines.length === 0) {
    if (r.errors.length > 0) return { tone: "bad", title: "Nothing was imported", lines: [] };
    if (already > 0) {
      return { tone: "good", title: "Already up to date", lines: [`${already.toLocaleString()} item${already === 1 ? " was" : "s were"} already imported earlier, so nothing was added again.`] };
    }
    return { tone: "warn", title: "Nothing new to import", lines: ["The selected data was empty."] };
  }
  if (already > 0) lines.push(`${already.toLocaleString()} already imported earlier and left as they were`);
  return { tone: r.errors.length > 0 ? "warn" : "good", title: r.errors.length > 0 ? "Imported with some problems" : "Import complete", lines };
}
