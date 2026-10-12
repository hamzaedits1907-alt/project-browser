import { useState } from "react";
import type { RefObject } from "react";

import { api, errorMessage } from "../../lib/ipc";
import { hostOf } from "../../lib/format";
import { toast } from "../../lib/store";
import { useLoad } from "../../lib/useLoad";
import type { FillOptions, FillResult, Tab } from "../../lib/types";
import { fillMessage } from "../../lib/vault";
import { Icon } from "../ui/Icon";
import { Popover } from "../ui/Popover";

export function FillPanel({
  tab,
  options,
  open,
  onClose,
  anchor,
}: {
  tab: Tab;
  options: FillOptions;
  open: boolean;
  onClose: () => void;
  anchor: RefObject<HTMLElement | null>;
}) {
  const [busy, setBusy] = useState<string | null>(null);
  const profiles = useLoad(() => api.autofillProfiles(), open && options.profileCount > 0 ? `tab-${tab.id}` : null);

  const run = async (key: string, action: () => Promise<FillResult>) => {
    setBusy(key);
    try {
      toast(fillMessage(await action()));
      onClose();
    } catch (e) {
      toast(errorMessage(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <Popover open={open} onClose={onClose} anchor={anchor} width={340} label="Fill saved details" className="fill-pop">
      <div className="site-section">
        <div className="site-origin truncate" data-selectable>{hostOf(tab.url)}</div>
        <p className="dim small">Fills the form for you. Nothing is submitted until you do it.</p>
      </div>
      {options.logins.length > 0 && (
        <div className="site-section">
          <h4>Saved logins</h4>
          {options.logins.map((l) => (
            <button key={l.id} type="button" className="fill-item" disabled={busy !== null} onClick={() => void run(`l${l.id}`, () => api.fillLogin(tab.id, l.id))}>
              <Icon name="key" size={16} />
              <span className="truncate">{l.username || "(no user name)"}</span>
              <span className="fill-action">{busy === `l${l.id}` ? "Filling…" : "Fill"}</span>
            </button>
          ))}
        </div>
      )}
      {options.profileCount > 0 && (
        <div className="site-section">
          <h4>Saved addresses</h4>
          {(profiles.data ?? []).map((p) => (
            <button key={p.id} type="button" className="fill-item" disabled={busy !== null} onClick={() => void run(`p${p.id}`, () => api.fillProfile(tab.id, p.id))}>
              <Icon name="user" size={16} />
              <span className="truncate">{p.label}</span>
              <span className="fill-action">{busy === `p${p.id}` ? "Filling…" : "Fill"}</span>
            </button>
          ))}
          {profiles.loading && <p className="dim small">Loading…</p>}
          {profiles.error && <p className="field-error">{profiles.error}</p>}
        </div>
      )}
    </Popover>
  );
}
