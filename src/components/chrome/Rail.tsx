import { useState } from "react";

import { ai, toggleAi } from "../../features/ai/aiStore";
import { hostOf } from "../../lib/format";
import { api } from "../../lib/ipc";
import { normalizeSiteUrl } from "../../lib/settings";
import { cue } from "../../lib/sounds";
import { activeTab, attempt, getState, toast, useStore } from "../../lib/store";
import { openCustomize, openSettingsSection, togglePanel, ui } from "../../lib/ui";
import type { PanelId } from "../../lib/ui";
import type { PinnedSite } from "../../lib/types";
import { useSettings } from "../../lib/useSettings";
import { Favicon } from "../ui/Favicon";
import { Icon } from "../ui/Icon";
import type { IconName } from "../ui/Icon";
import { MenuItem, MenuSeparator, Popover } from "../ui/Popover";
import { cx } from "../ui/controls";

export interface RailEntry {
  id: string;
  label: string;
  icon: IconName;
  /** A docked panel (bookmarks, history, downloads, shield) or the AI panel, else a page or a settings section. */
  panel?: PanelId | "ai";
  page?: string;
  section?: string;
}

/** Everything the sidebar can show, in catalogue order (ids match `SIDEBAR_ITEMS` in settings.rs). */
export const RAIL_ENTRIES: RailEntry[] = [
  { id: "bookmarks", label: "Bookmarks", icon: "bookmarks", panel: "bookmarks" },
  { id: "history", label: "History", icon: "history", panel: "history" },
  { id: "downloads", label: "Downloads", icon: "download", panel: "downloads" },
  { id: "ai", label: "AI assistant", icon: "sparkle", panel: "ai" },
  { id: "memory", label: "Project Memory", icon: "brain", page: "memory" },
  { id: "research", label: "Research", icon: "flask", page: "research" },
  { id: "gaming", label: "Gaming dashboard", icon: "gamepad", page: "gaming" },
  { id: "shield", label: "Project Shield", icon: "shieldCheck", panel: "shield" },
  { id: "passwords", label: "Passwords & autofill", icon: "key", section: "passwords" },
  { id: "privacy", label: "Privacy & Tunnel", icon: "tunnel", section: "privacy" },
  { id: "performance", label: "Performance", icon: "gauge", page: "performance" },
];

/** Switches to the tab showing a browser page, reuses an empty New Tab for it, or opens it in a new tab. */
export function openInternalPage(page: string): void {
  const url = `browser://${page}`;
  const state = getState();
  const existing = state.browser.tabs.find((t) => t.url === url);
  if (existing) void attempt(() => api.tabActivate(existing.id));
  else if (activeTab(state)?.url === "browser://newtab") void attempt(() => api.navigate(url));
  else void attempt(() => api.tabNew(url));
}

function openEntry(entry: RailEntry): void {
  cue("tap");
  if (entry.panel === "ai") toggleAi();
  else if (entry.panel) togglePanel(entry.panel);
  else if (entry.page) openInternalPage(entry.page);
  else if (entry.section) openSettingsSection(entry.section);
}

function SiteButton({ site, expanded, onMenu }: { site: PinnedSite; expanded: boolean; onMenu: (site: PinnedSite, x: number, y: number) => void }) {
  const tabs = useStore((s) => s.browser.tabs);
  const host = hostOf(site.url);
  const open = tabs.find((t) => t.url.startsWith(site.url) || hostOf(t.url) === host);
  return (
    <button
      type="button"
      className={cx("rail-item rail-site", open && "is-open")}
      title={`${site.title || host}${open ? " (open)" : ""}`}
      onClick={() => void attempt(() => (open ? api.tabActivate(open.id) : api.tabNew(site.url)))}
      onContextMenu={(e) => {
        e.preventDefault();
        onMenu(site, e.clientX, e.clientY);
      }}
    >
      <span className="rail-icon">
        <Favicon src={open?.favicon} fallback={(site.title || host).charAt(0).toUpperCase()} size={18} />
      </span>
      {expanded && <span className="rail-label truncate">{site.title || host}</span>}
    </button>
  );
}

/** The sidebar: panels and pages the user chose, pinned sites, Gaming Mode, profile and settings. */
export function Rail({ onShellMenu }: { onShellMenu: (x: number, y: number) => void }) {
  const { settings, update } = useSettings();
  const panel = ui.use((s) => s.panel);
  const aiOpen = ai.use((s) => s.open);
  const tab = useStore(activeTab);
  const profile = useStore((s) => s.profile);
  const expanded = settings.sidebar === "expanded";
  const [siteMenu, setSiteMenu] = useState<{ site: PinnedSite; x: number; y: number } | null>(null);

  const entries = settings.sidebarItems.map((id) => RAIL_ENTRIES.find((e) => e.id === id)).filter((e): e is RailEntry => !!e);
  const isActive = (e: RailEntry) =>
    (e.panel === "ai" && aiOpen) || (!!e.panel && e.panel !== "ai" && panel === e.panel) || (!!e.page && tab?.url === `browser://${e.page}`);

  const pinCurrent = () => {
    const t = activeTab(getState());
    const url = t && !t.url.startsWith("browser://") && !t.private ? normalizeSiteUrl(t.url) : null;
    if (!url) {
      toast("Open a website first, then pin it here.");
      return;
    }
    const origin = new URL(url).origin + "/";
    if (settings.sidebarSites.some((s) => s.url === origin)) {
      toast("That site is already pinned.");
      return;
    }
    void update({ sidebarSites: [...settings.sidebarSites, { title: hostOf(origin).replace(/^www\./, ""), url: origin }] });
  };

  return (
    <nav
      className={cx("rail", expanded && "is-expanded")}
      aria-label="Sidebar"
      onContextMenu={(e) => {
        if ((e.target as HTMLElement).closest(".rail-site")) return;
        e.preventDefault();
        onShellMenu(e.clientX, e.clientY);
      }}
    >
      <div className="rail-section">
        {entries.map((e) => (
          <button
            key={e.id}
            type="button"
            className={cx("rail-item", isActive(e) && "is-active")}
            aria-pressed={e.panel ? isActive(e) : undefined}
            title={e.label}
            onClick={() => openEntry(e)}
          >
            <span className="rail-icon">
              <Icon name={e.icon} size={19} />
            </span>
            {expanded && <span className="rail-label">{e.label}</span>}
          </button>
        ))}
      </div>

      <div className="rail-divider" />
      <div className="rail-section rail-sites" aria-label="Pinned sites">
        {settings.sidebarSites.map((s) => (
          <SiteButton key={s.url} site={s} expanded={expanded} onMenu={(site, x, y) => setSiteMenu({ site, x, y })} />
        ))}
        <button type="button" className="rail-item rail-add" title="Pin the current site to the sidebar" onClick={pinCurrent}>
          <span className="rail-icon">
            <Icon name="plus" size={17} />
          </span>
          {expanded && <span className="rail-label">Pin this site</span>}
        </button>
      </div>

      <div className="rail-spacer" />

      <div className="rail-section rail-bottom">
        <button
          type="button"
          className={cx("rail-item rail-core", settings.gamingMode && "is-on")}
          aria-pressed={settings.gamingMode}
          title={settings.gamingMode ? "Gaming Mode is on (Ctrl+Shift+G)" : "Turn on Gaming Mode (Ctrl+Shift+G)"}
          onClick={() => {
            cue(settings.gamingMode ? "toggleOff" : "power");
            void attempt(() => api.gamingSet(!settings.gamingMode));
          }}
        >
          <span className="rail-icon">
            <Icon name="bolt" size={18} filled={settings.gamingMode} />
          </span>
          {expanded && <span className="rail-label">{settings.gamingMode ? "Gaming Mode on" : "Gaming Mode"}</span>}
        </button>
        <button type="button" className="rail-item" title={`Profile: ${profile?.name ?? ""}`} onClick={() => openSettingsSection("profiles")}>
          <span className="rail-icon">
            <span className="rail-avatar" style={{ background: profile?.color ?? "var(--accent)" }}>
              {(profile?.name ?? "?").charAt(0).toUpperCase()}
            </span>
          </span>
          {expanded && <span className="rail-label truncate">{profile?.name ?? "Profile"}</span>}
        </button>
        <button type="button" className={cx("rail-item", tab?.url === "browser://settings" && "is-active")} title="Settings (Ctrl+,)" onClick={() => void attempt(() => api.runCommand({ type: "openSettings" }))}>
          <span className="rail-icon">
            <Icon name="settings" size={19} />
          </span>
          {expanded && <span className="rail-label">Settings</span>}
        </button>
        <button
          type="button"
          className="rail-item rail-toggle"
          title={expanded ? "Collapse the sidebar" : "Show labels"}
          onClick={() => void update({ sidebar: expanded ? "rail" : "expanded" })}
        >
          <span className="rail-icon">
            <Icon name={expanded ? "chevronLeft" : "chevronRight"} size={16} />
          </span>
          {expanded && <span className="rail-label">Collapse</span>}
        </button>
      </div>

      <Popover open={!!siteMenu} onClose={() => setSiteMenu(null)} point={siteMenu ? { x: siteMenu.x, y: siteMenu.y } : null} width={230} label="Pinned site" className="menu">
        {siteMenu && (
          <div role="menu">
            <MenuItem icon="plus" label="Open in new tab" onSelect={() => void attempt(() => api.tabNew(siteMenu.site.url))} />
            <MenuItem icon="window" label="Open in new window" onSelect={() => void attempt(() => api.windowNew(siteMenu.site.url))} />
            <MenuSeparator />
            <MenuItem icon="sidebar" label="Customize sidebar…" onSelect={() => openCustomize("sidebar")} />
            <MenuItem icon="trash" danger label="Unpin from sidebar" onSelect={() => void update({ sidebarSites: settings.sidebarSites.filter((s) => s.url !== siteMenu.site.url) })} />
          </div>
        )}
      </Popover>
    </nav>
  );
}
