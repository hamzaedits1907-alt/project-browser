import type { PinnedSite, SearchEngine, Settings } from "./types";

/** Same lists as `SIDEBAR_ITEMS`, `TOOLBAR_ITEMS`, `NTP_WIDGETS`, `THEMES` and `NTP_BACKGROUNDS` in settings.rs (contract-tested). */
export const SIDEBAR_ITEMS = ["bookmarks", "history", "downloads", "ai", "memory", "research", "gaming", "shield", "passwords", "privacy", "performance"] as const;
export const DEFAULT_SIDEBAR_ITEMS = ["bookmarks", "history", "downloads", "ai", "gaming", "shield", "passwords"];
export const TOOLBAR_ITEMS = ["gaming", "shield", "ai", "downloads", "bookmarks", "history", "memory", "performance", "privateWindow"] as const;
export const DEFAULT_TOOLBAR_ITEMS = ["gaming", "shield", "ai", "downloads"];
export const NTP_WIDGETS = ["shortcuts", "continue", "closed", "bookmarks", "downloads", "system", "gaming", "ai"] as const;
export const DEFAULT_NTP_WIDGETS = ["shortcuts", "continue", "closed", "bookmarks", "downloads", "system", "gaming", "ai"];
export const THEMES = ["neon", "oled", "violet", "tactical", "crimson", "minimal", "performance", "custom"] as const;
export const NTP_BACKGROUNDS = ["grid", "aurora", "circuit", "horizon", "plain"] as const;

export type SidebarItem = (typeof SIDEBAR_ITEMS)[number];
export type ToolbarItem = (typeof TOOLBAR_ITEMS)[number];
export type NtpWidget = (typeof NTP_WIDGETS)[number];
export type ThemeId = (typeof THEMES)[number];

/** Same defaults as `Settings::default()` in Rust; only used before the core answers. */
export const DEFAULT_SETTINGS: Settings = {
  searchEngine: "google",
  customSearchTemplate: null,
  startup: "restoreSession",
  showBookmarksBar: true,
  downloadDir: null,
  accent: "auto",
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
  protectPinnedTabs: true,
  memoryLimitMb: 0,
  aiMaxContextChars: 24000,
  checkUpdates: true,
  popupAllowedHosts: [],
  newWindowRequests: "tab",
  theme: "neon",
  customHue: 190,
  cornerStyle: "angular",
  density: "comfortable",
  motion: "standard",
  uiSounds: false,
  tabStyle: "shaped",
  verticalTabs: false,
  compactTitleBar: false,
  sidebar: "rail",
  sidebarItems: DEFAULT_SIDEBAR_ITEMS,
  sidebarSites: [],
  toolbarItems: DEFAULT_TOOLBAR_ITEMS,
  ntpLayout: "full",
  ntpWidgets: DEFAULT_NTP_WIDGETS,
  ntpShortcuts: [],
  ntpHiddenSites: [],
};

export const SEARCH_ENGINES: { id: SearchEngine; label: string; host: string }[] = [
  { id: "google", label: "Google", host: "google.com" },
  { id: "duckduckgo", label: "DuckDuckGo", host: "duckduckgo.com" },
  { id: "brave", label: "Brave Search", host: "search.brave.com" },
  { id: "bing", label: "Bing", host: "bing.com" },
  { id: "startpage", label: "Startpage", host: "startpage.com" },
  { id: "ecosia", label: "Ecosia", host: "ecosia.org" },
];

/** The label of the engine the address bar searches with (a custom address counts as "Custom"). */
export function engineLabel(s: Settings): string {
  if (s.customSearchTemplate) return "Custom search";
  return SEARCH_ENGINES.find((e) => e.id === s.searchEngine)?.label ?? "Search";
}

// ---- themes ------------------------------------------------------------------------------------

export interface Palette {
  /** Window, chrome, raised surfaces, controls, hover, pressed. */
  bg: [string, string, string, string, string, string];
  text: [string, string, string, string];
  /** Accent hue, saturation and lightness, and the secondary hue used in gradients. */
  accent: [number, number, number];
  accent2: number;
  /** 0 = no glow at all, 1 = full glow. */
  glow: number;
  /** Decorative gradients, light lines and background patterns. */
  effects: boolean;
}

export interface ThemeDef {
  id: ThemeId;
  label: string;
  blurb: string;
  palette: Palette;
}

const TEXT_COOL: Palette["text"] = ["#eaeef6", "#a6afc2", "#6d768b", "#485065"];
const TEXT_NEUTRAL: Palette["text"] = ["#ececee", "#a9a9b0", "#707078", "#4b4b52"];

export const THEME_DEFS: ThemeDef[] = [
  {
    id: "neon",
    label: "Neon Cyan",
    blurb: "The signature look: deep blue-black with a cyan edge.",
    palette: { bg: ["#06080c", "#0a0d13", "#0f131b", "#151a25", "#1c2230", "#252c3c"], text: TEXT_COOL, accent: [188, 100, 56], accent2: 262, glow: 1, effects: true },
  },
  {
    id: "oled",
    label: "OLED Black",
    blurb: "True black surfaces for OLED screens; light only where it matters.",
    palette: { bg: ["#000000", "#000000", "#08090b", "#0f1114", "#17191d", "#202328"], text: TEXT_NEUTRAL, accent: [190, 92, 60], accent2: 220, glow: 0.45, effects: true },
  },
  {
    id: "violet",
    label: "Violet Pulse",
    blurb: "Indigo night with an electric violet accent.",
    palette: { bg: ["#07060e", "#0c0a17", "#120f21", "#19152d", "#211c3a", "#2b254a"], text: ["#ecebf7", "#aba7c6", "#726d8f", "#4c4866"], accent: [266, 100, 71], accent2: 318, glow: 1, effects: true },
  },
  {
    id: "tactical",
    label: "Tactical Green",
    blurb: "Low-light olive tones and a night-vision green.",
    palette: { bg: ["#060805", "#0a0d09", "#0f130d", "#151b13", "#1d251a", "#273023"], text: ["#e8ede4", "#a6b0a0", "#6e7868", "#4a5245"], accent: [110, 64, 52], accent2: 46, glow: 0.7, effects: true },
  },
  {
    id: "crimson",
    label: "Crimson",
    blurb: "Warm black with a racing red accent.",
    palette: { bg: ["#090506", "#0f080a", "#160c0f", "#1e1115", "#28171c", "#331e25"], text: ["#f3eaec", "#bda7ad", "#836d73", "#5a484d"], accent: [352, 100, 62], accent2: 22, glow: 0.9, effects: true },
  },
  {
    id: "minimal",
    label: "Minimal Dark",
    blurb: "Neutral greys, a quiet blue accent, no glow.",
    palette: { bg: ["#0e0e10", "#141416", "#19191c", "#202024", "#28282d", "#323238"], text: TEXT_NEUTRAL, accent: [214, 72, 64], accent2: 214, glow: 0, effects: false },
  },
  {
    id: "performance",
    label: "Performance",
    blurb: "Flat surfaces, no glow, no background effects, no animation.",
    palette: { bg: ["#0b0c0f", "#101115", "#15171c", "#1b1e24", "#23262e", "#2c3039"], text: TEXT_COOL, accent: [196, 80, 56], accent2: 196, glow: 0, effects: false },
  },
  {
    id: "custom",
    label: "Custom",
    blurb: "The signature surfaces with an accent hue you pick.",
    palette: { bg: ["#06080c", "#0a0d13", "#0f131b", "#151a25", "#1c2230", "#252c3c"], text: TEXT_COOL, accent: [190, 100, 58], accent2: 260, glow: 0.85, effects: true },
  },
];

export interface AccentOption {
  id: string;
  label: string;
  hsl: [number, number, number];
}

/** Accent overrides. "auto" uses the theme's own accent, "custom" uses `customHue`. */
export const ACCENTS: AccentOption[] = [
  { id: "cyan", label: "Cyan", hsl: [188, 100, 56] },
  { id: "violet", label: "Violet", hsl: [262, 100, 72] },
  { id: "magenta", label: "Magenta", hsl: [322, 100, 68] },
  { id: "crimson", label: "Crimson", hsl: [352, 100, 64] },
  { id: "amber", label: "Amber", hsl: [38, 100, 58] },
  { id: "lime", label: "Lime", hsl: [96, 78, 56] },
  { id: "green", label: "Green", hsl: [140, 64, 52] },
  { id: "blue", label: "Blue", hsl: [214, 92, 64] },
];

/** CSS colour for an accent swatch. */
export function accentCss([h, sat, light]: [number, number, number]): string {
  return `hsl(${h} ${sat}% ${light}%)`;
}

export function themeDef(id: string): ThemeDef {
  return THEME_DEFS.find((t) => t.id === id) ?? (THEME_DEFS[0] as ThemeDef);
}

/** The accent colour in effect: the override if one is set, else the theme's. */
export function effectiveAccent(s: Pick<Settings, "theme" | "accent" | "customHue">): [number, number, number] {
  const theme = themeDef(s.theme);
  if (s.theme === "custom" || s.accent === "custom") {
    const [, sat, light] = theme.palette.accent;
    return [((s.customHue % 360) + 360) % 360, sat, light];
  }
  const named = ACCENTS.find((a) => a.id === s.accent);
  return named ? named.hsl : theme.palette.accent;
}

/** Accept the values older versions stored ("ion" was the default accent). */
export function accentOrDefault(id: string): string {
  if (id === "custom" || ACCENTS.some((a) => a.id === id)) return id;
  return "auto";
}

export const NEW_TAB_BACKGROUNDS: { id: (typeof NTP_BACKGROUNDS)[number]; label: string }[] = [
  { id: "grid", label: "Grid" },
  { id: "circuit", label: "Circuit" },
  { id: "horizon", label: "Horizon" },
  { id: "aurora", label: "Aurora" },
  { id: "plain", label: "Plain" },
];

export function backgroundOrDefault(id: string): string {
  return NEW_TAB_BACKGROUNDS.some((b) => b.id === id) ? id : "grid";
}

export type MotionMode = "off" | "subtle" | "standard" | "expressive";

/** True when motion-heavy effects should be off (user choice, the Performance theme, or Gaming Mode's reduce-animations). */
export function motionReduced(s: Settings): boolean {
  return s.reduceMotion || s.theme === "performance" || (s.gamingMode && s.gamingReduceAnimations);
}

export function motionMode(s: Settings): MotionMode {
  return motionReduced(s) ? "off" : s.motion;
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

/** Moves `id` one place up or down in an ordered id list. */
export function moveItem(list: string[], id: string, delta: -1 | 1): string[] {
  const i = list.indexOf(id);
  const j = i + delta;
  if (i < 0 || j < 0 || j >= list.length) return list;
  const next = [...list];
  next[i] = list[j] as string;
  next[j] = id;
  return next;
}

/** Adds or removes `id`; a re-added id goes back to its place in `catalog` order relative to the others. */
export function toggleItem(list: string[], id: string, on: boolean, catalog: readonly string[]): string[] {
  if (!on) return list.filter((x) => x !== id);
  if (list.includes(id)) return list;
  const rank = (x: string) => catalog.indexOf(x);
  const next = [...list];
  const at = next.findIndex((x) => rank(x) > rank(id));
  next.splice(at < 0 ? next.length : at, 0, id);
  return next;
}

/** Normalises what a user typed as a site address; `null` when it is not a usable http(s) address. */
export function normalizeSiteUrl(input: string): string | null {
  const raw = input.trim();
  if (!raw) return null;
  const withScheme = /^[a-z][a-z0-9+.-]*:/i.test(raw) ? raw : `https://${raw}`;
  try {
    const u = new URL(withScheme);
    if ((u.protocol !== "https:" && u.protocol !== "http:") || !u.hostname.includes(".")) return null;
    return u.toString();
  } catch {
    return null;
  }
}

export function sameSite(a: PinnedSite, url: string): boolean {
  return a.url === url;
}
