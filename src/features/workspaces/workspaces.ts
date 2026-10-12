// Workspaces: a named set of tabs saved for later (a research topic, a game night, a work project). A workspace is a
// bookmark folder inside "Other bookmarks/Workspaces", so it is stored by the core like any bookmark, shows up in
// bookmark exports and survives reinstalling. Opening one adds its pages as a tab group named after it.

import { OTHER_ID, childrenOf } from "../../lib/bookmarks";
import { api } from "../../lib/ipc";
import { getState } from "../../lib/store";
import type { Bookmark, Tab } from "../../lib/types";
import { isInternalUrl } from "../../lib/urls";

export const ROOT_TITLE = "Workspaces";

export interface Workspace {
  id: number;
  name: string;
  urls: { title: string; url: string }[];
  createdAt: number;
}

export function workspaceRoot(all: Bookmark[]): Bookmark | null {
  return childrenOf(all, OTHER_ID).find((b) => b.kind === "folder" && b.title === ROOT_TITLE) ?? null;
}

export function listWorkspaces(all: Bookmark[]): Workspace[] {
  const root = workspaceRoot(all);
  if (!root) return [];
  return childrenOf(all, root.id)
    .filter((f) => f.kind === "folder")
    .map((f) => ({
      id: f.id,
      name: f.title,
      createdAt: f.createdAt,
      urls: childrenOf(all, f.id)
        .filter((b) => b.kind === "url" && b.url)
        .map((b) => ({ title: b.title, url: b.url as string })),
    }));
}

/** The pages of a window worth saving: web pages only, private tabs never. */
export function savableTabs(tabs: Tab[]): { title: string; url: string }[] {
  return tabs.filter((t) => !t.private && !isInternalUrl(t.url)).map((t) => ({ title: t.title || t.url, url: t.url }));
}

export async function saveWorkspace(name: string, tabs: Tab[]): Promise<number> {
  const pages = savableTabs(tabs);
  if (pages.length === 0) throw new Error("There are no web pages in this window to save.");
  const all = await api.bookmarksAll();
  const root = workspaceRoot(all)?.id ?? (await api.bookmarkCreate(OTHER_ID, ROOT_TITLE));
  const existing = childrenOf(all, root).find((f) => f.kind === "folder" && f.title === name);
  if (existing) await api.bookmarkDelete(existing.id);
  const folder = await api.bookmarkCreate(root, name.trim() || "Workspace");
  for (const p of pages) await api.bookmarkCreate(folder, p.title, p.url);
  return pages.length;
}

export async function openWorkspace(ws: Workspace): Promise<void> {
  const before = new Set(getState().browser.tabs.map((t) => t.id));
  for (const p of ws.urls) await api.tabNew(p.url, false, true);
  // The new tabs arrive with the next state update; give it a moment, then group whatever is new.
  await new Promise((r) => setTimeout(r, 400));
  const added = getState()
    .browser.tabs.filter((t) => !before.has(t.id))
    .map((t) => t.id);
  if (added.length > 1) await api.tabGroup({ op: "create", tabs: added, title: ws.name, color: "cyan" });
  if (added[0] !== undefined) await api.tabActivate(added[0]);
}

export async function deleteWorkspace(ws: Workspace): Promise<void> {
  await api.bookmarkDelete(ws.id);
}
