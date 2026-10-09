import { Button, Select, Toggle } from "../../components/ui/controls";
import { Card, EmptyState, Notice, SettingRow } from "../../components/ui/Section";
import { hostOf, permissionName } from "../../lib/format";
import { api } from "../../lib/ipc";
import { useLoad } from "../../lib/useLoad";
import { attempt } from "../../lib/store";
import { openClearData } from "../../lib/ui";
import type { SitePermission } from "../../lib/types";
import { useSettings } from "../../lib/useSettings";

export function Privacy() {
  const { settings, update } = useSettings();
  return (
    <>
      <Card title="Browsing data">
        <SettingRow title="Clear browsing data" description="History, cookies, cache, downloads list, saved passwords and autofill.">
          <Button icon="broom" onClick={openClearData}>Clear data…</Button>
        </SettingRow>
      </Card>

      <Card title="Passwords and autofill" subtitle="These use the web engine's built-in features. Data stays on this computer in your profile folder, encrypted by Windows.">
        <SettingRow title="Offer to save passwords">
          <Toggle label="Offer to save passwords" checked={settings.savePasswords} onChange={(savePasswords) => void update({ savePasswords })} />
        </SettingRow>
        <SettingRow title="Save and fill addresses and forms">
          <Toggle label="Autofill" checked={settings.autofill} onChange={(autofill) => void update({ autofill })} />
        </SettingRow>
        <Notice tone="info">Private tabs never save passwords, autofill data, history or site permissions.</Notice>
      </Card>

      <SitePermissions />
    </>
  );
}

function SitePermissions() {
  const loaded = useLoad(() => api.sitePermissions(), 0);
  const perms = loaded.data;
  const error = loaded.error;

  const set = async (p: SitePermission, decision: "allow" | "block" | null) => {
    await attempt(() => api.sitePermissionSet(p.origin, p.kind, decision));
    loaded.reload();
  };

  const byOrigin = new Map<string, SitePermission[]>();
  for (const p of perms ?? []) byOrigin.set(p.origin, [...(byOrigin.get(p.origin) ?? []), p]);

  return (
    <Card title="Site permissions" subtitle="Sites ask before using your camera, microphone, location and similar features. Your saved answers are listed here.">
      {error && <Notice tone="bad">{error}</Notice>}
      {perms && perms.length === 0 && <EmptyState icon="shield" title="No saved choices">When you tick “Remember” on a permission request, it shows up here.</EmptyState>}
      {[...byOrigin.entries()].map(([origin, list]) => (
        <div key={origin} className="perm-group">
          <div className="perm-origin truncate" data-selectable>{hostOf(origin)}</div>
          {list.map((p) => (
            <div key={p.kind} className="perm-row">
              <span>{permissionName(p.kind)}</span>
              <Select
                label={`${permissionName(p.kind)} for ${hostOf(origin)}`}
                value={p.decision}
                options={[
                  { value: "allow", label: "Allow" },
                  { value: "block", label: "Block" },
                ]}
                onChange={(v) => void set(p, v)}
              />
              <Button size="sm" variant="ghost" onClick={() => void set(p, null)}>Forget</Button>
            </div>
          ))}
        </div>
      ))}
    </Card>
  );
}
