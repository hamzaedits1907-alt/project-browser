import { useCallback, useRef, useState } from "react";
import type { DragEvent, MouseEvent } from "react";

import { api } from "../../lib/ipc";
import { attempt, useStore } from "../../lib/store";
import { clampSlot, moveTargetIndex, slotForPointer } from "../../lib/tabs";
import type { Tab } from "../../lib/types";
import { IconButton } from "../ui/controls";
import { MenuItem, MenuSeparator, Popover } from "../ui/Popover";
import { TabItem } from "./TabItem";

interface MenuState {
  tab: Tab;
  x: number;
  y: number;
}

/** Natural width of the strip: full-size tabs until the window runs out of room (then they shrink). */
function preferredWidth(tabs: Tab[]): number {
  const pinned = tabs.filter((t) => t.pinned).length;
  return pinned * 46 + (tabs.length - pinned) * 226;
}

export function TabStrip() {
  const tabs = useStore((s) => s.browser.tabs);
  const activeId = useStore((s) => s.browser.activeId);
  const closedCount = useStore((s) => s.browser.closedCount);
  const listRef = useRef<HTMLDivElement>(null);
  const [dragId, setDragId] = useState<number | null>(null);
  const [slot, setSlot] = useState<number | null>(null);
  const [menu, setMenu] = useState<MenuState | null>(null);

  const onDragStart = useCallback((tab: Tab, e: DragEvent) => {
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/x-pb-tab", String(tab.id));
    setDragId(tab.id);
  }, []);

  const onDragEnd = useCallback(() => {
    setDragId(null);
    setSlot(null);
  }, []);

  const rects = () =>
    [...(listRef.current?.querySelectorAll<HTMLElement>("[data-tab-id]") ?? [])].map((el) => {
      const r = el.getBoundingClientRect();
      return { left: r.left, width: r.width };
    });

  const onDragOver = (e: DragEvent) => {
    if (dragId === null) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    const dragged = tabs.find((t) => t.id === dragId);
    if (!dragged) return;
    setSlot(clampSlot(slotForPointer(e.clientX, rects()), dragged, tabs));
  };

  const onDrop = (e: DragEvent) => {
    e.preventDefault();
    const dragged = tabs.find((t) => t.id === dragId);
    const from = tabs.findIndex((t) => t.id === dragId);
    if (dragged && from >= 0 && slot !== null) {
      const to = moveTargetIndex(from, slot);
      if (to !== from) void attempt(() => api.tabMove(dragged.id, to));
    }
    onDragEnd();
  };

  const onContextMenu = useCallback((tab: Tab, e: MouseEvent) => setMenu({ tab, x: e.clientX, y: e.clientY }), []);

  return (
    <div className="tabstrip" role="tablist" aria-label="Open tabs">
      <div ref={listRef} className="tabs" style={{ width: preferredWidth(tabs) }} onDragOver={onDragOver} onDrop={onDrop} onDragLeave={(e) => e.currentTarget === e.target && setSlot(null)}>
        {tabs.map((t, i) => (
          <div key={t.id} className={`tab-slot${t.pinned ? " is-pinned" : ""}${slot === i ? " drop-before" : ""}${slot === tabs.length && i === tabs.length - 1 ? " drop-after" : ""}`}>
            <TabItem tab={t} active={t.id === activeId} dragging={dragId === t.id} onContextMenu={onContextMenu} onDragStart={onDragStart} onDragEnd={onDragEnd} />
          </div>
        ))}
      </div>
      <IconButton icon="plus" label="New tab (Ctrl+T)" className="new-tab-btn" size={17} onClick={() => void attempt(() => api.tabNew())} />
      <TabContextMenu menu={menu} onClose={() => setMenu(null)} tabCount={tabs.length} closedCount={closedCount} activeId={activeId} />
    </div>
  );
}

function TabContextMenu({ menu, onClose, tabCount, closedCount, activeId }: { menu: MenuState | null; onClose: () => void; tabCount: number; closedCount: number; activeId: number | null }) {
  const tab = menu?.tab;
  const point = menu ? { x: menu.x, y: menu.y } : null;
  const canSleep = !!tab && tab.id !== activeId && tab.lifecycle === "live";
  return (
    <Popover open={!!menu} onClose={onClose} point={point} width={264} label="Tab menu" className="menu">
      {tab && (
        <div role="menu">
          <MenuItem icon="plus" label="New tab" hint="Ctrl+T" onSelect={() => void attempt(() => api.tabNew())} />
          <MenuItem icon="reload" label="Reload" onSelect={() => void attempt(() => api.nav("reload", tab.id))} />
          <MenuItem icon="copy" label="Duplicate" onSelect={() => void attempt(() => api.tabDuplicate(tab.id))} />
          <MenuItem icon="pin" label={tab.pinned ? "Unpin tab" : "Pin tab"} onSelect={() => void attempt(() => api.tabPin(tab.id, !tab.pinned))} />
          {(tab.audible || tab.muted) && (
            <MenuItem icon={tab.muted ? "volume" : "mute"} label={tab.muted ? "Unmute site" : "Mute site"} onSelect={() => void attempt(() => api.tabMute(tab.id, !tab.muted))} />
          )}
          {!tab.url.startsWith("browser://") && !tab.error && (
            <MenuItem icon="star" filledIcon={tab.bookmarked} label={tab.bookmarked ? "Remove bookmark" : "Bookmark this tab"} onSelect={() => void attempt(() => api.bookmarkToggle(tab.id))} />
          )}
          <MenuSeparator />
          {canSleep && <MenuItem icon="moon" label="Put to sleep" onSelect={() => void attempt(() => api.tabSleep(tab.id, false))} />}
          {canSleep && <MenuItem icon="trash" label="Unload (free all memory)" onSelect={() => void attempt(() => api.tabSleep(tab.id, true))} />}
          {canSleep && <MenuSeparator />}
          <MenuItem icon="close" label="Close tab" hint="Ctrl+W" onSelect={() => void attempt(() => api.tabClose(tab.id))} />
          <MenuItem icon="layers" label="Close other tabs" disabled={tabCount < 2} onSelect={() => void attempt(() => api.tabCloseOthers(tab.id))} />
          <MenuItem icon="history" label="Reopen closed tab" hint="Ctrl+Shift+T" disabled={closedCount === 0} onSelect={() => void attempt(() => api.tabReopen())} />
        </div>
      )}
    </Popover>
  );
}
