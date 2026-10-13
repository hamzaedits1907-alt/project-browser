// The shell's part of a backup (backup.rs has the rest): only these preference keys, never anything else in the
// shell's storage (the AI conversation, caches, window layout).

export const SHELL_KEYS = ["site.prefs", "focus.config", "reader.prefs"] as const;

export function collectShell(read: (key: string) => string | null): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const k of SHELL_KEYS) {
    const raw = read(`pb:${k}`);
    if (raw === null) continue;
    try {
      out[k] = JSON.parse(raw);
    } catch {
      /* a damaged value is left out */
    }
  }
  return out;
}

/** Applies the shell part of a restored backup; returns how many preference sets were restored. Unknown keys are ignored. */
export function applyShell(data: Record<string, unknown> | null, write: (key: string, value: string) => void): number {
  if (!data || typeof data !== "object") return 0;
  let n = 0;
  for (const k of SHELL_KEYS) {
    if (k in data && data[k] !== undefined) {
      write(`pb:${k}`, JSON.stringify(data[k]));
      n++;
    }
  }
  return n;
}
