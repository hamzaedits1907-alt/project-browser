import { useCallback, useEffect, useState } from "react";

import { Button, Segmented, Select, TextField, Toggle } from "../../components/ui/controls";
import { Card, SettingRow } from "../../components/ui/Section";
import { api, errorMessage } from "../../lib/ipc";
import { SEARCH_ENGINES } from "../../lib/settings";
import { useSettings } from "../../lib/useSettings";
import type { DefaultBrowserStatus, SearchEngine, StartupBehavior } from "../../lib/types";

/** What the default-browser card says, from the real Windows registration. */
export function describeDefault(s: DefaultBrowserStatus | null): { title: string; detail: string } {
  if (!s) return { title: "Checking…", detail: "Reading the Windows registration." };
  if (s.defaultHttp && s.defaultHttps) return { title: "Project Browser is your default browser", detail: "Links from other apps open here in a new tab." };
  if (s.defaultHttp || s.defaultHttps)
    return { title: "Partly the default", detail: `Windows opens ${s.defaultHttps ? "https" : "http"} links here but not ${s.defaultHttps ? "http" : "https"} links. Choose Project Browser for both in Windows Settings.` };
  if (!s.registered)
    return { title: "Not registered with Windows", detail: "This copy was not installed with the installer, so Windows does not list it as a browser. Install Project Browser to make it available." };
  if (!s.registeredHere)
    return { title: "Registered to another copy", detail: "Windows lists Project Browser, but its registration starts a different copy than this one." };
  return { title: "Not your default browser", detail: "Windows lists Project Browser as a browser. Choose it in Windows Settings to open links here." };
}

function DefaultBrowserCard() {
  const [status, setStatus] = useState<DefaultBrowserStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const refresh = useCallback(() => {
    api
      .defaultBrowserStatus()
      .then((s) => {
        setStatus(s);
        setError(null);
      })
      .catch((e) => setError(errorMessage(e)));
  }, []);
  useEffect(() => {
    refresh();
    // Coming back from Windows Settings: read the choice again.
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, [refresh]);
  const d = describeDefault(status);
  const isDefault = !!status && status.defaultHttp && status.defaultHttps;
  return (
    <Card title="Default browser">
      <SettingRow title={d.title} description={error ?? d.detail}>
        {!isDefault && (
          <Button
            disabled={!status}
            onClick={() =>
              void api
                .defaultBrowserOpenSettings()
                .then(() => setError(null))
                .catch((e) => setError(errorMessage(e)))
            }
          >
            Open Windows settings
          </Button>
        )}
      </SettingRow>
    </Card>
  );
}

export function isValidSearchTemplate(t: string): boolean {
  return t.includes("%s") && /^https?:\/\//i.test(t.trim());
}

export function General() {
  const { settings, update } = useSettings();
  const [template, setTemplate] = useState(settings.customSearchTemplate ?? "");
  const [templateError, setTemplateError] = useState<string | null>(null);
  const [dir, setDir] = useState(settings.downloadDir ?? "");
  const [dirError, setDirError] = useState<string | null>(null);

  const saveTemplate = async () => {
    const t = template.trim();
    if (t && !isValidSearchTemplate(t)) {
      setTemplateError("Must start with https:// and contain %s where the search words go.");
      return;
    }
    setTemplateError(null);
    await update({ customSearchTemplate: t || null });
  };

  const saveDir = async (value: string) => {
    const v = value.trim();
    const ok = await update({ downloadDir: v || null });
    setDirError(ok ? null : "That folder doesn't exist. Enter a full path such as D:\\Downloads.");
    if (ok && !v) setDir("");
  };

  return (
    <>
      <DefaultBrowserCard />

      <Card title="On startup">
        <SettingRow title="When the browser opens" description="Pinned tabs always come back.">
          <Segmented<StartupBehavior>
            label="Startup behaviour"
            value={settings.startup}
            options={[
              { value: "restoreSession", label: "Restore my tabs" },
              { value: "newTab", label: "Open a new tab" },
            ]}
            onChange={(startup) => void update({ startup })}
          />
        </SettingRow>
      </Card>

      <Card title="Search">
        <SettingRow title="Default search engine" description="Used for anything typed in the address bar that isn't a web address.">
          <Select<SearchEngine>
            label="Search engine"
            value={settings.searchEngine}
            options={SEARCH_ENGINES.map((e) => ({ value: e.id, label: e.label }))}
            onChange={(searchEngine) => void update({ searchEngine })}
          />
        </SettingRow>
        <SettingRow title="Custom search address" description="Optional. Overrides the engine above." stacked>
          <div className="inline-form">
            <TextField
              label="Custom search URL"
              placeholder="https://example.com/search?q=%s"
              value={template}
              error={templateError}
              onChange={(e) => setTemplate(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && void saveTemplate()}
            />
            <Button onClick={() => void saveTemplate()}>Save</Button>
          </div>
        </SettingRow>
      </Card>

      <Card title="Toolbar">
        <SettingRow title="Show bookmarks bar" description="Ctrl+Shift+B toggles it anywhere.">
          <Toggle label="Show bookmarks bar" checked={settings.showBookmarksBar} onChange={(showBookmarksBar) => void update({ showBookmarksBar })} />
        </SettingRow>
      </Card>

      <Card title="Downloads">
        <SettingRow title="Save files to" description={settings.downloadDir ? "Custom folder" : "Your Windows Downloads folder"} stacked>
          <div className="inline-form">
            <TextField
              label="Download folder"
              placeholder="Default (Downloads)"
              value={dir}
              error={dirError}
              onChange={(e) => setDir(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && void saveDir(dir)}
            />
            <Button onClick={() => void saveDir(dir)}>Save</Button>
            {settings.downloadDir && (
              <Button
                variant="ghost"
                onClick={() => {
                  setDir("");
                  void saveDir("");
                }}
              >
                Use default
              </Button>
            )}
          </div>
        </SettingRow>
      </Card>
    </>
  );
}
