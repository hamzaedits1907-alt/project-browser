import { getCurrentWindow } from "@tauri-apps/api/window";

// Thin wrappers so the UI keeps working (as a plain web page) outside Tauri, e.g. in `npm run dev`.

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export async function minimizeWindow(): Promise<void> {
  if (inTauri) await getCurrentWindow().minimize();
}

export async function toggleMaximize(): Promise<void> {
  if (inTauri) await getCurrentWindow().toggleMaximize();
}

export async function closeWindow(): Promise<void> {
  if (inTauri) await getCurrentWindow().close();
}

export async function isMaximized(): Promise<boolean> {
  return inTauri ? getCurrentWindow().isMaximized() : false;
}

/** Calls `fn` when the window moves. Resolves to an unsubscribe function. */
export async function onWindowMoved(fn: () => void): Promise<() => void> {
  if (!inTauri) return () => {};
  return getCurrentWindow().onMoved(fn);
}

/** Whether this browser window is in front (Windows' foreground window). Outside Tauri: whether the page has focus. */
export async function windowIsActive(): Promise<boolean> {
  if (!inTauri) return document.hasFocus();
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<boolean>("window_active");
  } catch {
    return true;
  }
}

/** This window's label ("main", "win-2", …); "main" outside Tauri. */
export function windowLabel(): string {
  return inTauri ? getCurrentWindow().label : "main";
}
