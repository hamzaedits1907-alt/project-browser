// Messages between a browser window's shell and the right-click menu's own window ("ctxmenu", see menu_surface.rs).
// The shell builds the menu model and sends it with `pb:menu-show`; the menu window answers with `pb:menu-pick`
// (the chosen entry's key, or null when the menu was dismissed). Each request is answered exactly once.

import type { MenuEntry, MenuModel } from "./model";

export const MENU_LABEL = "ctxmenu";
export const MENU_SHOW = "pb:menu-show";
export const MENU_PICK = "pb:menu-pick";

export interface MenuShow {
  requestId: number;
  /** Label of the browser window the menu belongs to (where the pick goes). */
  owner: string;
  model: MenuModel;
}

export interface MenuPick {
  requestId: number;
  key: string | null;
}

/** Every entry of a model, navigation row included. */
export function allEntries(model: MenuModel): MenuEntry[] {
  return [...model.navRow, ...model.sections.flatMap((s) => s.entries)];
}

/** The entry a pick refers to, if the key belongs to this model. */
export function entryForKey(model: MenuModel, key: string | null): MenuEntry | null {
  if (key === null) return null;
  return allEntries(model).find((e) => e.key === key) ?? null;
}
