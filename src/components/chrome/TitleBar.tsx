import { useRef, useState } from "react";

import { useStore } from "../../lib/store";
import { TabSearch } from "../menus/TabSearch";
import { Icon } from "../ui/Icon";
import { IconButton } from "../ui/controls";
import { TabStrip } from "./TabStrip";
import { WindowControls } from "./WindowControls";

export function TitleBar() {
  const searchRef = useRef<HTMLDivElement>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const privateWindow = useStore((s) => s.browser.privateWindow);
  return (
    <div className="titlebar" data-tauri-drag-region>
      <div className="brand" data-tauri-drag-region aria-hidden="true">
        <svg viewBox="0 0 32 32" width="20" height="20">
          <defs>
            <linearGradient id="pb-logo" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0" stopColor="var(--accent-hi)" />
              <stop offset="1" stopColor="var(--accent-lo)" />
            </linearGradient>
          </defs>
          <rect x="3" y="3" width="26" height="26" rx="8" fill="url(#pb-logo)" />
          <path d="M11 22V10h6.2a3.6 3.6 0 010 7.2H11" fill="none" stroke="var(--on-accent)" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </div>
      <TabStrip />
      <div className="drag-fill" data-tauri-drag-region />
      {privateWindow && (
        <div className="private-badge" role="status" title="Nothing in this window is saved: no history, cookies or sessions outlive it.">
          <Icon name="incognito" size={15} /> Private
        </div>
      )}
      <div ref={searchRef} className="titlebar-tools">
        <IconButton icon="chevronDown" label="Search tabs" size={16} active={searchOpen} onClick={() => setSearchOpen((o) => !o)} />
      </div>
      <TabSearch open={searchOpen} onClose={() => setSearchOpen(false)} anchor={searchRef} />
      <WindowControls />
    </div>
  );
}
