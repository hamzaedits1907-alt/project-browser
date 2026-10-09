import { createStore } from "../../lib/createStore";
import { api, errorMessage, on } from "../../lib/ipc";
import { toast } from "../../lib/store";
import {
  applyEvent,
  applyFailure,
  applyStatus,
  askToInstall,
  canStartCheck,
  canStartInstall,
  cancelConfirm,
  INITIAL_UPDATE,
  shouldAnnounce,
  startChecking,
  startInstall,
} from "./updateModel";
import type { UpdateModel } from "./updateModel";

export const update = createStore<UpdateModel>(INITIAL_UPDATE);

let listening = false;

/** Progress comes from the core as `pb:update`; one listener for the life of the page. */
function ensureListening(): void {
  if (listening) return;
  listening = true;
  void on("pb:update", (e) => update.set((m) => applyEvent(m, e)));
}

/**
 * Asks the update server. `silent` is the startup check: failures stay quiet, and an available update is
 * announced with a toast (never installed).
 */
export async function checkForUpdates(silent = false): Promise<void> {
  ensureListening();
  const before = update.get();
  if (!canStartCheck(before)) return;
  update.set(startChecking(before));
  try {
    const status = await api.updateCheck();
    const next = applyStatus(update.get(), status, Date.now());
    update.set(next);
    if (silent && shouldAnnounce(before, next)) toast(`Version ${next.version} is available. Open Settings → About to install it.`);
  } catch (e) {
    update.set(silent ? { ...before, phase: before.phase === "checking" ? "idle" : before.phase } : applyFailure(update.get(), errorMessage(e)));
  }
}

export function askInstall(): void {
  update.set((m) => askToInstall(m));
}

export function cancelInstallPrompt(): void {
  update.set((m) => cancelConfirm(m));
}

/** Downloads, verifies and installs the version the user confirmed. The app restarts itself when it succeeds. */
export async function installUpdate(): Promise<void> {
  ensureListening();
  if (!canStartInstall(update.get())) return;
  update.set((m) => startInstall(m));
  try {
    await api.updateInstall(); // does not return when the installer takes over
  } catch (e) {
    update.set((m) => applyFailure(m, errorMessage(e)));
  }
}

export function dismissUpdateError(): void {
  update.set((m) => ({ ...m, phase: m.version ? "available" : "idle", error: null }));
}
