import { hostOf } from "../../lib/format";
import { api } from "../../lib/ipc";
import { activeTab, attempt, useStore } from "../../lib/store";
import { Icon } from "../ui/Icon";
import { Button } from "../ui/controls";

/** Shown when the active page tried to open a pop-up nobody asked for. The pop-up was blocked, not lost. */
export function PopupBar() {
  const tab = useStore(activeTab);
  if (!tab?.blockedPopup) return null;
  const site = hostOf(tab.url);
  const decide = (action: "open" | "allow" | "dismiss") => void attempt(() => api.popupDecision(tab.id, action));
  return (
    <div className="permbar" role="alert" aria-label="Pop-up blocked">
      <span className="perm-icon"><Icon name="shield" size={18} /></span>
      <div className="perm-text">
        <strong>{site}</strong> tried to open a pop-up window on its own, so it was blocked.
        <span className="dim"> It wanted to go to {hostOf(tab.blockedPopup)}.</span>
      </div>
      <Button size="sm" onClick={() => decide("open")}>Open it once</Button>
      <Button size="sm" onClick={() => decide("allow")}>Always allow from {site}</Button>
      <Button size="sm" variant="ghost" onClick={() => decide("dismiss")}>Dismiss</Button>
    </div>
  );
}
