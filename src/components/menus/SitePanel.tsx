import type { RefObject } from "react";

import { api } from "../../lib/ipc";
import { useLoad } from "../../lib/useLoad";
import { attempt } from "../../lib/store";
import type { Tab } from "../../lib/types";
import { hostOf, permissionName } from "../../lib/format";
import { originOf } from "../../lib/urls";
import { Icon } from "../ui/Icon";
import type { IconName } from "../ui/Icon";
import { IconButton, Select } from "../ui/controls";
import { Popover } from "../ui/Popover";

const COPY: Record<Tab["security"], { icon: IconName; title: string; body: string; tone: string }> = {
  secure: { icon: "lock", title: "Connection is secure", body: "Information you send to this site (like passwords or card numbers) is private.", tone: "good" },
  insecure: { icon: "warning", title: "Connection is not secure", body: "Don't enter sensitive information. Others on the network may be able to read or change what you send.", tone: "warn" },
  certerror: { icon: "warning", title: "Certificate problem", body: "This site's security certificate could not be verified.", tone: "bad" },
  local: { icon: "file", title: "Local file", body: "This page is stored on your computer.", tone: "neutral" },
  internal: { icon: "shieldCheck", title: "Project Browser page", body: "This page is built into the browser. It never leaves your computer.", tone: "good" },
};

export function SitePanel({ tab, open, onClose, anchor }: { tab: Tab | null; open: boolean; onClose: () => void; anchor: RefObject<HTMLElement | null> }) {
  const origin = tab ? originOf(tab.error?.url ?? tab.url) : null;
  const loaded = useLoad(() => api.sitePermissions(), open && origin ? origin : null);
  const perms = (loaded.data ?? []).filter((p) => p.origin === origin);
  const error = loaded.error;

  const setDecision = async (kind: string, decision: "allow" | "block" | "ask") => {
    if (!origin) return;
    await attempt(() => api.sitePermissionSet(origin, kind, decision === "ask" ? null : decision));
    loaded.reload();
  };

  if (!tab) return null;
  const info = COPY[tab.security];
  return (
    <Popover open={open} onClose={onClose} anchor={anchor} width={340} label="Site information" className="site-pop">
      <div className={`site-head tone-${info.tone}`}>
        <span className="site-icon"><Icon name={info.icon} size={20} /></span>
        <div>
          <h3>{info.title}</h3>
          <p className="dim">{info.body}</p>
        </div>
      </div>
      {origin && (
        <div className="site-section">
          <div className="site-origin truncate" data-selectable>{hostOf(origin)}</div>
          <h4>Permissions you've set</h4>
          {error && <p className="field-error">{error}</p>}
          {perms.length === 0 ? (
            <p className="dim small">No saved choices. The site will ask before using your camera, location or other features.</p>
          ) : (
            perms.map((p) => (
              <div key={p.kind} className="perm-row">
                <span>{permissionName(p.kind)}</span>
                <Select
                  label={`${permissionName(p.kind)} permission`}
                  value={p.decision}
                  options={[
                    { value: "allow", label: "Allow" },
                    { value: "block", label: "Block" },
                    { value: "ask", label: "Ask again" },
                  ]}
                  onChange={(v) => void setDecision(p.kind, v)}
                />
              </div>
            ))
          )}
        </div>
      )}
      {!tab.url.startsWith("browser://") && (
        <div className="site-section site-zoom">
          <span>Page zoom</span>
          <div className="zoom-controls">
            <IconButton icon="minus" label="Zoom out" size={15} onClick={() => void attempt(() => api.zoom("out", undefined, tab.id))} />
            <button type="button" className="zoom-value" onClick={() => void attempt(() => api.zoom("reset", undefined, tab.id))} title="Reset zoom">
              {Math.round(tab.zoom * 100)}%
            </button>
            <IconButton icon="plus" label="Zoom in" size={15} onClick={() => void attempt(() => api.zoom("in", undefined, tab.id))} />
          </div>
        </div>
      )}
    </Popover>
  );
}
