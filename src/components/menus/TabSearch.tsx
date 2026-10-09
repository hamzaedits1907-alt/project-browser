import { useEffect, useRef, useState } from "react";
import type { RefObject } from "react";

import { formatRelative } from "../../lib/format";
import { api } from "../../lib/ipc";
import { attempt, useStore } from "../../lib/store";
import { filterTabs } from "../../lib/tabs";
import { monogram, tabTitle } from "../../lib/urls";
import { Favicon } from "../ui/Favicon";
import { Icon } from "../ui/Icon";
import { Popover } from "../ui/Popover";

export function TabSearch({ open, onClose, anchor }: { open: boolean; onClose: () => void; anchor: RefObject<HTMLElement | null> }) {
  const tabs = useStore((s) => s.browser.tabs);
  const activeId = useStore((s) => s.browser.activeId);
  const [q, setQ] = useState("");
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    const t = setTimeout(() => input.current?.focus(), 30);
    return () => clearTimeout(t);
  }, [open]);

  const close = () => {
    setQ("");
    onClose();
  };

  const list = filterTabs(tabs, q);
  return (
    <Popover open={open} onClose={close} anchor={anchor} align="end" width={360} label="Search tabs" className="tabsearch">
      <div className="tabsearch-input">
        <Icon name="search" size={16} />
        <input ref={input} value={q} onChange={(e) => setQ(e.target.value)} placeholder={`Search ${tabs.length} tabs`} aria-label="Search tabs" />
      </div>
      <div className="pop-list">
        {list.length === 0 && <p className="dim small pad">No tabs match “{q}”.</p>}
        {list.map((t) => (
          <button
            key={t.id}
            type="button"
            className={`tabsearch-row${t.id === activeId ? " is-active" : ""}`}
            onClick={() => {
              close();
              void attempt(() => api.tabActivate(t.id));
            }}
          >
            <Favicon src={t.favicon} fallback={monogram(t)} size={16} />
            <span className="truncate ts-title">{tabTitle(t)}</span>
            {t.lifecycle === "sleeping" && <Icon name="moon" size={13} />}
            {t.audible && <Icon name="volume" size={13} />}
            <span className="ts-meta">{t.id === activeId ? "Current" : formatRelative(t.lastActive)}</span>
          </button>
        ))}
      </div>
    </Popover>
  );
}
