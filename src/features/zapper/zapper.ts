// Element zapper: "Hide an element on this page" lets the user click anything on a site (a banner, a sticky video, a
// sidebar) and keeps it hidden on that site from now on. Rules are CSS selectors per site, kept in this profile's
// shell storage, and put into the page as one style sheet whenever the site loads.

import { api } from "../../lib/ipc";
import { readLocal, writeLocal } from "../../lib/storage";
import { attempt, toast } from "../../lib/store";
import type { Tab } from "../../lib/types";
import { siteKey } from "../sitetools/siteprefs";

const KEY = "zapper.rules";
const listeners = new Set<() => void>();

export function allZapRules(): Record<string, string[]> {
  const v = readLocal<Record<string, string[]>>(KEY, {});
  return v && typeof v === "object" ? v : {};
}

export function zapRulesFor(url: string): string[] {
  const k = siteKey(url);
  return k ? (allZapRules()[k] ?? []) : [];
}

export function subscribeZapper(l: () => void): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}

/** The style sheet that hides a site's zapped elements. */
export function zapCss(rules: string[]): string {
  return rules.map((r) => `${r}{display:none!important}`).join("\n");
}

function setRules(site: string, rules: string[]): void {
  const all = allZapRules();
  if (rules.length) all[site] = rules;
  else delete all[site];
  writeLocal(KEY, all);
  listeners.forEach((l) => l());
}

/** Puts a page's hide rules into it (after it loads, or after a change). Empty rules remove the style sheet's content. */
export async function applyZapRules(tabId: number, url: string): Promise<void> {
  await api.siteCss(tabId, zapCss(zapRulesFor(url))).catch(() => {});
}

/** "Hide an element on this page…": the user clicks something; it is hidden now and on every later visit. */
export async function startZapper(tab: Tab): Promise<void> {
  const site = siteKey(tab.url);
  if (!site) {
    toast("Elements can only be hidden on web pages.");
    return;
  }
  toast("Click the thing to hide. Esc cancels.");
  const selector = await attempt(() => api.zapperPick(tab.id), "Could not start the element picker");
  if (!selector) return;
  const rules = [...new Set([...zapRulesFor(tab.url), selector])];
  if (!tab.private) setRules(site, rules);
  await api.siteCss(tab.id, zapCss(rules)).catch(() => {});
  toast(tab.private ? "Hidden on this page (private tabs don't remember it)." : `Hidden on ${site}. Undo in the site information panel.`);
}

/** Shows everything zapped on a site again. */
export async function clearZapRules(tab: Tab): Promise<void> {
  const site = siteKey(tab.url);
  if (!site) return;
  setRules(site, []);
  await api.siteCss(tab.id, "").catch(() => {});
}
