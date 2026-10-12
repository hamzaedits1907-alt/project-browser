import { createStore } from "./createStore";
import { api } from "./ipc";
import { readLocal, writeLocal } from "./storage";

/** Side panels docked next to the page (they push the page aside: nothing has to float over the native view). */
export type PanelId = "bookmarks" | "history" | "downloads" | "shield" | "notes" | "media" | "focus";
const PANELS: PanelId[] = ["bookmarks", "history", "downloads", "shield", "notes", "media", "focus"];

function savedPanel(): PanelId | null {
  const p = readLocal<string | null>("panel", null);
  return PANELS.includes(p as PanelId) ? (p as PanelId) : null;
}

/** Shell-level UI state that is not part of the browser core (dialogs, panels). */
export const ui = createStore({
  importOpen: false,
  clearDataOpen: false,
  panel: savedPanel(),
  /** The "customize toolbar / sidebar" dialog. */
  customize: null as null | "toolbar" | "sidebar",
  /** Bumped when something asks Settings to show a section (an already open Settings page follows it). */
  settingsJump: { section: "", n: 0 },
  /** Shell dialogs and full-page views opened from the Command Center, menus and shortcuts. */
  dialog: null as null | "palette" | "reader" | "screenshot" | "shortcuts" | "share" | "workspaces",
});

export type ShellDialog = NonNullable<ReturnType<typeof ui.get>["dialog"]>;
export const openDialog = (dialog: ShellDialog) => ui.set({ dialog });
export const closeDialog = () => ui.set({ dialog: null });

export const openImportWizard = () => ui.set({ importOpen: true });
export const closeImportWizard = () => ui.set({ importOpen: false });
export const openClearData = () => ui.set({ clearDataOpen: true });
export const closeClearData = () => ui.set({ clearDataOpen: false });
export const openCustomize = (what: "toolbar" | "sidebar") => ui.set({ customize: what });
export const closeCustomize = () => ui.set({ customize: null });

/** Opens a side panel, or closes it when it is already open. */
export function togglePanel(panel: PanelId): void {
  const next = ui.get().panel === panel ? null : panel;
  ui.set({ panel: next });
  writeLocal("panel", next);
}

export function closePanel(): void {
  ui.set({ panel: null });
  writeLocal("panel", null);
}

/** Opens Settings on a given section. */
export function openSettingsSection(section: string): void {
  writeLocal("settings.section", section);
  ui.set((s) => ({ settingsJump: { section, n: s.settingsJump.n + 1 } }));
  void api.runCommand({ type: "openSettings" }).catch(() => {});
}
