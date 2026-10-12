import { useRef, useState } from "react";

import { activeTab, useStore } from "../../lib/store";
import { tabTitle } from "../../lib/urls";
import { TabSearch } from "../menus/TabSearch";
import { Icon } from "../ui/Icon";
import { Logo } from "../ui/Logo";
import { IconButton } from "../ui/controls";
import { TabStrip } from "./TabStrip";
import { WindowControls } from "./WindowControls";

/**
 * The window's title bar: brand mark, the tab strip (or, with vertical tabs, the page title), tab search and the
 * window buttons. Empty areas drag the window; double-clicking them maximises or restores it.
 */
export function TitleBar({ onShellMenu }: { onShellMenu: (x: number, y: number) => void }) {
  const searchRef = useRef<HTMLDivElement>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const privateWindow = useStore((s) => s.browser.privateWindow);
  const vertical = useStore((s) => s.settings.verticalTabs);
  const compact = useStore((s) => s.settings.compactTitleBar);
  const tab = useStore(activeTab);
  return (
    <header
      className={`titlebar${vertical ? " has-vertical-tabs" : ""}${compact ? " is-compact" : ""}`}
      data-tauri-drag-region
      onContextMenu={(e) => {
        if (e.target !== e.currentTarget && !(e.target as HTMLElement).hasAttribute("data-tauri-drag-region")) return;
        e.preventDefault();
        onShellMenu(e.clientX, e.clientY);
      }}
    >
      <div className="brand" data-tauri-drag-region title="Project Browser">
        <Logo size={20} />
        {(vertical || compact) && <span className="brand-name" data-tauri-drag-region>Project Browser</span>}
      </div>
      {vertical ? (
        <div className="titlebar-title truncate" data-tauri-drag-region>
          {tab ? tabTitle(tab) : ""}
        </div>
      ) : (
        <TabStrip />
      )}
      <div className="drag-fill" data-tauri-drag-region />
      {privateWindow && (
        <div className="private-badge" role="status" title="Nothing in this window is saved: no history, cookies or sessions outlive it.">
          <Icon name="incognito" size={15} /> Private
        </div>
      )}
      <div ref={searchRef} className="titlebar-tools">
        <IconButton icon="search" label="Search tabs" size={15} active={searchOpen} onClick={() => setSearchOpen((o) => !o)} />
      </div>
      <TabSearch open={searchOpen} onClose={() => setSearchOpen(false)} anchor={searchRef} />
      <WindowControls />
    </header>
  );
}
