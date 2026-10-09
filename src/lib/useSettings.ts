import { useCallback, useEffect, useRef } from "react";

import { api, errorMessage } from "./ipc";
import { getState, setSettingsLocal, toast, useStore } from "./store";
import type { Settings } from "./types";

/**
 * Settings are owned by the core. Edits are applied optimistically, sent to the
 * core, and replaced by the core's sanitised copy (or rolled back on error).
 */
export function useSettings(): { settings: Settings; update: (changes: Partial<Settings>) => Promise<boolean>; updateSoon: (changes: Partial<Settings>) => void } {
  const settings = useStore((s) => s.settings);
  const timer = useRef<number | undefined>(undefined);
  const pending = useRef<Partial<Settings>>({});

  const update = useCallback(async (changes: Partial<Settings>): Promise<boolean> => {
    const before = getState().settings;
    const next = { ...before, ...changes };
    setSettingsLocal(next);
    try {
      setSettingsLocal(await api.settingsSet(next));
      return true;
    } catch (e) {
      setSettingsLocal(before);
      toast(errorMessage(e));
      return false;
    }
  }, []);

  const updateSoon = useCallback(
    (changes: Partial<Settings>) => {
      pending.current = { ...pending.current, ...changes };
      setSettingsLocal({ ...getState().settings, ...changes });
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => {
        const batch = pending.current;
        pending.current = {};
        void update(batch);
      }, 350);
    },
    [update],
  );

  useEffect(
    () => () => {
      window.clearTimeout(timer.current);
      const batch = pending.current;
      if (Object.keys(batch).length) void update(batch);
    },
    [update],
  );

  return { settings, update, updateSoon };
}
