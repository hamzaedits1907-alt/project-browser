import type { SearchEngine, Settings } from "./types";

/** Same defaults as `Settings::default()` in Rust; only used before the core answers. */
export const DEFAULT_SETTINGS: Settings = {
  searchEngine: "google",
  customSearchTemplate: null,
  startup: "restoreSession",
  showBookmarksBar: true,
  downloadDir: null,
  accent: "ion",
  newTabBackground: "grid",
  reduceMotion: false,
  showRamOnNewTab: true,
  sleepingTabsEnabled: true,
  sleepAfterMinutes: 30,
  neverSleepHosts: [],
  gamingMode: false,
  gamingBackgroundAction: "sleep",
  gamingPauseDownloads: true,
  gamingReduceAnimations: true,
  gamingLowMemoryTarget: true,
  savePasswords: true,
  autofill: true,
  heavyTabMemoryMb: 500,
};

export const SEARCH_ENGINES: { id: SearchEngine; label: string; host: string }[] = [
  { id: "google", label: "Google", host: "google.com" },
  { id: "duckduckgo", label: "DuckDuckGo", host: "duckduckgo.com" },
  { id: "brave", label: "Brave Search", host: "search.brave.com" },
  { id: "bing", label: "Bing", host: "bing.com" },
  { id: "startpage", label: "Startpage", host: "startpage.com" },
  { id: "ecosia", label: "Ecosia", host: "ecosia.org" },
];

export interface AccentOption {
  id: string;
  label: string;
  /** Preview swatch only; the real colour comes from CSS (`[data-accent]`). */
  swatch: string;
}

export const ACCENTS: AccentOption[] = [
  { id: "ion", label: "Ion", swatch: "#3fc8ff" },
  { id: "violet", label: "Violet", swatch: "#9d7bff" },
  { id: "magenta", label: "Magenta", swatch: "#ff5fc8" },
  { id: "lime", label: "Lime", swatch: "#9be85a" },
  { id: "amber", label: "Amber", swatch: "#ffb03c" },
  { id: "crimson", label: "Crimson", swatch: "#ff5a6e" },
];

export const NEW_TAB_BACKGROUNDS: { id: string; label: string }[] = [
  { id: "grid", label: "Grid" },
  { id: "aurora", label: "Aurora" },
  { id: "plain", label: "Plain" },
];

export function accentOrDefault(id: string): string {
  return ACCENTS.some((a) => a.id === id) ? id : "ion";
}

export function backgroundOrDefault(id: string): string {
  return NEW_TAB_BACKGROUNDS.some((b) => b.id === id) ? id : "grid";
}

/** True when motion-heavy effects should be off (user choice, or Gaming Mode's reduce-animations). */
export function motionReduced(s: Settings): boolean {
  return s.reduceMotion || (s.gamingMode && s.gamingReduceAnimations);
}

/** Returns a copy of `s` with the given changes applied. */
export function patch(s: Settings, changes: Partial<Settings>): Settings {
  return { ...s, ...changes };
}

/** Parses a never-sleep host list typed one per line or comma separated. */
export function parseHostList(text: string): string[] {
  const seen = new Set<string>();
  for (const raw of text.split(/[\s,;]+/)) {
    const host = raw
      .trim()
      .replace(/^https?:\/\//i, "")
      .replace(/\/.*$/, "")
      .toLowerCase();
    if (host) seen.add(host);
  }
  return [...seen].sort();
}
