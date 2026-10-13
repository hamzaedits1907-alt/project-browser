import { useRef, useState } from "react";

import { ai, toggleAi } from "../../features/ai/aiStore";
import { badgeText } from "../../features/shield/shieldModel";
import { api } from "../../lib/ipc";
import { cue } from "../../lib/sounds";
import { activeTab, attempt, useStore } from "../../lib/store";
import { openDialog, togglePanel, ui } from "../../lib/ui";
import { startScreenshot } from "../../features/dialogs/screenshotState";
import { openImportWizard } from "../../lib/ui";
import type { Download } from "../../lib/types";
import { MainMenu } from "../menus/MainMenu";
import { darkFor, toggleDark } from "../../features/sitetools/siteprefs";
import { trace } from "../../lib/trace";
import { ShieldPopover } from "../menus/ShieldPopover";
import { Icon } from "../ui/Icon";
import { IconButton, cx } from "../ui/controls";
import { Omnibox } from "./Omnibox";

/** Combined progress of the downloads that are running: 0..1, or null when none reports a size. */
export function downloadsProgress(downloads: Download[]): number | null {
  const active = downloads.filter((d) => d.state === "inprogress" || d.state === "paused");
  const sized = active.filter((d) => d.totalBytes && d.totalBytes > 0);
  if (sized.length === 0) return null;
  const total = sized.reduce((n, d) => n + (d.totalBytes ?? 0), 0);
  const got = sized.reduce((n, d) => n + d.receivedBytes, 0);
  return Math.max(0, Math.min(1, got / total));
}

function GamingToggle() {
  const gaming = useStore((s) => s.settings.gamingMode);
  return (
    <button
      type="button"
      className={cx("gaming-toggle", gaming && "is-on")}
      aria-pressed={gaming}
      aria-label="Gaming Mode"
      title={gaming ? "Gaming Mode is on (Ctrl+Shift+G)" : "Turn on Gaming Mode (Ctrl+Shift+G)"}
      onClick={() => {
        cue(gaming ? "toggleOff" : "power");
        void attempt(() => api.gamingSet(!gaming));
      }}
    >
      <Icon name="bolt" size={15} filled={gaming} />
      <span>{gaming ? "Gaming" : "Game"}</span>
    </button>
  );
}

function DownloadsButton() {
  const downloads = useStore((s) => s.downloads);
  const panel = ui.use((s) => s.panel);
  const running = downloads.filter((d) => d.state === "inprogress" || d.state === "paused").length;
  const progress = running > 0 ? downloadsProgress(downloads) : null;
  const R = 13;
  const C = 2 * Math.PI * R;
  return (
    <div className="tool-anchor">
      <IconButton icon="download" label="Downloads (Ctrl+J)" active={panel === "downloads"} onClick={() => togglePanel("downloads")} />
      {running > 0 && (
        <svg className="dl-ring" viewBox="0 0 32 32" aria-hidden="true">
          <circle cx="16" cy="16" r={R} className="dl-ring-track" />
          <circle
            cx="16"
            cy="16"
            r={R}
            className={cx("dl-ring-fill", progress === null && "is-indeterminate")}
            strokeDasharray={`${progress === null ? C * 0.25 : C * progress} ${C}`}
          />
        </svg>
      )}
      {running > 0 && <span className="tool-badge">{running}</span>}
    </div>
  );
}

function ShieldButton() {
  const tab = useStore(activeTab);
  const ref = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const blocked = useStore((s) => (s.browser.activeId === null ? 0 : (s.shieldBlocked[String(s.browser.activeId)] ?? 0)));
  const web = !!tab && !tab.url.startsWith("browser://");
  if (!web) return null;
  return (
    <div ref={ref} className="tool-anchor">
      <IconButton icon="shieldCheck" label="Project Shield: ads and trackers blocked on this page" active={open} onClick={() => setOpen((o) => !o)} />
      {badgeText(blocked) !== "" && <span className="tool-badge is-shield">{badgeText(blocked)}</span>}
      <ShieldPopover open={open && web} onClose={() => setOpen(false)} anchor={ref} />
    </div>
  );
}

/** Dark pages for this site: one click darkens a bright page, another click puts it back (Settings > Appearance). */
function DarkButton() {
  const tab = useStore(activeTab);
  const mode = useStore((s) => s.settings.darkPages);
  useStore((s) => s.browser.tabs);
  const web = !!tab && !tab.url.startsWith("browser://") && !tab.private;
  const on = web && darkFor(tab.url, mode);
  return (
    <IconButton
      icon="moon"
      label={on ? "Dark page: on for this site (click to restore the original look)" : "Make this page dark"}
      active={on}
      disabled={!web}
      onClick={() => tab && void attempt(() => toggleDark(tab))}
    />
  );
}

function SplitButton() {
  const split = useStore((s) => s.browser.split);
  const activeId = useStore((s) => s.browser.activeId);
  const tab = useStore(activeTab);
  const on = !!split && (split.left === activeId || split.right === activeId);
  const web = !!tab && !tab.url.startsWith("browser://");
  return (
    <IconButton
      icon="columns"
      label={on ? "Close split view (Ctrl+Shift+E)" : "Split view: show this page next to another tab (Ctrl+Shift+E)"}
      active={on}
      disabled={!on && !web}
      onClick={() => void attempt(() => api.splitView(on ? { op: "close" } : { op: "open" }))}
    />
  );
}

function ToolbarItem({ id }: { id: string }) {
  const aiOpen = ai.use((s) => s.open);
  const panel = ui.use((s) => s.panel);
  switch (id) {
    case "gaming":
      return <GamingToggle />;
    case "shield":
      return <ShieldButton />;
    case "ai":
      return <IconButton icon="sparkle" label="AI assistant" active={aiOpen} onClick={toggleAi} />;
    case "downloads":
      return <DownloadsButton />;
    case "bookmarks":
      return <IconButton icon="bookmarks" label="Bookmarks panel" active={panel === "bookmarks"} onClick={() => togglePanel("bookmarks")} />;
    case "history":
      return <IconButton icon="history" label="History panel" active={panel === "history"} onClick={() => togglePanel("history")} />;
    case "memory":
      return <IconButton icon="brain" label="Project Memory" onClick={() => void attempt(() => api.tabNew("browser://memory"))} />;
    case "performance":
      return <IconButton icon="gauge" label="Performance monitor (Shift+Esc)" onClick={() => void attempt(() => api.runCommand({ type: "openPerformance" }))} />;
    case "split":
      return <SplitButton />;
    case "reader":
      return <IconButton icon="book" label="Reader view and Listen (F9)" onClick={() => openDialog("reader")} />;
    case "screenshot":
      return <IconButton icon="crop" label="Screenshot (Ctrl+Shift+S)" onClick={() => void startScreenshot(false)} />;
    case "dark":
      return <DarkButton />;
    case "commands":
      return <IconButton icon="command" label="Command Center (Ctrl+K)" onClick={() => openDialog("palette")} />;
    case "privateWindow":
      return <IconButton icon="incognito" label="New private window (Ctrl+Shift+N)" onClick={() => void attempt(() => api.runCommand({ type: "newPrivateWindow" }))} />;
    default:
      return null;
  }
}

export function Toolbar({ onShellMenu }: { onShellMenu: (x: number, y: number) => void }) {
  const tab = useStore(activeTab);
  const items = useStore((s) => s.settings.toolbarItems);
  const menuRef = useRef<HTMLDivElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const web = !!tab && !tab.url.startsWith("browser://");
  const loading = !!tab?.loading;

  return (
    <div
      className="toolbar"
      role="toolbar"
      aria-label="Navigation"
      onContextMenu={(e) => {
        const t = e.target as HTMLElement;
        if (t.closest("input, button, .omnibox")) return;
        e.preventDefault();
        onShellMenu(e.clientX, e.clientY);
      }}
    >
      <div className="nav-group">
        <IconButton icon="back" label="Back (Alt+Left)" disabled={!tab?.canGoBack} onClick={() => void attempt(() => api.nav("back"))} />
        <IconButton icon="forward" label="Forward (Alt+Right)" disabled={!tab?.canGoForward} onClick={() => void attempt(() => api.nav("forward"))} />
        <IconButton
          icon={loading ? "cancel" : "reload"}
          label={loading ? "Stop loading (Esc)" : "Reload (F5)"}
          className={cx("reload-btn", loading && "is-loading")}
          disabled={!web && !loading}
          onClick={() => void attempt(() => api.nav(loading ? "stop" : "reload"))}
        />
      </div>

      <Omnibox />

      <div className="tool-group">
        {items.map((id) => (
          <ToolbarItem key={id} id={id} />
        ))}
        <div ref={menuRef} className="tool-anchor">
          <IconButton icon="dotsV" label="Menu" active={menuOpen} onPointerDown={() => trace("toolbar: menu button pressed")} onClick={() => { trace(`toolbar: menu button clicked (open: ${!menuOpen})`); setMenuOpen((o) => !o); }} />
        </div>
      </div>

      <MainMenu open={menuOpen} onClose={() => setMenuOpen(false)} anchor={menuRef} onImport={openImportWizard} />
    </div>
  );
}
