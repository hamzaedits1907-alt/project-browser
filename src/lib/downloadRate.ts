// Download speed and time left, estimated from the progress events the core sends (at most 4 per second). Pure.
//
// Only shown when it means something: a speed needs two samples at least half a second apart, and a time left also
// needs a known total size. The speed is smoothed (exponential moving average) so it does not jump with every event.

export interface RateSample {
  bytes: number;
  at: number;
  /** Bytes per second, smoothed; null until it can be estimated. */
  rate: number | null;
}

const MIN_INTERVAL_MS = 500;
const SMOOTHING = 0.3;

export function nextSample(prev: RateSample | undefined, bytes: number, at: number): RateSample {
  if (!prev || bytes < prev.bytes) return { bytes, at, rate: null };
  const dt = at - prev.at;
  if (dt < MIN_INTERVAL_MS) return prev;
  const now = ((bytes - prev.bytes) * 1000) / dt;
  const rate = prev.rate === null ? now : prev.rate + SMOOTHING * (now - prev.rate);
  return { bytes, at, rate };
}

/** Seconds left, or null when it cannot be estimated meaningfully. */
export function secondsLeft(received: number, total: number | null, rate: number | null): number | null {
  if (total === null || total <= 0 || rate === null || rate < 1 || received >= total) return null;
  return Math.ceil((total - received) / rate);
}

export function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds} s left`;
  const m = Math.round(seconds / 60);
  if (m < 60) return `${m} min left`;
  const h = Math.floor(m / 60);
  return `${h} h ${m % 60} min left`;
}
