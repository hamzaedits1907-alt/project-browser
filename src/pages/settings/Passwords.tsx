import { useState } from "react";

import { Button, IconButton, TextField } from "../../components/ui/controls";
import { Modal } from "../../components/ui/Modal";
import { Card, EmptyState, Notice } from "../../components/ui/Section";
import { PasswordImport } from "../../features/import/PasswordImport";
import { hostOf } from "../../lib/format";
import { api } from "../../lib/ipc";
import { attempt, toast } from "../../lib/store";
import { useLoad } from "../../lib/useLoad";
import type { LoginSummary } from "../../lib/types";
import { filterLogins, usedText } from "../../lib/vault";

export function Passwords() {
  const logins = useLoad(() => api.passwordsList(), "logins");
  const profiles = useLoad(() => api.autofillProfiles(), "profiles");
  const [query, setQuery] = useState("");
  const [importing, setImporting] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);

  const all = logins.data ?? [];
  const shown = filterLogins(all, query);

  const remove = async (l: LoginSummary) => {
    await attempt(() => api.passwordsDelete(l.id));
    logins.reload();
  };

  const clearAll = async () => {
    const n = await attempt(() => api.passwordsClear());
    setConfirmClear(false);
    logins.reload();
    if (n !== undefined) toast(`Deleted ${n} saved login${n === 1 ? "" : "s"}`);
  };

  const removeProfile = async (id: number) => {
    await attempt(() => api.autofillProfileDelete(id));
    profiles.reload();
  };

  return (
    <>
      <Card
        title="Saved logins"
        subtitle="Logins you brought over from another browser. Each password is stored encrypted for your Windows account and is never shown here."
        actions={<Button icon="file" onClick={() => setImporting(true)}>Import from a file…</Button>}
      >
        {logins.error && <Notice tone="bad">{logins.error}</Notice>}
        {all.length === 0 && !logins.loading ? (
          <EmptyState icon="key" title="No saved logins">
            Export your passwords from Chrome, Edge or Brave and import the file here. Then a key button appears in the address bar on those sites; it fills the login when you click it.
          </EmptyState>
        ) : (
          <>
            <TextField label="Search saved logins" value={query} placeholder="Site or user name" onChange={(e) => setQuery(e.target.value)} />
            <ul className="login-list">
              {shown.map((l) => (
                <li key={l.id}>
                  <div className="login-main">
                    <strong className="truncate" data-selectable>{hostOf(l.origin)}</strong>
                    <span className="dim truncate" data-selectable>{l.username || "(no user name)"}</span>
                  </div>
                  <span className="dim small login-when">Used {usedText(l.lastUsed)}</span>
                  <IconButton icon="trash" label={`Delete the login for ${hostOf(l.origin)}`} size={16} onClick={() => void remove(l)} />
                </li>
              ))}
              {shown.length === 0 && <li className="dim">No saved login matches “{query}”.</li>}
            </ul>
            <div className="row-actions">
              <span className="dim small">{all.length.toLocaleString()} saved login{all.length === 1 ? "" : "s"}</span>
              {confirmClear ? (
                <>
                  <Button variant="danger" onClick={() => void clearAll()}>Yes, delete all {all.length.toLocaleString()}</Button>
                  <Button variant="ghost" onClick={() => setConfirmClear(false)}>Keep them</Button>
                </>
              ) : (
                <Button variant="ghost" disabled={all.length === 0} onClick={() => setConfirmClear(true)}>Delete all…</Button>
              )}
            </div>
          </>
        )}
      </Card>

      <Card title="Saved addresses" subtitle="Addresses brought over from another browser, stored encrypted. Used to fill empty name, address, e-mail and phone fields when you ask.">
        {profiles.error && <Notice tone="bad">{profiles.error}</Notice>}
        {(profiles.data ?? []).length === 0 && !profiles.loading ? (
          <p className="dim">No saved addresses. The import wizard can bring them over from Chrome, Edge or Brave.</p>
        ) : (
          <ul className="login-list">
            {(profiles.data ?? []).map((p) => (
              <li key={p.id}>
                <div className="login-main"><strong className="truncate" data-selectable>{p.label}</strong></div>
                <IconButton icon="trash" label="Delete this saved address" size={16} onClick={() => void removeProfile(p.id)} />
              </li>
            ))}
          </ul>
        )}
      </Card>

      <Card title="How filling works">
        <ul className="plain-list">
          <li>Nothing is filled until you click the key button in the address bar and choose a login or address.</li>
          <li>A login is offered only on the exact site it was saved for (same address and port), and only over https.</li>
          <li>The form is filled but never submitted; you press Sign in yourself.</li>
          <li>Passwords are encrypted with Windows data protection for your account. They are not written to the browser's settings and never sent to this page, to a website, or to the AI assistant.</li>
          <li>Passwords the website itself offers to save (Settings → Privacy) are kept by the web engine, separately from this list.</li>
        </ul>
      </Card>

      <Modal open={importing} title="Import passwords from a file" onClose={() => setImporting(false)} width={560}>
        {importing && (
          <PasswordImport
            onFinished={() => logins.reload()}
            onClose={() => {
              setImporting(false);
              logins.reload();
            }}
          />
        )}
      </Modal>
    </>
  );
}
