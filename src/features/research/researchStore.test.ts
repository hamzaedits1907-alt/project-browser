import { beforeEach, describe, expect, it, vi } from "vitest";

import type { AiEvent, PageSnapshotView } from "../../lib/types";

const handlers: ((e: AiEvent) => void)[] = [];

const api = vi.hoisted(() => ({
  aiPageSnapshot: vi.fn(),
  aiAsk: vi.fn(),
  aiCancel: vi.fn(),
}));

vi.mock("../../lib/ipc", async (original) => {
  const real = await original<typeof import("../../lib/ipc")>();
  return {
    ...real,
    api,
    on: vi.fn(async (name: string, handler: (e: AiEvent) => void) => {
      if (name === "pb:ai") handlers.push(handler);
      return () => void handlers.splice(handlers.indexOf(handler), 1);
    }),
  };
});

import { cancelResearch, research, resetResearch, runBlocker, runResearch, setMode, setQuestion, setSelected, toggleTab } from "./researchStore";

const TEXT_A = "The card has 12 GB of GDDR6X memory and draws 200 W under typical load.";
const TEXT_B = "The vendor lists 16 GB of memory for this card in its datasheet.";

function snap(tabId: number, text: string): PageSnapshotView {
  return { tabId, title: `Page ${tabId}`, url: `https://site${tabId}.example/page`, text, selection: "", totalChars: text.length, truncated: false };
}

const TABS = [
  { id: 1, title: "A", url: "https://site1.example/page" },
  { id: 2, title: "B", url: "https://site2.example/page" },
  { id: 3, title: "C", url: "https://site3.example/page" },
];

/** Plays a model reply back through the same events the core emits. */
function modelReplies(reply: string | { error: string }) {
  api.aiAsk.mockImplementation(async (requestId: string) => {
    queueMicrotask(() => {
      for (const h of [...handlers]) {
        if (typeof reply === "string") {
          h({ requestId, kind: "delta", text: reply.slice(0, 20) });
          h({ requestId, kind: "delta", text: reply.slice(20) });
          h({ requestId, kind: "done", text: "" });
        } else {
          h({ requestId, kind: "error", text: reply.error, state: "rateLimited" });
        }
      }
    });
  });
}

beforeEach(() => {
  resetResearch();
  handlers.length = 0;
  for (const fn of Object.values(api)) fn.mockReset();
  api.aiCancel.mockResolvedValue(undefined);
});

describe("choosing what to run", () => {
  it("needs two pages to compare and a question to check a claim", () => {
    expect(runBlocker("compare", "", 1)).toContain("at least two");
    expect(runBlocker("compare", "", 2)).toBeNull();
    expect(runBlocker("claim", "", 1)).toContain("question");
    expect(runBlocker("claim", "does it have 16 GB?", 0)).toContain("at least one");
    expect(runBlocker("claim", "does it have 16 GB?", 1)).toBeNull();
  });

  it("keeps exactly the tabs the user ticked, in order", () => {
    toggleTab(2);
    toggleTab(1);
    toggleTab(2);
    toggleTab(3);
    expect(research.get().selected).toEqual([1, 3]);
  });
});

describe("running a comparison", () => {
  it("reads only the ticked tabs, sends only their text, and checks the answer against it", async () => {
    setMode("compare");
    setSelected([1, 2]);
    api.aiPageSnapshot.mockImplementation(async (id: number) => snap(id, id === 1 ? TEXT_A : TEXT_B));
    modelReplies(JSON.stringify({ summary: "Disagree.", claims: [{ text: "12 GB", sources: ["S1"], quote: "has 12 GB of GDDR6X memory" }, { text: "16 GB", sources: ["S2"], quote: "lists 16 GB of memory for this card" }] }));

    await runResearch(TABS, 24_000);

    expect(api.aiPageSnapshot.mock.calls.map((c) => c[0])).toEqual([1, 2]);
    expect(api.aiPageSnapshot.mock.calls.every((c) => typeof c[1] === "number" && c[1] <= 24_000)).toBe(true);
    const [, system, messages, maxTokens] = api.aiAsk.mock.calls[0]!;
    expect(system).toContain("DATA");
    expect(messages[0].content).toContain(TEXT_A);
    expect(messages[0].content).toContain(TEXT_B);
    expect(maxTokens).toBe(3000);
    const state = research.get();
    expect(state.phase).toBe("done");
    expect(state.result?.mode).toBe("compare");
    expect(state.result && state.result.mode === "compare" && state.result.claims.every((c) => c.grounded)).toBe(true);
    expect(state.estimatedTokens).toBeGreaterThan(0);
  });

  it("leaves out a tab the core refuses and carries on with the rest, saying why", async () => {
    setSelected([1, 2, 3]);
    api.aiPageSnapshot.mockImplementation(async (id: number) => {
      if (id === 2) throw "Private tabs are never shared with the assistant.";
      return snap(id, id === 1 ? TEXT_A : TEXT_B);
    });
    modelReplies(JSON.stringify({ claims: [] }));
    await runResearch(TABS, 24_000);
    expect(research.get().phase).toBe("done");
    expect(research.get().skipped).toEqual([{ tabId: 2, title: "B", reason: "Private tabs are never shared with the assistant." }]);
    expect(research.get().sources.map((s) => s.tabId)).toEqual([1, 3]);
    expect(api.aiAsk.mock.calls[0]![2][0].content).not.toContain("S3:");
  });

  it("stops with an explanation when too few pages can be read, and asks the model nothing", async () => {
    setSelected([1, 2]);
    api.aiPageSnapshot.mockRejectedValue("That tab is asleep. Open it first, then try again.");
    await runResearch(TABS, 24_000);
    expect(research.get().phase).toBe("error");
    expect(research.get().error).toContain("Not enough pages");
    expect(api.aiAsk).not.toHaveBeenCalled();
  });

  it("shows the provider's refusal as an error, never as findings", async () => {
    setSelected([1, 2]);
    api.aiPageSnapshot.mockImplementation(async (id: number) => snap(id, TEXT_A));
    modelReplies({ error: "NVIDIA is limiting requests right now (rate limit)." });
    await runResearch(TABS, 24_000);
    expect(research.get()).toMatchObject({ phase: "error", result: null, error: "NVIDIA is limiting requests right now (rate limit)." });
  });

  it("shows a reply that isn't structured as unchecked text, with no result", async () => {
    setSelected([1, 2]);
    api.aiPageSnapshot.mockImplementation(async (id: number) => snap(id, TEXT_A));
    modelReplies("I think both pages say the same thing.");
    await runResearch(TABS, 24_000);
    const state = research.get();
    expect(state.phase).toBe("error");
    expect(state.result).toBeNull();
    expect(state.raw).toBe("I think both pages say the same thing.");
  });

  it("refuses to start without the pages or question it needs", async () => {
    setMode("claim");
    setSelected([1]);
    setQuestion("hi");
    await runResearch(TABS, 24_000);
    expect(research.get().phase).toBe("error");
    expect(api.aiPageSnapshot).not.toHaveBeenCalled();
  });

  it("can be cancelled while the model is working, and keeps nothing", async () => {
    setSelected([1, 2]);
    api.aiPageSnapshot.mockImplementation(async (id: number) => snap(id, TEXT_A));
    api.aiAsk.mockResolvedValue(undefined); // the model never answers
    const running = runResearch(TABS, 24_000);
    await vi.waitFor(() => expect(research.get().phase).toBe("asking"));
    cancelResearch();
    await running;
    expect(api.aiCancel).toHaveBeenCalled();
    expect(research.get()).toMatchObject({ phase: "idle", result: null });
    expect(handlers).toHaveLength(0);
  });

  it("never puts a key or a request header anywhere in its state", async () => {
    setSelected([1, 2]);
    api.aiPageSnapshot.mockImplementation(async (id: number) => snap(id, TEXT_A));
    modelReplies(JSON.stringify({ claims: [] }));
    await runResearch(TABS, 24_000);
    expect(JSON.stringify(research.get())).not.toMatch(/nvapi|bearer|authorization/i);
  });
});
