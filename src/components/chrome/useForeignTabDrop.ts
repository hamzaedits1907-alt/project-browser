// The receiving side of a tab dragged in from another window (`pb:tabdrag`). While it is over this window the strip
// shows where it would land; when it is released here this window takes it (`tab_to_window` with the drop index).

import { useEffect, useState } from "react";
import type { RefObject } from "react";

import { api, on } from "../../lib/ipc";
import { attempt, getState } from "../../lib/store";
import { foreignDropIndex, insertionBefore } from "../../lib/tabDrag";
import type { TabBox } from "../../lib/tabDrag";
import type { TabId } from "../../lib/types";
import { inTauri, windowLabel } from "../../lib/window";

export interface ForeignDrop {
  /** The tab the incoming one would go in front of (`null`: at the end). */
  before: TabId | null;
}

const NEAR = 48;

export function useForeignTabDrop(list: RefObject<HTMLElement | null>, axis: "x" | "y"): ForeignDrop | null {
  const [marker, setMarker] = useState<ForeignDrop | null>(null);

  useEffect(() => {
    if (!inTauri) return;
    const where = (x: number, y: number): number | undefined => {
      const root = list.current;
      if (!root) return undefined;
      const r = root.getBoundingClientRect();
      const near = axis === "x" ? y >= r.top - NEAR && y <= r.bottom + NEAR : x >= r.left - NEAR && x <= r.right + NEAR;
      // Released over the page rather than the strip: the tab goes to the end.
      if (!near) return undefined;
      const boxes: TabBox[] = [...root.querySelectorAll<HTMLElement>("[data-tab-id]")].map((el) => {
        const b = el.getBoundingClientRect();
        return { id: Number(el.dataset.tabId), start: axis === "x" ? b.left : b.top, size: axis === "x" ? b.width : b.height };
      });
      return foreignDropIndex(axis === "x" ? x : y, boxes, getState().browser.tabs);
    };
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void on("pb:tabdrag", (e) => {
      if (e.phase === "leave") {
        setMarker(null);
        return;
      }
      const index = where(e.x, e.y);
      const tabs = getState().browser.tabs;
      if (e.phase === "over") {
        setMarker({ before: index === undefined ? null : insertionBefore(index, null, tabs) });
        return;
      }
      setMarker(null);
      void attempt(() => api.tabToWindow(e.tabId, windowLabel(), index), "Could not move the tab here");
    }).then((u) => {
      if (disposed) u();
      else unlisten = u;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [list, axis]);

  return marker;
}
