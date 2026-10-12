// Whole-window tab tools: close duplicates, sort by site, group by site, close to the right, copy all links.
// The planning is pure (tested); `runTabTool` carries a plan out through the core's normal tab commands.

import { hostOf } from "../../lib/format";
import { api } from "../../lib/ipc";
import { GROUP_COLORS } from "../../lib/types";
import type { Tab, TabId } from "../../lib/types";
import { isInternalUrl } from "../../lib/urls";

/** Address used to compare tabs: without the fragment and a trailing slash. */
export function comparableUrl(url: string): string {
  return url.replace(/#.*$/, "").replace(/\/$/, "");
}

/** Tabs that show the same page as an earlier tab (the active and pinned copies are kept). */
export function duplicateTabs(tabs: Tab[], activeId: TabId | null): TabId[] {
  const keep = new Map<string, Tab>();
  const close: TabId[] = [];
  for (const t of tabs) {
    if (isInternalUrl(t.url) && t.url !== "browser://newtab") continue;
    const key = comparableUrl(t.url);
    const first = keep.get(key);
    if (!first) {
      keep.set(key, t);
      continue;
    }
    // Prefer to keep the active or pinned copy.
    const preferNew = t.id === activeId || (t.pinned && !first.pinned);
    if (preferNew && !first.pinned && first.id !== activeId) {
      close.push(first.id);
      keep.set(key, t);
    } else if (!t.pinned && t.id !== activeId) {
      close.push(t.id);
    }
  }
  return close;
}

/** The order that puts tabs of the same site together (pinned tabs stay first, sites alphabetical, stable inside). */
export function sortedBySite(tabs: Tab[]): TabId[] {
  const key = (t: Tab) => hostOf(t.url).toLowerCase();
  const last = (t: Tab) => (isInternalUrl(t.url) ? 1 : 0);
  const pinned = tabs.filter((t) => t.pinned);
  const rest = tabs
    .filter((t) => !t.pinned)
    .map((t, i) => ({ t, i }))
    .sort((a, b) => last(a.t) - last(b.t) || key(a.t).localeCompare(key(b.t)) || a.i - b.i)
    .map((x) => x.t);
  return [...pinned, ...rest].map((t) => t.id);
}

/** Sites with at least two ungrouped, unpinned web tabs, and those tabs. */
export function siteGroups(tabs: Tab[]): { site: string; tabs: TabId[] }[] {
  const by = new Map<string, TabId[]>();
  for (const t of tabs) {
    if (t.pinned || t.group !== null || isInternalUrl(t.url)) continue;
    const site = hostOf(t.url);
    by.set(site, [...(by.get(site) ?? []), t.id]);
  }
  return [...by.entries()].filter(([, ids]) => ids.length >= 2).map(([site, ids]) => ({ site, tabs: ids }));
}

export function tabsToTheRight(tabs: Tab[], id: TabId): TabId[] {
  const i = tabs.findIndex((t) => t.id === id);
  return i < 0 ? [] : tabs.slice(i + 1).filter((t) => !t.pinned).map((t) => t.id);
}

/** All tab links as Markdown list lines (browser pages left out). */
export function linksAsMarkdown(tabs: Tab[]): string {
  return tabs
    .filter((t) => !isInternalUrl(t.url))
    .map((t) => `- [${(t.title || t.url).replace(/[[\]]/g, "")}](${t.url})`)
    .join("\n");
}

export async function closeDuplicates(tabs: Tab[], activeId: TabId | null): Promise<number> {
  const ids = duplicateTabs(tabs, activeId);
  for (const id of ids) await api.tabClose(id);
  return ids.length;
}

export async function sortBySite(tabs: Tab[]): Promise<void> {
  const order = sortedBySite(tabs);
  for (let i = 0; i < order.length; i++) await api.tabMove(order[i] as TabId, i);
}

export async function groupBySite(tabs: Tab[]): Promise<number> {
  const plan = siteGroups(tabs);
  for (const [i, g] of plan.entries()) {
    await api.tabGroup({ op: "create", tabs: g.tabs, title: g.site, color: GROUP_COLORS[i % GROUP_COLORS.length] as string });
  }
  return plan.length;
}

export async function closeToTheRight(tabs: Tab[], id: TabId): Promise<number> {
  const ids = tabsToTheRight(tabs, id);
  for (const t of ids) await api.tabClose(t);
  return ids.length;
}
