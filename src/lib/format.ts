// Pure formatting helpers (unit-tested).

const UNITS = ["B", "KB", "MB", "GB", "TB"];

export function formatBytes(bytes: number | null | undefined, digits = 1): string {
  if (bytes == null || !Number.isFinite(bytes) || bytes < 0) return "-";
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  const d = unit === 0 ? 0 : value >= 100 ? 0 : digits;
  return `${value.toFixed(d)} ${UNITS[unit]}`;
}

export function formatPercent(p: number | null | undefined, digits = 0): string {
  if (p == null || !Number.isFinite(p)) return "-";
  return `${p.toFixed(digits)}%`;
}

export function formatDuration(ms: number | null | undefined): string {
  if (ms == null || !Number.isFinite(ms) || ms < 0) return "-";
  if (ms < 1000) return `${Math.round(ms)} ms`;
  const s = ms / 1000;
  return s < 10 ? `${s.toFixed(2)} s` : `${s.toFixed(1)} s`;
}

/** "3 min ago", "yesterday", ... relative to `now`. */
export function formatRelative(ts: number, now = Date.now()): string {
  const diff = Math.max(0, now - ts);
  const min = Math.floor(diff / 60_000);
  if (min < 1) return "just now";
  if (min < 60) return `${min} min ago`;
  const h = Math.floor(min / 60);
  if (h < 24) return `${h} hr ago`;
  const d = Math.floor(h / 24);
  if (d === 1) return "yesterday";
  if (d < 30) return `${d} days ago`;
  return new Date(ts).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}

export function formatTime(ts: number): string {
  return new Date(ts).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
}

/** Group heading for a history day: Today, Yesterday, or a long date. */
export function dayLabel(ts: number, now = Date.now()): string {
  const d = startOfDay(ts);
  const today = startOfDay(now);
  const dayMs = 86_400_000;
  if (d === today) return "Today";
  if (d === today - dayMs) return "Yesterday";
  return new Date(ts).toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric", year: "numeric" });
}

export function startOfDay(ts: number): number {
  const d = new Date(ts);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

export function groupByDay<T>(items: T[], ts: (item: T) => number, now = Date.now()): { label: string; items: T[] }[] {
  const groups: { label: string; items: T[] }[] = [];
  let currentDay = -1;
  for (const item of items) {
    const day = startOfDay(ts(item));
    if (day !== currentDay) {
      groups.push({ label: dayLabel(ts(item), now), items: [] });
      currentDay = day;
    }
    groups[groups.length - 1]?.items.push(item);
  }
  return groups;
}

export function downloadProgress(received: number, total: number | null): number | null {
  if (!total || total <= 0) return null;
  return Math.max(0, Math.min(1, received / total));
}

const PERMISSION_LABELS: Record<string, string> = {
  microphone: "Use your microphone",
  camera: "Use your camera",
  geolocation: "Know your location",
  notifications: "Show notifications",
  sensors: "Use motion and light sensors",
  clipboard: "See text and images you copy",
  downloads: "Download multiple files automatically",
  files: "Edit files and folders on your device",
  fonts: "See the fonts installed on your device",
  midi: "Use your MIDI devices",
  windowManagement: "Manage windows on all your displays",
};

const PERMISSION_NAMES: Record<string, string> = {
  microphone: "Microphone",
  camera: "Camera",
  geolocation: "Location",
  notifications: "Notifications",
  sensors: "Motion sensors",
  clipboard: "Clipboard",
  downloads: "Automatic downloads",
  files: "File editing",
  fonts: "Local fonts",
  midi: "MIDI devices",
  windowManagement: "Window management",
};

/** Short noun, e.g. "Camera". */
export function permissionName(kind: string): string {
  return PERMISSION_NAMES[kind] ?? kind.charAt(0).toUpperCase() + kind.slice(1);
}

/** Verb phrase for prompts, e.g. "Use your camera". */
export function permissionLabel(kind: string): string {
  return PERMISSION_LABELS[kind] ?? `Use ${kind}`;
}

export function hostOf(url: string): string {
  try {
    return new URL(url).host.replace(/^www\./, "");
  } catch {
    return url;
  }
}

export function clamp(n: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, n));
}
