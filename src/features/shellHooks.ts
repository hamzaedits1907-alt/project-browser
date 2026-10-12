// Background helpers the shell runs for its window: Screen Time and Focus Mode enforcement, per-site zoom and dark
// pages, and the shortcuts that open shell dialogs.

import { useEffect, useRef } from "react";

import { api } from "../lib/ipc";
import { writeLocal } from "../lib/storage";
import { activeTab, getState, onBus, subscribe } from "../lib/store";
import { openDialog } from "../lib/ui";
import { isInternalUrl } from "../lib/urls";
import { startScreenshot } from "./dialogs/screenshotState";
import { addUsage, blockReason, loadConfig, loadUsage, saveUsage, siteOf } from "./focus/focus";
import { DARK_CSS, prefsFor, siteKey, updatePrefs } from "./sitetools/siteprefs";

const TICK_S = 5;

function hostOf(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return "";
  }
}

/** Counts time on the active site while this window is in front, and sends blocked sites to the Focus page. */
export function useScreenTime(): void {
  useEffect(() => {
    const enforce = () => {
      const tab = activeTab(getState());
      if (!tab || isInternalUrl(tab.url) || tab.private) return;
      const host = hostOf(tab.url);
      const reason = blockReason(loadConfig(), loadUsage(), host, Date.now());
      if (reason) {
        writeLocal("focus.blocked", { host: siteOf(host), reason, at: Date.now() });
        void api.navigate("browser://focus", tab.id).catch(() => {});
      }
    };
    const tick = () => {
      const tab = activeTab(getState());
      if (!document.hasFocus() || !tab || tab.private || isInternalUrl(tab.url) || tab.lifecycle !== "live") return;
      const site = siteOf(hostOf(tab.url));
      if (!site) return;
      saveUsage(addUsage(loadUsage(), site, TICK_S, Date.now()));
      enforce();
    };
    let lastUrl = "";
    const unsub = subscribe(() => {
      const url = activeTab(getState())?.url ?? "";
      if (url !== lastUrl) {
        lastUrl = url;
        enforce();
      }
    });
    window.addEventListener("pb-focus", enforce);
    const timer = window.setInterval(tick, TICK_S * 1000);
    return () => {
      unsub();
      window.removeEventListener("pb-focus", enforce);
      window.clearInterval(timer);
    };
  }, []);
}

/** Re-applies a site's remembered zoom and dark look when it loads, and remembers zoom changes per site. */
export function useSitePrefs(): void {
  const last = useRef<{ id: number; site: string | null; loading: boolean; zoom: number } | null>(null);
  useEffect(() => {
    return subscribe(() => {
      const tab = activeTab(getState());
      if (!tab || isInternalUrl(tab.url) || tab.lifecycle !== "live") {
        last.current = null;
        return;
      }
      const site = siteKey(tab.url);
      const prev = last.current;
      last.current = { id: tab.id, site, loading: tab.loading, zoom: tab.zoom };
      if (!site) return;
      const arrived = !prev || prev.id !== tab.id || prev.site !== site;
      const finished = !!prev && prev.id === tab.id && prev.site === site && prev.loading && !tab.loading;
      const prefs = prefsFor(tab.url);
      if (arrived || finished) {
        if (prefs.zoom && Math.abs(prefs.zoom - tab.zoom) > 0.001 && !tab.private) void api.zoom("set", prefs.zoom, tab.id).catch(() => {});
        if (prefs.dark && finished) void api.pageStyle(tab.id, DARK_CSS).catch(() => {});
        return;
      }
      // Same page, zoom changed by the user: remember it for the site (never from private tabs).
      if (prev && prev.zoom !== tab.zoom && !tab.private) updatePrefs(tab.url, { zoom: tab.zoom });
    });
  }, []);
}

/** Shortcut events from the core (Ctrl+K, F9, Ctrl+Shift+S, F1) open the matching shell dialog. */
export function useShellShortcuts(): void {
  useEffect(() => {
    const offs = [
      onBus("openPalette", () => openDialog("palette")),
      onBus("reader", () => openDialog("reader")),
      onBus("shortcuts", () => openDialog("shortcuts")),
      onBus("screenshot", () => void startScreenshot(false)),
    ];
    return () => offs.forEach((off) => off());
  }, []);
}
