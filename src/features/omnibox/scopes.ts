// Address-bar scopes, like Chrome's "@tabs", "@bookmarks" and "@history": typing a scope and a space searches only that.

import type { Suggestion } from "../../lib/types";

export const SCOPES = [
  { key: "@tabs", kind: "tab", label: "Search your open tabs" },
  { key: "@bookmarks", kind: "bookmark", label: "Search your bookmarks" },
  { key: "@history", kind: "history", label: "Search your history" },
] as const;

export type ScopeKind = (typeof SCOPES)[number]["kind"];

/** The scope a typed text starts with ("@tabs news" -> tabs, "news"), or null. Pure. */
export function parseScope(text: string): { kind: ScopeKind; key: string; query: string } | null {
  const m = /^(@\w+)\s+(.*)$/s.exec(text);
  if (!m) return null;
  const scope = SCOPES.find((s) => s.key === m[1]?.toLowerCase());
  return scope ? { kind: scope.kind, key: scope.key, query: (m[2] ?? "").trim() } : null;
}

/** While a scope's name is being typed ("@ta"), the scopes it could be, as suggestions that fill it in. Pure. */
export function scopeHints(text: string): Suggestion[] {
  const t = text.trim().toLowerCase();
  if (!t.startsWith("@") || /\s/.test(text.trim())) return [];
  return SCOPES.filter((s) => s.key.startsWith(t)).map((s) => ({ kind: "url", title: s.label, url: `${s.key} `, tabId: null }));
}

/** Only the suggestions of a scope's kind. Pure. */
export function inScope(list: Suggestion[], kind: ScopeKind): Suggestion[] {
  return list.filter((s) => s.kind === kind);
}
