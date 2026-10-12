import { useEffect, useRef, useState } from "react";

import { Badge, Button, Slider, Spinner } from "../../components/ui/controls";
import { Icon } from "../../components/ui/Icon";
import { Card, Notice, SettingRow } from "../../components/ui/Section";
import { capabilityView, featureGate, formatCount, isTransientFailure, KIND_LABEL, resolveSelection, setupStage, stateView } from "../../features/ai/aiModels";
import type { SetupStage } from "../../features/ai/aiModels";
import { ai, connect, loadAiConfig, loadModels, probeFeature, removeKey, removeStaleKey, saveKeyUnchecked, selectModel, testSaved } from "../../features/ai/aiStore";
import type { ConnectionUi } from "../../features/ai/aiStore";
import { ModelPicker } from "../../features/ai/ModelPicker";
import { api } from "../../lib/ipc";
import { toast } from "../../lib/store";
import { useSettings } from "../../lib/useSettings";
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

const STAGE_TEXT: Record<SetupStage["stage"], { tone: "good" | "warn" | "bad" | "neutral"; text: string }> = {
  loading: { tone: "neutral", text: "Reading your saved settings…" },
  noKey: { tone: "neutral", text: "Step 1: paste your NVIDIA API key." },
  keyEntered: { tone: "neutral", text: "Press Connect to check the key with NVIDIA and save it." },
  keySaved: { tone: "neutral", text: "Your key is saved. Loading models…" },
  catalogLoading: { tone: "neutral", text: "Loading NVIDIA's model list…" },
  invalidKey: { tone: "bad", text: "NVIDIA rejected the key. Paste a new one." },
  providerFailure: { tone: "warn", text: "Couldn't finish talking to NVIDIA. Your key is saved; try again." },
  noUsableModels: { tone: "warn", text: "NVIDIA's list has no model the assistant can chat with right now." },
  chooseModel: { tone: "neutral", text: "Step 2: choose a model below." },
  modelUnavailable: { tone: "warn", text: "The chosen model can't be used. Choose another below." },
  verified: { tone: "good", text: "Connected. Choose a model below to start." },
  ready: { tone: "good", text: "Ready to chat." },
};

function StageLine({ stage }: { stage: SetupStage }) {
  const view = STAGE_TEXT[stage.stage];
  const detail = stage.stage === "modelUnavailable" ? ` ${stage.reason}` : stage.stage === "ready" && !stage.verified ? " (not yet tested: press Test connection to confirm)" : "";
  return (
    <p className={`setup-stage tone-${view.tone}`} role="status">
      <Badge tone={view.tone}>{view.tone === "good" ? "OK" : view.tone === "bad" ? "Problem" : view.tone === "warn" ? "Attention" : "Next"}</Badge>{" "}
      {view.text}
      {detail}
    </p>
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
  const { settings, updateSoon } = useSettings();

  useEffect(() => {
    void loadAiConfig().then((c) => {
      if (c?.hasKey) void loadModels();
    });
  }, []);

  const hasKey = !!config?.hasKey;
  const stage = setupStage({
    hasKey: config ? config.hasKey : null,
    keyTyped,
    models,
    connection: { phase: connection.phase, state: connection.result?.state ?? null, keyOk: connection.result?.keyOk ?? false },
    selectedModel: config?.model ?? "",
    unavailable,
  });
  const canSaveUnchecked = keyTyped && connection.phase === "done" && !!connection.result && !connection.result.keyOk && isTransientFailure(connection.result.state);
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

  const onSaveUnchecked = async () => {
    const value = keyInput.current?.value.trim() ?? "";
    if (!value) return;
    if (await saveKeyUnchecked(value)) {
      if (keyInput.current) keyInput.current.value = "";
      setKeyTyped(false);
      toast("NVIDIA key saved (not checked)");
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
              disabled={busy}
              placeholder={hasKey ? "Paste a new key to replace the saved one" : "nvapi-…"}
              onChange={(e) => setKeyTyped(e.target.value.trim().length > 0)}
              onKeyDown={(e) => e.key === "Enter" && void onConnect()}
            />
            <Button variant="primary" disabled={busy || !keyTyped} onClick={() => void onConnect()}>
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

        <StageLine stage={stage} />

        <ConnectionStatus connection={connection} />

        {canSaveUnchecked && (
          <Notice tone="info">
            NVIDIA couldn't be reached to check this key, which says nothing about the key itself. You can save it anyway and test it later.{" "}
            <Button size="sm" onClick={() => void onSaveUnchecked()}>Save without checking</Button>
          </Notice>
        )}
        {!config && (
          <Notice tone="warn">
            Your saved AI settings couldn't be read, but you can still enter a key.{" "}
            <Button size="sm" onClick={() => void loadAiConfig()}>Try again</Button>
          </Notice>
        )}

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

      <Card title="Context and cost" subtitle="Every request that carries page text is cut to this size, and the core refuses anything larger, whatever the page or feature asks for.">
        <SettingRow title="Most page text per request" description="About four characters make a token. Smaller is cheaper and faster; larger lets Research compare more or longer pages.">
          <Slider
            label="Most page text per request, in characters"
            min={4000}
            max={120000}
            step={2000}
            value={settings.aiMaxContextChars}
            format={(v) => `${Math.round(v / 4).toLocaleString("en-US")} tokens`}
            onChange={(aiMaxContextChars) => updateSoon({ aiMaxContextChars })}
          />
        </SettingRow>
        <p className="dim small">Replies are limited to 4,096 tokens and stop after three minutes. Stop buttons cancel a request at once. A page's menus, footers and scripts are left out before anything is counted.</p>
      </Card>

      <Card title="Project Memory and Research">
        <p className="dim">Notes, summaries and research you choose to keep stay on this computer, in your profile. Research and Actions read only the tabs you select.</p>
        <div className="row-actions">
          <Button icon="bookmarks" onClick={() => void api.tabNew("browser://memory")}>Open Project Memory</Button>
          <Button icon="layers" onClick={() => void api.tabNew("browser://research")}>Open Research</Button>
        </div>
      </Card>

      <Card title="Privacy">
        <ul className="plain-list">
          <li>Nothing is sent until you press Send in the assistant panel, Connect, Test connection, Run in Research, or Run on a plan you have read.</li>
          <li>The page's text is shared only when you switch on “Include this page”, tick pages in Research, or confirm a plan that reads the page, and never from private tabs.</li>
          <li>Web pages are treated as untrusted text. They cannot add steps to a plan or grant themselves permissions, and plans are made without seeing any page.</li>
          <li>Your cookies, saved passwords and files are never part of a request.</li>
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
