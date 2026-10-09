import type { RefObject } from "react";

import { api } from "../../lib/ipc";
import { attempt, activeTab, useStore } from "../../lib/store";
import type { BrowserCommand } from "../../lib/types";
import { toggleAi } from "../../features/ai/aiStore";
import { update } from "../../features/update/updateStore";
import { writeLocal } from "../../lib/storage";
import { closeWindow } from "../../lib/window";
import { IconButton } from "../ui/controls";
import { MenuHeading, MenuItem, MenuSeparator, Popover } from "../ui/Popover";

const run = (command: BrowserCommand) => void attempt(() => api.runCommand(command));

export function MainMenu({ open, onClose, anchor, onImport }: { open: boolean; onClose: () => void; anchor: RefObject<HTMLElement | null>; onImport: () => void }) {
  const tab = useStore(activeTab);
  const closedCount = useStore((s) => s.browser.closedCount);
  const showBar = useStore((s) => s.settings.showBookmarksBar);
  const gaming = useStore((s) => s.settings.gamingMode);
  const web = !!tab && !tab.url.startsWith("browser://");
  const updateVersion = update.use((u) => (u.phase === "available" ? u.version : null));

  return (
    <Popover open={open} onClose={onClose} anchor={anchor} align="end" width={300} label="Main menu" className="menu">
      <div role="menu">
        {updateVersion && (
          <>
            <MenuItem
              icon="download"
              label={`Update available: ${updateVersion}`}
              onSelect={() => {
                writeLocal("settings.section", "about");
                run({ type: "openSettings" });
              }}
            />
            <MenuSeparator />
          </>
        )}
        <MenuItem icon="plus" label="New tab" hint="Ctrl+T" onSelect={() => run({ type: "newTab" })} />
        <MenuItem icon="incognito" label="New private tab" hint="Ctrl+Shift+N" onSelect={() => run({ type: "newPrivateTab" })} />
        <MenuItem icon="history" label="Reopen closed tab" hint="Ctrl+Shift+T" disabled={closedCount === 0} onSelect={() => run({ type: "reopenTab" })} />
        <MenuSeparator />
        <MenuItem icon="bolt" label={gaming ? "Turn off Gaming Mode" : "Turn on Gaming Mode"} hint="Ctrl+Shift+G" checked={gaming} onSelect={() => run({ type: "toggleGamingMode" })} />
        <MenuItem icon="gamepad" label="Gaming dashboard" hint="Ctrl+Shift+D" onSelect={() => run({ type: "openGaming" })} />
        <MenuItem icon="gauge" label="Performance monitor" hint="Shift+Esc" onSelect={() => run({ type: "openPerformance" })} />
        <MenuItem icon="sparkle" label="AI assistant" onSelect={toggleAi} />
        <MenuSeparator />
        <MenuItem icon="history" label="History" hint="Ctrl+H" onSelect={() => run({ type: "openHistory" })} />
        <MenuItem icon="bookmarks" label="Bookmarks" hint="Ctrl+Shift+O" onSelect={() => run({ type: "openBookmarks" })} />
        <MenuItem icon="download" label="Downloads" hint="Ctrl+J" onSelect={() => run({ type: "openDownloads" })} />
        <MenuItem icon="star" label={showBar ? "Hide bookmarks bar" : "Show bookmarks bar"} hint="Ctrl+Shift+B" onSelect={() => run({ type: "toggleBookmarksBar" })} />
        <MenuItem icon="import" label="Import from Chrome, Edge or Brave…" onSelect={onImport} />
        <MenuSeparator />
        <MenuHeading>This page</MenuHeading>
        <div className="menu-zoom">
          <span>Zoom</span>
          <div className="zoom-controls">
            <IconButton icon="minus" label="Zoom out" size={15} disabled={!web} onClick={() => run({ type: "zoomOut" })} />
            <button type="button" className="zoom-value" disabled={!web} onClick={() => run({ type: "zoomReset" })} title="Reset zoom">
              {web && tab ? `${Math.round(tab.zoom * 100)}%` : "100%"}
            </button>
            <IconButton icon="plus" label="Zoom in" size={15} disabled={!web} onClick={() => run({ type: "zoomIn" })} />
            <IconButton icon="fullscreen" label="Full screen (F11)" size={15} onClick={() => run({ type: "fullscreen" })} />
          </div>
        </div>
        <MenuItem icon="search" label="Find in page…" hint="Ctrl+F" disabled={!web} onSelect={() => run({ type: "find" })} />
        <MenuSeparator />
        <MenuItem icon="settings" label="Settings" hint="Ctrl+," onSelect={() => run({ type: "openSettings" })} />
        <MenuItem icon="close" label="Exit" onSelect={() => void closeWindow()} />
      </div>
    </Popover>
  );
}
