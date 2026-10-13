// The shell's part of a backup (backup.rs has the rest): only these preference keys, never anything else in the
// shell's storage (the AI conversation, caches, window layout).

export const SHELL_KEYS = ["site.prefs", "focus.config", "reader.prefs"] as const;
/** Notes: one key per site ("notes.site.<host>") and the scratchpad ("notes.all"). */
export const NOTE_PREFIX = "notes.";

const known = (k: string) => (SHELL_KEYS as readonly string[]).includes(k) || (k.startsWith(NOTE_PREFIX) && k.length < 300);

export function collectShell(read: (key: string) => string | null, keys: string[] = []): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  const all = [...SHELL_KEYS, ...keys.filter((k) => k.startsWith(`pb:${NOTE_PREFIX}`)).map((k) => k.slice(3))];
  for (const k of all) {
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
  for (const k of Object.keys(data)) {
    if (known(k) && data[k] !== undefined) {
      write(`pb:${k}`, JSON.stringify(data[k]));
      n++;
    }
  }
  return n;
}
