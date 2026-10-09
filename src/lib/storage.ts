// Per-viewer UI conveniences (panel widths, last used tabs). Never used for
// anything that must persist reliably: those live in the Rust core.

export function readLocal<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(`pb:${key}`);
    return raw === null ? fallback : (JSON.parse(raw) as T);
  } catch {
    return fallback;
  }
}

export function writeLocal(key: string, value: unknown): void {
  try {
    localStorage.setItem(`pb:${key}`, JSON.stringify(value));
  } catch {
    /* storage unavailable: the preference simply is not remembered */
  }
}
