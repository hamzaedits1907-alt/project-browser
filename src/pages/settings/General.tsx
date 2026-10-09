import { useState } from "react";

import { Button, Segmented, Select, TextField, Toggle } from "../../components/ui/controls";
import { Card, SettingRow } from "../../components/ui/Section";
import { SEARCH_ENGINES } from "../../lib/settings";
import { useSettings } from "../../lib/useSettings";
import type { SearchEngine, StartupBehavior } from "../../lib/types";

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
