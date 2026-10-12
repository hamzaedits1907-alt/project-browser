import { useCallback, useEffect, useRef, useState } from "react";
import type { MouseEvent, PointerEvent, ReactNode, WheelEvent } from "react";

import { api } from "../../lib/ipc";
import { useFlip } from "../../lib/motion";
import { cue } from "../../lib/sounds";
import { attempt, useStore } from "../../lib/store";
import { shiftFor } from "../../lib/tabDrag";
import type { Tab, TabGroup } from "../../lib/types";
import { IconButton, cx } from "../ui/controls";
import { GroupChip, GroupContextMenu, TabContextMenu } from "./TabMenus";
import type { MenuAt } from "./TabMenus";
import { TabItem } from "./TabItem";
import { useForeignTabDrop } from "./useForeignTabDrop";
import { useTabDrag } from "./useTabDrag";

/**
 * The tabs of this window, horizontally (title bar) or as a vertical list. Tabs can be dragged to reorder, pulled out
 * into a new window, or dropped onto another window's strip; groups show as coloured chips that collapse their tabs.
 */
export function TabStrip({ vertical = false }: { vertical?: boolean }) {
  const tabs = useStore((s) => s.browser.tabs);
  const groups = useStore((s) => s.browser.groups);
  const activeId = useStore((s) => s.browser.activeId);
  const listRef = useRef<HTMLDivElement>(null);
  const axis = vertical ? "y" : "x";
  const { drag, handlers } = useTabDrag(listRef, axis);
  const foreign = useForeignTabDrop(listRef, axis);
  const [tabMenu, setTabMenu] = useState<MenuAt<Tab> | null>(null);
  const [groupMenu, setGroupMenu] = useState<MenuAt<TabGroup> | null>(null);

  useFlip(listRef, tabs, axis);

  // Keep the active tab in view when the strip scrolls (many tabs).
  useEffect(() => {
    const el = listRef.current?.querySelector<HTMLElement>(".tab.is-active");
    el?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [activeId]);

  const onTabPointerDown = useCallback((tab: Tab, e: PointerEvent<HTMLElement>) => handlers.onPointerDown(tab.id, e), [handlers]);
  const onTabMenu = useCallback((tab: Tab, e: MouseEvent) => setTabMenu({ item: tab, x: e.clientX, y: e.clientY }), []);
  const onGroupMenu = useCallback((group: TabGroup, x: number, y: number) => setGroupMenu({ item: group, x, y }), []);
  const closeTabMenu = useCallback(() => setTabMenu(null), []);
  const closeGroupMenu = useCallback(() => setGroupMenu(null), []);

  // A mouse wheel over a crowded horizontal strip scrolls it sideways.
  const onWheel = (e: WheelEvent<HTMLDivElement>) => {
    const el = listRef.current;
    if (vertical || !el || el.scrollWidth <= el.clientWidth) return;
    el.scrollLeft += e.deltaY + e.deltaX;
  };

  // Natural width of the horizontal strip: full-size tabs until the window runs out of room (then they shrink).
  const visibleTabs = tabs.filter((t) => t.group === null || !groups.find((g) => g.id === t.group)?.collapsed);
  const naturalWidth = visibleTabs.reduce((w, t) => w + (t.pinned ? 43 : 227), 0) + groups.length * 64 + 12;

  const counts = new Map<number, number>();
  for (const t of tabs) if (t.group !== null) counts.set(t.group, (counts.get(t.group) ?? 0) + 1);

  const items: ReactNode[] = [];
  let seenGroup: number | null = null;
  tabs.forEach((t, i) => {
    const group = t.group === null ? null : (groups.find((g) => g.id === t.group) ?? null);
    if (group && seenGroup !== group.id) {
      items.push(<GroupChip key={`g${group.id}`} group={group} count={counts.get(group.id) ?? 0} vertical={vertical} onContextMenu={onGroupMenu} />);
    }
    seenGroup = group?.id ?? null;
    if (group?.collapsed) return;
    const dragged = drag?.id === t.id;
    const shift = !drag ? 0 : dragged ? drag.offset : drag.torn ? 0 : shiftFor(i, drag.from, drag.to, drag.size);
    items.push(
      <TabItem
        key={t.id}
        tab={t}
        active={t.id === activeId}
        groupColor={group?.color ?? null}
        shift={shift}
        dragged={dragged}
        torn={!!drag?.torn && dragged}
        dropBefore={foreign?.before === t.id}
        vertical={vertical}
        onPointerDown={onTabPointerDown}
        onContextMenu={onTabMenu}
      />,
    );
  });

  const newTab = (
    <IconButton
      icon="plus"
      label="New tab (Ctrl+T)"
      className="new-tab-btn"
      size={17}
      onClick={() => {
        cue("open");
        void attempt(() => api.tabNew());
      }}
    />
  );

  return (
    <div className={cx("tabstrip", vertical ? "is-vertical" : "is-horizontal", drag?.torn && "is-tearing")} role="tablist" aria-orientation={vertical ? "vertical" : "horizontal"} aria-label="Open tabs">
      {vertical && <div className="vtabs-head">{newTab}<span className="eyebrow">{tabs.length} {tabs.length === 1 ? "tab" : "tabs"}</span></div>}
      <div
        ref={listRef}
        className={cx("tabs", foreign && foreign.before === null && "drop-at-end")}
        style={vertical ? undefined : { width: naturalWidth }}
        onWheel={onWheel}
        onPointerMove={handlers.onPointerMove}
        onPointerUp={handlers.onPointerUp}
        onPointerCancel={handlers.onPointerCancel}
        onLostPointerCapture={handlers.onLostPointerCapture}
      >
        {items}
      </div>
      {!vertical && newTab}
      {drag?.torn && (
        <div className="tear-hint" role="status">
          Release to open in a new window, or over another window to move it there
        </div>
      )}
      <TabContextMenu menu={tabMenu} onClose={closeTabMenu} />
      <GroupContextMenu menu={groupMenu} onClose={closeGroupMenu} />
    </div>
  );
}
