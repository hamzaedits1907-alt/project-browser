import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import type { AiCapabilities, ConnectionState, ModelInfo } from "../../lib/types";
import {
  assistantReady,
  capabilityView,
  featureGate,
  groupByPublisher,
  isChatModel,
  isModelProblem,
  matchesQuery,
  resolveSelection,
  stateView,
  visibleModels,
} from "./aiModels";

function model(id: string, kind: ModelInfo["kind"] = "chat", chat: "yes" | "no" = "yes", vision: "yes" | "unknown" = "unknown"): ModelInfo {
  const [publisher = "", name = id] = id.split("/");
  return { id, publisher, name, kind, hints: { chat, vision, tools: "unknown" } };
}

const LIST: ModelInfo[] = [
  model("google/gemma-3-4b-it"),
  model("meta/llama-3.1-70b-instruct"),
  model("meta/llama-3.2-11b-vision-instruct", "vision", "yes", "yes"),
  model("nvidia/nv-embedqa-mistral-7b-v2", "embedding", "no"),
  model("nvidia/nemotron-parse", "document", "no"),
];

describe("parsing the real NVIDIA list (same fixture the Rust tests use)", () => {
  // The Rust side owns parsing; this checks the shape the shell receives would be handled by the pure helpers.
  const fixture = JSON.parse(readFileSync(join(process.cwd(), "src-tauri/tests/fixtures/nvidia-models-2026-10-10.json"), "utf8")) as {
    models: { data: { id: string; owned_by: string }[] };
    chatRouteStatus: Record<string, number>;
  };

  it("the fixture is the real 80-model answer", () => {
    expect(fixture.models.data).toHaveLength(80);
    expect(Object.keys(fixture.chatRouteStatus)).toHaveLength(80);
  });

  it("searching a realistic list finds models by publisher, name and capability words", () => {
    const models = fixture.models.data.map((m) => {
      const [publisher = "", name = m.id] = m.id.split("/");
      const embedding = /embed|nvclip/.test(name);
      return model(`${publisher}/${name}`, embedding ? "embedding" : /vision|neva|vila|fuyu/.test(name) ? "vision" : "chat", embedding ? "no" : "yes", /vision|neva|vila|fuyu/.test(name) ? "yes" : "unknown");
    });
    expect(visibleModels(models, { query: "nvidia nemotron", showAll: false }).shown.length).toBeGreaterThan(3);
    expect(visibleModels(models, { query: "vision", showAll: false }).shown.map((m) => m.id)).toContain("meta/llama-3.2-11b-vision-instruct");
    const hiddenEmbeddings = visibleModels(models, { query: "embed", showAll: false });
    expect(hiddenEmbeddings.shown).toHaveLength(0);
    expect(hiddenEmbeddings.hiddenNonChat).toBeGreaterThanOrEqual(5);
    expect(visibleModels(models, { query: "embed", showAll: true }).shown.length).toBe(hiddenEmbeddings.hiddenNonChat);
  });
});

describe("searching and grouping", () => {
  it("matches every word, in any order, case-insensitively", () => {
    const m = LIST[1] as ModelInfo;
    expect(matchesQuery(m, "")).toBe(true);
    expect(matchesQuery(m, "LLAMA 70b")).toBe(true);
    expect(matchesQuery(m, "70b meta")).toBe(true);
    expect(matchesQuery(m, "llama gemma")).toBe(false);
    expect(matchesQuery(m, "chat")).toBe(true);
  });

  it("hides models that cannot chat unless asked, and says how many it hid", () => {
    expect(visibleModels(LIST, { query: "", showAll: false })).toMatchObject({ hiddenNonChat: 2 });
    expect(visibleModels(LIST, { query: "", showAll: false }).shown.map(isChatModel)).toEqual([true, true, true]);
    expect(visibleModels(LIST, { query: "", showAll: true }).shown).toHaveLength(5);
    expect(visibleModels(LIST, { query: "zzz", showAll: false })).toEqual({ shown: [], hiddenNonChat: 0 });
  });

  it("finds vision models by the word 'vision' even when it is not in their name", () => {
    const m = model("acme/mystery-1", "vision", "yes", "yes");
    expect(matchesQuery(m, "image")).toBe(true);
  });

  it("groups by publisher in order of first appearance", () => {
    const groups = groupByPublisher(LIST);
    expect(groups.map((g) => [g.publisher, g.models.length])).toEqual([["google", 1], ["meta", 2], ["nvidia", 2]]);
    expect(groupByPublisher([model("A/x"), model("a/y")]).map((g) => g.models.length)).toEqual([2]);
    expect(groupByPublisher([{ ...model("x"), publisher: "" }])[0]?.publisher).toBe("Other");
  });
});

describe("model selection", () => {
  it("has no selection until one is made", () => {
    expect(resolveSelection(LIST, "")).toEqual({ status: "none" });
  });

  it("cannot judge a choice before the list is loaded", () => {
    expect(resolveSelection([], "meta/llama-3.1-70b-instruct")).toEqual({ status: "unchecked", id: "meta/llama-3.1-70b-instruct" });
  });

  it("accepts a chat model that is in the list", () => {
    const s = resolveSelection(LIST, "meta/llama-3.1-70b-instruct");
    expect(s.status).toBe("ready");
  });

  it("notices when the saved model has been retired from NVIDIA's list", () => {
    expect(resolveSelection(LIST, "meta/llama-3.1-8b-instruct")).toEqual({ status: "missing", id: "meta/llama-3.1-8b-instruct" });
  });

  it("refuses embedding and document models with a reason", () => {
    const s = resolveSelection(LIST, "nvidia/nv-embedqa-mistral-7b-v2");
    expect(s).toMatchObject({ status: "unsupported" });
    expect(s.status === "unsupported" && s.reason).toContain("Embeddings models can't hold a conversation");
  });

  it("refuses a model NVIDIA turned down during this session", () => {
    const s = resolveSelection(LIST, "google/gemma-3-4b-it", ["google/gemma-3-4b-it"]);
    expect(s).toMatchObject({ status: "unsupported" });
    expect(resolveSelection(LIST, "google/gemma-3-4b-it", [])).toMatchObject({ status: "ready" });
  });

  it("the assistant is ready only with a key and a usable model", () => {
    const ready = resolveSelection(LIST, "google/gemma-3-4b-it");
    expect(assistantReady(true, ready)).toBe(true);
    expect(assistantReady(false, ready)).toBe(false);
    expect(assistantReady(true, resolveSelection(LIST, ""))).toBe(false);
    expect(assistantReady(true, resolveSelection(LIST, "gone/model"))).toBe(false);
    expect(assistantReady(true, resolveSelection([], "google/gemma-3-4b-it"))).toBe(true);
  });
});

describe("capability gates", () => {
  const unknown: AiCapabilities = { chat: "unknown", vision: "unknown", tools: "unknown" };

  it("vision and tools stay off until a real check confirmed them", () => {
    expect(featureGate("vision", unknown)).toMatchObject({ enabled: false });
    expect(featureGate("vision", unknown).reason).toContain("hasn't been checked");
    expect(featureGate("tools", { ...unknown, tools: "no" })).toMatchObject({ enabled: false });
    expect(featureGate("tools", { ...unknown, tools: "no" }).reason).toContain("isn't supported");
    expect(featureGate("vision", { ...unknown, vision: "yes" })).toEqual({ enabled: true, reason: "" });
  });

  it("a guess from the model's name is shown as a guess and never enables a feature", () => {
    expect(capabilityView("yes", "unknown")).toEqual({ label: "Likely, from its name", tone: "neutral", confirmed: false });
    expect(featureGate("vision", unknown).enabled).toBe(false);
    expect(capabilityView("yes", "yes")).toMatchObject({ label: "Confirmed", confirmed: true });
    expect(capabilityView("yes", "no")).toMatchObject({ label: "Not supported", tone: "bad", confirmed: true });
    expect(capabilityView("unknown", "unknown").label).toBe("Not checked");
    expect(capabilityView("no", "unknown").tone).toBe("bad");
  });
});

describe("connection states", () => {
  const ALL: ConnectionState[] = ["ok", "noKey", "invalidKey", "rateLimited", "network", "timeout", "serverError", "badResponse", "empty", "modelUnavailable", "modelRejected", "other"];

  it("every state has its own headline and only 'ok' is good", () => {
    const headlines = new Set(ALL.map((s) => stateView(s).headline));
    expect(headlines.size).toBe(ALL.length - 0);
    expect(ALL.filter((s) => stateView(s).tone === "good")).toEqual(["ok"]);
    expect(stateView("invalidKey").tone).toBe("bad");
    expect(stateView("network").tone).toBe("bad");
  });

  it("tells a problem with the model apart from a problem with the connection", () => {
    expect(isModelProblem("modelUnavailable")).toBe(true);
    expect(isModelProblem("modelRejected")).toBe(true);
    for (const s of ALL.filter((s) => s !== "modelUnavailable" && s !== "modelRejected")) expect(isModelProblem(s)).toBe(false);
    expect(isModelProblem(null)).toBe(false);
    expect(isModelProblem(undefined)).toBe(false);
  });
});
