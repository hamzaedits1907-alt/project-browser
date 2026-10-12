import { beforeEach, describe, expect, it, vi } from "vitest";

import type { AiConfigView, AiEvent, AiTestResult, ModelInfo, ModelsResponse } from "../../lib/types";

const handlers: { ai?: (e: AiEvent) => void } = {};

const api = vi.hoisted(() => ({
  aiConfigGet: vi.fn(),
  aiConfigSet: vi.fn(),
  aiModels: vi.fn(),
  aiTest: vi.fn(),
  aiProbe: vi.fn(),
  aiChat: vi.fn(),
  aiCancel: vi.fn(),
}));

vi.mock("../../lib/ipc", async (original) => {
  const real = await original<typeof import("../../lib/ipc")>();
  return {
    ...real,
    api,
    on: vi.fn(async (name: string, handler: (e: AiEvent) => void) => {
      if (name === "pb:ai") handlers.ai = handler;
      return () => {};
    }),
  };
});

import { ai, loadAiConfig, loadModels, probeFeature, removeKey, resetAiStoreForTests, saveKey, selectModel, sendMessage, testSaved } from "./aiStore";

const KEY = "nvapi-super-secret-key-1234567890";

function config(over: Partial<AiConfigView> = {}): AiConfigView {
  return { provider: "nvidia", model: "", hasKey: false, staleKey: false, verified: { chat: "unknown", vision: "unknown", tools: "unknown" }, ...over };
}

function model(id: string, chat: "yes" | "no" = "yes"): ModelInfo {
  const [publisher = "", name = id] = id.split("/");
  return { id, publisher, name, kind: chat === "yes" ? "chat" : "embedding", hints: { chat, vision: "unknown", tools: "unknown" } };
}

function models(over: Partial<ModelsResponse> = {}): ModelsResponse {
  return { state: "ok", message: null, models: [model("google/gemma-3-4b-it"), model("meta/llama-3.1-70b-instruct")], stale: false, fetchedAt: 1_700_000_000_000, ...over };
}

function test(over: Partial<AiTestResult> = {}): AiTestResult {
  return { state: "ok", message: "Connected.", keyOk: true, modelCount: 2, testedModel: "google/gemma-3-4b-it", latencyMs: 200, ...over };
}

beforeEach(() => {
  resetAiStoreForTests();
  for (const fn of Object.values(api)) fn.mockReset();
  api.aiConfigGet.mockResolvedValue(config());
});

describe("loading the model list", () => {
  it("asks for nothing without a key", async () => {
    await loadModels();
    expect(api.aiModels).not.toHaveBeenCalled();
    expect(ai.get().models).toMatchObject({ phase: "idle", state: "noKey", models: [] });
  });

  it("goes loading → ready with NVIDIA's models", async () => {
    api.aiConfigGet.mockResolvedValue(config({ hasKey: true }));
    let during: string | null = null;
    api.aiModels.mockImplementation(async () => {
      during = ai.get().models.phase;
      return models();
    });
    await loadModels();
    expect(during).toBe("loading");
    expect(ai.get().models).toMatchObject({ phase: "ready", state: "ok", stale: false });
    expect(ai.get().models.models.map((m) => m.id)).toEqual(["google/gemma-3-4b-it", "meta/llama-3.1-70b-instruct"]);
    expect(api.aiModels).toHaveBeenCalledWith(false);
  });

  it("passes refresh through when the user asks for a fresh list", async () => {
    api.aiConfigGet.mockResolvedValue(config({ hasKey: true }));
    api.aiModels.mockResolvedValue(models());
    await loadModels(true);
    expect(api.aiModels).toHaveBeenCalledWith(true);
  });

  it("reports an empty list as empty, not as an error", async () => {
    api.aiConfigGet.mockResolvedValue(config({ hasKey: true }));
    api.aiModels.mockResolvedValue(models({ state: "empty", message: "NVIDIA returned an empty model list.", models: [] }));
    await loadModels();
    expect(ai.get().models).toMatchObject({ phase: "empty", state: "empty" });
  });

  it("keeps the last good list when a refresh fails, and says why it is old", async () => {
    api.aiConfigGet.mockResolvedValue(config({ hasKey: true }));
    api.aiModels.mockResolvedValue(models({ state: "network", message: "Couldn't reach NVIDIA.", stale: true }));
    await loadModels(true);
    expect(ai.get().models).toMatchObject({ phase: "ready", stale: true, state: "network", message: "Couldn't reach NVIDIA." });
    expect(ai.get().models.models).toHaveLength(2);
  });

  it("shows network, invalid-key and timeout failures as errors with NVIDIA's explanation", async () => {
    api.aiConfigGet.mockResolvedValue(config({ hasKey: true }));
    for (const state of ["network", "invalidKey", "timeout"] as const) {
      api.aiModels.mockResolvedValue(models({ state, message: `message for ${state}`, models: [] }));
      await loadModels(true);
      expect(ai.get().models).toMatchObject({ phase: "error", state, message: `message for ${state}`, models: [] });
    }
  });

  it("turns a command failure into an error state instead of throwing", async () => {
    api.aiConfigGet.mockResolvedValue(config({ hasKey: true }));
    api.aiModels.mockRejectedValue("The credential store is unavailable");
    await loadModels();
    expect(ai.get().models).toMatchObject({ phase: "error", message: "The credential store is unavailable" });
  });
});

describe("choosing a model", () => {
  it("saves the choice in the browser's preferences and keeps no key anywhere in the call", async () => {
    api.aiConfigSet.mockResolvedValue(config({ hasKey: true, model: "meta/llama-3.1-70b-instruct" }));
    expect(await selectModel("meta/llama-3.1-70b-instruct")).toBe(true);
    expect(api.aiConfigSet).toHaveBeenCalledWith({ model: "meta/llama-3.1-70b-instruct" });
    expect(ai.get().config?.model).toBe("meta/llama-3.1-70b-instruct");
  });

  it("forgets that a model was refused once the user picks it again", async () => {
    ai.set({ unavailable: ["a/b", "c/d"] });
    api.aiConfigSet.mockResolvedValue(config({ model: "a/b" }));
    await selectModel("a/b");
    expect(ai.get().unavailable).toEqual(["c/d"]);
  });

  it("reports a failed save instead of pretending it worked", async () => {
    api.aiConfigSet.mockRejectedValue("That isn't a valid model id. Choose a model from the list.");
    expect(await selectModel("bad id")).toBe(false);
    expect(ai.get().connection.error).toContain("valid model id");
  });
});

describe("saving a key", () => {
  it("only stores the key: no test, no model list, no model choice is needed first", async () => {
    api.aiConfigSet.mockResolvedValue(config({ hasKey: true }));
    expect(await saveKey(KEY)).toBe(true);
    expect(api.aiConfigSet).toHaveBeenCalledTimes(1);
    expect(api.aiConfigSet).toHaveBeenCalledWith({ apiKey: KEY });
    expect(api.aiTest).not.toHaveBeenCalled();
    expect(api.aiModels).not.toHaveBeenCalled();
    expect(ai.get().config?.hasKey).toBe(true);
  });

  it("keeps the key when the model catalog cannot be loaded (offline, provider down)", async () => {
    api.aiConfigSet.mockResolvedValue(config({ hasKey: true }));
    expect(await saveKey(KEY)).toBe(true);
    for (const state of ["network", "timeout", "serverError", "rateLimited"] as const) {
      api.aiModels.mockResolvedValue(models({ state, models: [], message: state }));
      await loadModels(true);
      expect(ai.get().models.phase).toBe("error");
      expect(ai.get().config?.hasKey).toBe(true);
    }
    api.aiModels.mockRejectedValue("offline");
    await loadModels(true);
    expect(ai.get().config?.hasKey).toBe(true);
    expect(api.aiConfigSet).toHaveBeenCalledTimes(1);
  });

  it("keeps the key when the previously selected model is retired or no model answers", async () => {
    api.aiConfigSet.mockResolvedValue(config({ hasKey: true, model: "old/model" }));
    api.aiConfigGet.mockResolvedValue(config({ hasKey: true, model: "old/model" }));
    expect(await saveKey(KEY)).toBe(true);
    api.aiTest.mockResolvedValue(test({ state: "modelUnavailable", keyOk: false, message: "retired" }));
    await testSaved();
    api.aiTest.mockResolvedValue(test({ state: "unconfirmed", keyOk: false, message: "none answered" }));
    await testSaved();
    expect(ai.get().config?.hasKey).toBe(true);
    expect(api.aiConfigSet).toHaveBeenCalledTimes(1);
  });

  it("an authentication failure from a later test does not delete the saved key either", async () => {
    api.aiConfigSet.mockResolvedValue(config({ hasKey: true }));
    api.aiConfigGet.mockResolvedValue(config({ hasKey: true }));
    await saveKey(KEY);
    api.aiTest.mockResolvedValue(test({ state: "invalidKey", keyOk: false, message: "rejected" }));
    await testSaved();
    expect(ai.get().connection.result?.state).toBe("invalidKey");
    expect(ai.get().config?.hasKey).toBe(true);
  });

  it("a key that is not shaped like a key is refused with the reason, and nothing is stored", async () => {
    api.aiConfigSet.mockRejectedValue("That looks like an OpenAI or Anthropic key. This browser only works with NVIDIA keys, which start with nvapi-.");
    expect(await saveKey("sk-abc")).toBe(false);
    expect(ai.get().connection.error).toContain("NVIDIA keys");
    expect(ai.get().config).toBeNull();
  });

  it("a failing credential store is reported and does not pretend a key exists", async () => {
    api.aiConfigSet.mockRejectedValue("The Windows Credential Manager refused the key");
    expect(await saveKey(KEY)).toBe(false);
    expect(ai.get().connection.error).toContain("Credential Manager");
  });

  it("works from a never-configured state with no model chosen", async () => {
    api.aiConfigGet.mockResolvedValue(config());
    await loadAiConfig();
    expect(ai.get().config).toMatchObject({ hasKey: false, model: "" });
    api.aiConfigSet.mockResolvedValue(config({ hasKey: true }));
    expect(await saveKey(KEY)).toBe(true);
    expect(ai.get().config).toMatchObject({ hasKey: true, model: "" });
  });

  it("never keeps the key in the store", async () => {
    api.aiConfigSet.mockResolvedValue(config({ hasKey: true }));
    await saveKey(KEY);
    expect(JSON.stringify(ai.get())).not.toContain(KEY);
    api.aiConfigSet.mockRejectedValue("failed");
    await saveKey(KEY);
    expect(JSON.stringify(ai.get())).not.toContain(KEY);
  });
});

describe("testing and removing", () => {
  it("tests the saved key without sending a key, and marks a refused model", async () => {
    ai.set({ config: config({ hasKey: true, model: "old/model" }) });
    api.aiTest.mockResolvedValue(test({ state: "modelUnavailable", keyOk: true, message: "retired" }));
    api.aiConfigGet.mockResolvedValue(config({ hasKey: true, model: "old/model" }));
    await testSaved();
    expect(api.aiTest).toHaveBeenCalledWith();
    expect(ai.get().unavailable).toEqual(["old/model"]);
  });

  it("removing the key clears the model list and the test result", async () => {
    ai.set({ config: config({ hasKey: true }), models: { phase: "ready", models: [model("a/b")], state: "ok", message: null, stale: false, fetchedAt: 1 } });
    api.aiConfigSet.mockResolvedValue(config({ hasKey: false }));
    expect(await removeKey()).toBe(true);
    expect(api.aiConfigSet).toHaveBeenCalledWith({ apiKey: "" });
    expect(ai.get().models).toMatchObject({ phase: "idle", models: [], state: "noKey" });
    expect(ai.get().config?.hasKey).toBe(false);
  });

  it("capability checks call the core for the selected model and refresh what is known", async () => {
    ai.set({ config: config({ hasKey: true, model: "meta/llama-3.2-11b-vision-instruct" }) });
    api.aiProbe.mockResolvedValue({ supported: "yes", message: "Confirmed" });
    api.aiConfigGet.mockResolvedValue(config({ hasKey: true, model: "meta/llama-3.2-11b-vision-instruct", verified: { chat: "unknown", vision: "yes", tools: "unknown" } }));
    const r = await probeFeature("vision");
    expect(api.aiProbe).toHaveBeenCalledWith("meta/llama-3.2-11b-vision-instruct", "vision");
    expect(r?.supported).toBe("yes");
    expect(ai.get().config?.verified.vision).toBe("yes");
    expect(ai.get().probing).toBeNull();
  });

  it("a failed capability check is reported as unknown, never as supported", async () => {
    ai.set({ config: config({ hasKey: true, model: "a/b" }) });
    api.aiProbe.mockRejectedValue("Add your NVIDIA API key in AI settings first");
    expect(await probeFeature("tools")).toEqual({ supported: "unknown", message: "Add your NVIDIA API key in AI settings first" });
    expect(await (async () => { ai.set({ config: config() }); return probeFeature("tools"); })()).toBeNull();
  });
});

describe("chat errors", () => {
  async function startChat(): Promise<string> {
    await loadAiConfig(); // registers the event listener
    api.aiChat.mockResolvedValue(undefined);
    ai.set({ config: config({ hasKey: true, model: "old/model" }) });
    await sendMessage("hello");
    const id = ai.get().requestId;
    expect(id).toBeTruthy();
    return id as string;
  }

  it("shows a refusal from the command (no model chosen, no key) as an error message", async () => {
    api.aiChat.mockRejectedValue("Choose a model in AI settings first");
    await sendMessage("hi");
    const last = ai.get().messages.at(-1);
    expect(last).toMatchObject({ role: "assistant", error: true, content: "Choose a model in AI settings first" });
    expect(ai.get().streaming).toBe(false);
  });

  it("marks a model NVIDIA refuses mid-conversation as unavailable", async () => {
    const id = await startChat();
    handlers.ai?.({ requestId: id, kind: "error", text: "NVIDIA's hosted API doesn't offer this model for chat.", state: "modelUnavailable" });
    expect(ai.get().unavailable).toEqual(["old/model"]);
    expect(ai.get().messages.at(-1)).toMatchObject({ error: true });
    expect(ai.get().streaming).toBe(false);
  });

  it("does not blame the model for an invalid key or a network problem", async () => {
    for (const state of ["invalidKey", "network", "rateLimited"] as const) {
      resetAiStoreForTests();
      const id = await startChat();
      handlers.ai?.({ requestId: id, kind: "error", text: state, state });
      expect(ai.get().unavailable).toEqual([]);
    }
  });

  it("streams text into the reply and ignores events of other requests", async () => {
    const id = await startChat();
    handlers.ai?.({ requestId: "someone-else", kind: "delta", text: "NO" });
    handlers.ai?.({ requestId: id, kind: "delta", text: "Hel" });
    handlers.ai?.({ requestId: id, kind: "delta", text: "lo" });
    handlers.ai?.({ requestId: id, kind: "done", text: "" });
    expect(ai.get().messages.at(-1)).toMatchObject({ role: "assistant", content: "Hello" });
    expect(ai.get().streaming).toBe(false);
  });
});
