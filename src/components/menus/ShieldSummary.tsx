import { useState } from "react";

import { formatCount } from "../../features/shield/shieldModel";
import { api } from "../../lib/ipc";
import { activeTab, attempt, toast, useStore } from "../../lib/store";
import { useLoad } from "../../lib/useLoad";
import { Button, Spinner, Toggle } from "../ui/controls";
import { Notice } from "../ui/Section";

/**
 * What Project Shield did on the current page, and the switch for this site. Shared by the toolbar popover and the
 * sidebar panel. It loads only while shown (`active`); nothing runs while it is closed.
 */
export function ShieldSummary({ active, onReload }: { active: boolean; onReload?: () => void }) {
  const tab = useStore(activeTab);
  const blockedNow = useStore((s) => (s.browser.activeId === null ? 0 : (s.shieldBlocked[String(s.browser.activeId)] ?? 0)));
  const url = tab?.url ?? "";
  const tabId = tab?.id ?? null;
  const web = !!tab && !url.startsWith("browser://");
  const [changed, setChanged] = useState(false);

  const state = useLoad(() => api.shieldState(web ? url : undefined), active ? `${tabId}:${url}` : null);
  const summary = useLoad(() => (tabId === null || !web ? Promise.resolve(null) : api.shieldTab(tabId)), active ? `${tabId}:${url}:${blockedNow}` : null);
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

  if (state.loading && !view) {
    return (
      <div className="ai-status" role="status">
        <Spinner size={16} /> <span>Loading…</span>
      </div>
    );
  }
  if (state.error) return <Notice tone="bad">{state.error}</Notice>;
  if (!view) return null;
  return (
    <div className="shield-summary">
      <div className="shield-count">
        <strong className="num">{formatCount(web ? (summary.data?.blocked ?? blockedNow) : view.blockedSession)}</strong>
        <span className="dim">
          {!view.enabled
            ? "blocking is switched off"
            : !web
              ? "blocked since the browser started"
              : site && !site.enabled
                ? "not blocking on this site"
                : "blocked on this page"}
        </span>
      </div>
      {summary.data && summary.data.hosts.length > 0 && (
        <ul className="shield-hosts" aria-label="Most blocked addresses">
          {summary.data.hosts.slice(0, 6).map((h) => (
            <li key={h.host}>
              <span className="truncate" data-selectable>{h.host}</span>
              <span className="num">{formatCount(h.count)}</span>
            </li>
          ))}
        </ul>
      )}
      {site && view.enabled && (
        <div className="shield-site">
          <div className="shield-site-row">
            <span className="truncate">
              Block on <strong>{site.host}</strong>
            </span>
            <Toggle label={`Block ads and trackers on ${site.host}`} checked={site.enabled} onChange={(on) => void toggleSite(on)} />
          </div>
          {site.enabled && !changed && (
            <Button
              size="sm"
              variant="ghost"
              icon="refresh"
              title="Turns blocking off for this site only and reloads the page. You can switch it back on here."
              onClick={async () => {
                const next = await attempt(() => api.shieldSite(site.host, false));
                if (next === undefined) return;
                toast(`Project Shield is off on ${site.host}`);
                onReload?.();
                void attempt(() => api.nav("reload"));
              }}
            >
              Page broken? Allow this site and reload
            </Button>
          )}
          {changed && (
            <Button
              size="sm"
              onClick={() => {
                setChanged(false);
                onReload?.();
                void attempt(() => api.nav("reload"));
              }}
            >
              Reload to apply
            </Button>
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
      <p className="dim small shield-total">
        {formatCount(view.blockedSession)} blocked since start · {formatCount(view.blockedLifetime)} in total · {formatCount(view.engine.rules)} rules
      </p>
    </div>
  );
}
