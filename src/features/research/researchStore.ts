import { createStore } from "../../lib/createStore";
import { api, errorMessage } from "../../lib/ipc";
import { ask, AskError } from "../ai/ask";
import type { AskHandle } from "../ai/ask";
import { researchMessages, researchSystem } from "./prompts";
import type { ResearchMode } from "./prompts";
import { parseClaimBattle, parseComparison } from "./provenance";
import type { ResearchResult } from "./provenance";
import { estimateRequestTokens, perSourceChars, toSources } from "./sources";
import type { ResearchSource } from "./sources";

export interface SkippedPage {
  tabId: number;
  title: string;
  reason: string;
}

export interface ResearchState {
  mode: ResearchMode;
  question: string;
  /** The tabs the user ticked, in the order they should be numbered. Nothing is read from a tab that isn't here. */
  selected: number[];
  phase: "idle" | "reading" | "asking" | "done" | "error";
  /** What is happening, in words. */
  progress: string;
  sources: ResearchSource[];
  skipped: SkippedPage[];
  result: ResearchResult | null;
  /** The model's reply when it couldn't be read as structured findings. */
  raw: string | null;
  error: string | null;
  /** Rough size of the request that was sent, in tokens. */
  estimatedTokens: number | null;
}

const INITIAL: ResearchState = {
  mode: "compare",
  question: "",
  selected: [],
  phase: "idle",
  progress: "",
  sources: [],
  skipped: [],
  result: null,
  raw: null,
  error: null,
  estimatedTokens: null,
};

export const research = createStore<ResearchState>(INITIAL);

let current: AskHandle | null = null;
let runToken = 0;

export const MIN_PAGES: Record<ResearchMode, number> = { compare: 2, claim: 1 };

/** Why Run is unavailable (`null` = it may run). */
export function runBlocker(mode: ResearchMode, question: string, selectedCount: number): string | null {
  if (selectedCount < MIN_PAGES[mode]) {
    return mode === "compare" ? "Tick at least two pages to compare." : "Tick at least one page to check the claim against.";
  }
  if (mode === "claim" && question.trim().length < 5) return "Write the question or claim to check.";
  return null;
}

export function setMode(mode: ResearchMode): void {
  research.set({ mode });
}

export function setQuestion(question: string): void {
  research.set({ question });
}

export function toggleTab(id: number): void {
  research.set((s) => ({ selected: s.selected.includes(id) ? s.selected.filter((t) => t !== id) : [...s.selected, id] }));
}

export function setSelected(ids: number[]): void {
  research.set({ selected: ids });
}

export function resetResearch(): void {
  cancelResearch();
  research.set({ ...INITIAL });
}

export function cancelResearch(): void {
  runToken++;
  current?.cancel();
  current = null;
  if (research.get().phase === "reading" || research.get().phase === "asking") {
    research.set({ phase: "idle", progress: "Cancelled. Nothing was kept." });
  }
}

/**
 * Reads the ticked tabs (the core refuses private tabs, browser pages and tabs that aren't running), sends only that
 * text to the model, and checks the answer against the same text. `maxContext` is the user's context limit.
 */
export async function runResearch(tabs: readonly { id: number; title: string; url: string }[], maxContext: number): Promise<void> {
  const { mode, question, selected } = research.get();
  const blocker = runBlocker(mode, question, selected.length);
  if (blocker) {
    research.set({ phase: "error", error: blocker });
    return;
  }
  const token = ++runToken;
  const live = () => token === runToken;
  research.set({ phase: "reading", progress: "Reading the pages…", result: null, raw: null, error: null, sources: [], skipped: [], estimatedTokens: null });

  const per = perSourceChars(maxContext, selected.length);
  const snapshots = [];
  const skipped: SkippedPage[] = [];
  for (const id of selected) {
    const tab = tabs.find((t) => t.id === id);
    try {
      snapshots.push(await api.aiPageSnapshot(id, per));
    } catch (e) {
      skipped.push({ tabId: id, title: tab?.title || tab?.url || `Tab ${id}`, reason: errorMessage(e) });
    }
    if (!live()) return;
  }
  const readable = snapshots.filter((s) => s.text.trim().length > 0);
  for (const s of snapshots) {
    if (s.text.trim().length === 0) skipped.push({ tabId: s.tabId, title: s.title || s.url, reason: "The page has no readable text yet." });
  }
  if (readable.length < MIN_PAGES[mode]) {
    research.set({ phase: "error", skipped, error: `Not enough pages could be read (${readable.length} of ${MIN_PAGES[mode]} needed).` });
    return;
  }

  const sources = toSources(readable);
  const estimatedTokens = estimateRequestTokens(sources);
  research.set({ phase: "asking", progress: `Asking the model about ${sources.length} pages (about ${estimatedTokens.toLocaleString("en-US")} tokens)…`, sources, skipped, estimatedTokens });

  const handle = ask(researchSystem(), researchMessages(mode, question, sources), { maxTokens: 3000 });
  current = handle;
  let reply: string;
  try {
    reply = await handle.promise;
  } catch (e) {
    if (!live() || (e instanceof AskError && e.cancelled)) return;
    research.set({ phase: "error", error: errorMessage(e instanceof AskError ? e.message : e) });
    return;
  } finally {
    if (current === handle) current = null;
  }
  if (!live()) return;

  const outcome = mode === "compare" ? parseComparison(reply, sources) : parseClaimBattle(reply, question.trim(), sources);
  if (outcome.ok) research.set({ phase: "done", progress: "", result: outcome.result, raw: null });
  else research.set({ phase: "error", error: outcome.error, raw: reply });
}
