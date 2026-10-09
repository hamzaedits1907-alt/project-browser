import { useEffect } from "react";

import { BookmarksBar } from "./components/chrome/BookmarksBar";
import { ContentHost } from "./components/chrome/ContentHost";
import { FindBar } from "./components/chrome/FindBar";
import { PermissionBar } from "./components/chrome/PermissionBar";
import { TitleBar } from "./components/chrome/TitleBar";
import { Toasts } from "./components/chrome/Toasts";
import { Toolbar } from "./components/chrome/Toolbar";
import { Button } from "./components/ui/controls";
import { ClearDataDialog } from "./features/clear/ClearDataDialog";
import { AiPanel } from "./features/ai/AiPanel";
import { ai } from "./features/ai/aiStore";
import { FirstRun } from "./features/firstrun/FirstRun";
import { ImportWizard } from "./features/import/ImportWizard";
import { accentOrDefault, motionReduced } from "./lib/settings";
import { init, useStore } from "./lib/store";

export function App() {
  const ready = useStore((s) => s.ready);
  const fatal = useStore((s) => s.fatal);
  const firstRun = useStore((s) => s.firstRun);
  const settings = useStore((s) => s.settings);
  const fullscreen = useStore((s) => s.browser.fullscreen);
  const activeId = useStore((s) => s.browser.activeId);
  const aiOpen = ai.use((s) => s.open);

  useEffect(() => {
    void init();
  }, []);

  // Theme attributes live on <html> so every portal-free popup inherits them.
  useEffect(() => {
    const root = document.documentElement;
    root.dataset.accent = accentOrDefault(settings.accent);
    root.dataset.gaming = String(settings.gamingMode);
    root.dataset.motion = motionReduced(settings) ? "reduced" : "full";
  }, [settings]);

  // Shell keys that are not browser commands: stop the webview's own context menu on chrome.
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
        <h1>Project Browser couldn't start</h1>
        <p className="dim" data-selectable>{fatal}</p>
        <Button variant="primary" onClick={() => location.reload()}>Try again</Button>
      </div>
    );
  }
  if (!ready) return <div className="boot" aria-busy="true" />;

  return (
    <div className={`app${fullscreen ? " is-fullscreen" : ""}${settings.gamingMode ? " is-gaming" : ""}`}>
      {!fullscreen && <TitleBar />}
      {!fullscreen && <Toolbar />}
      {!fullscreen && settings.showBookmarksBar && <BookmarksBar />}
      <FindBar key={activeId ?? 0} />
      <PermissionBar />
      <div className="workspace">
        <ContentHost />
        {aiOpen && !fullscreen && <AiPanel />}
      </div>
      <Toasts />
      <ImportWizard />
      <ClearDataDialog />
      {firstRun && <FirstRun />}
    </div>
  );
}
