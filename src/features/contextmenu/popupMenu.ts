// Shell popups drawn by the menu window (menu_surface.rs) instead of by the shell: the main menu and the address bar's
// suggestion list. The page underneath stays visible and keeps running, and keyboard focus never moves (the menu window
// never takes it), so nothing typed into the address bar is lost. When the menu window is not ready yet, callers fall
// back to drawing the popup themselves (over a still capture of the page).

import { emitTo, listen } from "@tauri-apps/api/event";

import { api } from "../../lib/ipc";
import { windowLabel } from "../../lib/window";
import type { MenuModel } from "./model";
import { MENU_DISMISS, MENU_LABEL, MENU_PICK, MENU_SHOW } from "./surface";
import type { MenuPick, MenuShow } from "./surface";

// Ids far above WebView2's context menu request ids, so the right-click menu host never mistakes a pick for its own.
let nextId = 2_000_000_000;
const waiting = new Map<number, (key: string | null) => void>();
let listening: Promise<void> | null = null;

function ensureListening(): Promise<void> {
  if (!listening) {
    listening = listen<MenuPick>(MENU_PICK, (e) => {
      const done = waiting.get(e.payload.requestId);
      if (!done) return;
      waiting.delete(e.payload.requestId);
      done(e.payload.key);
    }).then(() => undefined);
  }
  return listening;
}

export interface PopupPlace {
  /** Point in this window's CSS pixels: top-left corner, or top-right with `alignEnd`. */
  x: number;
  y: number;
  alignEnd?: boolean;
  /** The menu owns Escape, the arrows and Enter (true for menus; false for the suggestion list). */
  keyboard?: boolean;
}

export interface PopupHandle {
  /** Resolves with the chosen entry's key, or null when the popup was dismissed. */
  result: Promise<string | null>;
  /** Replaces what the open popup shows (same request; the suggestion list as the user types). */
  update: (model: MenuModel, extra?: Pick<MenuShow, "hot" | "width">) => void;
  /** Closes it as dismissed. */
  close: () => void;
}

/**
 * Opens `model` in the menu window at `place`. Resolves to null when the menu window cannot show it (not ready yet);
 * the caller then draws the popup itself.
 */
export async function openPopup(model: MenuModel, place: PopupPlace, extra: Omit<MenuShow, "requestId" | "owner" | "model"> = {}): Promise<PopupHandle | null> {
  let ok: boolean;
  try {
    await ensureListening();
    ok = await api.menuAnchor(place.x, place.y, !!place.alignEnd, place.keyboard ?? true);
  } catch {
    ok = false;
  }
  if (!ok) return null;
  const requestId = nextId++;
  let finished = false;
  const result = new Promise<string | null>((resolve) => {
    waiting.set(requestId, (key) => {
      finished = true;
      resolve(key);
    });
  });
  const send = (m: MenuModel, more: Omit<MenuShow, "requestId" | "owner" | "model">) => {
    const show: MenuShow = { requestId, owner: windowLabel(), model: m, ...more };
    return emitTo(MENU_LABEL, MENU_SHOW, show);
  };
  try {
    await send(model, extra);
  } catch {
    waiting.delete(requestId);
    return null;
  }
  return {
    result,
    update: (m, more) => {
      if (!finished) void send(m, { ...extra, ...more }).catch(() => {});
    },
    close: () => {
      if (!finished) void emitTo(MENU_LABEL, MENU_DISMISS, { requestId }).catch(() => {});
    },
  };
}
