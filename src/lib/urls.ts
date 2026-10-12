// URL helpers for the tab strip and address bar (pure, unit-tested).

import type { Tab } from "./types";

export const NEW_TAB_URL = "browser://newtab";

export const INTERNAL_PAGES = ["newtab", "settings", "history", "bookmarks", "downloads", "performance", "gaming", "memory", "research"] as const;
export type InternalPage = (typeof INTERNAL_PAGES)[number];

export function isInternalUrl(url: string): boolean {
  return url.startsWith("browser://");
}

export function internalPageOf(url: string): InternalPage | null {
  if (!isInternalUrl(url)) return null;
  const name = url.slice("browser://".length).split(/[/?#]/)[0]?.toLowerCase() ?? "";
  return (INTERNAL_PAGES as readonly string[]).includes(name) ? (name as InternalPage) : null;
}

export interface UrlParts {
  scheme: string;
  host: string;
  rest: string;
}

/** Splits a URL for the address bar: dim scheme, bright host, dim path. */
export function splitUrl(url: string): UrlParts {
  try {
    const u = new URL(url);
    if (u.protocol === "http:" || u.protocol === "https:") {
      const rest = (u.pathname === "/" ? "" : u.pathname) + u.search + u.hash;
      return { scheme: `${u.protocol}//`, host: u.host, rest };
    }
  } catch {
    /* fall through */
  }
  return { scheme: "", host: url, rest: "" };
}

/** What the address bar shows while it is not being edited. */
export function idleAddress(tab: Tab | null): string {
  if (!tab) return "";
  if (tab.url === NEW_TAB_URL) return "";
  return tab.error?.url ?? tab.url;
}

export function tabTitle(tab: Tab): string {
  if (tab.title) return tab.title;
  if (tab.url === NEW_TAB_URL) return "New Tab";
  try {
    return new URL(tab.url).host || tab.url;
  } catch {
    return tab.url || "New Tab";
  }
}

/** First letter used when a site has no favicon. */
export function monogram(tab: Pick<Tab, "title" | "url">): string {
  const source = tab.title || (() => {
    try {
      return new URL(tab.url).host.replace(/^www\./, "");
    } catch {
      return tab.url;
    }
  })();
  const ch = source.trim().charAt(0);
  return ch ? ch.toUpperCase() : "•";
}

export function originOf(url: string): string | null {
  try {
    const u = new URL(url);
    return u.protocol === "http:" || u.protocol === "https:" ? u.origin : null;
  } catch {
    return null;
  }
}

/** Uses the site's own icon service-free favicon fallback: nothing leaves the machine. */
export function isSafeImageSrc(src: string | null | undefined): src is string {
  return !!src && (src.startsWith("https://") || src.startsWith("http://") || src.startsWith("data:image/"));
}
