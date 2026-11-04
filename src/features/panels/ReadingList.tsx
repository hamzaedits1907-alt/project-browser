// Reading List, like Chrome's: pages saved to read later, unread first, in a side panel. Items are ordinary bookmarks
// in an "Reading list" folder under Other bookmarks, so they are backed up, imported and exported with bookmarks and
// never lost; whether an item has been read is a display state kept in local storage.

import { useEffect, useSyncExternalStore } from "react";

import { Favicon } from "../../components/ui/Favicon";
import { Button, IconButton } from "../../components/ui/controls";
import { EmptyState } from "../../components/ui/Section";
import { OTHER_ID } from "../../lib/bookmarks";
import { hostOf } from "../../lib/format";
import { api } from "../../lib/ipc";
import { readLocal, writeLocal } from "../../lib/storage";
import { activeTab, attempt, toast, useStore } from "../../lib/store";
import type { Bookmark } from "../../lib/types";
import { useLoad } from "../../lib/useLoad";

export const READING_FOLDER = "Reading list";

async function folderId(create: boolean): Promise<number | null> {
  const all = await api.bookmarksAll();
  const found = all.find((b) => b.kind === "folder" && b.parentId === OTHER_ID && b.title === READING_FOLDER);
  if (found) return found.id;
  return create ? api.bookmarkCreate(OTHER_ID, READING_FOLDER) : null;
}

let readIds: number[] = readLocal<number[]>("reading.read", []);
const listeners = new Set<() => void>();
function setRead(id: number, read: boolean) {
  readIds = read ? [...new Set([...readIds, id])] : readIds.filter((r) => r !== id);
  writeLocal("reading.read", readIds);
  listeners.forEach((l) => l());
}
function useRead(): number[] {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => readIds,
  );
}

const changed = new Set<() => void>();

/** Saves a page to the Reading List (from the panel, the right-click menu or the Command Center). */
export async function addToReadingList(url: string, title: string): Promise<void> {
  if (!/^https?:\/\//i.test(url)) {
    toast("Only web pages can be saved to the Reading List.");
    return;
  }
  const ok = await attempt(async () => {
    const parent = await folderId(true);
    if (parent === null) return false;
    const all = await api.bookmarksAll();
    if (all.some((b) => b.parentId === parent && b.url === url)) {
      toast("Already in your Reading List.");
      return false;
    }
    await api.bookmarkCreate(parent, (title || url).slice(0, 200), url);
    return true;
  }, "Could not save to the Reading List");
  if (ok) {
    toast("Saved to your Reading List");
    changed.forEach((f) => f());
  }
}

export function ReadingListPanel() {
  const tab = useStore(activeTab);
  const read = useRead();
  const list = useLoad(async () => {
    const parent = await folderId(false);
    if (parent === null) return [] as Bookmark[];
    return (await api.bookmarksAll()).filter((b) => b.parentId === parent && b.kind === "url").sort((a, b) => b.createdAt - a.createdAt);
  }, "reading");
  const reload = list.reload;
  useEffect(() => {
    changed.add(reload);
    return () => {
      changed.delete(reload);
    };
  }, [reload]);

  const items = list.data ?? [];
  const unread = items.filter((b) => !read.includes(b.id));
  const done = items.filter((b) => read.includes(b.id));
  const web = !!tab && /^https?:\/\//i.test(tab.url);

  const open = (b: Bookmark) => {
    setRead(b.id, true);
    if (b.url) void attempt(() => api.navigate(b.url as string));
  };
  const remove = (b: Bookmark) => void attempt(() => api.bookmarkDelete(b.id)).then(() => list.reload());

  const row = (b: Bookmark) => (
    <div key={b.id} className="panel-row">
      <button type="button" className="panel-link" title={b.url ?? ""} onClick={() => open(b)}>
        <Favicon src={null} fallback={(hostOf(b.url ?? "").charAt(0) || "•").toUpperCase()} seed={hostOf(b.url ?? "")} size={16} />
        <span className="panel-text">
          <span className="truncate">{b.title || b.url}</span>
          <span className="panel-sub truncate">{hostOf(b.url ?? "")}</span>
        </span>
      </button>
      <IconButton icon={read.includes(b.id) ? "eyeOff" : "check"} label={read.includes(b.id) ? "Mark as unread" : "Mark as read"} size={13} onClick={() => setRead(b.id, !read.includes(b.id))} />
      <IconButton icon="close" label="Remove from Reading List" size={13} className="panel-del" onClick={() => remove(b)} />
    </div>
  );

  return (
    <div className="panel-body">
      <div className="reading-tools">
        <Button size="sm" icon="plus" disabled={!web} onClick={() => tab && void addToReadingList(tab.url, tab.title)}>
          Add this page
        </Button>
      </div>
      {list.data && items.length === 0 && (
        <EmptyState icon="book" title="Nothing saved yet">
          Save a page to read later with “Add this page”, or right-click a link and choose “Add link to Reading List”.
        </EmptyState>
      )}
      {unread.length > 0 && <h3 className="reading-heading">Unread</h3>}
      {unread.map(row)}
      {done.length > 0 && <h3 className="reading-heading">Read</h3>}
      {done.map(row)}
    </div>
  );
}
