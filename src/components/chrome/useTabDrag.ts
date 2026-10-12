// Pointer-driven tab dragging for the horizontal strip and the vertical tab list.
//
// The tab under the pointer captures it, so moves keep arriving even when the pointer leaves the window. Inside the
// strip the tab follows the pointer and its neighbours make room (live reorder; released = `tab_move`). Pulled away
// from the strip it is "torn off": the strip shows it lifted and the core is told where the pointer is, so another
// window can show where it would land. Released there it joins that window; released anywhere else it becomes a new
// window at the pointer (`tab_drag_end`). Escape cancels. A press that never moves past the threshold is a click.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";

import { api } from "../../lib/ipc";
import { cue } from "../../lib/sounds";
import { attempt, getState, toast } from "../../lib/store";
import { DRAG_THRESHOLD, dropIndex, isTornOff } from "../../lib/tabDrag";
import type { TabBox } from "../../lib/tabDrag";
import type { TabId } from "../../lib/types";

export interface DragView {
  id: TabId;
  /** Offset of the dragged tab from where it started, along the axis. */
  offset: number;
  /** Its size along the axis (neighbours make room by this much). */
  size: number;
  /** Strip index it started at and the `tab_move` index it would land at. */
  from: number;
  to: number;
  /** Pulled out of the strip. */
  torn: boolean;
}

interface Press {
  id: TabId;
  pointerId: number;
  start: { x: number; y: number };
  boxes: TabBox[];
  box: TabBox;
  from: number;
  dragging: boolean;
  everTorn: boolean;
  lastHover: number;
}

const HOVER_EVERY_MS = 60;

export function useTabDrag(list: React.RefObject<HTMLElement | null>, axis: "x" | "y") {
  const [drag, setDrag] = useState<DragView | null>(null);
  const press = useRef<Press | null>(null);
  // The latest view, readable synchronously on release (state may not have rendered yet).
  const latest = useRef<DragView | null>(null);
  const show = useCallback((view: DragView | null) => {
    latest.current = view;
    setDrag(view);
  }, []);

  const measure = useCallback((): TabBox[] => {
    const root = list.current;
    if (!root) return [];
    return [...root.querySelectorAll<HTMLElement>("[data-tab-id]")].map((el) => {
      const r = el.getBoundingClientRect();
      return { id: Number(el.dataset.tabId), start: axis === "x" ? r.left : r.top, size: axis === "x" ? r.width : r.height };
    });
  }, [list, axis]);

  const finish = useCallback(() => {
    const p = press.current;
    press.current = null;
    show(null);
    document.documentElement.classList.remove("is-tab-dragging");
    return p;
  }, [show]);

  const onPointerDown = useCallback(
    (id: TabId, e: ReactPointerEvent<HTMLElement>) => {
      if (e.button !== 0) return;
      const boxes = measure();
      const box = boxes.find((b) => b.id === id);
      const tabs = getState().browser.tabs;
      const from = tabs.findIndex((t) => t.id === id);
      if (!box || from < 0) return;
      e.currentTarget.setPointerCapture(e.pointerId);
      press.current = { id, pointerId: e.pointerId, start: { x: e.clientX, y: e.clientY }, boxes, box, from, dragging: false, everTorn: false, lastHover: 0 };
    },
    [measure],
  );

  const onPointerMove = useCallback(
    (e: ReactPointerEvent<HTMLElement>) => {
      const p = press.current;
      if (!p || e.pointerId !== p.pointerId) return;
      const along = axis === "x" ? e.clientX - p.start.x : e.clientY - p.start.y;
      if (!p.dragging) {
        if (Math.hypot(e.clientX - p.start.x, e.clientY - p.start.y) < DRAG_THRESHOLD) return;
        p.dragging = true;
        document.documentElement.classList.add("is-tab-dragging");
      }
      const rect = list.current?.getBoundingClientRect();
      const torn =
        !!rect &&
        isTornOff({ x: e.clientX, y: e.clientY }, rect, { width: window.innerWidth, height: window.innerHeight }, axis) &&
        getState().browser.tabs.length >= 1;
      if (torn) {
        p.everTorn = true;
        const now = performance.now();
        if (now - p.lastHover > HOVER_EVERY_MS) {
          p.lastHover = now;
          void api.tabDragHover(p.id).catch(() => {});
        }
      }
      const tabs = getState().browser.tabs;
      const center = p.box.start + along + p.box.size / 2;
      const to = torn ? p.from : dropIndex(center, p.boxes, p.id, tabs);
      show({ id: p.id, offset: torn ? 0 : along, size: p.box.size, from: p.from, to, torn });
    },
    [axis, list, show],
  );

  const onPointerUp = useCallback(
    (e: ReactPointerEvent<HTMLElement>) => {
      const p = press.current;
      if (!p || e.pointerId !== p.pointerId) return;
      const view = latest.current;
      finish();
      if (!p.dragging) return;
      // A drag is not a click: keep the release from activating anything underneath.
      e.preventDefault();
      if (view?.torn) {
        cue("power");
        void attempt(async () => {
          const outcome = await api.tabDragEnd(p.id, true);
          if (outcome.kind === "cancelled") toast("The tab stayed where it was.");
        }, "Could not move the tab");
        return;
      }
      if (p.everTorn) void api.tabDragEnd(p.id, false).catch(() => {});
      if (view && view.to !== view.from) {
        cue("tap");
        void attempt(() => api.tabMove(p.id, view.to));
      }
    },
    [finish],
  );

  const cancel = useCallback(() => {
    const p = finish();
    if (p?.everTorn) void api.tabDragEnd(p.id, true, true).catch(() => {});
  }, [finish]);

  // Escape cancels a drag in progress.
  useEffect(() => {
    if (!drag) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        cancel();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [drag, cancel]);

  const onLostPointerCapture = useCallback(
    (e: ReactPointerEvent<HTMLElement>) => {
      if (press.current && press.current.pointerId === e.pointerId) cancel();
    },
    [cancel],
  );

  // Stable identities, so memoised tabs are not re-rendered by every drag frame.
  const handlers = useMemo(
    () => ({ onPointerDown, onPointerMove, onPointerUp, onPointerCancel: cancel, onLostPointerCapture }),
    [onPointerDown, onPointerMove, onPointerUp, cancel, onLostPointerCapture],
  );
  return { drag, handlers };
}
