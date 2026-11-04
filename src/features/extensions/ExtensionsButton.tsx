// The toolbar's extensions button (the puzzle piece in Chrome) and the pinned extension icons beside it. A pinned icon
// or a row in the list opens the extension's pop-up below the toolbar, or its options page when it has no pop-up.

import { useRef, useState } from "react";

import { Icon } from "../../components/ui/Icon";
import { Button, IconButton } from "../../components/ui/controls";
import { Popover } from "../../components/ui/Popover";
import { api } from "../../lib/ipc";
import { activeTab, attempt, useStore } from "../../lib/store";
import { openSettingsSection } from "../../lib/ui";
import type { ExtensionDetails } from "../../lib/types";
import { installFromStore, openExtension, openOptions, storeIdFromUrl, togglePinned, useExtensions, usePinned } from "./extensions";

function ExtIcon({ ext, size = 18 }: { ext: ExtensionDetails; size?: number }) {
  if (ext.icon) return <img className="ext-icon" src={ext.icon} width={size} height={size} alt="" draggable={false} />;
  return <span className="ext-icon ext-letter" style={{ width: size, height: size }}>{(ext.name.trim()[0] ?? "?").toUpperCase()}</span>;
}

function PinnedIcon({ ext }: { ext: ExtensionDetails }) {
  const ref = useRef<HTMLButtonElement>(null);
  return (
    <button ref={ref} type="button" className={`icon-btn ext-pinned${ext.enabled ? "" : " is-off"}`} title={ext.name} aria-label={ext.name} onClick={() => void openExtension(ext, ref.current)}>
      <ExtIcon ext={ext} />
    </button>
  );
}

export function ExtensionsButton() {
  const { list } = useExtensions();
  const pinned = usePinned();
  const anchor = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const tab = useStore(activeTab);
  const storeId = tab ? storeIdFromUrl(tab.url) : null;
  const installed = storeId ? list.some((e) => e.storeId === storeId) : false;
  const shown = list.filter((e) => pinned.includes(e.id));

  return (
    <>
      {shown.map((e) => (
        <PinnedIcon key={e.id} ext={e} />
      ))}
      <div ref={anchor} className="tool-anchor">
        <IconButton icon="widgets" label="Extensions" active={open} onClick={() => setOpen((o) => !o)} />
      </div>
      <Popover open={open} onClose={() => setOpen(false)} anchor={anchor} align="end" width={320} label="Extensions" className="ext-pop">
        <div className="pop-head">
          <h3>Extensions</h3>
        </div>
        {storeId && !installed && (
          <button type="button" className="menu-item ext-store-add" role="menuitem" onClick={() => { setOpen(false); void installFromStore(storeId); }}>
            <Icon name="plus" size={16} />
            <span>Add this extension to Project Browser</span>
          </button>
        )}
        {list.length === 0 && <p className="muted ext-empty">No extensions yet. Open the Chrome Web Store and add one with a click.</p>}
        <div className="ext-list">
          {list.map((e) => (
            <div key={e.id} className={`ext-row${e.enabled ? "" : " is-off"}`}>
              <button
                type="button"
                className="ext-open"
                role="menuitem"
                onClick={() => {
                  setOpen(false);
                  void openExtension(e, anchor.current);
                }}
              >
                <ExtIcon ext={e} />
                <span className="ext-name">{e.name}</span>
                {!e.enabled && <span className="ext-off">Off</span>}
              </button>
              {e.options && <IconButton icon="settings" size={15} label={`${e.name} options`} onClick={() => { setOpen(false); void openOptions(e); }} />}
              <IconButton icon={pinned.includes(e.id) ? "pinFilled" : "pin"} size={15} label={pinned.includes(e.id) ? "Unpin from the toolbar" : "Pin to the toolbar"} active={pinned.includes(e.id)} onClick={() => togglePinned(e.id)} />
            </div>
          ))}
        </div>
        <div className="ext-foot">
          <button type="button" className="menu-item" role="menuitem" onClick={() => { setOpen(false); void attempt(() => api.tabNew("https://chromewebstore.google.com/")); }}>
            <Icon name="globe" size={16} />
            <span>Chrome Web Store</span>
          </button>
          <button type="button" className="menu-item" role="menuitem" onClick={() => { setOpen(false); openSettingsSection("extensions"); }}>
            <Icon name="sliders" size={16} />
            <span>Manage extensions</span>
          </button>
        </div>
      </Popover>
    </>
  );
}

/** Shown on a Chrome Web Store extension page: one click installs it here (the store's own button only works in Chrome). */
export function StoreBar() {
  const tab = useStore(activeTab);
  const { list, loaded } = useExtensions();
  const [busy, setBusy] = useState(false);
  const storeId = tab ? storeIdFromUrl(tab.url) : null;
  if (!storeId || !loaded) return null;
  const installed = list.some((e) => e.storeId === storeId);
  return (
    <div className="permbar store-bar" role="region" aria-label="Chrome Web Store">
      <span className="perm-icon"><Icon name="widgets" size={18} /></span>
      <div className="perm-text">
        {installed ? <strong>This extension is installed.</strong> : <><strong>Add this extension to Project Browser.</strong><span className="dim"> The store's own button only works in Chrome; this one installs it here.</span></>}
      </div>
      {!installed && (
        <Button
          variant="primary"
          size="sm"
          icon="plus"
          disabled={busy}
          onClick={() => {
            setBusy(true);
            void installFromStore(storeId).finally(() => setBusy(false));
          }}
        >
          {busy ? "Installing…" : "Add to Project Browser"}
        </Button>
      )}
    </div>
  );
}
