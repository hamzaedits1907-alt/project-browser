import { createStore } from "./createStore";
import { api } from "./ipc";
import { writeLocal } from "./storage";

/** Shell-level UI state that is not part of the browser core (dialogs). */
export const ui = createStore({
  importOpen: false,
  clearDataOpen: false,
});

export const openImportWizard = () => ui.set({ importOpen: true });
export const closeImportWizard = () => ui.set({ importOpen: false });
export const openClearData = () => ui.set({ clearDataOpen: true });
export const closeClearData = () => ui.set({ clearDataOpen: false });

/** Opens Settings on a given section. */
export function openSettingsSection(section: string): void {
  writeLocal("settings.section", section);
  void api.runCommand({ type: "openSettings" }).catch(() => {});
}
