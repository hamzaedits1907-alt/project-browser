import { useEffect, useState } from "react";

import { api, on } from "./ipc";
import type { PerfSample } from "./types";

export const HISTORY_LENGTH = 60;

/** Appends to a bounded history, dropping the oldest entries. */
export function pushSample<T>(history: T[], sample: T, max = HISTORY_LENGTH): T[] {
  const next = history.length >= max ? history.slice(history.length - max + 1) : history.slice();
  next.push(sample);
  return next;
}

/** A cached sample is only shown straight away if it is recent; otherwise wait for a fresh one. */
export function isFresh(sample: PerfSample, now = Date.now(), maxAgeMs = 15_000): boolean {
  return now - sample.timestamp <= maxAgeMs;
}

export interface PerfFeed {
  sample: PerfSample | null;
  history: PerfSample[];
}

/**
 * Live performance samples. The core only samples while at least one key is
 * subscribed, so this runs the sampler exactly as long as the page is on screen.
 */
export function usePerf(key: string): PerfFeed {
  const [feed, setFeed] = useState<PerfFeed>({ sample: null, history: [] });

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void on("pb:perf", (sample) => setFeed((f) => ({ sample, history: pushSample(f.history, sample) }))).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
    api
      .perfSubscribe(key, true)
      .then((last) => {
        if (!cancelled && last && isFresh(last)) setFeed((f) => (f.sample ? f : { sample: last, history: pushSample(f.history, last) }));
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
      void api.perfSubscribe(key, false).catch(() => {});
    };
  }, [key]);

  return feed;
}

/** One sample, then the sampler is released again (used by the new-tab page). */
export function usePerfOnce(key: string, enabled: boolean): PerfSample | null {
  const [sample, setSample] = useState<PerfSample | null>(null);

  useEffect(() => {
    if (!enabled) return;
    let done = false;
    let unlisten: (() => void) | undefined;
    const finish = (s: PerfSample) => {
      if (done) return;
      done = true;
      setSample(s);
      unlisten?.();
      void api.perfSubscribe(key, false).catch(() => {});
    };
    void on("pb:perf", finish).then((u) => {
      if (done) u();
      else unlisten = u;
    });
    api
      .perfSubscribe(key, true)
      .then((last) => last && isFresh(last) && finish(last))
      .catch(() => {});
    return () => {
      if (!done) {
        done = true;
        unlisten?.();
        void api.perfSubscribe(key, false).catch(() => {});
      }
    };
  }, [key, enabled]);

  return enabled ? sample : null;
}
