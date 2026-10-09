import { useSyncExternalStore } from "react";

export interface Store<T> {
  get: () => T;
  set: (patch: Partial<T> | ((s: T) => Partial<T>)) => void;
  subscribe: (l: () => void) => () => void;
  use: <S>(selector: (s: T) => S) => S;
}

/** Minimal external store (React `useSyncExternalStore` underneath). */
export function createStore<T extends object>(initial: T): Store<T> {
  let state = initial;
  const listeners = new Set<() => void>();
  const get = () => state;
  const set: Store<T>["set"] = (patch) => {
    const next = typeof patch === "function" ? patch(state) : patch;
    state = { ...state, ...next };
    listeners.forEach((l) => l());
  };
  const subscribe = (l: () => void) => {
    listeners.add(l);
    return () => listeners.delete(l);
  };
  const use = <S,>(selector: (s: T) => S): S => useSyncExternalStore(subscribe, () => selector(state));
  return { get, set, subscribe, use };
}
