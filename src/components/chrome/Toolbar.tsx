import { useRef, useState } from "react";

import { api } from "../../lib/ipc";
import { activeTab, attempt, useStore } from "../../lib/store";
import { openImportWizard } from "../../lib/ui";
import { ai, toggleAi } from "../../features/ai/aiStore";
import { badgeText } from "../../features/shield/shieldModel";
import { ShieldPopover } from "../menus/ShieldPopover";
import { DownloadsPopover } from "../menus/DownloadsPopover";
import { MainMenu } from "../menus/MainMenu";
import { Icon } from "../ui/Icon";
import { IconButton } from "../ui/controls";
import { Omnibox } from "./Omnibox";

export function Toolbar() {
  const tab = useStore(activeTab);
  const gaming = useStore((s) => s.settings.gamingMode);
  const downloads = useStore((s) => s.downloads);
  const aiOpen = ai.use((s) => s.open);
  const menuRef = useRef<HTMLDivElement>(null);
  const dlRef = useRef<HTMLDivElement>(null);
  const shieldRef = useRef<HTMLDivElement>(null);
  const blocked = useStore((s) => (s.browser.activeId === null ? 0 : (s.shieldBlocked[String(s.browser.activeId)] ?? 0)));
  const [menuOpen, setMenuOpen] = useState(false);
  const [dlOpen, setDlOpen] = useState(false);
  const [shieldOpen, setShieldOpen] = useState(false);

  const activeDownloads = downloads.filter((d) => d.state === "inprogress" || d.state === "paused").length;
  const web = !!tab && !tab.url.startsWith("browser://");
  const loading = !!tab?.loading;

  return (
    <div className="toolbar">
      <div className="nav-group">
        <IconButton icon="back" label="Back (Alt+Left)" disabled={!tab?.canGoBack} onClick={() => void attempt(() => api.nav("back"))} />
        <IconButton icon="forward" label="Forward (Alt+Right)" disabled={!tab?.canGoForward} onClick={() => void attempt(() => api.nav("forward"))} />
        <IconButton
          icon={loading ? "cancel" : "reload"}
          label={loading ? "Stop loading" : "Reload (F5)"}
          disabled={!web && !loading}
          onClick={() => void attempt(() => api.nav(loading ? "stop" : "reload"))}
        />
      </div>

      <Omnibox />

      <div className="tool-group">
        <button
          type="button"
          className={`gaming-toggle${gaming ? " is-on" : ""}`}
          aria-pressed={gaming}
          aria-label="Gaming Mode"
          title={gaming ? "Gaming Mode is on (Ctrl+Shift+G)" : "Turn on Gaming Mode (Ctrl+Shift+G)"}
          onClick={() => void attempt(() => api.gamingSet(!gaming))}
        >
          <Icon name="bolt" size={16} filled={gaming} />
          <span>{gaming ? "Gaming" : "Game"}</span>
        </button>
        {web && (
          <div ref={shieldRef} className="tool-anchor">
            <IconButton icon="shieldCheck" label="Project Shield: ads and trackers blocked on this page" active={shieldOpen} onClick={() => setShieldOpen((o) => !o)} />
            {badgeText(blocked) !== "" && <span className="tool-badge">{badgeText(blocked)}</span>}
          </div>
        )}
        <IconButton icon="sparkle" label="AI assistant" active={aiOpen} onClick={toggleAi} />
        <div ref={dlRef} className="tool-anchor">
          <IconButton icon="download" label="Downloads (Ctrl+J)" active={dlOpen} onClick={() => setDlOpen((o) => !o)} />
          {activeDownloads > 0 && <span className="tool-badge">{activeDownloads}</span>}
        </div>
        <div ref={menuRef} className="tool-anchor">
          <IconButton icon="menu" label="Menu" active={menuOpen} onClick={() => setMenuOpen((o) => !o)} />
        </div>
      </div>

      <ShieldPopover open={shieldOpen && web} onClose={() => setShieldOpen(false)} anchor={shieldRef} />
      <DownloadsPopover open={dlOpen} onClose={() => setDlOpen(false)} anchor={dlRef} />
      <MainMenu open={menuOpen} onClose={() => setMenuOpen(false)} anchor={menuRef} onImport={openImportWizard} />
    </div>
  );
}
