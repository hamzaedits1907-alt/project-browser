import { useState } from "react";

import { GROUP_HEX, groupHex, groupLabel, nextGroupColor } from "../../lib/groups";
import { api } from "../../lib/ipc";
import { attempt, getState, useStore } from "../../lib/store";
import { useLoad } from "../../lib/useLoad";
import { GROUP_COLORS } from "../../lib/types";
import type { Tab, TabGroup } from "../../lib/types";
import { Icon } from "../ui/Icon";
import { MenuHeading, MenuItem, MenuSeparator, Popover, usePopoverClose } from "../ui/Popover";

export interface MenuAt<T> {
  item: T;
  x: number;
  y: number;
}

/** Closes every unpinned tab after `tab` in strip order. */
function closeTabsToTheRight(tab: Tab): void {
  const tabs = getState().browser.tabs;
  const i = tabs.findIndex((t) => t.id === tab.id);
  for (const t of tabs.slice(i + 1).filter((x) => !x.pinned)) void attempt(() => api.tabClose(t.id));
}

export function TabContextMenu({ menu, onClose }: { menu: MenuAt<Tab> | null; onClose: () => void }) {
  const tab = menu?.item;
  const tabs = useStore((s) => s.browser.tabs);
  const groups = useStore((s) => s.browser.groups);
  const activeId = useStore((s) => s.browser.activeId);
  const closedCount = useStore((s) => s.browser.closedCount);
  const windowCount = useStore((s) => s.browser.windowCount);
  // The window list is fetched when the menu opens, so it is never out of date.
  const others = useLoad(() => api.windowsList(), menu && windowCount > 1 ? `${tab?.id}:${windowCount}` : null).data?.filter((w) => !w.current && (!w.private || !!tab?.private)) ?? [];
  const point = menu ? { x: menu.x, y: menu.y } : null;
  const canSleep = !!tab && tab.id !== activeId && tab.lifecycle === "live";
  const index = tab ? tabs.findIndex((t) => t.id === tab.id) : -1;
  const hasRight = index >= 0 && tabs.slice(index + 1).some((t) => !t.pinned);
  const web = !!tab && !tab.url.startsWith("browser://") && !tab.error;
  return (
    <Popover open={!!menu} onClose={onClose} point={point} width={272} label="Tab menu" className="menu">
      {tab && (
        <div role="menu">
          <MenuItem icon="plus" label="New tab" hint="Ctrl+T" onSelect={() => void attempt(() => api.tabNew())} />
          <MenuItem icon="reload" label="Reload" hint="F5" onSelect={() => void attempt(() => api.nav("reload", tab.id))} />
          <MenuItem icon="copy" label="Duplicate" onSelect={() => void attempt(() => api.tabDuplicate(tab.id))} />
          <MenuItem icon="pin" label={tab.pinned ? "Unpin tab" : "Pin tab"} onSelect={() => void attempt(() => api.tabPin(tab.id, !tab.pinned))} />
          {(tab.audible || tab.muted) && (
            <MenuItem icon={tab.muted ? "volume" : "mute"} label={tab.muted ? "Unmute site" : "Mute site"} onSelect={() => void attempt(() => api.tabMute(tab.id, !tab.muted))} />
          )}
          {web && <MenuItem icon="star" filledIcon={tab.bookmarked} label={tab.bookmarked ? "Remove bookmark" : "Bookmark this tab"} onSelect={() => void attempt(() => api.bookmarkToggle(tab.id))} />}
          {!tab.pinned && (
            <>
              <MenuSeparator />
              <MenuHeading>Tab group</MenuHeading>
              <MenuItem
                icon="group"
                label="Add to a new group"
                onSelect={() => void attempt(() => api.tabGroup({ op: "create", tabs: [tab.id], title: "", color: nextGroupColor(groups) }))}
              />
              {groups
                .filter((g) => g.id !== tab.group)
                .map((g) => (
                  <MenuItem
                    key={g.id}
                    icon="group"
                    label={`Add to “${groupLabel(g, tabs.filter((t) => t.group === g.id).length)}”`}
                    trailing={<span className="group-dot" style={{ background: groupHex(g.color) }} />}
                    onSelect={() => void attempt(() => api.tabGroup({ op: "add", tab: tab.id, group: g.id }))}
                  />
                ))}
              {tab.group !== null && <MenuItem icon="close" label="Remove from group" onSelect={() => void attempt(() => api.tabGroup({ op: "remove", tab: tab.id }))} />}
            </>
          )}
          <MenuSeparator />
          <MenuItem icon="windowNew" label="Move tab to a new window" disabled={tabs.length < 2} onSelect={() => void attempt(() => api.tabToWindow(tab.id), "Could not open a new window")} />
          {others.map((w) => (
            <MenuItem
              key={w.label}
              icon={w.private ? "incognito" : "window"}
              label={`Move to: ${w.title || "New tab"}`}
              hint={`${w.tabCount} ${w.tabCount === 1 ? "tab" : "tabs"}`}
              onSelect={() => void attempt(() => api.tabToWindow(tab.id, w.label), "Could not move the tab")}
            />
          ))}
          {canSleep && (
            <>
              <MenuSeparator />
              <MenuItem icon="moon" label="Put to sleep" onSelect={() => void attempt(() => api.tabSleep(tab.id, false))} />
              <MenuItem icon="memory" label="Unload (free all its memory)" onSelect={() => void attempt(() => api.tabSleep(tab.id, true))} />
            </>
          )}
          <MenuSeparator />
          <MenuItem icon="close" label="Close tab" hint="Ctrl+W" onSelect={() => void attempt(() => api.tabClose(tab.id))} />
          <MenuItem icon="layers" label="Close other tabs" disabled={tabs.length < 2} onSelect={() => void attempt(() => api.tabCloseOthers(tab.id))} />
          <MenuItem icon="chevronRight" label="Close tabs to the right" disabled={!hasRight} onSelect={() => closeTabsToTheRight(tab)} />
          <MenuItem icon="history" label="Reopen closed tab" hint="Ctrl+Shift+T" disabled={closedCount === 0} onSelect={() => void attempt(() => api.tabReopen())} />
        </div>
      )}
    </Popover>
  );
}

function GroupEditor({ group }: { group: TabGroup }) {
  const close = usePopoverClose();
  const [title, setTitle] = useState(group.title);
  const save = (next: string) => void attempt(() => api.tabGroup({ op: "update", group: group.id, title: next }));
  return (
    <div className="group-editor">
      <input
        className="input"
        value={title}
        placeholder="Name this group"
        aria-label="Group name"
        maxLength={40}
        autoFocus
        spellCheck={false}
        onChange={(e) => setTitle(e.target.value)}
        onBlur={() => title !== group.title && save(title)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            save(title);
            close();
          }
        }}
      />
      <div className="group-colors" role="radiogroup" aria-label="Group colour">
        {GROUP_COLORS.map((c) => (
          <button
            key={c}
            type="button"
            role="radio"
            aria-checked={group.color === c}
            aria-label={c}
            title={c}
            className={`group-swatch${group.color === c ? " is-on" : ""}`}
            style={{ background: GROUP_HEX[c] }}
            onClick={() => void attempt(() => api.tabGroup({ op: "update", group: group.id, color: c }))}
          />
        ))}
      </div>
    </div>
  );
}

export function GroupContextMenu({ menu, onClose }: { menu: MenuAt<TabGroup> | null; onClose: () => void }) {
  const live = useStore((s) => s.browser.groups.find((g) => g.id === menu?.item.id) ?? null);
  const group = live ?? menu?.item ?? null;
  const point = menu ? { x: menu.x, y: menu.y } : null;
  return (
    <Popover open={!!menu} onClose={onClose} point={point} width={264} label="Tab group menu" className="menu">
      {group && (
        <div role="menu">
          <GroupEditor key={group.id} group={group} />
          <MenuSeparator />
          <MenuItem
            icon={group.collapsed ? "chevronDown" : "chevronUp"}
            label={group.collapsed ? "Expand group" : "Collapse group"}
            onSelect={() => void attempt(() => api.tabGroup({ op: "update", group: group.id, collapsed: !group.collapsed }))}
          />
          <MenuItem icon="layers" label="Ungroup" onSelect={() => void attempt(() => api.tabGroup({ op: "ungroup", group: group.id }))} />
          <MenuItem icon="close" danger label="Close group" onSelect={() => void attempt(() => api.tabGroup({ op: "close", group: group.id }))} />
        </div>
      )}
    </Popover>
  );
}

export function GroupChip({ group, count, vertical, onContextMenu }: { group: TabGroup; count: number; vertical: boolean; onContextMenu: (g: TabGroup, x: number, y: number) => void }) {
  const label = groupLabel(group, count);
  return (
    <button
      type="button"
      className={`group-chip${group.collapsed ? " is-collapsed" : ""}${vertical ? " is-vertical" : ""}`}
      style={{ ["--group" as string]: groupHex(group.color) }}
      aria-expanded={!group.collapsed}
      title={`${label}: ${group.collapsed ? "click to show its tabs" : "click to collapse"} · right-click for options`}
      onClick={() => void attempt(() => api.tabGroup({ op: "update", group: group.id, collapsed: !group.collapsed }))}
      onContextMenu={(e) => {
        e.preventDefault();
        onContextMenu(group, e.clientX, e.clientY);
      }}
    >
      <span className="group-chip-label">{group.title || (group.collapsed ? String(count) : "")}</span>
      {group.collapsed && group.title && <span className="group-chip-count">{count}</span>}
      {vertical && <Icon name={group.collapsed ? "chevronRight" : "chevronDown"} size={14} />}
    </button>
  );
}
