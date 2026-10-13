import { useSyncExternalStore } from "react";

import { api, errorMessage, on } from "./ipc";
import { DEFAULT_SETTINGS } from "./settings";
import { cue } from "./sounds";
import type { BrowserState, Download, FindState, PermissionPrompt, ProfileMeta, Settings, Tab } from "./types";

export interface Toast {
  id: number;
  message: string;
}

export interface AppState {
  ready: boolean;
  fatal: string | null;
  version: string;
  profile: ProfileMeta | null;
  firstRun: boolean;
  browser: BrowserState;
  settings: Settings;
  downloads: Download[];
  prompts: PermissionPrompt[];
  find: FindState | null;
  toasts: Toast[];
  /** Bumped whenever bookmarks change so pages can refetch. */
  bookmarksRev: number;
  /** What Project Shield blocked, per tab id (tabs with nothing blocked are absent). */
  shieldBlocked: Record<string, number>;
}

const EMPTY_BROWSER: BrowserState = {
  tabs: [],
  groups: [],
  activeId: null,
  closedCount: 0,
  fullscreen: false,
  browserFullscreen: false,
  gaming: false,
  privateWindow: false,
  windowCount: 1,
  split: null,
};

let state: AppState = {
  ready: false,
  fatal: null,
  version: "",
  profile: null,
  firstRun: false,
  browser: EMPTY_BROWSER,
  settings: DEFAULT_SETTINGS,
  downloads: [],
  prompts: [],
  find: null,
  toasts: [],
  bookmarksRev: 0,
  shieldBlocked: {},
};

const listeners = new Set<() => void>();

function set(next: Partial<AppState>): void {
  state = { ...state, ...next };
  listeners.forEach((l) => l());
}

export function getState(): AppState {
  return state;
}

export function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Subscribe a component to a slice of state. The selector must return a stable reference. */
export function useStore<T>(selector: (s: AppState) => T): T {
  return useSyncExternalStore(subscribe, () => selector(state));
}

export function activeTab(s: AppState): Tab | null {
  return s.browser.tabs.find((t) => t.id === s.browser.activeId) ?? null;
}

// ---- UI event bus (focus omnibox, open find) -------------------------------------

type BusEvent = "focusOmnibox" | "openFind" | "openSiteInfo" | "openFill" | "openPalette" | "screenshot" | "reader" | "shortcuts";
const bus = new Map<BusEvent, Set<() => void>>();

export function onBus(name: BusEvent, fn: () => void): () => void {
  let fns = bus.get(name);
  if (!fns) bus.set(name, (fns = new Set()));
  const mine = fns;
  mine.add(fn);
  return () => mine.delete(fn);
}

export function emitBus(name: BusEvent): void {
  bus.get(name)?.forEach((fn) => fn());
}

// ---- toasts ------------------------------------------------------------------------

let toastSeq = 1;
const TOAST_MS = 3200;

export function toast(message: string): void {
  const id = toastSeq++;
  set({ toasts: [...state.toasts.slice(-2), { id, message }] });
  setTimeout(() => dismissToast(id), TOAST_MS);
}

export function dismissToast(id: number): void {
  if (state.toasts.some((t) => t.id === id)) set({ toasts: state.toasts.filter((t) => t.id !== id) });
}

/** Runs an action and reports a failure as a toast. */
export async function attempt<T>(action: () => Promise<T>, failure?: string): Promise<T | undefined> {
  try {
    return await action();
  } catch (e) {
    toast(failure ? `${failure}: ${errorMessage(e)}` : errorMessage(e));
    return undefined;
  }
}

// ---- lifecycle -----------------------------------------------------------------------

export function markOnboarded(): void {
  set({ firstRun: false });
}

export function setSettingsLocal(settings: Settings): void {
  set({ settings });
}

let started = false;

/** Connects to the Rust core: subscribes to events first, then asks for the initial snapshot. */
export async function init(): Promise<void> {
  if (started) return;
  started = true;
  try {
    await Promise.all([
      on("pb:state", (browser) => set({ browser })),
      on("pb:settings", (settings) => set({ settings })),
      on("pb:downloads", (downloads) => {
        // A download that just finished gets the "done" cue (when interface sounds are on).
        const before = new Map(getState().downloads.map((d) => [d.id, d.state]));
        if (downloads.some((d) => d.state === "completed" && before.has(d.id) && before.get(d.id) !== "completed")) cue("done");
        set({ downloads });
      }),
      on("pb:prompts", (prompts) => set({ prompts })),
      on("pb:find", (find) => set({ find })),
      on("pb:bookmarks", () => set({ bookmarksRev: state.bookmarksRev + 1 })),
      on("pb:shield", (e) => set({ shieldBlocked: e.tabs })),
      on("pb:ui", (e) => {
        if (e.kind === "toast") toast(e.message);
        else emitBus(e.kind);
      }),
    ]);
    const boot = await api.shellReady();
    set({
      ready: true,
      version: boot.version,
      profile: boot.profile,
      firstRun: boot.firstRun,
      browser: boot.state,
      settings: boot.settings,
      downloads: boot.downloads,
      prompts: boot.prompts,
    });
  } catch (e) {
    set({ fatal: errorMessage(e) });
  }
}
