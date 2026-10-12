import type { RefObject } from "react";
import { useState } from "react";

import { formatCount } from "../../features/shield/shieldModel";
import { api } from "../../lib/ipc";
import { activeTab, attempt, toast, useStore } from "../../lib/store";
import { openSettingsSection } from "../../lib/ui";
import { useLoad } from "../../lib/useLoad";
import { Spinner, Toggle } from "../ui/controls";
import { Button } from "../ui/controls";
import { Popover } from "../ui/Popover";
import { Notice } from "../ui/Section";

/** What the Shield did on the current page, and the switch for this site. */
export function ShieldPopover({ open, onClose, anchor }: { open: boolean; onClose: () => void; anchor: RefObject<HTMLElement | null> }) {
  const tab = useStore(activeTab);
  const blockedNow = useStore((s) => (s.browser.activeId === null ? 0 : (s.shieldBlocked[String(s.browser.activeId)] ?? 0)));
  const url = tab?.url ?? "";
  const tabId = tab?.id ?? null;
  const [changed, setChanged] = useState(false);

  // Fetched when the popover opens (and again for another page); nothing runs while it is closed.
  const state = useLoad(() => api.shieldState(url), open ? `${tabId}:${url}` : null);
  const summary = useLoad(() => (tabId === null ? Promise.resolve(null) : api.shieldTab(tabId)), open ? `${tabId}:${url}:${blockedNow}` : null);
  const view = state.data;
  const site = view?.site ?? null;

  const toggleSite = async (enabled: boolean) => {
    if (!site) return;
    const next = await attempt(() => api.shieldSite(site.host, enabled));
    if (next) {
      setChanged(true);
      state.reload();
    }
  };

  const reload = () => {
    setChanged(false);
    onClose();
    void attempt(() => api.nav("reload"));
  };

  const close = () => {
    setChanged(false);
    onClose();
  };

  return (
    <Popover open={open} onClose={close} anchor={anchor} align="end" width={340} label="Project Shield" className="shield-pop">
      <div className="pop-head">
        <h3>Project Shield</h3>
        <Button size="sm" variant="ghost" onClick={() => { close(); openSettingsSection("shield"); }}>
          Settings
        </Button>
      </div>
      {state.loading && !view ? (
        <div className="ai-status" role="status"><Spinner size={16} /> <span>Loading…</span></div>
      ) : state.error ? (
        <Notice tone="bad">{state.error}</Notice>
      ) : view ? (
        <>
          <div className="shield-count">
            <strong>{formatCount(summary.data?.blocked ?? blockedNow)}</strong>
            <span className="dim">{!view.enabled ? "blocking is switched off" : site && !site.enabled ? "not blocking on this site" : "blocked on this page"}</span>
          </div>
          {summary.data && summary.data.hosts.length > 0 && (
            <ul className="shield-hosts" aria-label="Most blocked addresses">
              {summary.data.hosts.slice(0, 5).map((h) => (
                <li key={h.host}>
                  <span className="truncate" data-selectable>{h.host}</span>
                  <span>{formatCount(h.count)}</span>
                </li>
              ))}
            </ul>
          )}
          {site && view.enabled && (
            <div className="shield-site">
              <div className="shield-site-row">
                <span className="truncate">Block on <strong>{site.host}</strong></span>
                <Toggle label={`Block ads and trackers on ${site.host}`} checked={site.enabled} onChange={(on) => void toggleSite(on)} />
              </div>
              {changed && (
                <Button size="sm" onClick={reload}>Reload to apply</Button>
              )}
            </div>
          )}
          {!view.enabled && (
            <div className="shield-site">
              <Button size="sm" variant="primary" onClick={() => void attempt(() => api.shieldSet({ enabled: true })).then(() => { toast("Project Shield is on"); state.reload(); })}>
                Switch on
              </Button>
            </div>
          )}
        </>
      ) : null}
    </Popover>
  );
}
