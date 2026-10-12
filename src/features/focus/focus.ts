// Screen Time and Focus Mode.
//
// Screen Time counts the seconds the browser window is focused on each site (only the active tab of a focused
// window counts, private tabs never do). Focus Mode blocks a list of distracting sites until a timer ends: opening
// one shows the Focus page instead. Daily limits block a site for the rest of the day once its time is used up.
// Everything stays in this profile's shell storage on this computer; nothing is sent anywhere.

import { readLocal, writeLocal } from "../../lib/storage";

export interface FocusSession {
  /** Epoch ms when the session ends. */
  until: number;
  startedAt: number;
}

export interface FocusConfig {
  /** Sites blocked during a focus session (host names; subdomains are included). */
  blocked: string[];
  /** Daily limits in minutes per site. */
  limits: Record<string, number>;
  session: FocusSession | null;
}

/** Seconds per site per day: { "2026-10-12": { "youtube.com": 1830 } }. */
export type Usage = Record<string, Record<string, number>>;

export const DEFAULT_BLOCKED = ["youtube.com", "reddit.com", "x.com", "twitter.com", "instagram.com", "tiktok.com", "facebook.com", "twitch.tv"];
const KEEP_DAYS = 30;

export function dayKey(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** The site a host belongs to for Screen Time: without "www." and "m.". */
export function siteOf(host: string): string {
  return host.toLowerCase().replace(/^(www|m|mobile)\./, "");
}

export function matchesSite(host: string, site: string): boolean {
  const h = siteOf(host);
  const s = siteOf(site.trim());
  return !!s && (h === s || h.endsWith(`.${s}`));
}

/** Adds `seconds` for `site` on the day of `now`, dropping days older than a month. Returns a new object. */
export function addUsage(usage: Usage, site: string, seconds: number, now: number): Usage {
  const day = dayKey(now);
  const next: Usage = {};
  const cutoff = dayKey(now - KEEP_DAYS * 86_400_000);
  for (const [d, sites] of Object.entries(usage)) if (d >= cutoff) next[d] = sites;
  const today = { ...(next[day] ?? {}) };
  today[site] = (today[site] ?? 0) + seconds;
  next[day] = today;
  return next;
}

export function topSites(usage: Usage, day: string, n = 8): { site: string; seconds: number }[] {
  return Object.entries(usage[day] ?? {})
    .map(([site, seconds]) => ({ site, seconds }))
    .sort((a, b) => b.seconds - a.seconds)
    .slice(0, n);
}

export function totalFor(usage: Usage, day: string): number {
  return Object.values(usage[day] ?? {}).reduce((a, b) => a + b, 0);
}

export function sessionActive(cfg: FocusConfig, now: number): boolean {
  return !!cfg.session && cfg.session.until > now;
}

/** Why a host is blocked right now, or null. */
export function blockReason(cfg: FocusConfig, usage: Usage, host: string, now: number): "focus" | "limit" | null {
  if (!host) return null;
  if (sessionActive(cfg, now) && cfg.blocked.some((s) => matchesSite(host, s))) return "focus";
  const site = siteOf(host);
  for (const [limitSite, minutes] of Object.entries(cfg.limits)) {
    if (minutes > 0 && matchesSite(host, limitSite)) {
      const used = Object.entries(usage[dayKey(now)] ?? {})
        .filter(([s]) => matchesSite(s, limitSite))
        .reduce((a, [, v]) => a + v, 0);
      if (used >= minutes * 60 || (site === siteOf(limitSite) && used >= minutes * 60)) return "limit";
    }
  }
  return null;
}

export function formatDuration(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return `${h} h ${m} min`;
  if (m > 0) return `${m} min`;
  return `${s} s`;
}

// ---- storage ------------------------------------------------------------------------------------

const CFG_KEY = "focus.config";
const USAGE_KEY = "focus.usage";

export function loadConfig(): FocusConfig {
  const c = readLocal<Partial<FocusConfig>>(CFG_KEY, {});
  return {
    blocked: Array.isArray(c.blocked) ? c.blocked.filter((s) => typeof s === "string") : DEFAULT_BLOCKED,
    limits: c.limits && typeof c.limits === "object" ? c.limits : {},
    session: c.session && typeof c.session.until === "number" ? c.session : null,
  };
}

export function saveConfig(cfg: FocusConfig): void {
  writeLocal(CFG_KEY, cfg);
  window.dispatchEvent(new Event("pb-focus"));
}

export function loadUsage(): Usage {
  return readLocal<Usage>(USAGE_KEY, {});
}

export function saveUsage(u: Usage): void {
  writeLocal(USAGE_KEY, u);
}

export function startSession(minutes: number, now = Date.now()): FocusConfig {
  const cfg = { ...loadConfig(), session: { startedAt: now, until: now + minutes * 60_000 } };
  saveConfig(cfg);
  return cfg;
}

export function endSession(): FocusConfig {
  const cfg = { ...loadConfig(), session: null };
  saveConfig(cfg);
  return cfg;
}
