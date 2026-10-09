import { useState } from "react";

import { Button, Select } from "../../components/ui/controls";
import { Modal } from "../../components/ui/Modal";
import { Notice } from "../../components/ui/Section";
import { api, errorMessage } from "../../lib/ipc";
import { toast } from "../../lib/store";
import { closeClearData, ui } from "../../lib/ui";
import type { ClearOptions } from "../../lib/types";
import { sinceFor } from "./range";
import type { Range } from "./range";

type Key = "history" | "downloads" | "cookies" | "cache" | "passwords" | "autofill";

const ITEMS: { key: Key; title: string; desc: string }[] = [
  { key: "history", title: "Browsing history", desc: "Pages you visited, in the time range above." },
  { key: "downloads", title: "Download list", desc: "Removes finished downloads from the list. Files stay on your computer." },
  { key: "cookies", title: "Cookies and other site data", desc: "Signs you out of most sites. Always clears all time." },
  { key: "cache", title: "Cached images and files", desc: "Frees disk space. Pages may load a little slower once. Always clears all time." },
  { key: "passwords", title: "Saved passwords", desc: "Passwords saved by this browser. Always clears all time." },
  { key: "autofill", title: "Autofill data", desc: "Saved addresses and form entries. Always clears all time." },
];

export function ClearDataDialog() {
  const open = ui.use((s) => s.clearDataOpen);
  const [range, setRange] = useState<Range>("all");
  const [checked, setChecked] = useState<Record<Key, boolean>>({ history: true, downloads: false, cookies: false, cache: true, passwords: false, autofill: false });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const any = Object.values(checked).some(Boolean);

  const run = async () => {
    setBusy(true);
    setError(null);
    const options: ClearOptions = { ...checked, since: sinceFor(range) };
    try {
      await api.clearData(options);
      toast("Browsing data cleared");
      closeClearData();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      open={open}
      title="Clear browsing data"
      onClose={closeClearData}
      locked={busy}
      width={520}
      footer={
        <>
          <Button variant="ghost" onClick={closeClearData} disabled={busy}>Cancel</Button>
          <Button variant="danger" icon="trash" disabled={!any || busy} onClick={() => void run()}>{busy ? "Clearing…" : "Clear data"}</Button>
        </>
      }
    >
      <div className="clear-range">
        <span>Time range for history</span>
        <Select<Range>
          label="Time range"
          value={range}
          options={[
            { value: "hour", label: "Last hour" },
            { value: "day", label: "Last 24 hours" },
            { value: "week", label: "Last 7 days" },
            { value: "month", label: "Last 4 weeks" },
            { value: "all", label: "All time" },
          ]}
          onChange={setRange}
        />
      </div>
      <div className="check-list">
        {ITEMS.map((it) => {
          return (
            <label key={it.key} className="check-row">
              <input type="checkbox" checked={checked[it.key]} onChange={(e) => setChecked({ ...checked, [it.key]: e.target.checked })} />
              <span>
                <strong>{it.title}</strong>
                <small>{it.desc}</small>
              </span>
            </label>
          );
        })}
      </div>
      {error && <Notice tone="bad">{error}</Notice>}
    </Modal>
  );
}
