import { useState } from "react";

import { Badge, Button, Segmented, Toggle } from "../../components/ui/controls";
import { Card, Notice, SettingRow } from "../../components/ui/Section";
import { installFromStore, openOptions, reloadExtensions, togglePinned, useExtensions, usePinned } from "../../features/extensions/extensions";
import { api } from "../../lib/ipc";
import { attempt, toast } from "../../lib/store";
import type { DevtoolsDock, ExtensionDetails } from "../../lib/types";
import { useSettings } from "../../lib/useSettings";

function Row({ ext }: { ext: ExtensionDetails }) {
  const pinned = usePinned();
  const after = () => void reloadExtensions();
  return (
    <SettingRow
      title={ext.name || ext.id}
      description={
        <span className="ext-desc">
          {ext.icon && <img className="ext-icon" src={ext.icon} width={20} height={20} alt="" />}
          <span className="truncate">{ext.description ?? (ext.storeId ? "From the Chrome Web Store" : "Loaded from a folder")}</span>
          {ext.version && <span className="muted"> · {ext.version}</span>}
        </span>
      }
    >
      <div className="inline-form">
        {!ext.enabled && <Badge>Off</Badge>}
        {ext.options && (
          <Button variant="ghost" size="sm" icon="settings" onClick={() => void openOptions(ext)}>
            Options
          </Button>
        )}
        <Button variant="ghost" size="sm" icon={pinned.includes(ext.id) ? "pinFilled" : "pin"} onClick={() => togglePinned(ext.id)}>
          {pinned.includes(ext.id) ? "Pinned" : "Pin"}
        </Button>
        <Toggle label={`Enable ${ext.name}`} checked={ext.enabled} onChange={(on) => void attempt(() => api.extensionSetEnabled(ext.id, on), "Could not change the extension").then(after)} />
        <Button variant="ghost" size="sm" icon="trash" aria-label={`Remove ${ext.name}`} onClick={() => void attempt(() => api.extensionRemove(ext.id), "Could not remove the extension").then(after)}>
          Remove
        </Button>
      </div>
    </SettingRow>
  );
}

function StoreInstall() {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const id = /[a-p]{32}/.exec(text)?.[0] ?? null;
  return (
    <SettingRow title="Install from the Chrome Web Store" description="Open an extension's page in the store and press “Add to Project Browser”, or paste its store address here." stacked>
      <div className="inline-form">
        <input className="input" placeholder="https://chromewebstore.google.com/detail/…" value={text} onChange={(e) => setText(e.target.value)} aria-label="Chrome Web Store address" />
        <Button
          variant="primary"
          icon="plus"
          disabled={!id || busy}
          onClick={() => {
            if (!id) return;
            setBusy(true);
            void installFromStore(id).then((ok) => ok && setText("")).finally(() => setBusy(false));
          }}
        >
          {busy ? "Installing…" : "Install"}
        </Button>
        <Button icon="globe" onClick={() => void attempt(() => api.tabNew("https://chromewebstore.google.com/"))}>Open the store</Button>
      </div>
    </SettingRow>
  );
}

function DevtoolsCard() {
  const { settings, update } = useSettings();
  return (
    <Card title="Developer tools">
      <SettingRow title="Where DevTools opens" description="F12, Ctrl+Shift+I or “Inspect” in the right-click menu. Docked DevTools belongs to its tab, like in Chrome; drag its bar to resize it.">
        <Segmented<DevtoolsDock>
          label="DevTools position"
          value={settings.devtoolsDock}
          options={[
            { value: "bottom", label: "Bottom" },
            { value: "right", label: "Right" },
            { value: "window", label: "Own window" },
          ]}
          onChange={(devtoolsDock) => void update({ devtoolsDock })}
        />
      </SettingRow>
    </Card>
  );
}

/** Chrome extensions: from the Chrome Web Store, from a .crx file, or unpacked from a folder (all run by WebView2). */
export function ExtensionsSettings() {
  const { list, loaded } = useExtensions();

  const addUnpacked = async () => {
    const added = await attempt(() => api.extensionAdd(), "Could not add the extension");
    if (added) {
      toast(`${added.name} added`);
      await reloadExtensions();
    }
  };
  const addCrx = async () => {
    const added = await attempt(() => api.extensionInstallCrx(), "Could not install the extension");
    if (added) {
      toast(`${added.name} added`);
      await reloadExtensions();
    }
  };

  return (
    <>
      <Card
        title="Extensions"
        actions={
          <div className="inline-form">
            <Button icon="file" onClick={() => void addCrx()}>Install .crx…</Button>
            <Button icon="folder" onClick={() => void addUnpacked()}>Load unpacked…</Button>
          </div>
        }
      >
        <StoreInstall />
        {!loaded && <p className="muted">Loading…</p>}
        {loaded && list.length === 0 && <p className="muted">No extensions installed.</p>}
        {list.map((ext) => (
          <Row key={ext.id} ext={ext} />
        ))}
      </Card>
      <Card title="How extensions work here">
        <Notice>
          Extensions run in every normal tab and stay installed after a restart; private tabs never run them. Their toolbar pop-ups open from the extensions button (or a pinned icon).
          Store installs download the same package Chrome would, straight from Google, and only when you ask.
        </Notice>
        <Notice tone="warn">
          Pages are rendered by Microsoft Edge WebView2. A few extension features need parts of Chrome WebView2 does not have (for example some <code>chrome.tabs</code> and side
          panel APIs), so a small number of extensions only partly work.
        </Notice>
      </Card>
      <DevtoolsCard />
    </>
  );
}
