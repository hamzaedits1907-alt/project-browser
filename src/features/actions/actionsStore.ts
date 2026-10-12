import { createStore } from "../../lib/createStore";
import { api, errorMessage } from "../../lib/ipc";
import { readLocal, writeLocal } from "../../lib/storage";
import { activeTab, getState } from "../../lib/store";
import type { Tab } from "../../lib/types";
import { isInternalUrl } from "../../lib/urls";
import { ask, AskError } from "../ai/ask";
import { setQuestion } from "../research/researchStore";
import { Canceller, addHistory, historyEntry, parsePlan, plannerMessages, plannerSystem, runPlan, stepBlocker } from "./actions";
import type { ActionDeps, ActiveTabInfo, HistoryEntry, PageContext, Plan, StepOutcome } from "./actions";

export interface ActionsState {
  phase: "idle" | "planning" | "review" | "running" | "done" | "error";
  request: string;
  plan: Plan | null;
  /** Which steps the user left ticked. Only these run. */
  enabled: boolean[];
  outcomes: (StepOutcome | undefined)[];
  error: string | null;
  history: HistoryEntry[];
}

const HISTORY_KEY = "ai.actionHistory";

export const actions = createStore<ActionsState>({
  phase: "idle",
  request: "",
  plan: null,
  enabled: [],
  outcomes: [],
  error: null,
  history: readLocal<HistoryEntry[]>(HISTORY_KEY, []),
});

let canceller: Canceller | null = null;
let planning: { cancel: () => void } | null = null;

export function contextOf(tab: Pick<Tab, "url" | "private"> | null): PageContext {
  if (!tab) return "none";
  if (tab.private) return "private";
  return isInternalUrl(tab.url) ? "browser" : "web";
}

function activeInfo(): ActiveTabInfo | null {
  const tab = activeTab(getState());
  if (!tab) return null;
  return { id: tab.id, url: tab.url, title: tab.title, private: tab.private, web: !isInternalUrl(tab.url) };
}

/** The real browser, as the actions see it. Every capability here is a typed core command; there is no other way out. */
export function liveDeps(maxContext: number): ActionDeps {
  return {
    active: activeInfo,
    openPage: (url, o) => api.tabNew(url, o.private, o.background),
    bookmarks: () => api.bookmarksAll(),
    memorySearch: (q) => api.memoryList(q, 15),
    memorySave: (d) => api.memorySave(d),
    snapshot: (tabId, maxChars) => api.aiPageSnapshot(tabId, maxChars),
    ask: (system, messages, maxTokens) => ask(system, messages, { maxTokens }),
    setStyle: (tabId, css) => api.pageStyle(tabId, css),
    openResearch: (question) => {
      setQuestion(question);
      void api.tabNew("browser://research");
    },
    maxContext,
  };
}

export async function planRequest(request: string): Promise<void> {
  const text = request.trim();
  if (text.length < 3) {
    actions.set({ phase: "error", error: "Describe what you want done.", request });
    return;
  }
  actions.set({ phase: "planning", request: text, plan: null, enabled: [], outcomes: [], error: null });
  const handle = ask(plannerSystem(contextOf(activeTab(getState()))), plannerMessages(text), { maxTokens: 1200 });
  planning = handle;
  try {
    const outcome = parsePlan(await handle.promise);
    if (!outcome.ok) {
      actions.set({ phase: "error", error: outcome.error });
      return;
    }
    const context = contextOf(activeTab(getState()));
    // Steps that can't run here start unticked, with the reason shown.
    const enabled = outcome.plan.steps.map((s) => stepBlocker(s, context) === null);
    actions.set({ phase: "review", plan: outcome.plan, enabled, outcomes: [] });
  } catch (e) {
    if (e instanceof AskError && e.cancelled) return;
    actions.set({ phase: "error", error: errorMessage(e) });
  } finally {
    planning = null;
  }
}

export function toggleStep(index: number): void {
  actions.set((s) => ({ enabled: s.enabled.map((on, i) => (i === index ? !on : on)) }));
}

/** Runs the plan the user confirmed, with only the steps they left ticked. */
export async function confirmPlan(maxContext: number): Promise<void> {
  const { plan, enabled, request } = actions.get();
  if (!plan || actions.get().phase !== "review") return;
  const c = new Canceller();
  canceller = c;
  actions.set({ phase: "running", outcomes: plan.steps.map(() => undefined) });
  const outcomes = await runPlan(plan.steps, enabled, liveDeps(maxContext), c, (i, outcome) => {
    actions.set((s) => ({ outcomes: s.outcomes.map((o, j) => (j === i ? outcome : o)) }));
  });
  canceller = null;
  const history = addHistory(actions.get().history, historyEntry(request, plan, outcomes, Date.now()));
  writeLocal(HISTORY_KEY, history);
  actions.set({ phase: "done", history });
}

/** Stops planning or a running plan. */
export function cancelActions(): void {
  planning?.cancel();
  canceller?.cancel();
  if (actions.get().phase === "planning") actions.set({ phase: "idle" });
}

export function discardPlan(): void {
  actions.set({ phase: "idle", plan: null, enabled: [], outcomes: [], error: null });
}

export function clearActionHistory(): void {
  writeLocal(HISTORY_KEY, []);
  actions.set({ history: [] });
}
