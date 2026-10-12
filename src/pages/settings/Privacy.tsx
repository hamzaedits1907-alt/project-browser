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

      <Card
        title="Pop-ups and new windows"
        subtitle="A page can ask for a new window. Sign-in windows that a click opens always work and stay signed in to the same profile; a pop-up that no click asked for is blocked."
      >
        <SettingRow title="A link or page asks for a new window" description="Pop-ups that request a size (most sign-in windows) stay small pop-up windows either way, so the page that opened them keeps working.">
          <Select
            label="Where new-window requests open"
            value={settings.newWindowRequests}
            options={[
              { value: "tab", label: "In a new tab" },
              { value: "window", label: "In a new browser window" },
            ]}
            onChange={(newWindowRequests) => void update({ newWindowRequests })}
          />
        </SettingRow>
        {settings.popupAllowedHosts.length === 0 ? (
          <p className="dim">No site is allowed to open pop-ups on its own. When one tries, a bar appears with the choice.</p>
        ) : (
          <ul className="shield-sites" aria-label="Sites allowed to open pop-ups on their own">
            {settings.popupAllowedHosts.map((host) => (
              <li key={host}>
                <span data-selectable>{host}</span>
                <Button size="sm" variant="ghost" onClick={() => void update({ popupAllowedHosts: settings.popupAllowedHosts.filter((h) => h !== host) })}>
                  Block again
                </Button>
              </li>
            ))}
          </ul>
        )}
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
