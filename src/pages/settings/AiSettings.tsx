import { useEffect, useRef, useState } from "react";

import { Badge, Button, Spinner } from "../../components/ui/controls";
import { Icon } from "../../components/ui/Icon";
import { Card, Notice } from "../../components/ui/Section";
import { capabilityView, featureGate, formatCount, KIND_LABEL, resolveSelection, stateView } from "../../features/ai/aiModels";
import { ai, connect, loadAiConfig, loadModels, probeFeature, removeKey, removeStaleKey, selectModel, testSaved } from "../../features/ai/aiStore";
import type { ConnectionUi } from "../../features/ai/aiStore";
import { ModelPicker } from "../../features/ai/ModelPicker";
import { toast } from "../../lib/store";
import type { AiCapabilities, AiFeature, ModelInfo } from "../../lib/types";

const NOTICE_TONE = { good: "good", warn: "warn", bad: "bad", neutral: "info" } as const;

function ConnectionStatus({ connection }: { connection: ConnectionUi }) {
  if (connection.phase === "testing") {
    return (
      <div className="ai-status" role="status">
        <Spinner size={16} /> <span>Checking with NVIDIA…</span>
      </div>
    );
  }
  if (connection.error) return <Notice tone="bad">{connection.error}</Notice>;
  const r = connection.result;
  if (!r) return null;
  const view = stateView(r.state);
  const facts = [
    r.modelCount > 0 ? formatCount(r.modelCount, "model", "models") : null,
    r.testedModel ? `tested with ${r.testedModel}` : null,
    r.latencyMs != null ? `${r.latencyMs} ms` : null,
  ].filter(Boolean);
  return (
    <Notice tone={NOTICE_TONE[view.tone]}>
      <strong>{view.headline}.</strong> {r.message}
      {facts.length > 0 && <div className="dim small">{facts.join(" · ")}</div>}
    </Notice>
  );
}

function CapabilityRow({
  name,
  hint,
  verified,
  feature,
  probing,
  onCheck,
}: {
  name: string;
  hint: AiCapabilities["chat"];
  verified: AiCapabilities["chat"];
  feature?: AiFeature;
  probing: AiFeature | null;
  onCheck?: (feature: AiFeature) => void;
}) {
  const view = capabilityView(hint, verified);
  const tone = view.tone === "good" ? "good" : view.tone === "bad" ? "bad" : "neutral";
  return (
    <div className="cap-row">
      <span className="cap-name">{name}</span>
      <Badge tone={tone}>{view.label}</Badge>
      {feature && onCheck && (
        <Button size="sm" variant="ghost" disabled={probing !== null} onClick={() => onCheck(feature)}>
          {probing === feature ? "Checking…" : view.confirmed ? "Check again" : "Check"}
        </Button>
      )}
    </div>
  );
}

function SelectedModel({ model, verified, probing }: { model: ModelInfo; verified: AiCapabilities; probing: AiFeature | null }) {
  const vision = featureGate("vision", verified);
  const tools = featureGate("tools", verified);
  const check = (feature: AiFeature) => {
    void probeFeature(feature).then((r) => r && toast(r.message));
  };
  return (
    <div className="model-detail">
      <dl className="kv">
        <dt>Publisher</dt>
        <dd>{model.publisher || "Unknown"}</dd>
        <dt>Model id</dt>
        <dd className="mono" data-selectable>{model.id}</dd>
        <dt>Type</dt>
        <dd>{KIND_LABEL[model.kind]}</dd>
      </dl>
      <CapabilityRow name="Chat" hint={model.hints.chat} verified={verified.chat} probing={probing} />
      <CapabilityRow name="Image input" hint={model.hints.vision} verified={verified.vision} feature="vision" probing={probing} onCheck={check} />
      <CapabilityRow name="Tool calling" hint={model.hints.tools} verified={verified.tools} feature="tools" probing={probing} onCheck={check} />
      <p className="dim small">
        NVIDIA's list only gives a model's name and publisher, so what a model can do is a guess until it is checked. Checking sends one tiny
        request. Features that need image input or tool calling stay off until a check confirms them
        {vision.enabled || tools.enabled ? "." : " (none of the assistant's current features need them)."}
      </p>
    </div>
  );
}

export function AiSettings() {
  const config = ai.use((s) => s.config);
  const models = ai.use((s) => s.models);
  const connection = ai.use((s) => s.connection);
  const unavailable = ai.use((s) => s.unavailable);
  const probing = ai.use((s) => s.probing);
  // The key is typed into an uncontrolled field and read once, so it never sits in React state or the store.
  const keyInput = useRef<HTMLInputElement>(null);
  const [keyTyped, setKeyTyped] = useState(false);

  useEffect(() => {
    void loadAiConfig().then((c) => {
      if (c?.hasKey) void loadModels();
    });
  }, []);

  const hasKey = !!config?.hasKey;
  const busy = connection.phase === "testing";
  const selection = resolveSelection(models.models, config?.model ?? "", unavailable);
  const selectedModel = selection.status === "ready" ? selection.model : selection.status === "unsupported" ? selection.model : null;

  const onConnect = async () => {
    const value = keyInput.current?.value.trim() ?? "";
    if (!value) return;
    const result = await connect(value);
    if (result?.keyOk) {
      if (keyInput.current) keyInput.current.value = "";
      setKeyTyped(false);
      toast("NVIDIA key saved");
    }
  };

  const onRemove = async () => {
    if (await removeKey()) toast("NVIDIA key removed");
  };

  const onPick = async (id: string) => {
    if (await selectModel(id)) toast("Model saved");
  };

  return (
    <>
      <Card
        title="NVIDIA AI"
        subtitle="Ask questions about the page you're reading, or chat, using NVIDIA's hosted models. All you need is your NVIDIA API key."
      >
        <div className="field">
          <label htmlFor="ai-key">NVIDIA API key</label>
          <div className="key-row">
            <input
              id="ai-key"
              ref={keyInput}
              className="input"
              type="password"
              autoComplete="new-password"
              spellCheck={false}
              disabled={!config || busy}
              placeholder={hasKey ? "Paste a new key to replace the saved one" : "nvapi-…"}
              onChange={(e) => setKeyTyped(e.target.value.trim().length > 0)}
              onKeyDown={(e) => e.key === "Enter" && void onConnect()}
            />
            <Button variant="primary" disabled={!config || busy || !keyTyped} onClick={() => void onConnect()}>
              {busy && keyTyped ? "Checking…" : hasKey ? "Replace key" : "Connect"}
            </Button>
          </div>
          {hasKey ? (
            <p className="field-hint key-status">
              <Icon name="lock" size={13} /> A key is saved in Windows Credential Manager. It is never shown here again.
            </p>
          ) : (
            <p className="field-hint">
              Create a key at build.nvidia.com (it starts with nvapi-). It is checked with NVIDIA first, then saved to Windows Credential Manager, not
              to the browser's own files.
            </p>
          )}
        </div>

        <ConnectionStatus connection={connection} />

        {config?.staleKey && (
          <Notice tone="warn">
            An API key from an older version of this browser (for a provider that is no longer supported) is still stored in Windows Credential
            Manager. It is never used or sent anywhere.{" "}
            <Button size="sm" variant="ghost" onClick={() => void removeStaleKey()}>Remove it</Button>
          </Notice>
        )}

        <div className="row-actions">
          <Button icon="refresh" disabled={!hasKey || busy} onClick={() => void testSaved()}>
            Test connection
          </Button>
          {hasKey && (
            <Button variant="ghost" disabled={busy} onClick={() => void onRemove()}>
              Remove key from Credential Manager
            </Button>
          )}
        </div>
      </Card>

      <Card title="Model" subtitle="Pick from the models NVIDIA currently offers. The assistant panel and “Include this page” both use the model chosen here.">
        {!hasKey ? (
          <p className="dim">Connect your NVIDIA key above to load the model list.</p>
        ) : models.phase === "loading" && models.models.length === 0 ? (
          <div className="ai-status" role="status">
            <Spinner size={16} /> <span>Loading models from NVIDIA…</span>
          </div>
        ) : models.phase === "error" ? (
          <>
            <Notice tone="bad">
              <strong>{stateView(models.state ?? "other").headline}.</strong> {models.message}
            </Notice>
            <Button icon="refresh" onClick={() => void loadModels(true)}>Try again</Button>
          </>
        ) : models.phase === "empty" ? (
          <>
            <Notice tone="warn">NVIDIA returned no models right now. This is usually temporary.</Notice>
            <Button icon="refresh" onClick={() => void loadModels(true)}>Try again</Button>
          </>
        ) : (
          <>
            <div className="model-row">
              <ModelPicker models={models.models} value={config?.model ?? ""} unavailable={unavailable} busy={models.phase === "loading"} onChange={(id) => void onPick(id)} />
              <Button icon="refresh" variant="ghost" disabled={models.phase === "loading"} onClick={() => void loadModels(true)}>
                Refresh list
              </Button>
            </div>
            <p className="dim small">
              {formatCount(models.models.length, "model", "models")}
              {models.fetchedAt > 0 && ` · updated ${new Date(models.fetchedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`}
            </p>
            {models.stale && (
              <Notice tone="warn">
                Showing the last list that loaded. {models.message}
              </Notice>
            )}
            {selection.status === "none" && <Notice tone="info">Choose a model to start using the assistant.</Notice>}
            {selection.status === "missing" && (
              <Notice tone="warn">
                “{selection.id}” is no longer in NVIDIA's list, so it was probably retired. Choose another model.
              </Notice>
            )}
            {selection.status === "unsupported" && <Notice tone="warn">{selection.reason}</Notice>}
            {selectedModel && <SelectedModel model={selectedModel} verified={config?.verified ?? { chat: "unknown", vision: "unknown", tools: "unknown" }} probing={probing} />}
          </>
        )}
      </Card>

      <Card title="Privacy">
        <ul className="plain-list">
          <li>Nothing is sent until you press Send in the assistant panel, Connect, or Test connection.</li>
          <li>The page's text is shared only when you switch on “Include this page”, and never from private tabs.</li>
          <li>Your messages (and the page, if included) go to NVIDIA's hosted API at integrate.api.nvidia.com.</li>
          <li>
            Your API key is kept in Windows Credential Manager, encrypted for your Windows account. It is not written to the browser's database or
            settings, and it is never sent back to this page after saving. Only the model you chose is remembered in the browser's settings.
          </li>
        </ul>
      </Card>
    </>
  );
}
