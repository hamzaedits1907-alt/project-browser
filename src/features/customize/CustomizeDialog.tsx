import { RAIL_ENTRIES } from "../../components/chrome/Rail";
import { Icon } from "../../components/ui/Icon";
import type { IconName } from "../../components/ui/Icon";
import { Modal } from "../../components/ui/Modal";
import { Button, IconButton, Toggle } from "../../components/ui/controls";
import { DEFAULT_SIDEBAR_ITEMS, DEFAULT_TOOLBAR_ITEMS, SIDEBAR_ITEMS, TOOLBAR_ITEMS, moveItem, toggleItem } from "../../lib/settings";
import { closeCustomize, ui } from "../../lib/ui";
import { useSettings } from "../../lib/useSettings";

export const TOOLBAR_LABELS: Record<string, { label: string; icon: IconName; hint: string }> = {
  gaming: { label: "Gaming Mode switch", icon: "bolt", hint: "Ctrl+Shift+G" },
  shield: { label: "Project Shield", icon: "shieldCheck", hint: "Blocked count for this page" },
  ai: { label: "AI assistant", icon: "sparkle", hint: "Opens the assistant panel" },
  downloads: { label: "Downloads", icon: "download", hint: "With live progress" },
  bookmarks: { label: "Bookmarks panel", icon: "bookmarks", hint: "Docked next to the page" },
  history: { label: "History panel", icon: "history", hint: "Docked next to the page" },
  memory: { label: "Project Memory", icon: "brain", hint: "Saved notes and research" },
  performance: { label: "Performance monitor", icon: "gauge", hint: "Shift+Esc" },
  privateWindow: { label: "New private window", icon: "incognito", hint: "Ctrl+Shift+N" },
};

function OrderList({ catalog, chosen, label, onChange }: { catalog: readonly string[]; chosen: string[]; label: (id: string) => { label: string; icon: IconName; hint?: string }; onChange: (next: string[]) => void }) {
  // Shown items in the user's order first, then the hidden ones in catalogue order.
  const hidden = catalog.filter((id) => !chosen.includes(id));
  return (
    <ul className="order-list" aria-label="Items">
      {[...chosen, ...hidden].map((id) => {
        const on = chosen.includes(id);
        const meta = label(id);
        const at = chosen.indexOf(id);
        return (
          <li key={id} className={on ? "is-on" : "is-off"}>
            <span className="order-icon">
              <Icon name={meta.icon} size={17} />
            </span>
            <span className="order-text">
              <strong>{meta.label}</strong>
              {meta.hint && <small>{meta.hint}</small>}
            </span>
            <IconButton icon="arrowUp" label={`Move ${meta.label} up`} size={14} disabled={!on || at === 0} onClick={() => onChange(moveItem(chosen, id, -1))} />
            <IconButton icon="arrowDown" label={`Move ${meta.label} down`} size={14} disabled={!on || at === chosen.length - 1} onClick={() => onChange(moveItem(chosen, id, 1))} />
            <Toggle label={`Show ${meta.label}`} checked={on} onChange={(v) => onChange(toggleItem(chosen, id, v, catalog))} />
          </li>
        );
      })}
    </ul>
  );
}

/** Choose and order the optional toolbar buttons or the sidebar entries. Changes apply at once. */
export function CustomizeDialog() {
  const what = ui.use((s) => s.customize);
  const { settings, update } = useSettings();
  const toolbar = what === "toolbar";
  return (
    <Modal
      open={!!what}
      title={toolbar ? "Customize the toolbar" : "Customize the sidebar"}
      onClose={closeCustomize}
      width={520}
      footer={
        <>
          <Button variant="ghost" onClick={() => void update(toolbar ? { toolbarItems: DEFAULT_TOOLBAR_ITEMS } : { sidebarItems: DEFAULT_SIDEBAR_ITEMS })}>
            Restore defaults
          </Button>
          <Button variant="primary" onClick={closeCustomize}>
            Done
          </Button>
        </>
      }
    >
      <p className="dim">
        {toolbar
          ? "Back, forward, reload, the address bar and the menu are always there. Everything else is up to you; what you hide stays in the menu."
          : "The sidebar opens panels next to the page and takes you to browser pages. Hidden entries stay reachable from the menu."}
      </p>
      {toolbar ? (
        <OrderList catalog={TOOLBAR_ITEMS} chosen={settings.toolbarItems} label={(id) => TOOLBAR_LABELS[id] ?? { label: id, icon: "dots" }} onChange={(toolbarItems) => void update({ toolbarItems })} />
      ) : (
        <>
          <OrderList
            catalog={SIDEBAR_ITEMS}
            chosen={settings.sidebarItems}
            label={(id) => {
              const e = RAIL_ENTRIES.find((x) => x.id === id);
              return { label: e?.label ?? id, icon: e?.icon ?? "dots", hint: e?.panel ? "Opens a panel" : e?.page ? "Opens a page" : "Opens settings" };
            }}
            onChange={(sidebarItems) => void update({ sidebarItems })}
          />
          <div className="setting-row">
            <div className="setting-text">
              <div className="setting-title">Show labels</div>
              <div className="setting-desc">A wider sidebar with names next to the icons.</div>
            </div>
            <Toggle label="Show labels" checked={settings.sidebar === "expanded"} onChange={(on) => void update({ sidebar: on ? "expanded" : "rail" })} />
          </div>
          <div className="setting-row">
            <div className="setting-text">
              <div className="setting-title">Hide the sidebar</div>
              <div className="setting-desc">Right-click the toolbar to bring it back.</div>
            </div>
            <Toggle label="Hide the sidebar" checked={settings.sidebar === "hidden"} onChange={(on) => void update({ sidebar: on ? "hidden" : "rail" })} />
          </div>
        </>
      )}
    </Modal>
  );
}
