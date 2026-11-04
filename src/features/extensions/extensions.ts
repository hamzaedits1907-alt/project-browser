// Shared extension state for the toolbar button, the store bar and the settings page: one list, reloaded after every
// change, and the user's pinned extensions (kept per window in local storage; it is a display preference).

import { useEffect, useSyncExternalStore } from "react";

import { api } from "../../lib/ipc";
import { readLocal, writeLocal } from "../../lib/storage";
import { attempt, toast } from "../../lib/store";
import type { ExtensionDetails } from "../../lib/types";

let list: ExtensionDetails[] = [];
let loaded = false;
let loading: Promise<void> | null = null;
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

export function reloadExtensions(): Promise<void> {
  loading ??= api
    .extensionsDetails()
    .then((l) => {
      list = l;
    })
    .catch(() => {})
    .finally(() => {
      loaded = true;
      loading = null;
      emit();
    });
  return loading;
}

function subscribe(l: () => void) {
  listeners.add(l);
  return () => listeners.delete(l);
}

/** The installed extensions (loaded on first use). */
export function useExtensions(): { list: ExtensionDetails[]; loaded: boolean } {
  const snapshot = useSyncExternalStore(subscribe, () => list);
  const done = useSyncExternalStore(subscribe, () => loaded);
  useEffect(() => {
    if (!loaded) void reloadExtensions();
  }, []);
  return { list: snapshot, loaded: done };
}

const PINNED_KEY = "extensions.pinned";
let pinned: string[] = (() => {
  const v: unknown = readLocal<unknown>(PINNED_KEY, []);
  return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
})();

export function usePinned(): string[] {
  return useSyncExternalStore(subscribe, () => pinned);
}

export function togglePinned(id: string): void {
  pinned = pinned.includes(id) ? pinned.filter((p) => p !== id) : [...pinned, id];
  writeLocal(PINNED_KEY, pinned);
  emit();
}

/** Opens an extension the way Chrome's toolbar does: its pop-up below the button, else its options page in a tab. */
export async function openExtension(ext: ExtensionDetails, anchor: HTMLElement | null): Promise<void> {
  if (!ext.enabled) {
    toast(`${ext.name} is turned off. Turn it on in Settings > Extensions.`);
    return;
  }
  if (ext.popup) {
    const r = anchor?.getBoundingClientRect();
    await attempt(() => api.extensionPopup(ext.id, r ? r.right : window.innerWidth - 8, r ? r.bottom + 4 : 80), "Could not open the extension");
    return;
  }
  if (ext.options) {
    await openOptions(ext);
    return;
  }
  toast(`${ext.name} works on pages by itself; it has no pop-up or options page.`);
}

export async function openOptions(ext: ExtensionDetails): Promise<void> {
  const url = await attempt(() => api.extensionOptionsUrl(ext.id), "Could not open the options");
  if (url) await attempt(() => api.tabNew(url));
}

/** Installs an extension from the Chrome Web Store. */
export async function installFromStore(storeId: string): Promise<boolean> {
  toast("Installing the extension…");
  const added = await attempt(() => api.extensionInstallStore(storeId), "Could not install the extension");
  if (!added) return false;
  toast(`${added.name} added. It runs in every normal tab.`);
  await reloadExtensions();
  return true;
}

/** The Chrome Web Store id of a store detail page address, or null. Mirrors extension_store.rs. */
export function storeIdFromUrl(url: string): string | null {
  let u: URL;
  try {
    u = new URL(url);
  } catch {
    return null;
  }
  if (u.protocol !== "https:") return null;
  const parts = u.pathname.split("/").filter(Boolean);
  const detail =
    (u.hostname === "chromewebstore.google.com" && parts[0] === "detail") ||
    (u.hostname === "chrome.google.com" && parts[0] === "webstore" && parts[1] === "detail");
  if (!detail) return null;
  return [...parts].reverse().find((p) => /^[a-p]{32}$/.test(p)) ?? null;
}
