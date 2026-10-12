import { useEffect, useMemo, useState } from "react";
import qrcode from "qrcode-generator";

import { Modal } from "../../components/ui/Modal";
import { Button, Spinner } from "../../components/ui/controls";
import { Icon } from "../../components/ui/Icon";
import { hostOf } from "../../lib/format";
import { api, errorMessage } from "../../lib/ipc";
import { SHORTCUT_GROUPS } from "../../lib/shortcuts";
import { activeTab, toast, useStore } from "../../lib/store";
import type { Bookmark } from "../../lib/types";
import { closeDialog, ui } from "../../lib/ui";
import { isInternalUrl } from "../../lib/urls";
import { cleanLink } from "../palette/commands";
import { getShot, startScreenshot } from "./screenshotState";
import { deleteWorkspace, listWorkspaces, openWorkspace, saveWorkspace } from "../workspaces/workspaces";

// ---- Screenshot ------------------------------------------------------------------------------------

export function ScreenshotDialog() {
  const open = ui.use((s) => s.dialog === "screenshot");
  const shot = open ? getShot() : null;
  const [busy, setBusy] = useState(false);
  if (!shot) return null;
  const copy = async () => {
    try {
      await navigator.clipboard.write([new ClipboardItem({ "image/png": shot.blob })]);
      toast("Screenshot copied");
      closeDialog();
    } catch {
      toast("Could not copy the picture. Save it instead.");
    }
  };
  const save = async () => {
    setBusy(true);
    try {
      const path = await api.screenshotSave(shot.tabId, shot.full);
      toast(`Saved to ${path.split(/[\\/]/).slice(-2).join("\\")}`);
      closeDialog();
    } catch (e) {
      toast(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      open={open}
      title={shot.full ? `Whole page · ${shot.host}` : `Screenshot · ${shot.host}`}
      onClose={closeDialog}
      width={760}
      footer={
        <>
          {!shot.full && (
            <Button variant="ghost" icon="crop" onClick={() => void startScreenshot(true)}>
              Capture the whole page
            </Button>
          )}
          <span className="spacer" />
          <Button icon="copy" onClick={() => void copy()}>
            Copy
          </Button>
          <Button variant="primary" icon="save" disabled={busy} onClick={() => void save()} data-autofocus>
            Save to Downloads
          </Button>
        </>
      }
    >
      <div className="shot-preview">
        <img src={shot.url} alt="Screenshot preview" />
      </div>
      <p className="dim small">{`${Math.round(shot.blob.size / 1024)} KB · PNG${shot.full ? " · pages taller than 16 000 pixels are cut off" : ""}`}</p>
    </Modal>
  );
}

// ---- Keyboard shortcuts ------------------------------------------------------------------------------

export function ShortcutsDialog() {
  const open = ui.use((s) => s.dialog === "shortcuts");
  return (
    <Modal open={open} title="Keyboard shortcuts" onClose={closeDialog} width={760}>
      <div className="shortcut-grid">
        {SHORTCUT_GROUPS.map((g) => (
          <section key={g.title}>
            <h3>{g.title}</h3>
            {g.items.map((it) => (
              <div key={it.label} className="shortcut-row">
                <span>{it.label}</span>
                <span className="keys">
                  {it.keys.map((k) => (
                    <kbd key={k}>{k}</kbd>
                  ))}
                </span>
              </div>
            ))}
          </section>
        ))}
      </div>
    </Modal>
  );
}

// ---- Share: QR code and links -----------------------------------------------------------------------

/** A QR code for `text` as SVG rectangles (no HTML strings are injected). */
export function QrCode({ text, size = 220 }: { text: string; size?: number }) {
  const cells = useMemo(() => {
    const qr = qrcode(0, "M");
    qr.addData(text);
    qr.make();
    const n = qr.getModuleCount();
    const dark: [number, number][] = [];
    for (let r = 0; r < n; r++) for (let c = 0; c < n; c++) if (qr.isDark(r, c)) dark.push([r, c]);
    return { n, dark };
  }, [text]);
  const m = 4;
  return (
    <svg className="qr" width={size} height={size} viewBox={`0 0 ${cells.n + m * 2} ${cells.n + m * 2}`} role="img" aria-label="QR code">
      <rect width="100%" height="100%" fill="#fff" />
      {cells.dark.map(([r, c]) => (
        <rect key={`${r}-${c}`} x={c + m} y={r + m} width="1.02" height="1.02" fill="#000" />
      ))}
    </svg>
  );
}

export function ShareDialog() {
  const open = ui.use((s) => s.dialog === "share");
  const tab = useStore(activeTab);
  if (!tab || isInternalUrl(tab.url)) return null;
  const clean = cleanLink(tab.url);
  const copy = (text: string, what: string) => void navigator.clipboard.writeText(text).then(() => toast(`${what} copied`), () => toast("Could not copy"));
  return (
    <Modal open={open} title="Share this page" onClose={closeDialog} width={460}>
      <div className="share-body">
        {clean.length <= 1200 ? <QrCode text={clean} /> : <p className="dim">This address is too long for a QR code.</p>}
        <p className="dim small">Scan with your phone's camera to open this page there.</p>
        <div className="share-actions">
          <Button icon="link" onClick={() => copy(tab.url, "Link")}>
            Copy link
          </Button>
          {clean !== tab.url && (
            <Button icon="broom" onClick={() => copy(clean, "Clean link")}>
              Copy without tracking
            </Button>
          )}
          <Button icon="code" onClick={() => copy(`[${tab.title || tab.url}](${tab.url})`, "Markdown link")}>
            Markdown
          </Button>
        </div>
      </div>
    </Modal>
  );
}

// ---- Workspaces --------------------------------------------------------------------------------------

export function WorkspacesDialog() {
  const open = ui.use((s) => s.dialog === "workspaces");
  const rev = useStore((s) => s.bookmarksRev);
  const tabs = useStore((s) => s.browser.tabs);
  const [all, setAll] = useState<Bookmark[] | null>(null);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (!open) return;
    api.bookmarksAll().then(setAll, () => setAll([]));
  }, [open, rev]);
  const list = all ? listWorkspaces(all) : null;
  const web = tabs.filter((t) => !t.private && !isInternalUrl(t.url)).length;

  const save = async () => {
    setBusy(true);
    try {
      const n = await saveWorkspace(name.trim() || `Workspace ${new Date().toLocaleDateString()}`, tabs);
      toast(`Saved ${n} page${n === 1 ? "" : "s"}`);
      setName("");
    } catch (e) {
      toast(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal open={open} title="Workspaces" onClose={closeDialog} width={560}>
      <p className="dim small">A workspace keeps a set of tabs for later. It is saved as a bookmark folder (Other bookmarks › Workspaces), and opening it brings the pages back as a tab group.</p>
      <div className="ws-save">
        <input className="input" placeholder="Name, e.g. Thesis research" value={name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void save()} aria-label="Workspace name" data-autofocus />
        <Button variant="primary" icon="save" disabled={busy || web === 0} onClick={() => void save()}>
          Save {web} tab{web === 1 ? "" : "s"}
        </Button>
      </div>
      <div className="ws-list">
        {list === null && <Spinner size={18} />}
        {list?.length === 0 && <p className="dim small">No workspaces yet.</p>}
        {list?.map((ws) => (
          <div key={ws.id} className="ws-row">
            <Icon name="workspace" size={18} />
            <div className="ws-text">
              <strong className="truncate">{ws.name}</strong>
              <span className="dim small truncate">{ws.urls.map((u) => hostOf(u.url)).slice(0, 4).join(" · ") || "empty"}</span>
            </div>
            <span className="dim small">{ws.urls.length} tabs</span>
            <Button size="sm" disabled={ws.urls.length === 0} onClick={() => (closeDialog(), void openWorkspace(ws).catch((e) => toast(errorMessage(e))))}>
              Open
            </Button>
            <Button size="sm" variant="ghost" icon="trash" aria-label={`Delete ${ws.name}`} onClick={() => void deleteWorkspace(ws).catch((e) => toast(errorMessage(e)))} />
          </div>
        ))}
      </div>
    </Modal>
  );
}
