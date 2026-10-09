import { useState } from "react";

import { Button, Spinner, TextField } from "../../components/ui/controls";
import { Icon } from "../../components/ui/Icon";
import { Card, Notice } from "../../components/ui/Section";
import { api, errorMessage } from "../../lib/ipc";
import { toast } from "../../lib/store";
import { useLoad } from "../../lib/useLoad";

export function Profiles() {
  const loaded = useLoad(() => api.profilesList(), 0);
  const info = loaded.data;
  const [actionError, setError] = useState<string | null>(null);
  const error = actionError ?? loaded.error;
  const [name, setName] = useState("");
  const [editing, setEditing] = useState<{ id: string; name: string } | null>(null);
  const [switching, setSwitching] = useState(false);

  const create = async () => {
    const n = name.trim();
    if (!n) return;
    try {
      await api.profileCreate(n);
      setName("");
      toast(`Profile “${n}” created`);
      loaded.reload();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const rename = async () => {
    if (!editing || !editing.name.trim()) return;
    try {
      await api.profileRename(editing.id, editing.name);
      setEditing(null);
      loaded.reload();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const switchTo = async (id: string) => {
    setSwitching(true);
    try {
      await api.profileSwitch(id); // the browser restarts into the other profile
    } catch (e) {
      setSwitching(false);
      setError(errorMessage(e));
    }
  };

  return (
    <>
      <Card title="Profiles" subtitle="Each profile keeps its own tabs, history, bookmarks, cookies and settings. Switching restarts the browser.">
        {error && <Notice tone="bad">{error}</Notice>}
        {!info && !error && <Spinner />}
        <div className="profile-list">
          {info?.profiles.map((p) => {
            const current = p.id === info.current;
            return (
              <div key={p.id} className={`profile-row${current ? " is-current" : ""}`}>
                <span className="profile-avatar" style={{ background: p.color || "var(--accent)" }}>{p.name.charAt(0).toUpperCase()}</span>
                {editing?.id === p.id ? (
                  <TextField
                    label="Profile name"
                    className="profile-edit"
                    value={editing.name}
                    data-autofocus
                    onChange={(e) => setEditing({ id: p.id, name: e.target.value })}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void rename();
                      if (e.key === "Escape") setEditing(null);
                    }}
                  />
                ) : (
                  <div className="profile-name">
                    <strong>{p.name}</strong>
                    {current && <span className="dim">Current profile</span>}
                  </div>
                )}
                <div className="profile-actions">
                  {editing?.id === p.id ? (
                    <>
                      <Button size="sm" variant="primary" onClick={() => void rename()}>Save</Button>
                      <Button size="sm" variant="ghost" onClick={() => setEditing(null)}>Cancel</Button>
                    </>
                  ) : (
                    <>
                      <Button size="sm" variant="ghost" onClick={() => setEditing({ id: p.id, name: p.name })}>Rename</Button>
                      {!current && <Button size="sm" disabled={switching} onClick={() => void switchTo(p.id)}>{switching ? "Restarting…" : "Switch"}</Button>}
                      {current && <Icon name="check" size={16} />}
                    </>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      </Card>
      <Card title="Add a profile">
        <div className="inline-form">
          <TextField label="New profile name" placeholder="e.g. Work" value={name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void create()} />
          <Button variant="primary" icon="plus" disabled={!name.trim()} onClick={() => void create()}>Create</Button>
        </div>
      </Card>
    </>
  );
}
