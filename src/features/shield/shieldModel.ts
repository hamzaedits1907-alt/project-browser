// Pure helpers for Project Shield's UI: no React and no IPC, so they are unit-tested.

import type { ShieldList, ShieldUpdateSummary } from "../../lib/types";

/** The number on the toolbar button: nothing for zero, "99+" for large counts. */
export function badgeText(blocked: number): string {
  if (!Number.isFinite(blocked) || blocked <= 0) return "";
  return blocked > 99 ? "99+" : String(Math.floor(blocked));
}

export function formatCount(n: number): string {
  return Math.max(0, Math.floor(n)).toLocaleString("en-US");
}

/** One line per list: what is stored and how old it is. */
export function listStatus(list: ShieldList, now: number): { text: string; tone: "good" | "warn" | "bad" | "neutral" } {
  if (!list.enabled) return { text: "Off", tone: "neutral" };
  switch (list.state) {
    case "updating":
      return { text: "Updating…", tone: "neutral" };
    case "missing":
      return { text: list.error ? `Not downloaded yet. ${list.error}` : "Not downloaded yet", tone: list.error ? "bad" : "warn" };
    case "failed":
      return { text: `Last update failed: ${list.error ?? "unknown reason"}${list.rules > 0 ? " The earlier copy is still in use." : ""}`, tone: "warn" };
    case "ready": {
      const age = list.fetchedAt ? ageText(now - list.fetchedAt) : "";
      return { text: `${formatCount(list.rules)} rules${age ? `, checked ${age}` : ""}`, tone: "good" };
    }
  }
}

export function ageText(ms: number): string {
  if (ms < 0) return "just now";
  const minutes = Math.floor(ms / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 48) return `${hours} h ago`;
  return `${Math.floor(hours / 24)} days ago`;
}

/** What to tell the user after pressing "Update lists". */
export function updateMessage(s: ShieldUpdateSummary): string {
  if (s.alreadyRunning) return "An update is already running.";
  const parts: string[] = [];
  if (s.updated > 0) parts.push(`${s.updated} ${s.updated === 1 ? "list" : "lists"} updated`);
  if (s.unchanged > 0) parts.push(`${s.unchanged} already up to date`);
  if (s.failed > 0) parts.push(`${s.failed} could not be updated`);
  return parts.length > 0 ? `${parts.join(", ")}.` : "Nothing to update.";
}

/** Splits the custom-rules box into the lines that will be read (comments and blanks are not counted). */
export function ruleLines(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter((l) => l !== "" && !l.startsWith("!"));
}

/** Adds an exception line for a host to the custom rules, once. */
export function withException(text: string, host: string): string {
  const line = `@@||${host}^`;
  if (ruleLines(text).includes(line)) return text;
  return text.trim() === "" ? `${line}\n` : `${text.replace(/\s+$/, "")}\n${line}\n`;
}
