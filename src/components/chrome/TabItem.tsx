import { memo } from "react";
import type { CSSProperties, MouseEvent, PointerEvent } from "react";

import { groupHex } from "../../lib/groups";
import { api } from "../../lib/ipc";
import { cue } from "../../lib/sounds";
import { attempt } from "../../lib/store";
import type { Tab } from "../../lib/types";
import { monogram, tabTitle } from "../../lib/urls";
import { Favicon } from "../ui/Favicon";
import { Icon } from "../ui/Icon";
import type { IconName } from "../ui/Icon";
import { cx } from "../ui/controls";

export interface TabItemProps {
  tab: Tab;
  active: boolean;
  groupColor: string | null;
  /** Transform while a drag is in progress: this tab's own offset (dragged) or the room it makes (neighbour). */
  shift: number;
  dragged: boolean;
  torn: boolean;
  dropBefore: boolean;
  vertical: boolean;
  onPointerDown: (tab: Tab, e: PointerEvent<HTMLElement>) => void;
  onContextMenu: (tab: Tab, e: MouseEvent) => void;
}

export function internalIcon(url: string): IconName {
  if (url.includes("settings")) return "settings";
  if (url.includes("history")) return "history";
  if (url.includes("bookmarks")) return "bookmarks";
  if (url.includes("downloads")) return "download";
  if (url.includes("performance")) return "gauge";
  if (url.includes("gaming")) return "gamepad";
  if (url.includes("memory")) return "brain";
  if (url.includes("research")) return "flask";
  return "sparkle";
}

function TabItemView({ tab, active, groupColor, shift, dragged, torn, dropBefore, vertical, onPointerDown, onContextMenu }: TabItemProps) {
  const title = tabTitle(tab);
  const sleeping = tab.lifecycle === "sleeping";
  const status = [sleeping ? "Sleeping to save memory" : null, tab.heavy ? "Using a lot of memory or CPU" : null, tab.error ? "Failed to load" : null, tab.muted ? "Muted" : null].filter(Boolean);
  const style: CSSProperties = {};
  if (shift !== 0) style.transform = vertical ? `translateY(${shift}px)` : `translateX(${shift}px)`;
  if (groupColor) (style as Record<string, string>)["--group"] = groupHex(groupColor);

  return (
    <div
      role="tab"
      aria-selected={active}
      tabIndex={active ? 0 : -1}
      data-tab-id={tab.id}
      data-flip={tab.id}
      data-flip-skip={dragged ? "" : undefined}
      className={cx(
        "tab",
        active && "is-active",
        tab.pinned && "is-pinned",
        sleeping && "is-sleeping",
        dragged && "is-dragging",
        dragged && torn && "is-torn",
        tab.private && "is-private",
        tab.error && "has-error",
        groupColor && "in-group",
        dropBefore && "drop-before",
        shift !== 0 && !dragged && "is-shifting",
      )}
      style={style}
      title={status.length ? `${title}\n${status.join(" · ")}` : title}
      // A press starts a possible drag; the tab is activated on release if it was a click (useTabDrag), so dragging a
      // background tab never switches to it.
      onPointerDown={(e) => onPointerDown(tab, e)}
      onDragStart={(e) => e.preventDefault()}
      onAuxClick={(e) => {
        if (e.button === 1) {
          e.preventDefault();
          cue("close");
          void attempt(() => api.tabClose(tab.id));
        }
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        onContextMenu(tab, e);
      }}
      onKeyDown={(e) => {
        if (e.key === "Delete") void attempt(() => api.tabClose(tab.id));
        if (e.key === "Enter" || e.key === " ") void attempt(() => api.tabActivate(tab.id));
        if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
          e.preventDefault();
          const r = e.currentTarget.getBoundingClientRect();
          onContextMenu(tab, { clientX: r.left + 8, clientY: r.bottom } as MouseEvent);
        }
      }}
    >
      <span className="tab-icon">
        {tab.loading ? (
          <span className="tab-loader" aria-label="Loading" />
        ) : tab.private ? (
          <Icon name="incognito" size={15} />
        ) : tab.url.startsWith("browser://") ? (
          <Icon name={internalIcon(tab.url)} size={15} />
        ) : (
          <Favicon src={tab.favicon} fallback={monogram(tab)} size={16} />
        )}
        {sleeping && (
          <span className="tab-badge tab-badge-sleep" aria-label="Sleeping">
            <Icon name="moon" size={8} filled strokeWidth={1} />
          </span>
        )}
        {tab.heavy && !sleeping && <span className="tab-badge tab-badge-heavy" aria-label="Heavy" />}
        {(tab.audible || tab.muted) && tab.pinned && <span className={cx("tab-badge tab-badge-audio", tab.muted && "is-muted")} />}
      </span>
      {!tab.pinned && <span className="tab-title fade-end">{title}</span>}
      {(tab.audible || tab.muted) && !tab.pinned && (
        <button
          type="button"
          className={cx("tab-btn tab-audio", tab.muted && "is-muted")}
          aria-label={tab.muted ? "Unmute tab" : "Mute tab"}
          title={tab.muted ? "Unmute tab" : "Mute tab"}
          tabIndex={-1}
          onPointerDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            e.stopPropagation();
            void attempt(() => api.tabMute(tab.id, !tab.muted));
          }}
        >
          <Icon name={tab.muted ? "mute" : "volume"} size={14} />
        </button>
      )}
      {!tab.pinned && (
        <button
          type="button"
          className="tab-btn tab-close"
          aria-label={`Close ${title}`}
          title="Close tab (Ctrl+W)"
          tabIndex={-1}
          onPointerDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            e.stopPropagation();
            cue("close");
            void attempt(() => api.tabClose(tab.id));
          }}
        >
          <Icon name="close" size={13} strokeWidth={2.1} />
        </button>
      )}
    </div>
  );
}

function same(a: TabItemProps, b: TabItemProps): boolean {
  const x = a.tab;
  const y = b.tab;
  return (
    a.active === b.active &&
    a.groupColor === b.groupColor &&
    a.shift === b.shift &&
    a.dragged === b.dragged &&
    a.torn === b.torn &&
    a.dropBefore === b.dropBefore &&
    a.vertical === b.vertical &&
    a.onPointerDown === b.onPointerDown &&
    a.onContextMenu === b.onContextMenu &&
    x.id === y.id &&
    x.title === y.title &&
    x.url === y.url &&
    x.favicon === y.favicon &&
    x.pinned === y.pinned &&
    x.private === y.private &&
    x.loading === y.loading &&
    x.audible === y.audible &&
    x.muted === y.muted &&
    x.lifecycle === y.lifecycle &&
    x.heavy === y.heavy &&
    (x.error === null) === (y.error === null)
  );
}

/** One tab. Re-renders only when something it shows changes (state events replace every tab object). */
export const TabItem = memo(TabItemView, same);
