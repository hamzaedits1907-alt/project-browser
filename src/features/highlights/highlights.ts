// Highlighter: text the user highlights on a page is kept (on this computer, in this profile's shell storage) and
// marked again whenever the page is opened. Private tabs never save highlights. The page's own elements are never
// changed: highlights are drawn with the CSS Custom Highlight API (src-tauri/src/native/highlight.js).

import { api } from "../../lib/ipc";
import { readLocal, writeLocal } from "../../lib/storage";
import { toast } from "../../lib/store";

export interface PageHighlights {
  url: string;
  title: string;
  texts: { text: string; at: number }[];
}

const KEY = "highlights";
const MAX_PAGES = 500;
const listeners = new Set<() => void>();

/** The address a page's highlights are filed under: without the #fragment (it changes while reading). */
export function pageKey(url: string): string | null {
  try {
    const u = new URL(url);
    if (u.protocol !== "http:" && u.protocol !== "https:") return null;
    u.hash = "";
    return u.toString();
  } catch {
    return null;
  }
}

export function allHighlights(): Record<string, PageHighlights> {
  const v = readLocal<Record<string, PageHighlights>>(KEY, {});
  return v && typeof v === "object" ? v : {};
}

function save(all: Record<string, PageHighlights>): void {
  const keys = Object.keys(all);
  if (keys.length > MAX_PAGES) {
    // Oldest pages go first.
    keys
      .sort((a, b) => Math.max(...(all[a]?.texts.map((t) => t.at) ?? [0])) - Math.max(...(all[b]?.texts.map((t) => t.at) ?? [0])))
      .slice(0, keys.length - MAX_PAGES)
      .forEach((k) => delete all[k]);
  }
  writeLocal(KEY, all);
  listeners.forEach((l) => l());
}

export function subscribeHighlights(l: () => void): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}

export function highlightsFor(url: string): string[] {
  const k = pageKey(url);
  return k ? (allHighlights()[k]?.texts.map((t) => t.text) ?? []) : [];
}

/** Marks a page's saved highlights (after it loads, or after a change). */
export async function applyHighlights(tabId: number, url: string): Promise<void> {
  const texts = highlightsFor(url);
  if (texts.length === 0) return;
  await api.tabHighlights(tabId, texts).catch(() => 0);
}

/** "Highlight" from the right-click menu. */
export async function addHighlight(tabId: number, url: string, title: string, text: string, isPrivate: boolean): Promise<void> {
  const clean = text.replace(/\s+/g, " ").trim().slice(0, 2000);
  const k = pageKey(url);
  if (!clean || !k) return;
  if (isPrivate) {
    // Shown, never saved.
    await api.tabHighlights(tabId, [...highlightsFor(url), clean]).catch(() => 0);
    toast("Highlighted (private tabs don't keep highlights)");
    return;
  }
  const all = allHighlights();
  const page = all[k] ?? { url: k, title, texts: [] };
  if (!page.texts.some((t) => t.text === clean)) page.texts.push({ text: clean, at: Date.now() });
  page.title = title || page.title;
  all[k] = page;
  save(all);
  const found = await api.tabHighlights(tabId, page.texts.map((t) => t.text)).catch(() => 0);
  toast(found < 0 ? "Saved. This page can't show highlights." : "Highlighted. It will be there next time you open this page.");
}

export function removeHighlight(key: string, text: string): void {
  const all = allHighlights();
  const page = all[key];
  if (!page) return;
  page.texts = page.texts.filter((t) => t.text !== text);
  if (page.texts.length === 0) delete all[key];
  save(all);
}

/**
 * A link that opens the page scrolled to `text` and marks it (a text fragment, `#:~:text=`), like Chrome's "Copy link
 * to highlight". Long selections use their first and last words.
 */
export function linkToHighlight(url: string, text: string): string {
  const base = url.split("#")[0] ?? url;
  const words = text.replace(/\s+/g, " ").trim().split(" ");
  const enc = (s: string) => encodeURIComponent(s).replace(/-/g, "%2D").replace(/,/g, "%2C");
  const fragment = words.length <= 8 ? enc(words.join(" ")) : `${enc(words.slice(0, 4).join(" "))},${enc(words.slice(-4).join(" "))}`;
  return `${base}#:~:text=${fragment}`;
}
