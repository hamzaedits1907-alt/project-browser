import { useEffect, useState } from "react";

import { Button, ProgressBar, Spinner, Toggle } from "../../components/ui/controls";
import { Modal } from "../../components/ui/Modal";
import { Card, Notice, SettingRow } from "../../components/ui/Section";
import { askInstall, cancelInstallPrompt, checkForUpdates, dismissUpdateError, installUpdate, update } from "../../features/update/updateStore";
import { isBusy, progressOf } from "../../features/update/updateModel";
import { formatBytes, formatRelative } from "../../lib/format";
import { api, errorMessage } from "../../lib/ipc";
import { useSettings } from "../../lib/useSettings";
import type { AppInfo } from "../../lib/types";

export function About() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [infoError, setInfoError] = useState<string | null>(null);
  const { settings, update: saveSettings } = useSettings();
  const m = update.use((s) => s);
  const busy = isBusy(m.phase);
  const progress = progressOf(m);

  useEffect(() => {
    api.appInfo().then(setInfo).catch((e) => setInfoError(errorMessage(e)));
  }, []);

  return (
    <>
      <Card title="Project Browser">
        <dl className="about-list" data-selectable>
          <dt>Version</dt>
          <dd>{info?.version ?? "…"}</dd>
          <dt>Web engine</dt>
          <dd>{info ? (info.webview2Version ? `Microsoft Edge WebView2 ${info.webview2Version}` : "WebView2 version unavailable") : "…"}</dd>
          <dt>Profile</dt>
          <dd>{info?.profileName ?? "…"}</dd>
          <dt>Profile folder</dt>
          <dd className="mono">{info?.profileDir ?? "…"}</dd>
          <dt>App data</dt>
          <dd className="mono">{info?.dataDir ?? "…"}</dd>
          <dt>Log file</dt>
          <dd className="mono">{info ? `${info.dataDir}\\logs\\project-browser.log` : "…"}</dd>
        </dl>
        {infoError && <Notice tone="bad">{infoError}</Notice>}
      </Card>

      <Card title="Updates" subtitle="Updates come from the project's GitHub Releases and are checked against a signature before anything is installed.">
        {m.phase === "notConfigured" || (info && !info.updateConfigured) ? (
          <Notice tone="info">This build isn't connected to an update server, so it can't check for updates. Install a newer installer manually to upgrade; your data is kept.</Notice>
        ) : (
          <>
            <SettingRow
              title="Check for updates"
              description={
                m.phase === "checking"
                  ? "Checking…"
                  : m.phase === "upToDate"
                    ? `You're up to date (version ${m.current ?? info?.version ?? ""}).`
                    : m.checkedAt
                      ? `Last checked ${formatRelative(m.checkedAt)}.`
                      : `You have version ${info?.version ?? "…"}.`
              }
            >
              {m.phase === "checking" ? <Spinner size={18} /> : null}
              <Button icon="refresh" disabled={busy || !info} onClick={() => void checkForUpdates()}>
                {m.phase === "checking" ? "Checking…" : "Check now"}
              </Button>
            </SettingRow>

            {m.phase === "available" && (
              <Notice tone="good">
                <strong>Version {m.version} is available</strong> (you have {m.current}).
                {m.notes && (
                  <pre className="update-notes" data-selectable>
                    {m.notes}
                  </pre>
                )}
                <div className="row-actions">
                  <Button variant="primary" onClick={askInstall}>
                    Download and install…
                  </Button>
                </div>
              </Notice>
            )}

            {m.phase === "downloading" && (
              <div role="status" aria-live="polite">
                <p>
                  Downloading version {m.version}… {m.total ? `${formatBytes(m.downloaded)} of ${formatBytes(m.total)}` : m.downloaded > 0 ? formatBytes(m.downloaded) : ""}
                </p>
                <ProgressBar value={progress} />
                <p className="dim small">The download is checked against the release signature before it is installed.</p>
              </div>
            )}

            {m.phase === "installing" && (
              <div role="status" aria-live="polite">
                <p>Installing version {m.version}…</p>
                <ProgressBar value={null} />
                <p className="dim small">Project Browser will close and reopen by itself. Your tabs are restored.</p>
              </div>
            )}

            {m.phase === "error" && m.error && (
              <Notice tone="bad">
                {m.error}
                <div className="row-actions">
                  <Button onClick={() => void checkForUpdates()}>Try again</Button>
                  {m.version && <Button variant="ghost" onClick={dismissUpdateError}>Dismiss</Button>}
                </div>
              </Notice>
            )}
          </>
        )}
        <SettingRow title="Check when the browser starts" description="Looks once, quietly. A new version is only announced, never installed without asking.">
          <Toggle label="Check for updates at startup" checked={settings.checkUpdates} onChange={(checkUpdates) => void saveSettings({ checkUpdates })} />
        </SettingRow>
      </Card>

      <Modal
        open={m.confirming}
        title={`Install version ${m.version ?? ""}?`}
        onClose={cancelInstallPrompt}
        footer={
          <>
            <Button variant="ghost" onClick={cancelInstallPrompt}>Not now</Button>
            <Button variant="primary" onClick={() => void installUpdate()}>Install and restart</Button>
          </>
        }
      >
        <p>Project Browser will download version {m.version}, close, install it and open again.</p>
        <ul className="plain-list">
          <li>Your tabs, bookmarks, history and settings are kept.</li>
          <li>Finish or pause downloads first: running downloads stop when the browser closes.</li>
          <li>Text typed into web pages that has not been submitted may be lost.</li>
        </ul>
      </Modal>
    </>
  );
}
