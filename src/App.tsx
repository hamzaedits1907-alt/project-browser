import { useCallback, useEffect, useState } from "react";

import { BookmarksBar } from "./components/chrome/BookmarksBar";
import { ContentHost } from "./components/chrome/ContentHost";
import { FindBar } from "./components/chrome/FindBar";
import { PermissionBar } from "./components/chrome/PermissionBar";
import { PopupBar } from "./components/chrome/PopupBar";
import { Rail } from "./components/chrome/Rail";
import { SidePanel } from "./components/chrome/SidePanel";
import { TabStrip } from "./components/chrome/TabStrip";
import { TitleBar } from "./components/chrome/TitleBar";
import { Toasts } from "./components/chrome/Toasts";
import { Toolbar } from "./components/chrome/Toolbar";
import { Logo } from "./components/ui/Logo";
import { Button } from "./components/ui/controls";
import { ClearDataDialog } from "./features/clear/ClearDataDialog";
import { ContextMenuHost } from "./features/contextmenu/ContextMenuHost";
import { CustomizeDialog } from "./features/customize/CustomizeDialog";
import { ShellMenu } from "./features/customize/ShellMenu";
import { AiPanel } from "./features/ai/AiPanel";
import { ai } from "./features/ai/aiStore";
import { checkForUpdates } from "./features/update/updateStore";
import { FirstRun } from "./features/firstrun/FirstRun";
import { ImportWizard } from "./features/import/ImportWizard";
import { applyLook } from "./lib/theme";
import { init, useStore } from "./lib/store";

export function App() {
  const ready = useStore((s) => s.ready);
  const fatal = useStore((s) => s.fatal);
  const firstRun = useStore((s) => s.firstRun);
  const settings = useStore((s) => s.settings);
  const fullscreen = useStore((s) => s.browser.fullscreen);
  const activeId = useStore((s) => s.browser.activeId);
  const aiOpen = ai.use((s) => s.open);
  const [shellMenu, setShellMenu] = useState<{ x: number; y: number } | null>(null);
  const openShellMenu = useCallback((x: number, y: number) => setShellMenu({ x, y }), []);

  useEffect(() => {
    void init();
  }, []);

  // One quiet look for a new version shortly after startup (announced with a toast, never installed unasked).
  const checkUpdates = settings.checkUpdates;
  useEffect(() => {
    if (!ready || !checkUpdates) return;
    const timer = setTimeout(() => void checkForUpdates(true), 20_000);
    return () => clearTimeout(timer);
    // Only the first ready state matters: toggling the setting later does not re-run the startup check.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready]);

  // The look (theme, accent, shape, density, motion) lives on <html>, so every popup inherits it.
  useEffect(() => {
    if (ready) applyLook(settings);
  }, [ready, settings]);

  // The shell has no browser context menu of its own: the core shows ours for text fields (see native/context_menu.rs)
  // and the chrome's own areas open theirs.
  useEffect(() => {
    const block = (e: MouseEvent) => {
      const t = e.target as HTMLElement;
      if (!t.closest("input, textarea, [data-selectable]")) e.preventDefault();
    };
    document.addEventListener("contextmenu", block);
    return () => document.removeEventListener("contextmenu", block);
  }, []);

  if (fatal) {
    return (
      <div className="fatal" role="alert">
        <Logo size={44} glow />
        <h1>Project Browser couldn't start</h1>
        <p className="dim" data-selectable>
          {fatal}
        </p>
        <Button variant="primary" onClick={() => location.reload()}>
          Try again
        </Button>
      </div>
    );
  }
  if (!ready) {
    return (
      <div className="boot" aria-busy="true">
        <Logo size={40} />
      </div>
    );
  }

  const chrome = !fullscreen;
  const sidebar = chrome && settings.sidebar !== "hidden";
  const vertical = chrome && settings.verticalTabs;
  return (
    <div className={`app${fullscreen ? " is-fullscreen" : ""}`}>
      {chrome && <TitleBar onShellMenu={openShellMenu} />}
      <div className="frame">
        {sidebar && <Rail onShellMenu={openShellMenu} />}
        {vertical && (
          <aside className="vtabs" aria-label="Tabs">
            <TabStrip vertical />
          </aside>
        )}
        <div className="main">
          {chrome && <Toolbar onShellMenu={openShellMenu} />}
          {chrome && settings.showBookmarksBar && <BookmarksBar />}
          <FindBar key={activeId ?? 0} />
          <PermissionBar />
          <PopupBar />
          <div className="workspace">
            {chrome && <SidePanel />}
            <ContentHost />
            {aiOpen && chrome && <AiPanel />}
          </div>
        </div>
      </div>
      <Toasts />
      <ContextMenuHost />
      <ShellMenu at={shellMenu} onClose={() => setShellMenu(null)} />
      <CustomizeDialog />
      <ImportWizard />
      <ClearDataDialog />
      {firstRun && <FirstRun />}
    </div>
  );
}
