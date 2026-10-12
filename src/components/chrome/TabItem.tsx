import type { DragEvent, MouseEvent } from "react";

import { api } from "../../lib/ipc";
import { attempt } from "../../lib/store";
import type { Tab } from "../../lib/types";
import { monogram, tabTitle } from "../../lib/urls";
import { Favicon } from "../ui/Favicon";
import { Icon } from "../ui/Icon";
import { cx, Spinner } from "../ui/controls";

interface TabItemProps {
  tab: Tab;
  active: boolean;
  dragging: boolean;
  onContextMenu: (tab: Tab, e: MouseEvent) => void;
  onDragStart: (tab: Tab, e: DragEvent) => void;
  onDragEnd: (e: DragEvent) => void;
}

export function TabItem({ tab, active, dragging, onContextMenu, onDragStart, onDragEnd }: TabItemProps) {
  const title = tabTitle(tab);
  const sleeping = tab.lifecycle === "sleeping";
  const showClose = !tab.pinned;
  const status = [
    sleeping ? "Sleeping to save memory" : null,
    tab.heavy ? "Using a lot of memory or CPU" : null,
    tab.error ? "Failed to load" : null,
  ].filter(Boolean);

  return (
    <div
      role="tab"
      aria-selected={active}
      data-tab-id={tab.id}
      className={cx("tab", active && "is-active", tab.pinned && "is-pinned", sleeping && "is-sleeping", dragging && "is-dragging", tab.private && "is-private", tab.error && "has-error")}
      title={status.length ? `${title}\n${status.join(" · ")}` : title}
      draggable
      onDragStart={(e) => onDragStart(tab, e)}
      onDragEnd={(e) => onDragEnd(e)}
      onPointerDown={(e) => {
        if (e.button === 0) void attempt(() => api.tabActivate(tab.id));
      }}
      onAuxClick={(e) => {
        if (e.button === 1) {
          e.preventDefault();
          void attempt(() => api.tabClose(tab.id));
        }
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        onContextMenu(tab, e);
      }}
    >
      <span className="tab-icon">
        {tab.loading ? (
          <Spinner size={14} />
        ) : tab.private ? (
          <Icon name="incognito" size={15} />
        ) : tab.url.startsWith("browser://") ? (
          <Icon name={internalIcon(tab.url)} size={15} />
        ) : (
          <Favicon src={tab.favicon} fallback={monogram(tab)} size={16} />
        )}
        {sleeping && <span className="tab-badge tab-badge-sleep"><Icon name="moon" size={9} filled strokeWidth={1} /></span>}
        {tab.heavy && !sleeping && <span className="tab-badge tab-badge-heavy" />}
      </span>
      {!tab.pinned && <span className="tab-title truncate">{title}</span>}
      {(tab.audible || tab.muted) && (
        <button
          type="button"
          className={cx("tab-audio", tab.muted && "is-muted")}
          aria-label={tab.muted ? "Unmute tab" : "Mute tab"}
          title={tab.muted ? "Unmute tab" : "Mute tab"}
          onPointerDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            e.stopPropagation();
            void attempt(() => api.tabMute(tab.id, !tab.muted));
          }}
        >
          <Icon name={tab.muted ? "mute" : "volume"} size={14} />
        </button>
      )}
      {showClose && (
        <button
          type="button"
          className="tab-close"
          aria-label={`Close ${title}`}
          title="Close tab"
          onPointerDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            e.stopPropagation();
            void attempt(() => api.tabClose(tab.id));
          }}
        >
          <Icon name="close" size={13} strokeWidth={2} />
        </button>
      )}
    </div>
  );
}

function internalIcon(url: string) {
  if (url.includes("settings")) return "settings" as const;
  if (url.includes("history")) return "history" as const;
  if (url.includes("bookmarks")) return "bookmarks" as const;
  if (url.includes("downloads")) return "download" as const;
  if (url.includes("performance")) return "gauge" as const;
  if (url.includes("gaming")) return "gamepad" as const;
  if (url.includes("memory")) return "bookmarks" as const;
  if (url.includes("research")) return "search" as const;
  return "sparkle" as const;
}
