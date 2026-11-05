import type { RefObject } from "react";

import { toggleAi } from "../../features/ai/aiStore";
import type { MenuEntry, MenuModel } from "../../features/contextmenu/model";
import { openPopup } from "../../features/contextmenu/popupMenu";
import { update as updateState } from "../../features/update/updateStore";
import { api } from "../../lib/ipc";
import { activeTab, attempt, getState, useStore } from "../../lib/store";
import type { BrowserCommand, Settings } from "../../lib/types";
import { useSettings } from "../../lib/useSettings";
import { openClearData, openCustomize, openSettingsSection, togglePanel } from "../../lib/ui";
import { openInternalPage } from "../chrome/Rail";
import { Icon } from "../ui/Icon";
import type { IconName } from "../ui/Icon";
import { IconButton } from "../ui/controls";
import { MenuHeading, MenuItem, MenuSeparator, Popover, usePopoverClose } from "../ui/Popover";

const run = (command: BrowserCommand) => void attempt(() => api.runCommand(command));

function Tile({ icon, label, hint, onSelect }: { icon: IconName; label: string; hint: string; onSelect: () => void }) {
  const close = usePopoverClose();
  return (
    <button
      type="button"
      role="menuitem"
      className="menu-tile"
      title={`${label} (${hint})`}
      onClick={() => {
        close();
        onSelect();
      }}
    >
      <Icon name={icon} size={18} />
      <span>{label}</span>
    </button>
  );
}

export function MainMenu({ open, onClose, anchor, onImport }: { open: boolean; onClose: () => void; anchor: RefObject<HTMLElement | null>; onImport: () => void }) {
  const tab = useStore(activeTab);
  const { update } = useSettings();
  const showBar = useStore((s) => s.settings.showBookmarksBar);
  const gaming = useStore((s) => s.settings.gamingMode);
  const vertical = useStore((s) => s.settings.verticalTabs);
  const web = !!tab && !tab.url.startsWith("browser://");
  const updateVersion = updateState.use((u) => (u.phase === "available" ? u.version : null));

  return (
    <Popover open={open} onClose={onClose} anchor={anchor} align="end" width={318} label="Main menu" className="menu main-menu">
      <div role="menu">
        {updateVersion && (
          <>
            <MenuItem icon="download" label={`Update available: ${updateVersion}`} onSelect={() => openSettingsSection("about")} />
            <MenuSeparator />
          </>
        )}
        <div className="menu-tiles">
          <Tile icon="plus" label="New tab" hint="Ctrl+T" onSelect={() => run({ type: "newTab" })} />
          <Tile icon="window" label="Window" hint="Ctrl+N" onSelect={() => run({ type: "newWindow" })} />
          <Tile icon="incognito" label="Private" hint="Ctrl+Shift+N" onSelect={() => run({ type: "newPrivateWindow" })} />
        </div>
        <MenuItem icon="incognito" label="New private tab" onSelect={() => run({ type: "newPrivateTab" })} />
        <MenuItem icon="history" label="Reopen closed tab or window" hint="Ctrl+Shift+T" onSelect={() => run({ type: "reopenTab" })} />
        <MenuSeparator />
        <MenuItem icon="bolt" label="Gaming Mode" hint="Ctrl+Shift+G" checked={gaming} onSelect={() => run({ type: "toggleGamingMode" })} />
        <MenuItem icon="gamepad" label="Gaming dashboard" hint="Ctrl+Shift+D" onSelect={() => run({ type: "openGaming" })} />
        <MenuItem icon="gauge" label="Performance monitor" hint="Shift+Esc" onSelect={() => run({ type: "openPerformance" })} />
        <MenuItem icon="sparkle" label="AI assistant" onSelect={toggleAi} />
        <MenuItem icon="flask" label="Research: compare pages" onSelect={() => openInternalPage("research")} />
        <MenuItem icon="brain" label="Project Memory" onSelect={() => openInternalPage("memory")} />
        <MenuSeparator />
        <MenuItem icon="bookmarks" label="Bookmarks" hint="Ctrl+Shift+O" onSelect={() => run({ type: "openBookmarks" })} />
        <MenuItem icon="history" label="History" hint="Ctrl+H" onSelect={() => run({ type: "openHistory" })} />
        <MenuItem icon="download" label="Downloads" hint="Ctrl+J" onSelect={() => run({ type: "openDownloads" })} />
        <MenuItem icon="sidebar" label="Bookmarks panel" onSelect={() => togglePanel("bookmarks")} />
        <MenuItem icon="key" label="Passwords and autofill" onSelect={() => openSettingsSection("passwords")} />
        <MenuItem icon="import" label="Import from Chrome, Edge or Brave…" onSelect={onImport} />
        <MenuItem icon="broom" label="Clear browsing data…" onSelect={openClearData} />
        <MenuSeparator />
        <MenuHeading>This page</MenuHeading>
        <div className="menu-zoom">
          <span>Zoom</span>
          <div className="zoom-controls">
            <IconButton icon="minus" label="Zoom out (Ctrl+-)" size={15} disabled={!web} onClick={() => run({ type: "zoomOut" })} />
            <button type="button" className="zoom-value" disabled={!web} onClick={() => run({ type: "zoomReset" })} title="Reset zoom (Ctrl+0)">
              {web && tab ? `${Math.round(tab.zoom * 100)}%` : "100%"}
            </button>
            <IconButton icon="plus" label="Zoom in (Ctrl+=)" size={15} disabled={!web} onClick={() => run({ type: "zoomIn" })} />
            <IconButton icon="fullscreen" label="Full screen (F11)" size={15} onClick={() => run({ type: "fullscreen" })} />
          </div>
        </div>
        <MenuItem icon="search" label="Find in page…" hint="Ctrl+F" disabled={!web} onSelect={() => run({ type: "find" })} />
        <MenuSeparator />
        <MenuHeading>Layout</MenuHeading>
        <MenuItem icon="star" label="Bookmarks bar" hint="Ctrl+Shift+B" checked={showBar} onSelect={() => run({ type: "toggleBookmarksBar" })} />
        <MenuItem icon="tabsVertical" label="Vertical tabs" checked={vertical} onSelect={() => void update({ verticalTabs: !vertical })} />
        <MenuItem icon="sliders" label="Customize toolbar…" onSelect={() => openCustomize("toolbar")} />
        <MenuItem icon="sidebar" label="Customize sidebar…" onSelect={() => openCustomize("sidebar")} />
        <MenuItem icon="palette" label="Themes and appearance…" onSelect={() => openSettingsSection("appearance")} />
        <MenuSeparator />
        <MenuItem icon="settings" label="Settings" hint="Ctrl+," onSelect={() => run({ type: "openSettings" })} />
        <MenuItem icon="close" label="Close window" hint="Ctrl+Shift+W" onSelect={() => run({ type: "closeWindow" })} />
        <MenuItem icon="power" label="Exit (close all windows)" onSelect={() => run({ type: "quit" })} />
      </div>
    </Popover>
  );
}

/** The main menu as a model for the menu window, with what each entry does. Same entries as `MainMenu`. */
export function mainMenuModel(onImport: () => void, update: (patch: Partial<Settings>) => unknown): { model: MenuModel; actions: Map<string, () => void> } {
  const s = getState();
  const tab = activeTab(s);
  const web = !!tab && !tab.url.startsWith("browser://");
  const u = updateState.get();
  const updateVersion = u.phase === "available" ? u.version : null;
  const actions = new Map<string, () => void>();
  let n = 0;
  const e = (label: string, icon: IconName | undefined, act: () => void, more: Partial<MenuEntry> = {}): MenuEntry => {
    const key = `m${n++}`;
    actions.set(key, act);
    return { key, label, icon, ...more };
  };
  const cmd = (label: string, icon: IconName | undefined, command: BrowserCommand, hint?: string, more: Partial<MenuEntry> = {}) =>
    e(label, icon, () => run(command), { hint, ...more });
  const zoom = web && tab ? `${Math.round(tab.zoom * 100)}%` : "100%";
  const sections: MenuModel["sections"] = [];
  if (updateVersion) sections.push({ entries: [e(`Update available: ${updateVersion}`, "download", () => openSettingsSection("about"))] });
  sections.push({
    entries: [
      cmd("New tab", "plus", { type: "newTab" }, "Ctrl+T"),
      cmd("New window", "window", { type: "newWindow" }, "Ctrl+N"),
      cmd("New private window", "incognito", { type: "newPrivateWindow" }, "Ctrl+Shift+N"),
      cmd("New private tab", "incognito", { type: "newPrivateTab" }),
      cmd("Reopen closed tab or window", "history", { type: "reopenTab" }, "Ctrl+Shift+T"),
    ],
  });
  sections.push({
    entries: [
      cmd("Gaming Mode", "bolt", { type: "toggleGamingMode" }, "Ctrl+Shift+G", { checked: s.settings.gamingMode }),
      cmd("Gaming dashboard", "gamepad", { type: "openGaming" }, "Ctrl+Shift+D"),
      cmd("Performance monitor", "gauge", { type: "openPerformance" }, "Shift+Esc"),
      e("AI assistant", "sparkle", toggleAi),
      e("Research: compare pages", "flask", () => openInternalPage("research")),
      e("Project Memory", "brain", () => openInternalPage("memory")),
    ],
  });
  sections.push({
    entries: [
      cmd("Bookmarks", "bookmarks", { type: "openBookmarks" }, "Ctrl+Shift+O"),
      cmd("History", "history", { type: "openHistory" }, "Ctrl+H"),
      cmd("Downloads", "download", { type: "openDownloads" }, "Ctrl+J"),
      e("Bookmarks panel", "sidebar", () => togglePanel("bookmarks")),
      e("Passwords and autofill", "key", () => openSettingsSection("passwords")),
      e("Import from Chrome, Edge or Brave…", "import", onImport),
      e("Clear browsing data…", "broom", openClearData),
    ],
  });
  sections.push({
    heading: "This page",
    entries: [cmd("Find in page…", "search", { type: "find" }, "Ctrl+F", { disabled: !web })],
  });
  sections.push({
    heading: "Layout",
    entries: [
      cmd("Bookmarks bar", "star", { type: "toggleBookmarksBar" }, "Ctrl+Shift+B", { checked: s.settings.showBookmarksBar }),
      e("Vertical tabs", "tabsVertical", () => void update({ verticalTabs: !s.settings.verticalTabs }), { checked: s.settings.verticalTabs }),
      e("Customize toolbar…", "sliders", () => openCustomize("toolbar")),
      e("Customize sidebar…", "sidebar", () => openCustomize("sidebar")),
      e("Themes and appearance…", "palette", () => openSettingsSection("appearance")),
    ],
  });
  sections.push({
    entries: [
      cmd("Settings", "settings", { type: "openSettings" }, "Ctrl+,"),
      cmd("Close window", "close", { type: "closeWindow" }, "Ctrl+Shift+W"),
      cmd("Exit (close all windows)", "power", { type: "quit" }),
    ],
  });
  // Zoom row on top, as in Chrome's menu.
  const navRow: MenuEntry[] = [
    cmd("Zoom out", "minus", { type: "zoomOut" }, "Ctrl+-", { disabled: !web }),
    cmd(zoom, undefined, { type: "zoomReset" }, "Ctrl+0", { disabled: !web }),
    cmd("Zoom in", "plus", { type: "zoomIn" }, "Ctrl+=", { disabled: !web }),
    cmd("Full screen", "fullscreen", { type: "fullscreen" }, "F11"),
  ];
  return { model: { title: null, navRow, sections }, actions };
}

let mainMenuOpen = false;
let mainMenuClosedAt = 0;

/**
 * Opens the main menu in the menu window under `anchor` (right edges aligned), over the live page: the page keeps
 * running while the menu is open. Resolves false when the menu window is not available; the caller then shows the menu
 * drawn by the shell.
 */
export async function openMainMenuSurface(anchor: HTMLElement, onImport: () => void, update: (patch: Partial<Settings>) => unknown): Promise<boolean> {
  // The press on the menu button while the menu is open already dismissed it (a click outside the menu window): that
  // click only closes the menu, as in Chrome.
  if (mainMenuOpen || performance.now() - mainMenuClosedAt < 300) return true;
  const { model, actions } = mainMenuModel(onImport, update);
  const r = anchor.getBoundingClientRect();
  mainMenuOpen = true;
  const handle = await openPopup(model, { x: r.right, y: r.bottom + 6, alignEnd: true }, { width: 340 });
  if (!handle) {
    mainMenuOpen = false;
    return false;
  }
  void handle.result.then((key) => {
    mainMenuOpen = false;
    mainMenuClosedAt = performance.now();
    const act = key ? actions.get(key) : undefined;
    act?.();
  });
  return true;
}
