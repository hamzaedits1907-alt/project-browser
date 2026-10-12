// Pure logic for the NVIDIA model list: searching, grouping, validating a selection, and deciding which
// features a model may be used for. No React and no IPC, so it is fully unit-tested.

import type { AiCapabilities, ConnectionState, ModelInfo, ModelKind, Support } from "../../lib/types";

export const KIND_LABEL: Record<ModelKind, string> = {
  chat: "Chat",
  vision: "Vision",
  code: "Code",
  embedding: "Embeddings",
  safety: "Safety",
  document: "Documents",
};

/** A model the assistant can hold a conversation with. Embedding, safety and document models cannot. */
export function isChatModel(m: ModelInfo): boolean {
  return m.hints.chat !== "no";
}

/** Every word of the query must appear in the model's id, publisher or kind (case-insensitive). */
export function matchesQuery(m: ModelInfo, query: string): boolean {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return true;
  const vision = m.hints.vision === "yes" ? " vision image multimodal" : "";
  const haystack = `${m.id} ${m.publisher} ${KIND_LABEL[m.kind]}${vision}`.toLowerCase();
  return words.every((w) => haystack.includes(w));
}

export interface VisibleModels {
  shown: ModelInfo[];
  /** Models matching the search that are hidden because they cannot chat. */
  hiddenNonChat: number;
}

export function visibleModels(models: readonly ModelInfo[], opts: { query: string; showAll: boolean }): VisibleModels {
  const matching = models.filter((m) => matchesQuery(m, opts.query));
  if (opts.showAll) return { shown: matching, hiddenNonChat: 0 };
  const shown = matching.filter(isChatModel);
  return { shown, hiddenNonChat: matching.length - shown.length };
}

export interface ModelGroup {
  publisher: string;
  models: ModelInfo[];
}

/** Groups consecutive-or-not models by publisher, keeping the order of first appearance. */
export function groupByPublisher(models: readonly ModelInfo[]): ModelGroup[] {
  const groups = new Map<string, ModelGroup>();
  for (const m of models) {
    const key = m.publisher.toLowerCase();
    const group = groups.get(key);
    if (group) group.models.push(m);
    else groups.set(key, { publisher: m.publisher || "Other", models: [m] });
  }
  return [...groups.values()];
}

export type Selection =
  | { status: "none" }
  /** The list has not been loaded, so the choice cannot be checked yet. */
  | { status: "unchecked"; id: string }
  | { status: "ready"; model: ModelInfo }
  /** The saved choice is no longer in NVIDIA's list (retired). */
  | { status: "missing"; id: string }
  | { status: "unsupported"; id: string; model: ModelInfo | null; reason: string };

/** Decides whether the saved model can be used, given the current list and what failed earlier this session. */
export function resolveSelection(models: readonly ModelInfo[], selectedId: string, unavailable: readonly string[] = []): Selection {
  if (!selectedId) return { status: "none" };
  if (models.length === 0) return { status: "unchecked", id: selectedId };
  const model = models.find((m) => m.id === selectedId);
  if (!model) return { status: "missing", id: selectedId };
  if (!isChatModel(model)) {
    return { status: "unsupported", id: selectedId, model, reason: `${KIND_LABEL[model.kind]} models can't hold a conversation. Choose a chat model.` };
  }
  if (unavailable.includes(selectedId)) {
    return { status: "unsupported", id: selectedId, model, reason: "NVIDIA didn't accept this model for chat. Choose another one." };
  }
  return { status: "ready", model };
}

/** The assistant is ready when a key is saved and the chosen model (if the list is known) can chat. */
export function assistantReady(hasKey: boolean, selection: Selection): boolean {
  return hasKey && (selection.status === "ready" || selection.status === "unchecked");
}

export type Feature = "chat" | "vision" | "tools";

const FEATURE_NAME: Record<Feature, string> = { chat: "Chatting", vision: "Understanding images", tools: "Using tools" };

/**
 * Features that depend on a model capability are enabled only when a real check of the selected model
 * confirmed it. A hint from the model's name is never enough.
 */
export function featureGate(feature: Feature, verified: AiCapabilities): { enabled: boolean; reason: string } {
  const support = verified[feature];
  if (support === "yes") return { enabled: true, reason: "" };
  if (support === "no") return { enabled: false, reason: `${FEATURE_NAME[feature]} isn't supported by this model.` };
  return { enabled: false, reason: `${FEATURE_NAME[feature]} hasn't been checked for this model yet.` };
}

export interface CapabilityView {
  label: string;
  tone: "good" | "neutral" | "bad";
  /** A real check confirmed it (rather than a guess from the name). */
  confirmed: boolean;
}

/** How one capability of the selected model is shown: what a check proved, else what the name suggests. */
export function capabilityView(hint: Support, verified: Support): CapabilityView {
  if (verified === "yes") return { label: "Confirmed", tone: "good", confirmed: true };
  if (verified === "no") return { label: "Not supported", tone: "bad", confirmed: true };
  if (hint === "yes") return { label: "Likely, from its name", tone: "neutral", confirmed: false };
  if (hint === "no") return { label: "Not for this kind of model", tone: "bad", confirmed: false };
  return { label: "Not checked", tone: "neutral", confirmed: false };
}

export interface StateView {
  tone: "good" | "warn" | "bad" | "neutral";
  headline: string;
}

/** Short headline for each connection state; the longer explanation always comes from the core. */
export function stateView(state: ConnectionState): StateView {
  switch (state) {
    case "ok":
      return { tone: "good", headline: "Connected" };
    case "noKey":
      return { tone: "neutral", headline: "No API key yet" };
    case "invalidKey":
      return { tone: "bad", headline: "NVIDIA rejected the key" };
    case "rateLimited":
      return { tone: "warn", headline: "Rate limited" };
    case "network":
      return { tone: "bad", headline: "Can't reach NVIDIA" };
    case "timeout":
      return { tone: "warn", headline: "NVIDIA didn't answer in time" };
    case "serverError":
      return { tone: "warn", headline: "NVIDIA had a problem" };
    case "badResponse":
      return { tone: "warn", headline: "Unexpected answer" };
    case "empty":
      return { tone: "warn", headline: "No models available" };
    case "modelUnavailable":
      return { tone: "warn", headline: "Model not available" };
    case "modelRejected":
      return { tone: "warn", headline: "Model not supported" };
    default:
      return { tone: "warn", headline: "Something went wrong" };
  }
}

/** Models whose failure means the model (not the connection) is the problem. */
export function isModelProblem(state: ConnectionState | null | undefined): boolean {
  return state === "modelUnavailable" || state === "modelRejected";
}

export function formatCount(n: number, one: string, many: string): string {
  return `${n.toLocaleString()} ${n === 1 ? one : many}`;
}

// ---- the setup state machine -------------------------------------------------------------------

/** Where the user is on the way to a working assistant. The key can be entered at every one of these stages. */
export type SetupStage =
  /** The saved configuration hasn't been read yet. */
  | { stage: "loading"; canEnterKey: true }
  /** No key is saved. */
  | { stage: "noKey"; canEnterKey: true }
  /** Something is typed in the key field but not saved. */
  | { stage: "keyEntered"; canEnterKey: true }
  /** A key is saved; the model list hasn't been asked for yet. */
  | { stage: "keySaved"; canEnterKey: true }
  | { stage: "catalogLoading"; canEnterKey: true }
  /** NVIDIA rejected the key. */
  | { stage: "invalidKey"; canEnterKey: true }
  /** Rate limit, network or provider failure while loading the list or testing. */
  | { stage: "providerFailure"; canEnterKey: true; state: ConnectionState; message: string | null }
  /** The list loaded but holds nothing the assistant can chat with. */
  | { stage: "noUsableModels"; canEnterKey: true }
  /** The list is there and no model is chosen. */
  | { stage: "chooseModel"; canEnterKey: true }
  /** The chosen model is retired, not in the list, or not a chat model. */
  | { stage: "modelUnavailable"; canEnterKey: true; reason: string }
  /** A real request with the key succeeded, but no usable model is chosen yet. */
  | { stage: "verified"; canEnterKey: true }
  /** A usable model is chosen. `verified`: a real request with the key and model has succeeded. */
  | { stage: "ready"; canEnterKey: true; verified: boolean };

export interface SetupInput {
  /** `null` until the configuration has been read. */
  hasKey: boolean | null;
  keyTyped: boolean;
  models: { phase: "idle" | "loading" | "ready" | "empty" | "error"; models: readonly ModelInfo[]; state: ConnectionState | null; message: string | null };
  /** The latest Connect / Test result, if any. */
  connection: { phase: "idle" | "testing" | "done"; state: ConnectionState | null; keyOk: boolean };
  selectedModel: string;
  unavailable: readonly string[];
}

/**
 * Names the setup stage. Missing authentication is never reported as a model problem, and a model problem is never
 * reported as a missing key: the key field is usable in every stage (`canEnterKey` is always true).
 */
export function setupStage(input: SetupInput): SetupStage {
  const { hasKey, keyTyped, models, connection } = input;
  if (hasKey === null) return { stage: keyTyped ? "keyEntered" : "loading", canEnterKey: true };
  if (!hasKey) return { stage: keyTyped ? "keyEntered" : "noKey", canEnterKey: true };
  if (connection.state === "invalidKey" || (models.phase === "error" && models.state === "invalidKey")) return { stage: "invalidKey", canEnterKey: true };
  if (models.phase === "loading" && models.models.length === 0) return { stage: "catalogLoading", canEnterKey: true };
  if (models.phase === "error") return { stage: "providerFailure", canEnterKey: true, state: models.state ?? "other", message: models.message };
  if (models.phase === "idle") return { stage: "keySaved", canEnterKey: true };
  if (models.phase === "empty" || !models.models.some(isChatModel)) return { stage: "noUsableModels", canEnterKey: true };
  const verified = connection.phase === "done" && connection.state === "ok" && connection.keyOk;
  const selection = resolveSelection(models.models, input.selectedModel, input.unavailable);
  switch (selection.status) {
    case "none":
      return { stage: verified ? "verified" : "chooseModel", canEnterKey: true };
    case "missing":
      return { stage: "modelUnavailable", canEnterKey: true, reason: `“${selection.id}” is no longer in NVIDIA's list.` };
    case "unsupported":
      return { stage: "modelUnavailable", canEnterKey: true, reason: selection.reason };
    default:
      return { stage: "ready", canEnterKey: true, verified };
  }
}

/** Failures that say nothing about the key itself: saving the key anyway (unchecked) is a reasonable choice. */
export function isTransientFailure(state: ConnectionState | null | undefined): boolean {
  return state === "network" || state === "timeout" || state === "rateLimited" || state === "serverError" || state === "badResponse" || state === "other";
}
