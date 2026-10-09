import { createStore } from "./createStore";

/** Shell-level UI state that is not part of the browser core (dialogs). */
export const ui = createStore({
  importOpen: false,
  clearDataOpen: false,
});

export const openImportWizard = () => ui.set({ importOpen: true });
export const closeImportWizard = () => ui.set({ importOpen: false });
export const openClearData = () => ui.set({ clearDataOpen: true });
export const closeClearData = () => ui.set({ clearDataOpen: false });
