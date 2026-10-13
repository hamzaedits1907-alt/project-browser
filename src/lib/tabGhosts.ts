// Closing animation for tabs: a tab that left the strip is drawn once more as a "ghost" (not interactive, no tab id)
// that collapses to nothing, so its neighbours slide into the gap instead of jumping. Pure; TabStrip renders them.

import type { TabId } from "./types";

export interface Drawn {
  id: TabId;
  title: string;
  /** Size along the strip's axis when last drawn (px). */
  size: number;
}

export interface Ghost extends Drawn {
  /** The tab it is drawn in front of (the one that followed it), or null for the end of the strip. */
  before: TabId | null;
}

/** Tabs drawn last time that are gone now, each placed before its nearest following tab that is still there. */
export function closedGhosts(previous: Drawn[], current: Set<TabId>, max = 8): Ghost[] {
  const out: Ghost[] = [];
  previous.forEach((p, i) => {
    if (current.has(p.id) || p.size <= 0) return;
    const next = previous.slice(i + 1).find((q) => current.has(q.id));
    out.push({ ...p, before: next?.id ?? null });
  });
  // Closing many tabs at once (close others) does not need dozens of ghosts.
  return out.slice(0, max);
}
