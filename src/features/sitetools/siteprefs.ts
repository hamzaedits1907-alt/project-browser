// Per-site preferences the shell re-applies whenever a site loads: the zoom level (as Chrome remembers it per site)
// and a forced dark look for sites that have none. Kept in this profile's shell storage; private tabs never save.

import { api } from "../../lib/ipc";
import { readLocal, writeLocal } from "../../lib/storage";
import { getState } from "../../lib/store";
import type { Tab } from "../../lib/types";

export interface SitePrefs {
  zoom?: number;
  /** true: always dark here; false: never (overrides "Force"); absent: follow Settings > Dark pages. */
  dark?: boolean;
}

const KEY = "site.prefs";

/** Inverts the page and turns pictures and video back, so they keep their real colours. No network, no script. */
export const DARK_CSS =
  "html{filter:invert(0.9) hue-rotate(180deg)!important;background:#fff!important}" +
  "img,video,picture,canvas,iframe,embed,object,svg image,[style*='background-image']{filter:invert(1) hue-rotate(180deg)!important}";

export function siteKey(url: string): string | null {
  try {
    const u = new URL(url);
    if (u.protocol !== "http:" && u.protocol !== "https:") return null;
    return u.hostname.replace(/^www\./, "").toLowerCase();
  } catch {
    return null;
  }
}

export function allPrefs(): Record<string, SitePrefs> {
  const p = readLocal<Record<string, SitePrefs>>(KEY, {});
  return p && typeof p === "object" ? p : {};
}

export function prefsFor(url: string): SitePrefs {
  const k = siteKey(url);
  return k ? (allPrefs()[k] ?? {}) : {};
}

/** Merges `patch` into the site's preferences; a preference set to its default is removed. */
export function updatePrefs(url: string, patch: SitePrefs): void {
  const k = siteKey(url);
  if (!k) return;
  const all = allPrefs();
  const next: SitePrefs = { ...(all[k] ?? {}), ...patch };
  if (next.zoom === undefined || Math.abs(next.zoom - 1) < 0.001) delete next.zoom;
  if (next.dark === undefined) delete next.dark;
  if (Object.keys(next).length === 0) delete all[k];
  else all[k] = next;
  writeLocal(KEY, all);
}

/** Whether pages of this site are darkened by the browser: the site's own choice, else the global mode. */
export function darkFor(url: string, mode: "off" | "auto" | "force"): boolean {
  const own = prefsFor(url).dark;
  return own === undefined ? mode === "force" : own;
}

/** The toolbar's dark-page switch for the site in this tab: on if it is not dark now, off (and the page restored) if it is. */
export async function toggleDark(tab: Tab): Promise<void> {
  const on = !darkFor(tab.url, getState().settings.darkPages);
  updatePrefs(tab.url, { dark: on });
  await api.pageStyle(tab.id, on ? DARK_CSS : null, on);
}

export function clearPrefs(site: string): void {
  const all = allPrefs();
  delete all[site];
  writeLocal(KEY, all);
}
