// Pointer-driven tab dragging for the horizontal strip and the vertical tab list, on top of the pure state machine in
// lib/tabDragMachine.ts (which decides what a pointer sequence means).
//
// A press captures the pointer on the tab and listens on the window for the rest of the gesture (move, release,
// cancel, Escape, blur), so nothing depends on the tab element surviving re-renders. Inside the strip the tab follows
// the pointer and its neighbours make room; near the strip's ends the strip auto-scrolls (positions are in content
// coordinates, so the target does not jump). Pulled away from the strip it is torn off: the core is told where the
// pointer is so another window can show where it would land, and on release the core moves it there or into a new
// window. A release inside the strip moves the tab (`tab_move`) and the new order is shown at once, until the core's
// state confirms it. A press that never moves is a click and activates the tab on release.

import { useCallback, useEffect, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";

import { api } from "../../lib/ipc";
import { cue } from "../../lib/sounds";
import { attempt, getState, toast } from "../../lib/store";
import { isTornOff } from "../../lib/tabDrag";
import type { TabBox } from "../../lib/tabDrag";
import { IDLE, autoScrollSpeed, cancel as cancelDrag, move, press, release, reordered } from "../../lib/tabDragMachine";
import type { DragOutcome, DragState, Point } from "../../lib/tabDragMachine";
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

const HOVER_EVERY_MS = 60;

function viewOf(s: DragState): DragView | null {
  return s.phase === "dragging" ? { id: s.id, offset: s.offset, size: s.box.size, from: s.from, to: s.to, torn: s.torn } : null;
}

function sameView(a: DragView | null, b: DragView | null): boolean {
  if (a === null || b === null) return a === b;
  return a.id === b.id && a.offset === b.offset && a.to === b.to && a.torn === b.torn;
}

/**
 * `beforeReorder` runs just before the strip shows a new order (the FLIP animation records where the tabs are drawn).
 * Returns the drag to draw, the optimistic order (or null) and the pointer-down handler for tabs.
 */
export function useTabDrag(list: React.RefObject<HTMLElement | null>, axis: "x" | "y", beforeReorder?: () => void) {
  const [drag, setDrag] = useState<DragView | null>(null);
  const [order, setOrder] = useState<TabId[] | null>(null);
  const state = useRef<DragState>(IDLE);
  const pointer = useRef<{ id: number; client: Point } | null>(null);
  const captured = useRef<HTMLElement | null>(null);
  const frame = useRef<number | null>(null);
  const lastHover = useRef(0);
  const detach = useRef<(() => void) | null>(null);
  const stepRef = useRef<(pointerId: number, client: Point) => void>(() => {});
  const before = useRef(beforeReorder);
  useEffect(() => {
    before.current = beforeReorder;
  }, [beforeReorder]);

  const along = useCallback(
    (client: Point): number => {
      const root = list.current;
      if (!root) return 0;
      const r = root.getBoundingClientRect();
      return axis === "x" ? client.x - r.left + root.scrollLeft : client.y - r.top + root.scrollTop;
    },
    [list, axis],
  );

  const measure = useCallback((): TabBox[] => {
    const root = list.current;
    if (!root) return [];
    const r = root.getBoundingClientRect();
    return [...root.querySelectorAll<HTMLElement>("[data-tab-id]")].map((el) => {
      const b = el.getBoundingClientRect();
      return axis === "x"
        ? { id: Number(el.dataset.tabId), start: b.left - r.left + root.scrollLeft, size: b.width }
        : { id: Number(el.dataset.tabId), start: b.top - r.top + root.scrollTop, size: b.height };
    });
  }, [list, axis]);

  const show = useCallback((next: DragState) => {
    state.current = next;
    const v = viewOf(next);
    setDrag((prev) => (sameView(prev, v) ? prev : v));
    document.documentElement.classList.toggle("is-tab-dragging", next.phase === "dragging");
  }, []);

  const stopScroll = () => {
    if (frame.current !== null) cancelAnimationFrame(frame.current);
    frame.current = null;
  };

  /** Ends the gesture: listeners, capture and auto-scroll go, then the outcome is carried out. */
  const finish = useCallback(
    (prev: DragState, outcome: DragOutcome | null) => {
      stopScroll();
      detach.current?.();
      detach.current = null;
      const el = captured.current;
      captured.current = null;
      if (el && pointer.current && el.hasPointerCapture?.(pointer.current.id)) el.releasePointerCapture(pointer.current.id);
      pointer.current = null;
      const everTorn = prev.phase === "dragging" && prev.everTorn;
      if (outcome?.kind === "reorder") before.current?.();
      show(IDLE);
      if (!outcome) return;
      switch (outcome.kind) {
        case "click":
          if (getState().browser.activeId !== outcome.id) {
            cue("tap");
            void attempt(() => api.tabActivate(outcome.id));
          }
          return;
        case "reorder": {
          if (everTorn) void api.tabDragEnd(outcome.id, false).catch(() => {});
          cue("tap");
          setOrder(reordered(getState().browser.tabs, outcome.id, outcome.to));
          void attempt(() => api.tabMove(outcome.id, outcome.to).then(() => true)).then((ok) => {
            // Refused: show the core's order again (the tab stays where it was).
            if (!ok) setOrder(null);
          });
          return;
        }
        case "tear":
          cue("power");
          void attempt(async () => {
            const result = await api.tabDragEnd(outcome.id, true);
            if (result.kind === "cancelled") toast("The tab stayed where it was.");
          }, "Could not move the tab");
          return;
        case "none":
          if (outcome.abortTear && outcome.id !== null) void api.tabDragEnd(outcome.id, true, true).catch(() => {});
          return;
      }
    },
    [show],
  );

  const step = useCallback(
    (pointerId: number, client: Point) => {
      const root = list.current;
      if (!root || state.current.phase === "idle") return;
      const r = root.getBoundingClientRect();
      const outside = isTornOff(client, r, { width: window.innerWidth, height: window.innerHeight }, axis);
      const prev = state.current;
      const next = move(prev, {
        pointerId,
        client,
        along: along(client),
        outside,
        extent: axis === "x" ? root.scrollWidth : root.scrollHeight,
        tabs: getState().browser.tabs,
      });
      if (next.phase === "idle") {
        // The dragged tab closed in the meantime.
        finish(prev, cancelDrag(prev).outcome);
        return;
      }
      show(next);
      if (next.phase !== "dragging") return;
      if (next.torn) {
        stopScroll();
        const now = performance.now();
        if (now - lastHover.current > HOVER_EVERY_MS) {
          lastHover.current = now;
          void api.tabDragHover(next.id).catch(() => {});
        }
        return;
      }
      const speed = axis === "x" ? autoScrollSpeed(client.x, r.left, r.right) : autoScrollSpeed(client.y, r.top, r.bottom);
      const scrollable = axis === "x" ? root.scrollWidth > root.clientWidth : root.scrollHeight > root.clientHeight;
      if (speed === 0 || !scrollable) {
        stopScroll();
        return;
      }
      if (frame.current === null) {
        const tick = () => {
          frame.current = null;
          const p = pointer.current;
          const el = list.current;
          if (!p || !el || state.current.phase !== "dragging") return;
          const rr = el.getBoundingClientRect();
          const v = axis === "x" ? autoScrollSpeed(p.client.x, rr.left, rr.right) : autoScrollSpeed(p.client.y, rr.top, rr.bottom);
          if (v === 0) return;
          if (axis === "x") el.scrollLeft += v;
          else el.scrollTop += v;
          stepRef.current(p.id, p.client);
          if (frame.current === null) frame.current = requestAnimationFrame(tick);
        };
        frame.current = requestAnimationFrame(tick);
      }
    },
    [list, axis, along, show, finish],
  );
  useEffect(() => {
    stepRef.current = step;
  }, [step]);

  const onPointerDown = useCallback(
    (id: TabId, e: ReactPointerEvent<HTMLElement>) => {
      if (e.button !== 0 || state.current.phase !== "idle") return;
      const client = { x: e.clientX, y: e.clientY };
      const next = press(id, e.pointerId, client, along(client), measure(), getState().browser.tabs);
      if (next.phase === "idle") return;
      const el = e.currentTarget;
      el.setPointerCapture(e.pointerId);
      captured.current = el;
      pointer.current = { id: e.pointerId, client };
      show(next);

      const onMove = (ev: PointerEvent) => {
        if (ev.pointerId !== pointer.current?.id) return;
        pointer.current = { id: ev.pointerId, client: { x: ev.clientX, y: ev.clientY } };
        step(ev.pointerId, pointer.current.client);
      };
      const onUp = (ev: PointerEvent) => {
        if (ev.pointerId !== pointer.current?.id) return;
        // Bring the drag up to the release point, then end it.
        step(ev.pointerId, { x: ev.clientX, y: ev.clientY });
        const prev = state.current;
        const { outcome } = release(prev, ev.pointerId);
        if (prev.phase === "dragging") ev.preventDefault();
        finish(prev, outcome);
      };
      const abort = () => {
        const prev = state.current;
        finish(prev, cancelDrag(prev).outcome);
      };
      const onCancel = (ev: PointerEvent) => {
        if (ev.pointerId === pointer.current?.id) abort();
      };
      const onKey = (ev: KeyboardEvent) => {
        if (ev.key === "Escape") {
          ev.preventDefault();
          ev.stopPropagation();
          abort();
        }
      };
      const onLost = (ev: PointerEvent) => {
        // After a normal release the gesture is already over; otherwise capture was taken away: cancel.
        if (ev.pointerId === pointer.current?.id) abort();
      };
      window.addEventListener("pointermove", onMove, true);
      window.addEventListener("pointerup", onUp, true);
      window.addEventListener("pointercancel", onCancel, true);
      window.addEventListener("keydown", onKey, true);
      window.addEventListener("blur", abort);
      el.addEventListener("lostpointercapture", onLost);
      detach.current = () => {
        window.removeEventListener("pointermove", onMove, true);
        window.removeEventListener("pointerup", onUp, true);
        window.removeEventListener("pointercancel", onCancel, true);
        window.removeEventListener("keydown", onKey, true);
        window.removeEventListener("blur", abort);
        el.removeEventListener("lostpointercapture", onLost);
      };
    },
    [along, measure, show, step, finish],
  );

  // Unmounting mid-drag (the window closes, the strip switches orientation) ends the drag cleanly.
  useEffect(
    () => () => {
      const prev = state.current;
      if (prev.phase !== "idle") finish(prev, cancelDrag(prev).outcome);
    },
    [finish],
  );

  const settle = useCallback(() => setOrder(null), []);
  return { drag, order, settle, onPointerDown };
}
