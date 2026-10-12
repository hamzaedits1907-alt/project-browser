import { Badge, Button, Toggle } from "../../components/ui/controls";
import { Card, Notice, SettingRow } from "../../components/ui/Section";
import { api } from "../../lib/ipc";
import { attempt, toast } from "../../lib/store";
import { useLoad } from "../../lib/useLoad";

/**
 * Unpacked Chromium extensions, loaded by WebView2 itself (see native/extensions.rs). Nothing here pretends to be the
 * Chrome Web Store: the user points at an extension folder they already have.
 */
export function ExtensionsSettings() {
  const list = useLoad(() => api.extensionsList(), "extensions");

  const add = async () => {
    const added = await attempt(() => api.extensionAdd(), "Could not add the extension");
    if (added) {
      toast(`${added.name} added`);
      list.reload();
    }
  };

  return (
    <>
      <Card title="Extensions" actions={<Button icon="plus" onClick={() => void add()}>Load unpacked…</Button>}>
        <Notice>
          Choose the <code>manifest.json</code> inside an unpacked extension folder. Extensions run in every normal tab and stay installed after a restart. Private tabs never run
          them.
        </Notice>
        {list.loading && <p className="muted">Loading…</p>}
        {list.error && <Notice tone="bad">{list.error}</Notice>}
        {list.data && list.data.length === 0 && <p className="muted">No extensions installed.</p>}
        {list.data?.map((ext) => (
          <SettingRow key={ext.id} title={ext.name || ext.id} description={<span className="mono truncate">{ext.id}</span>}>
            <div className="inline-form">
              {!ext.enabled && <Badge>Off</Badge>}
              <Toggle
                label={`Enable ${ext.name}`}
                checked={ext.enabled}
                onChange={(on) => void attempt(() => api.extensionSetEnabled(ext.id, on), "Could not change the extension").then(() => list.reload())}
              />
              <Button
                variant="ghost"
                icon="trash"
                aria-label={`Remove ${ext.name}`}
                onClick={() => void attempt(() => api.extensionRemove(ext.id), "Could not remove the extension").then(() => list.reload())}
              >
                Remove
              </Button>
            </div>
          </SettingRow>
        ))}
      </Card>
      <Card title="What works">
        <Notice tone="warn">
          Extensions are run by Microsoft Edge WebView2, which supports unpacked Manifest V3 extensions (content scripts, background service workers, blocking rules). It
          has no Chrome Web Store, cannot install <code>.crx</code> files, and has no extension toolbar, so extension pop-up windows cannot be opened. Extensions that only
          work through their pop-up are of limited use here.
        </Notice>
      </Card>
    </>
  );
}
