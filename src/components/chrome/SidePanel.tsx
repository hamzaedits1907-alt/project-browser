import { useCallback, useEffect, useRef, useState } from "react";
import type { MouseEvent, ReactNode } from "react";

import { BAR_ID, OTHER_ID, childrenOf, searchBookmarks } from "../../lib/bookmarks";
import { formatTime, groupByDay, hostOf } from "../../lib/format";
import { api, errorMessage } from "../../lib/ipc";
import { activeTab, attempt, useStore } from "../../lib/store";
import { closePanel, openClearData, openSettingsSection, ui } from "../../lib/ui";
import type { PanelId } from "../../lib/ui";
import type { Bookmark, HistoryEntry } from "../../lib/types";
import { useLoad } from "../../lib/useLoad";
import { ShieldSummary } from "../menus/ShieldSummary";
import { Favicon } from "../ui/Favicon";
import { Icon } from "../ui/Icon";
import type { IconName } from "../ui/Icon";
import { MenuItem, MenuSeparator, Popover } from "../ui/Popover";
import { EmptyState, Notice } from "../ui/Section";
import { Button, IconButton, Spinner } from "../ui/controls";
import { DownloadRow } from "./DownloadRow";

const TITLES: Record<PanelId, { title: string; icon: IconName }> = {
  bookmarks: { title: "Bookmarks", icon: "bookmarks" },
  history: { title: "History", icon: "history" },
  downloads: { title: "Downloads", icon: "download" },
  shield: { title: "Project Shield", icon: "shieldCheck" },
};

/** Opens a page in the current tab, or in a new one with Ctrl or the middle button. */
function openUrl(url: string, e?: MouseEvent): void {
  const newTab = !!e && (e.ctrlKey || e.button === 1);
  void attempt(() => (newTab ? api.tabNew(url, false, true) : api.navigate(url)));
}

function Row({ icon, url, title, sub, onMenu, onRemove }: { icon?: IconName; url: string; title: string; sub?: string; onMenu?: (e: MouseEvent) => void; onRemove?: () => void }) {
  return (
    <div className="panel-row">
      <button
        type="button"
        className="panel-link"
        title={url}
        onClick={(e) => openUrl(url, e)}
        onAuxClick={(e) => e.button === 1 && openUrl(url, e)}
        onContextMenu={(e) => {
          if (!onMenu) return;
          e.preventDefault();
          onMenu(e);
        }}
      >
        {icon ? <Icon name={icon} size={15} /> : <Favicon src={null} fallback={(hostOf(url).charAt(0) || "•").toUpperCase()} seed={hostOf(url)} size={16} />}
        <span className="panel-text">
          <span className="truncate">{title || url}</span>
          {sub && <span className="panel-sub truncate">{sub}</span>}
        </span>
      </button>
      {onRemove && <IconButton icon="close" label="Remove" size={13} className="panel-del" onClick={onRemove} />}
    </div>
  );
}

function useBookmarks(): Bookmark[] {
  const rev = useStore((s) => s.bookmarksRev);
  return useLoad(() => api.bookmarksAll(), rev).data ?? [];
}

function BookmarkTree({ all, parent, depth, onMenu }: { all: Bookmark[]; parent: number; depth: number; onMenu: (b: Bookmark, e: MouseEvent) => void }) {
  const [open, setOpen] = useState<Set<number>>(() => new Set());
  const kids = childrenOf(all, parent);
  if (kids.length === 0) return depth === 0 ? null : <p className="dim small panel-empty-folder">Empty folder</p>;
  return (
    <div className="panel-tree" style={{ ["--depth" as string]: depth }}>
      {kids.map((b) =>
        b.kind === "folder" ? (
          <div key={b.id}>
            <button
              type="button"
              className="panel-folder"
              aria-expanded={open.has(b.id)}
              onClick={() =>
                setOpen((s) => {
                  const n = new Set(s);
                  if (n.has(b.id)) n.delete(b.id);
                  else n.add(b.id);
                  return n;
                })
              }
            >
              <Icon name={open.has(b.id) ? "folderOpen" : "folder"} size={15} />
              <span className="truncate">{b.title}</span>
              <Icon name={open.has(b.id) ? "chevronDown" : "chevronRight"} size={13} />
            </button>
            {open.has(b.id) && <BookmarkTree all={all} parent={b.id} depth={depth + 1} onMenu={onMenu} />}
          </div>
        ) : (
          <Row key={b.id} url={b.url ?? ""} title={b.title} onMenu={(e) => onMenu(b, e)} />
        ),
      )}
    </div>
  );
}

function BookmarksPanel() {
  const all = useBookmarks();
  const tab = useStore(activeTab);
  const [q, setQ] = useState("");
  const [menu, setMenu] = useState<{ b: Bookmark; x: number; y: number } | null>(null);
  const found = searchBookmarks(all, q);
  const onMenu = useCallback((b: Bookmark, e: MouseEvent) => setMenu({ b, x: e.clientX, y: e.clientY }), []);
  const web = !!tab && !tab.url.startsWith("browser://") && !tab.error;
  return (
    <>
      <div className="panel-tools">
        <label className="search-field">
          <Icon name="search" size={15} />
          <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search bookmarks" aria-label="Search bookmarks" spellCheck={false} />
        </label>
        {web && <IconButton icon="star" filled={tab.bookmarked} label={tab.bookmarked ? "Remove bookmark for this page" : "Bookmark this page"} onClick={() => void attempt(() => api.bookmarkToggle(tab.id))} />}
      </div>
      <div className="panel-body">
        {q ? (
          found.length === 0 ? (
            <p className="dim small pad">No bookmarks match “{q}”.</p>
          ) : (
            found.map((b) => <Row key={b.id} url={b.url ?? ""} title={b.title} sub={hostOf(b.url ?? "")} onMenu={(e) => onMenu(b, e)} />)
          )
        ) : all.filter((b) => b.kind === "url").length === 0 ? (
          <EmptyState icon="bookmarks" title="No bookmarks yet">Press Ctrl+D on a page to bookmark it.</EmptyState>
        ) : (
          <>
            <div className="eyebrow panel-heading">Bookmarks bar</div>
            <BookmarkTree all={all} parent={BAR_ID} depth={0} onMenu={onMenu} />
            {childrenOf(all, OTHER_ID).length > 0 && (
              <>
                <div className="eyebrow panel-heading">Other bookmarks</div>
                <BookmarkTree all={all} parent={OTHER_ID} depth={0} onMenu={onMenu} />
              </>
            )}
          </>
        )}
      </div>
      <div className="panel-foot">
        <Button size="sm" variant="ghost" icon="external" onClick={() => void attempt(() => api.runCommand({ type: "openBookmarks" }))}>
          Manage bookmarks
        </Button>
      </div>
      <Popover open={!!menu} onClose={() => setMenu(null)} point={menu ? { x: menu.x, y: menu.y } : null} width={230} label="Bookmark menu" className="menu">
        {menu?.b.url && (
          <div role="menu">
            <MenuItem icon="plus" label="Open in new tab" onSelect={() => void attempt(() => api.tabNew(menu.b.url ?? undefined))} />
            <MenuItem icon="window" label="Open in new window" onSelect={() => void attempt(() => api.windowNew(menu.b.url ?? undefined))} />
            <MenuItem icon="incognito" label="Open in private window" onSelect={() => void attempt(() => api.windowNew(menu.b.url ?? undefined, true))} />
            <MenuSeparator />
            <MenuItem icon="trash" danger label="Delete" onSelect={() => void attempt(() => api.bookmarkDelete(menu.b.id), "Could not delete")} />
          </div>
        )}
      </Popover>
    </>
  );
}

function HistoryPanel() {
  const [q, setQ] = useState("");
  const [rows, setRows] = useState<HistoryEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const seq = useRef(0);
  useEffect(() => {
    const mine = ++seq.current;
    const t = setTimeout(
      () => {
        setLoading(true);
        api
          .historyQuery(q, undefined, 80)
          .then((r) => mine === seq.current && (setRows(r), setError(null)))
          .catch((e) => mine === seq.current && setError(errorMessage(e)))
          .finally(() => mine === seq.current && setLoading(false));
      },
      q ? 180 : 0,
    );
    return () => clearTimeout(t);
  }, [q]);
  const groups = groupByDay(rows, (r) => r.visitedAt);
  return (
    <>
      <div className="panel-tools">
        <label className="search-field">
          <Icon name="search" size={15} />
          <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search history" aria-label="Search history" spellCheck={false} />
        </label>
      </div>
      <div className="panel-body">
        {error && <Notice tone="bad">{error}</Notice>}
        {loading && rows.length === 0 && <div className="pad"><Spinner size={16} /></div>}
        {!loading && rows.length === 0 && !error && <EmptyState icon="history" title={q ? "Nothing matches" : "No history yet"}>{q ? "Try other words." : "Pages you visit appear here. Private tabs are never recorded."}</EmptyState>}
        {groups.map((g) => (
          <section key={g.label}>
            <div className="eyebrow panel-heading">{g.label}</div>
            {g.items.map((r) => (
              <Row
                key={r.visitId}
                url={r.url}
                title={r.title || r.url}
                sub={`${formatTime(r.visitedAt)} · ${hostOf(r.url)}`}
                onRemove={() => void attempt(() => api.historyDelete([r.visitId])).then(() => setRows((x) => x.filter((y) => y.visitId !== r.visitId)))}
              />
            ))}
          </section>
        ))}
      </div>
      <div className="panel-foot">
        <Button size="sm" variant="ghost" icon="external" onClick={() => void attempt(() => api.runCommand({ type: "openHistory" }))}>
          Full history
        </Button>
        <Button size="sm" variant="ghost" icon="broom" onClick={openClearData}>
          Clear data…
        </Button>
      </div>
    </>
  );
}

function DownloadsPanel() {
  const downloads = useStore((s) => s.downloads);
  const finished = downloads.some((d) => d.state !== "inprogress" && d.state !== "paused");
  return (
    <>
      <div className="panel-body">
        {downloads.length === 0 ? (
          <EmptyState icon="download" title="No downloads yet">Files you download show up here with their progress.</EmptyState>
        ) : (
          downloads.slice(0, 40).map((d) => <DownloadRow key={d.id} d={d} compact />)
        )}
      </div>
      <div className="panel-foot">
        <Button size="sm" variant="ghost" icon="external" onClick={() => void attempt(() => api.runCommand({ type: "openDownloads" }))}>
          All downloads
        </Button>
        {finished && (
          <Button size="sm" variant="ghost" icon="broom" onClick={() => void attempt(() => api.downloadsClear())}>
            Clear finished
          </Button>
        )}
      </div>
    </>
  );
}

function ShieldPanel() {
  return (
    <>
      <div className="panel-body">
        <ShieldSummary active />
      </div>
      <div className="panel-foot">
        <Button size="sm" variant="ghost" icon="settings" onClick={() => openSettingsSection("shield")}>
          Shield settings
        </Button>
      </div>
    </>
  );
}

const BODIES: Record<PanelId, () => ReactNode> = {
  bookmarks: () => <BookmarksPanel />,
  history: () => <HistoryPanel />,
  downloads: () => <DownloadsPanel />,
  shield: () => <ShieldPanel />,
};

/**
 * A panel docked between the sidebar and the page. It pushes the page aside instead of floating over it, so the page
 * stays live and usable next to it. Only the open panel exists; a closed one does no work at all.
 */
export function SidePanel() {
  const panel = ui.use((s) => s.panel);
  if (!panel) return null;
  const meta = TITLES[panel];
  return (
    <aside className="side-panel" aria-label={meta.title} key={panel}>
      <header className="panel-head">
        <Icon name={meta.icon} size={17} />
        <h2>{meta.title}</h2>
        <IconButton icon="close" label="Close panel" size={15} onClick={closePanel} />
      </header>
      {BODIES[panel]()}
    </aside>
  );
}
