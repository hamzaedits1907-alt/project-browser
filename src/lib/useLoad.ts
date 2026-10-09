import { useEffect, useEffectEvent, useState } from "react";

import { errorMessage } from "./ipc";

export interface Loaded<T> {
  data: T | null;
  error: string | null;
  loading: boolean;
  /** Fetch again (e.g. after a change). */
  reload: () => void;
}

/**
 * Loads data when `key` changes (or `reload()` is called). The fetcher may close
 * over anything; only `key` decides when it runs again.
 */
export function useLoad<T>(fetcher: () => Promise<T>, key: string | number | null): Loaded<T> {
  const [tick, setTick] = useState(0);
  const [state, setState] = useState<{ data: T | null; error: string | null; loading: boolean }>({ data: null, error: null, loading: true });
  const run = useEffectEvent(fetcher);

  useEffect(() => {
    if (key === null) return;
    let live = true;
    run()
      .then((data) => live && setState({ data, error: null, loading: false }))
      .catch((e) => live && setState((s) => ({ ...s, error: errorMessage(e), loading: false })));
    return () => {
      live = false;
    };
  }, [key, tick]);

  return { ...state, reload: () => setTick((t) => t + 1) };
}
