// Native tab webviews are drawn above the shell webview, so shell popups would be hidden behind the page. While any
// popup is open we ask the core to hide the page and hand back a snapshot, which the content area shows in its place.
// Menus that should not change how the page looks (the right-click menu) ask for a clear backdrop; everything else
// gets a dimmed one, which marks the page as inactive.

import { useEffect, useState, useSyncExternalStore } from "react";

import { api } from "./ipc";
import type { ContentRect } from "./types";
import { activeTab, getState } from "./store";
import { isInternalUrl } from "./urls";

export type Backdrop = "dim" | "clear";

interface OverlayState {
  open: boolean;
  snapshot: string | null;
  backdrop: Backdrop;
}

let overlay: OverlayState = { open: false, snapshot: null, backdrop: "dim" };
let holders = 0;
let dimHolders = 0;
let chain: Promise<void> = Promise.resolve();
const listeners = new Set<() => void>();

function setOverlay(next: OverlayState): void {
  overlay = next;
  listeners.forEach((l) => l());
}

function enqueue(step: () => Promise<void>): Promise<void> {
  chain = chain.then(step, step);
  return chain;
}

const wantedBackdrop = (): Backdrop => (dimHolders > 0 ? "dim" : "clear");

async function openStep(): Promise<void> {
  if (holders === 0) return;
  if (overlay.open) {
    if (overlay.backdrop !== wantedBackdrop()) setOverlay({ ...overlay, backdrop: wantedBackdrop() });
    return;
  }
  const tab = activeTab(getState());
  if (!tab || isInternalUrl(tab.url)) return; // internal pages are drawn by the shell: nothing to hide
  let snapshot: string | null = null;
  try {
    snapshot = await api.overlayOpen(!userIsTyping());
  } catch {
    /* the page is hidden either way; we just have no backdrop */
  }
  setOverlay({ open: true, snapshot, backdrop: wantedBackdrop() });
}

function userIsTyping(): boolean {
  const el = typeof document === "undefined" ? null : (document.activeElement as HTMLElement | null);
  return !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable === true);
}

async function closeStep(): Promise<void> {
  if (!overlay.open) return;
  if (holders > 0) {
    if (overlay.backdrop !== wantedBackdrop()) setOverlay({ ...overlay, backdrop: wantedBackdrop() });
    return;
  }
  const old = overlay.snapshot;
  setOverlay({ open: false, snapshot: null, backdrop: "dim" });
  if (old?.startsWith("blob:")) URL.revokeObjectURL(old);
  // Give focus back to the page unless one of the shell's own text fields is being used.
  const typing = userIsTyping();
  try {
    await api.overlayClose(!typing);
  } catch {
    /* nothing useful to do */
  }
}

export function acquireOverlay(backdrop: Backdrop = "dim"): { ready: Promise<void>; release: () => void } {
  holders++;
  if (backdrop === "dim") dimHolders++;
  const ready = enqueue(openStep);
  let released = false;
  return {
    ready,
    release: () => {
      if (released) return;
      released = true;
      holders--;
      if (backdrop === "dim") dimHolders--;
      void enqueue(closeStep);
    },
  };
}

/** Resolves when every queued overlay change (opening, closing, giving focus back to the page) has finished. */
export function overlayIdle(): Promise<void> {
  return enqueue(async () => {});
}

/**
 * Hides the native page while `active` is true. Returns true once the page is hidden and the popup may be drawn.
 */
export function useOverlay(active: boolean, backdrop: Backdrop = "dim"): boolean {
  const [ready, setReady] = useState(false);

  useEffect(() => {
    if (!active) return;
    const handle = acquireOverlay(backdrop);
    let cancelled = false;
    void handle.ready.then(() => {
      if (!cancelled) setReady(true);
    });
    return () => {
      cancelled = true;
      handle.release();
      setReady(false);
    };
  }, [active, backdrop]);

  return active && ready;
}

export function useOverlayState(): OverlayState {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => overlay,
  );
}

// ---- Holes: popups drawn over a live page ----------------------------------------------------------------------
// Popovers no longer hide the page behind a still capture. Each open popover reports where it is drawn; the core cuts
// a hole into the page there (a window region), so the popover shows and the page keeps running everywhere else.

const holes = new Map<number, ContentRect>();
let holeIds = 0;
let holeSend = 0;

function sendHoles(): void {
  holeSend = 0;
  void api.pageHoles([...holes.values()]).catch(() => {});
}

/** A new id for `setHole`. */
export function newHoleId(): number {
  return ++holeIds;
}

/** Sets (or with null removes) the rectangle, in this window's CSS pixels, a popover covers. */
export function setHole(id: number, rect: ContentRect | null): void {
  const old = holes.get(id);
  if (rect === null) {
    if (!old) return;
    holes.delete(id);
  } else {
    if (old && old.x === rect.x && old.y === rect.y && old.width === rect.width && old.height === rect.height) return;
    holes.set(id, rect);
  }
  // One message per frame at most, however many popovers move at once.
  if (!holeSend) holeSend = requestAnimationFrame(sendHoles);
}
