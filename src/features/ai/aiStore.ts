import { createStore } from "../../lib/createStore";
import { api, errorMessage, on } from "../../lib/ipc";
import type { AiChatMessage, AiConfigView, AiFeature, AiProbeResult, AiTestResult, ConnectionState, ModelInfo } from "../../lib/types";
import { isModelProblem, isTransientFailure } from "./aiModels";

export interface AiMessage extends AiChatMessage {
  id: number;
  error?: boolean;
  /** The page an assistant answer was based on, when the user shared one. Only used to credit it if the answer is saved. */
  source?: { url: string; title: string };
}

/** The model list as the UI sees it: what was loaded, or why it could not be. */
export interface ModelsState {
  phase: "idle" | "loading" | "ready" | "empty" | "error";
  models: ModelInfo[];
  state: ConnectionState | null;
  message: string | null;
  /** The shown models are from an earlier load because the latest one failed. */
  stale: boolean;
  fetchedAt: number;
}

/** The latest Connect / Test connection run. */
export interface ConnectionUi {
  phase: "idle" | "testing" | "done";
  result: AiTestResult | null;
  /** A problem that is not a connection state (for example a key that is not shaped like a key). */
  error: string | null;
}

interface AiState {
  open: boolean;
  config: AiConfigView | null;
  messages: AiMessage[];
  streaming: boolean;
  includePage: boolean;
  requestId: string | null;
  models: ModelsState;
  connection: ConnectionUi;
  /** Models NVIDIA refused during this session; offered again after a restart. */
  unavailable: string[];
  /** The capability check that is running, if any. */
  probing: AiFeature | null;
}

const NO_MODELS: ModelsState = { phase: "idle", models: [], state: null, message: null, stale: false, fetchedAt: 0 };
const NO_CONNECTION: ConnectionUi = { phase: "idle", result: null, error: null };

export const ai = createStore<AiState>({
  open: false,
  config: null,
  messages: [],
  streaming: false,
  includePage: false,
  requestId: null,
  models: NO_MODELS,
  connection: NO_CONNECTION,
  unavailable: [],
  probing: null,
});

let seq = 1;
let listening = false;

function ensureListening(): void {
  if (listening) return;
  listening = true;
  void on("pb:ai", (e) => {
    const s = ai.get();
    if (e.requestId !== s.requestId) return;
    if (e.kind === "delta") {
      ai.set((cur) => {
        const msgs = cur.messages.slice();
        const last = msgs[msgs.length - 1];
        if (last && last.role === "assistant") msgs[msgs.length - 1] = { ...last, content: last.content + e.text };
        return { messages: msgs };
      });
    } else if (e.kind === "done") {
      ai.set({ streaming: false, requestId: null });
    } else {
      // A model NVIDIA turned down is remembered so the picker can say so and the panel stops offering it.
      if (isModelProblem(e.state)) markUnavailable(s.config?.model ?? "");
      finishWithError(e.text);
    }
  });
}

function finishWithError(message: string): void {
  ai.set((cur) => {
    const msgs = cur.messages.slice();
    const last = msgs[msgs.length - 1];
    if (last && last.role === "assistant" && last.content === "") msgs.pop();
    msgs.push({ id: seq++, role: "assistant", content: message, error: true });
    return { messages: msgs, streaming: false, requestId: null };
  });
}

export function markUnavailable(model: string): void {
  if (!model) return;
  ai.set((cur) => (cur.unavailable.includes(model) ? {} : { unavailable: [...cur.unavailable, model] }));
}

export async function loadAiConfig(): Promise<AiConfigView | null> {
  ensureListening();
  try {
    const config = await api.aiConfigGet();
    ai.set({ config });
    return config;
  } catch {
    return null;
  }
}

/** Loads the model list from NVIDIA (through the core, which holds the key). */
export async function loadModels(refresh = false): Promise<void> {
  const config = ai.get().config ?? (await loadAiConfig());
  if (!config?.hasKey) {
    ai.set({ models: { ...NO_MODELS, state: "noKey" } });
    return;
  }
  ai.set((cur) => ({ models: { ...cur.models, phase: "loading" } }));
  try {
    const r = await api.aiModels(refresh);
    const usable = r.models.length > 0;
    if (r.state === "ok" && usable) {
      ai.set({ models: { phase: "ready", models: r.models, state: "ok", message: null, stale: false, fetchedAt: r.fetchedAt } });
    } else if (usable) {
      // The request failed but an earlier list is still good: keep working with it and say why it is old.
      ai.set({ models: { phase: "ready", models: r.models, state: r.state, message: r.message, stale: true, fetchedAt: r.fetchedAt } });
    } else if (r.state === "ok" || r.state === "empty") {
      ai.set({ models: { phase: "empty", models: [], state: "empty", message: r.message, stale: false, fetchedAt: r.fetchedAt } });
    } else {
      ai.set({ models: { phase: "error", models: [], state: r.state, message: r.message, stale: false, fetchedAt: 0 } });
    }
  } catch (e) {
    ai.set({ models: { phase: "error", models: [], state: "other", message: errorMessage(e), stale: false, fetchedAt: 0 } });
  }
}

/** Remembers the chosen model in the browser's preferences. The key is not involved. */
export async function selectModel(id: string): Promise<boolean> {
  try {
    const config = await api.aiConfigSet({ model: id });
    ai.set((cur) => ({ config, connection: NO_CONNECTION, unavailable: cur.unavailable.filter((m) => m !== id) }));
    return true;
  } catch (e) {
    ai.set({ connection: { phase: "done", result: null, error: errorMessage(e) } });
    return false;
  }
}

/**
 * Checks a key the user just typed and, if NVIDIA accepts it, saves it to Windows Credential Manager and loads
 * the models. A key NVIDIA rejects (or that could not be checked) is not saved. The key is only a parameter
 * here: it is never put into the store.
 */
export async function connect(key: string): Promise<AiTestResult | null> {
  ai.set({ connection: { phase: "testing", result: null, error: null } });
  let result: AiTestResult;
  try {
    result = await api.aiTest(key);
  } catch (e) {
    ai.set({ connection: { phase: "done", result: null, error: errorMessage(e) } });
    return null;
  }
  ai.set({ connection: { phase: "done", result, error: null } });
  if (!result.keyOk) return result;
  try {
    const config = await api.aiConfigSet({ apiKey: key });
    ai.set({ config, models: NO_MODELS });
    await loadModels(true);
  } catch (e) {
    ai.set({ connection: { phase: "done", result, error: errorMessage(e) } });
  }
  return result;
}

/**
 * Saves a key without NVIDIA having confirmed it. Offered only when the check failed for a reason that says nothing
 * about the key (the network is down, NVIDIA is rate limiting). A key NVIDIA actively rejected is never saved this way.
 */
export async function saveKeyUnchecked(key: string): Promise<boolean> {
  const last = ai.get().connection.result;
  if (last && !isTransientFailure(last.state)) {
    ai.set({ connection: { phase: "done", result: last, error: "That key was rejected, so it wasn't saved." } });
    return false;
  }
  try {
    const config = await api.aiConfigSet({ apiKey: key });
    ai.set({ config, models: NO_MODELS, connection: { phase: "idle", result: null, error: null } });
    await loadModels(true);
    return true;
  } catch (e) {
    ai.set({ connection: { phase: "done", result: last ?? null, error: errorMessage(e) } });
    return false;
  }
}

/** Re-checks the saved key and the selected model. */
export async function testSaved(): Promise<AiTestResult | null> {
  ai.set({ connection: { phase: "testing", result: null, error: null } });
  try {
    const result = await api.aiTest();
    ai.set({ connection: { phase: "done", result, error: null } });
    if (isModelProblem(result.state)) markUnavailable(ai.get().config?.model ?? "");
    await loadAiConfig();
    return result;
  } catch (e) {
    ai.set({ connection: { phase: "done", result: null, error: errorMessage(e) } });
    return null;
  }
}

export async function removeKey(): Promise<boolean> {
  try {
    const config = await api.aiConfigSet({ apiKey: "" });
    ai.set({ config, models: { ...NO_MODELS, state: "noKey" }, connection: NO_CONNECTION });
    return true;
  } catch (e) {
    ai.set({ connection: { phase: "done", result: null, error: errorMessage(e) } });
    return false;
  }
}

export async function removeStaleKey(): Promise<void> {
  try {
    ai.set({ config: await api.aiConfigSet({ removeStaleKey: true }) });
  } catch (e) {
    ai.set({ connection: { phase: "done", result: null, error: errorMessage(e) } });
  }
}

/** Asks NVIDIA, with a tiny real request, whether the selected model supports a feature. */
export async function probeFeature(feature: AiFeature): Promise<AiProbeResult | null> {
  const model = ai.get().config?.model;
  if (!model) return null;
  ai.set({ probing: feature });
  try {
    const result = await api.aiProbe(model, feature);
    await loadAiConfig();
    return result;
  } catch (e) {
    return { supported: "unknown", message: errorMessage(e) };
  } finally {
    ai.set({ probing: null });
  }
}

export function setAiOpen(open: boolean): void {
  ai.set({ open });
  if (open) void loadAiConfig();
}

export function toggleAi(): void {
  setAiOpen(!ai.get().open);
}

export function setIncludePage(includePage: boolean): void {
  ai.set({ includePage });
}

export function clearConversation(): void {
  const { requestId } = ai.get();
  if (requestId) void api.aiCancel(requestId).catch(() => {});
  ai.set({ messages: [], streaming: false, requestId: null });
}

export async function sendMessage(text: string, includePage?: boolean, source?: AiMessage["source"]): Promise<void> {
  const prompt = text.trim();
  const s = ai.get();
  if (!prompt || s.streaming) return;
  ensureListening();
  const requestId = `ai-${Date.now()}-${seq++}`;
  const history = s.messages.filter((m) => !m.error && m.content !== "").map(({ role, content }) => ({ role, content }));
  const messages: AiMessage[] = [
    ...s.messages,
    { id: seq++, role: "user", content: prompt },
    { id: seq++, role: "assistant", content: "", ...(source ? { source } : {}) },
  ];
  ai.set({ messages, streaming: true, requestId });
  try {
    await api.aiChat(requestId, [...history, { role: "user", content: prompt }], includePage ?? s.includePage);
  } catch (e) {
    finishWithError(errorMessage(e));
  }
}

export function stopStreaming(): void {
  const { requestId } = ai.get();
  if (requestId) void api.aiCancel(requestId).catch(() => {});
  ai.set({ streaming: false, requestId: null });
}

/** For tests: puts the store back to a clean state. */
export function resetAiStoreForTests(): void {
  ai.set({
    open: false,
    config: null,
    messages: [],
    streaming: false,
    includePage: false,
    requestId: null,
    models: NO_MODELS,
    connection: NO_CONNECTION,
    unavailable: [],
    probing: null,
  });
}
