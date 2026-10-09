import { useState } from "react";

import { Button, Select, TextField } from "../components/ui/controls";
import { Favicon } from "../components/ui/Favicon";
import { Icon } from "../components/ui/Icon";
import { Modal } from "../components/ui/Modal";
import { EmptyState, Notice, PageHeader } from "../components/ui/Section";
import { BAR_ID, childrenOf, countUrls, folderOptions, isRoot, searchBookmarks } from "../lib/bookmarks";
import { hostOf } from "../lib/format";
import { api, errorMessage } from "../lib/ipc";
import { useLoad } from "../lib/useLoad";
import { attempt, toast, useStore } from "../lib/store";
import type { Bookmark } from "../lib/types";

interface Draft {
  id: number | null; // null = new
  kind: "url" | "folder";
  parentId: number;
  title: string;
  url: string;
}

export default function Bookmarks() {
  const rev = useStore((s) => s.bookmarksRev);
  const [folderId, setFolderId] = useState(BAR_ID);
  const [query, setQuery] = useState("");
  const [draft, setDraft] = useState<Draft | null>(null);
  const [draftError, setDraftError] = useState<string | null>(null);
  const loaded = useLoad(() => api.bookmarksAll(), rev);
  const all = loaded.data ?? [];
  const error = loaded.error;

  const folders = folderOptions(all);
  const searching = query.trim().length > 0;
  const items = searching ? searchBookmarks(all, query) : childrenOf(all, folderId);
  const current = all.find((b) => b.id === folderId);

  const save = async (): Promise<boolean> => {
    if (!draft) return false;
    setDraftError(null);
    try {
      if (draft.id === null) {
        await api.bookmarkCreate(draft.parentId, draft.title.trim() || draft.url, draft.kind === "url" ? draft.url : undefined);
      } else {
        const existing = all.find((b) => b.id === draft.id);
        await api.bookmarkUpdate(draft.id, draft.title, draft.kind === "url" ? draft.url : undefined);
        if (existing && existing.parentId !== draft.parentId) {
          await api.bookmarkMove(draft.id, draft.parentId, childrenOf(all, draft.parentId).length);
        }
      }
      setDraft(null);
      return true;
    } catch (e) {
      setDraftError(errorMessage(e));
      return false;
    }
  };

  const remove = async (b: Bookmark) => {
    const n = b.kind === "folder" ? countUrls(all, b.id) : 0;
    if (b.kind === "folder" && n > 0 && !window.confirm(`Delete “${b.title}” and the ${n} bookmark${n === 1 ? "" : "s"} inside it?`)) return;
    if (b.id === folderId) setFolderId(b.parentId ?? BAR_ID);
    await attempt(() => api.bookmarkDelete(b.id), "Could not delete");
  };

  const move = async (b: Bookmark, delta: number) => {
    const siblings = childrenOf(all, b.parentId ?? BAR_ID);
    const index = siblings.findIndex((x) => x.id === b.id);
    const target = index + delta;
    if (target < 0 || target >= siblings.length) return;
    await attempt(() => api.bookmarkMove(b.id, b.parentId ?? BAR_ID, target), "Could not move");
  };

  const newDraft = (kind: "url" | "folder"): Draft => ({ id: null, kind, parentId: searching ? BAR_ID : folderId, title: "", url: "" });

  return (
    <div className="page">
      <PageHeader
        icon="bookmarks"
        title="Bookmarks"
        actions={
          <>
            <Button icon="plus" onClick={() => setDraft(newDraft("url"))}>Add bookmark</Button>
            <Button icon="folder" onClick={() => setDraft(newDraft("folder"))}>New folder</Button>
          </>
        }
      />
      {error && <Notice tone="bad">{error}</Notice>}
      <div className="bm-layout">
        <nav className="bm-folders" aria-label="Bookmark folders">
          {folders.map((f) => (
            <button key={f.id} type="button" className={`nav-item${!searching && f.id === folderId ? " is-active" : ""}`} style={{ paddingLeft: 10 + f.depth * 16 }} onClick={() => { setQuery(""); setFolderId(f.id); }}>
              <Icon name="folder" size={16} />
              <span className="truncate">{f.label}</span>
            </button>
          ))}
        </nav>
        <div className="bm-main">
          <label className="search-field">
            <Icon name="search" size={16} />
            <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search all bookmarks" aria-label="Search bookmarks" spellCheck={false} />
          </label>
          <h2 className="day-label">{searching ? `Results for “${query.trim()}”` : current?.title ?? "Bookmarks"}</h2>
          {items.length === 0 ? (
            <EmptyState icon="bookmarks" title={searching ? "No matching bookmarks" : "This folder is empty"}>
              {searching ? "Try a different search." : "Press Ctrl+D on any page to bookmark it, or add one here."}
            </EmptyState>
          ) : (
            <div className="list">
              {items.map((b, i) => (
                <div key={b.id} className="list-row">
                  {b.kind === "folder" ? <Icon name="folder" size={18} /> : <Favicon src={null} fallback={(hostOf(b.url ?? "").charAt(0) || "•").toUpperCase()} size={18} />}
                  {b.kind === "folder" ? (
                    <button type="button" className="row-link truncate" onClick={() => setFolderId(b.id)}>{b.title}</button>
                  ) : (
                    <button type="button" className="row-link truncate" title={b.url ?? ""} onClick={() => b.url && void attempt(() => api.navigate(b.url ?? ""))}>
                      {b.title || b.url}
                    </button>
                  )}
                  <span className="row-host truncate">{b.kind === "folder" ? `${countUrls(all, b.id)} bookmark${countUrls(all, b.id) === 1 ? "" : "s"}` : hostOf(b.url ?? "")}</span>
                  <span className="row-actions-inline">
                    {!searching && <button type="button" className="icon-btn" aria-label="Move up" disabled={i === 0} onClick={() => void move(b, -1)}><Icon name="chevronUp" size={15} /></button>}
                    {!searching && <button type="button" className="icon-btn" aria-label="Move down" disabled={i === items.length - 1} onClick={() => void move(b, 1)}><Icon name="chevronDown" size={15} /></button>}
                    {!isRoot(b) && (
                      <button type="button" className="icon-btn" aria-label="Edit" title="Edit" onClick={() => setDraft({ id: b.id, kind: b.kind, parentId: b.parentId ?? BAR_ID, title: b.title, url: b.url ?? "" })}>
                        <Icon name="edit" size={15} />
                      </button>
                    )}
                    {!isRoot(b) && (
                      <button type="button" className="icon-btn" aria-label="Delete" title="Delete" onClick={() => void remove(b)}>
                        <Icon name="trash" size={15} />
                      </button>
                    )}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      <Modal
        open={!!draft}
        title={draft ? `${draft.id === null ? "Add" : "Edit"} ${draft.kind === "folder" ? "folder" : "bookmark"}` : ""}
        onClose={() => setDraft(null)}
        footer={
          <>
            <Button variant="ghost" onClick={() => setDraft(null)}>Cancel</Button>
            <Button variant="primary" disabled={!draft || (draft.kind === "url" && !draft.url.trim()) || (draft.kind === "folder" && !draft.title.trim())} onClick={() => void save().then((ok) => ok && toast("Saved"))}>Save</Button>
          </>
        }
      >
        {draft && (
          <div className="form-grid">
            <TextField label="Name" value={draft.title} data-autofocus onChange={(e) => setDraft({ ...draft, title: e.target.value })} />
            {draft.kind === "url" && <TextField label="Address" placeholder="https://example.com" value={draft.url} onChange={(e) => setDraft({ ...draft, url: e.target.value })} />}
            <div className="field">
              <label>Folder</label>
              <Select<string>
                label="Folder"
                value={String(draft.parentId)}
                options={folderOptions(all, draft.id ?? undefined).map((f) => ({ value: String(f.id), label: `${"  ".repeat(f.depth)}${f.label}` }))}
                onChange={(v) => setDraft({ ...draft, parentId: Number(v) })}
              />
            </div>
            {draftError && <Notice tone="bad">{draftError}</Notice>}
          </div>
        )}
      </Modal>
    </div>
  );
}
