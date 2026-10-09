import { useEffect, useRef, useState } from "react";

import { AI_PRESETS, isLocalEndpoint, presetFor } from "../../features/ai/aiPresets";
import { loadAiConfig } from "../../features/ai/aiStore";
import { Button, TextField } from "../../components/ui/controls";
import { Icon } from "../../components/ui/Icon";
import { Card, Notice } from "../../components/ui/Section";
import { api, errorMessage } from "../../lib/ipc";
import { toast } from "../../lib/store";
import type { AiProvider } from "../../lib/types";

export function AiSettings() {
  const [provider, setProvider] = useState<AiProvider>("openAiCompatible");
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("");
  // The key is typed into an uncontrolled field and read once at save time, so it never
  // sits in React state. Only "has the user typed something" is tracked.
  const keyInput = useRef<HTMLInputElement>(null);
  const [keyTyped, setKeyTyped] = useState(false);
  const [hasKey, setHasKey] = useState(false);
  const [migrationPending, setMigrationPending] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let live = true;
    void loadAiConfig().then((c) => {
      if (!live || !c) return;
      setProvider(c.provider);
      setBaseUrl(c.baseUrl);
      setModel(c.model);
      setHasKey(c.hasKey);
      setMigrationPending(c.keyMigrationPending);
      setLoaded(true);
    });
    return () => {
      live = false;
    };
  }, []);

  const preset = presetFor(provider, baseUrl);
  const local = isLocalEndpoint(baseUrl);

  const applyPreset = (id: string) => {
    const p = AI_PRESETS.find((x) => x.id === id);
    if (!p) return;
    setProvider(p.provider);
    setBaseUrl(p.baseUrl);
    if (p.model) setModel(p.model);
    setError(null);
  };

  const clearKeyField = () => {
    if (keyInput.current) keyInput.current.value = "";
    setKeyTyped(false);
  };

  const save = async (key: string | undefined) => {
    setSaving(true);
    setError(null);
    try {
      const c = await api.aiConfigSet(provider, baseUrl, model, key);
      setHasKey(c.hasKey);
      setMigrationPending(c.keyMigrationPending);
      clearKeyField();
      await loadAiConfig();
      toast("AI settings saved");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <>
      <Card title="AI assistant" subtitle="Ask questions about the page you're reading, or chat. The assistant runs on a model you choose: a local one, or a provider you have your own key for.">
        <div className="preset-row" role="radiogroup" aria-label="Provider preset">
          {AI_PRESETS.map((p) => (
            <button key={p.id} type="button" role="radio" aria-checked={preset?.id === p.id} className={`chip${preset?.id === p.id ? " is-on" : ""}`} onClick={() => applyPreset(p.id)}>
              {p.label}
            </button>
          ))}
        </div>
        {preset && <p className="dim small">{preset.note}</p>}

        <div className="form-grid">
          <TextField label="Endpoint address" value={baseUrl} placeholder="https://api.openai.com/v1" onChange={(e) => setBaseUrl(e.target.value)} disabled={!loaded} />
          <TextField label="Model" value={model} placeholder="Model name" onChange={(e) => setModel(e.target.value)} disabled={!loaded} />
          <div className="field">
            <label htmlFor="ai-key">API key{local && !hasKey ? " (not needed for local models)" : ""}</label>
            <input
              id="ai-key"
              ref={keyInput}
              className="input"
              type="password"
              autoComplete="new-password"
              spellCheck={false}
              disabled={!loaded}
              placeholder={hasKey ? "Enter a new key to replace the saved one" : "sk-…"}
              onChange={(e) => setKeyTyped(e.target.value.length > 0)}
            />
            {hasKey ? (
              <p className="field-hint key-status">
                <Icon name="lock" size={13} /> A key is saved in Windows Credential Manager. It is never shown here again.
              </p>
            ) : (
              <p className="field-hint">Saved to Windows Credential Manager, not to the browser's own files.</p>
            )}
          </div>
        </div>
        {migrationPending && (
          <Notice tone="warn">
            An API key saved by an older version could not be moved into Windows Credential Manager, so it is still in the browser's
            old settings. Paste the key again and save to store it securely; it will then be removed from the old location.
          </Notice>
        )}
        {error && <Notice tone="bad">{error}</Notice>}
        <div className="row-actions">
          <Button variant="primary" disabled={!loaded || saving || !baseUrl.trim()} onClick={() => void save(keyInput.current?.value.trim() || undefined)}>
            {saving ? "Saving…" : keyTyped ? "Save with new key" : "Save"}
          </Button>
          {hasKey && <Button variant="ghost" disabled={saving} onClick={() => void save("")}>Remove key from Credential Manager</Button>}
        </div>
      </Card>

      <Card title="Privacy">
        <ul className="plain-list">
          <li>Nothing is sent until you press Send in the assistant panel.</li>
          <li>The page's text is shared only when you switch on “Include this page”, and never from private tabs.</li>
          <li>{local ? "This endpoint is on your computer: your messages stay on this machine." : "This endpoint is online: your messages (and the page, if included) are sent to that provider."}</li>
          <li>Your API key is kept in Windows Credential Manager, encrypted for your Windows account. It is not written to the browser's database or settings, and it is never sent back to this page after saving.</li>
        </ul>
      </Card>
    </>
  );
}
