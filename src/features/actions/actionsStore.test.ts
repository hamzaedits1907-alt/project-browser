import { beforeEach, describe, expect, it, vi } from "vitest";

import type { AiEvent, Tab } from "../../lib/types";

const handlers: ((e: AiEvent) => void)[] = [];

const api = vi.hoisted(() => ({
  aiAsk: vi.fn(),
  aiCancel: vi.fn(),
  aiPageSnapshot: vi.fn(),
  tabNew: vi.fn(),
  bookmarksAll: vi.fn(),
  memoryList: vi.fn(),
  memorySave: vi.fn(),
  pageStyle: vi.fn(),
}));

const browser = vi.hoisted(() => ({ tab: null as Partial<Tab> | null }));

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

vi.mock("../../lib/store", () => ({
  getState: () => ({ browser: { tabs: browser.tab ? [browser.tab] : [], activeId: browser.tab?.id ?? null } }),
  activeTab: (s: { browser: { tabs: Tab[]; activeId: number | null } }) => s.browser.tabs.find((t) => t.id === s.browser.activeId) ?? null,
}));

import { actions, cancelActions, clearActionHistory, confirmPlan, contextOf, discardPlan, planRequest, toggleStep } from "./actionsStore";

function plannerSays(reply: string) {
  api.aiAsk.mockImplementation(async (requestId: string) => {
    queueMicrotask(() => {
      for (const h of [...handlers]) {
        h({ requestId, kind: "delta", text: reply });
        h({ requestId, kind: "done", text: "" });
      }
    });
  });
}

const WEB: Partial<Tab> = { id: 5, url: "https://example.com/a", title: "A", private: false };

beforeEach(() => {
  handlers.length = 0;
  for (const fn of Object.values(api)) fn.mockReset();
  api.aiCancel.mockResolvedValue(undefined);
  api.tabNew.mockResolvedValue(undefined);
  browser.tab = WEB;
  clearActionHistory();
  discardPlan();
});

describe("planning", () => {
  it("shows the plan for review and runs nothing", async () => {
    plannerSays(JSON.stringify({ summary: "Open two pages", steps: [{ tool: "open_pages", args: { urls: ["https://example.com/1", "https://example.org/2"] }, why: "research" }] }));
    await planRequest("open two pages about trains");
    expect(actions.get().phase).toBe("review");
    expect(actions.get().plan?.steps).toHaveLength(1);
    expect(actions.get().enabled).toEqual([true]);
    expect(api.tabNew).not.toHaveBeenCalled();
  });

  it("sends the planner no page content, only the request and where the user is", async () => {
    plannerSays(JSON.stringify({ summary: "x", steps: [] }));
    await planRequest("summarize this");
    const [, system, messages] = api.aiAsk.mock.calls[0]!;
    expect(system).toContain("The user is looking at a web page.");
    expect(JSON.stringify(messages)).not.toContain("example.com");
    expect(api.aiPageSnapshot).not.toHaveBeenCalled();
  });

  it("rejects a plan with a tool that isn't allowed, and shows why", async () => {
    plannerSays(JSON.stringify({ summary: "x", steps: [{ tool: "run_powershell", args: { command: "del *" } }] }));
    await planRequest("clean my disk");
    expect(actions.get()).toMatchObject({ phase: "error", plan: null });
    expect(actions.get().error).toContain("isn't something the browser allows");
  });

  it("starts steps that can't run here unticked: page work in a private tab or on a browser page", async () => {
    browser.tab = { ...WEB, private: true };
    plannerSays(JSON.stringify({ summary: "x", steps: [{ tool: "summarize_page", args: {} }, { tool: "search_bookmarks", args: { query: "a" } }] }));
    await planRequest("summarize this and search");
    expect(actions.get().enabled).toEqual([false, true]);
    browser.tab = { id: 6, url: "browser://settings", title: "Settings", private: false };
    await planRequest("summarize this");
    expect(actions.get().enabled).toEqual([false, true]);
  });

  it("reports the provider's failure instead of a plan", async () => {
    api.aiAsk.mockImplementation(async (requestId: string) => {
      queueMicrotask(() => handlers.forEach((h) => h({ requestId, kind: "error", text: "NVIDIA rejected the API key.", state: "invalidKey" })));
    });
    await planRequest("open a page");
    expect(actions.get()).toMatchObject({ phase: "error", error: "NVIDIA rejected the API key." });
  });

  it("ignores a request that is too short", async () => {
    await planRequest("  ");
    expect(actions.get().phase).toBe("error");
    expect(api.aiAsk).not.toHaveBeenCalled();
  });

  it("can be cancelled while planning", async () => {
    api.aiAsk.mockResolvedValue(undefined);
    const planning = planRequest("open some pages");
    await vi.waitFor(() => expect(api.aiAsk).toHaveBeenCalled());
    cancelActions();
    await planning;
    expect(actions.get().phase).toBe("idle");
    expect(api.aiCancel).toHaveBeenCalled();
  });
});

describe("confirming", () => {
  it("runs only the steps left ticked, reports each, and records history without results", async () => {
    plannerSays(
      JSON.stringify({
        summary: "Open and search",
        steps: [
          { tool: "open_pages", args: { urls: ["https://example.com/1"] } },
          { tool: "search_bookmarks", args: { query: "rust" } },
        ],
      }),
    );
    api.bookmarksAll.mockResolvedValue([{ id: 1, parentId: 1, kind: "url", title: "Rust book", url: "https://doc.rust-lang.org/book/", position: 0, createdAt: 0 }]);
    await planRequest("open and search");
    toggleStep(0);
    await confirmPlan(24_000);
    expect(api.tabNew).not.toHaveBeenCalled();
    const state = actions.get();
    expect(state.phase).toBe("done");
    expect(state.outcomes.map((o) => o?.status)).toEqual(["skipped", "done"]);
    expect(state.history).toHaveLength(1);
    expect(state.history[0]!.steps.map((s) => s.status)).toEqual(["skipped", "done"]);
    expect(JSON.stringify(state.history)).not.toContain("Rust book");
  });

  it("opens pages in the background, and as private tabs when the user is in one", async () => {
    plannerSays(JSON.stringify({ summary: "x", steps: [{ tool: "open_pages", args: { urls: ["https://example.com/1"] } }] }));
    await planRequest("open it");
    await confirmPlan(24_000);
    expect(api.tabNew).toHaveBeenLastCalledWith("https://example.com/1", false, true);

    browser.tab = { ...WEB, private: true };
    await planRequest("open it");
    await confirmPlan(24_000);
    expect(api.tabNew).toHaveBeenLastCalledWith("https://example.com/1", true, true);
  });

  it("does nothing when there is no reviewed plan", async () => {
    await confirmPlan(24_000);
    expect(actions.get().phase).toBe("idle");
  });
});

describe("the page context", () => {
  it("tells the planner where the user is", () => {
    expect(contextOf(null)).toBe("none");
    expect(contextOf({ url: "https://a.example", private: false })).toBe("web");
    expect(contextOf({ url: "browser://history", private: false })).toBe("browser");
    expect(contextOf({ url: "https://a.example", private: true })).toBe("private");
  });
});
