import { useCallback, useEffect, useRef, useState } from "react";
import type { MouseEvent } from "react";

import { BAR_ID, childrenOf, splitOverflow } from "../../lib/bookmarks";
import { api } from "../../lib/ipc";
import { attempt, useStore } from "../../lib/store";
import type { Bookmark } from "../../lib/types";
import { Icon } from "../ui/Icon";
import { MenuHeading, MenuItem, MenuSeparator, Popover } from "../ui/Popover";

function useBookmarks(): Bookmark[] {
  const rev = useStore((s) => s.bookmarksRev);
  const [all, setAll] = useState<Bookmark[]>([]);
  useEffect(() => {
    let live = true;
    api.bookmarksAll().then((b) => live && setAll(b)).catch(() => {});
    return () => {
      live = false;
    };
  }, [rev]);
  return all;
}

type Open = { kind: "folder"; folder: Bookmark; el: HTMLElement } | { kind: "overflow"; el: HTMLElement } | { kind: "context"; item: Bookmark; x: number; y: number } | null;

export function BookmarksBar() {
  const all = useBookmarks();
  const items = childrenOf(all, BAR_ID);
  const barRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(800);
  const [open, setOpen] = useState<Open>(null);
  const anchor = useRef<HTMLElement | null>(null);

  useEffect(() => {
    const el = barRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);

  const close = useCallback(() => setOpen(null), []);
  const { visible, hidden } = splitOverflow(items, width - 16);

  const openUrl = (b: Bookmark, newTab: boolean) => {
    if (!b.url) return;
    const url = b.url;
    void attempt(() => (newTab ? api.tabNew(url) : api.navigate(url)));
  };

  const onChip = (b: Bookmark, e: MouseEvent<HTMLElement>) => {
    if (b.kind === "folder") {
      anchor.current = e.currentTarget;
      setOpen({ kind: "folder", folder: b, el: e.currentTarget });
    } else openUrl(b, e.ctrlKey);
  };

  if (items.length === 0) {
    return (
      <div className="bookmarks-bar is-empty" ref={barRef}>
        <span className="dim">Bookmarks you add with <kbd>Ctrl</kbd>+<kbd>D</kbd> show up here.</span>
      </div>
    );
  }

  return (
    <div className="bookmarks-bar" ref={barRef} role="toolbar" aria-label="Bookmarks">
      {visible.map((b) => (
        <button
          key={b.id}
          type="button"
          className="bm-chip"
          title={b.url ?? b.title}
          onClick={(e) => onChip(b, e)}
          onAuxClick={(e) => e.button === 1 && openUrl(b, true)}
          onContextMenu={(e) => {
            e.preventDefault();
            setOpen({ kind: "context", item: b, x: e.clientX, y: e.clientY });
          }}
        >
          <Icon name={b.kind === "folder" ? "folder" : "globe"} size={14} />
          <span className="truncate">{b.title || b.url}</span>
        </button>
      ))}
      {hidden.length > 0 && (
        <button
          type="button"
          className="bm-chip bm-more"
          aria-label="More bookmarks"
          onClick={(e) => {
            anchor.current = e.currentTarget;
            setOpen({ kind: "overflow", el: e.currentTarget });
          }}
        >
          <Icon name="chevronRight" size={14} />
        </button>
      )}

      <Popover open={open?.kind === "folder"} onClose={close} anchor={anchor} width={280} label="Bookmark folder" className="menu">
        {open?.kind === "folder" && <FolderList all={all} folderId={open.folder.id} onOpen={openUrl} />}
      </Popover>
      <Popover open={open?.kind === "overflow"} onClose={close} anchor={anchor} align="end" width={280} label="More bookmarks" className="menu">
        <div role="menu">
          {hidden.map((b) =>
            b.kind === "folder" ? (
              <div key={b.id}>
                <MenuHeading>{b.title}</MenuHeading>
                <FolderList all={all} folderId={b.id} onOpen={openUrl} />
              </div>
            ) : (
              <MenuItem key={b.id} icon="globe" label={b.title || b.url || ""} onSelect={() => openUrl(b, false)} />
            ),
          )}
        </div>
      </Popover>
      <Popover open={open?.kind === "context"} onClose={close} point={open?.kind === "context" ? { x: open.x, y: open.y } : null} width={220} label="Bookmark menu" className="menu">
        {open?.kind === "context" && (
          <div role="menu">
            {open.item.kind === "url" && <MenuItem icon="plus" label="Open in new tab" onSelect={() => openUrl(open.item, true)} />}
            {open.item.kind === "url" && <MenuItem icon="incognito" label="Open in private tab" onSelect={() => open.item.url && void attempt(() => api.tabNew(open.item.url ?? undefined, true))} />}
            <MenuItem icon="bookmarks" label="Manage bookmarks" onSelect={() => void attempt(() => api.runCommand({ type: "openBookmarks" }))} />
            <MenuSeparator />
            <MenuItem icon="trash" danger label="Delete" onSelect={() => void attempt(() => api.bookmarkDelete(open.item.id), "Could not delete")} />
          </div>
        )}
      </Popover>
    </div>
  );
}

function FolderList({ all, folderId, onOpen }: { all: Bookmark[]; folderId: number; onOpen: (b: Bookmark, newTab: boolean) => void }) {
  const kids = childrenOf(all, folderId);
  if (kids.length === 0) return <p className="dim small pad">Empty folder</p>;
  return (
    <div role="menu">
      {kids.map((k) =>
        k.kind === "folder" ? (
          <div key={k.id}>
            <MenuHeading>{k.title}</MenuHeading>
            <FolderList all={all} folderId={k.id} onOpen={onOpen} />
          </div>
        ) : (
          <MenuItem key={k.id} icon="globe" label={k.title || k.url || ""} onSelect={() => onOpen(k, false)} />
        ),
      )}
    </div>
  );
}
