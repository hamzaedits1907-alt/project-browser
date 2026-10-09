import { useEffect, useState } from "react";

import { Button } from "../../components/ui/controls";
import { Card, Notice, SettingRow } from "../../components/ui/Section";
import { api, errorMessage } from "../../lib/ipc";
import type { AppInfo, UpdateStatus } from "../../lib/types";

export function About() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.appInfo().then(setInfo).catch((e) => setError(errorMessage(e)));
  }, []);

  const check = async () => {
    setChecking(true);
    setError(null);
    try {
      setStatus(await api.updateCheck());
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setChecking(false);
    }
  };

  const install = async () => {
    setInstalling(true);
    try {
      await api.updateInstall(); // restarts the browser when finished
    } catch (e) {
      setInstalling(false);
      setError(errorMessage(e));
    }
  };

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
        </dl>
      </Card>

      <Card title="Updates">
        {info && !info.updateConfigured ? (
          <Notice tone="info">This build isn't connected to an update server, so it can't check for updates. Install a newer installer manually to upgrade; your data is kept.</Notice>
        ) : (
          <SettingRow title="Check for updates" description={status?.status === "upToDate" ? `You're on the latest version (${status.current}).` : undefined}>
            <Button icon="refresh" disabled={checking || !info} onClick={() => void check()}>{checking ? "Checking…" : "Check now"}</Button>
          </SettingRow>
        )}
        {status?.status === "available" && (
          <Notice tone="good">
            Version {status.version} is available (you have {status.current}).{status.notes ? ` ${status.notes}` : ""}
            <div className="row-actions">
              <Button variant="primary" disabled={installing} onClick={() => void install()}>{installing ? "Installing…" : "Install and restart"}</Button>
            </div>
          </Notice>
        )}
        {error && <Notice tone="bad">{error}</Notice>}
      </Card>
    </>
  );
}
