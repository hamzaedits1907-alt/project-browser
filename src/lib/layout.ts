import { useEffect } from "react";
import type { RefObject } from "react";

import { api } from "./ipc";
import type { ContentRect, Viewport } from "./types";
import { isMaximized, onWindowMoved } from "./window";

/** Rounds to device pixels so the native webview never lands on a half pixel. */
export function snapRect(r: { x: number; y: number; width: number; height: number }, scale: number): ContentRect {
  const s = scale > 0 ? scale : 1;
  const snap = (v: number) => Math.round(v * s) / s;
  const x = snap(r.x);
  const y = snap(r.y);
  return { x, y, width: Math.max(0, snap(r.x + r.width) - x), height: Math.max(0, snap(r.y + r.height) - y) };
}

export function sameLayout(a: { content: ContentRect; viewport: Viewport } | null, b: { content: ContentRect; viewport: Viewport }): boolean {
  if (!a) return false;
  const c = a.content;
  const d = b.content;
  const v = a.viewport;
  const w = b.viewport;
  return (
    c.x === d.x && c.y === d.y && c.width === d.width && c.height === d.height &&
    v.width === w.width && v.height === w.height && v.scale === w.scale && v.maximized === w.maximized && v.screenX === w.screenX && v.screenY === w.screenY
  );
}

/**
 * Tells the core where the page area is so it can place the native tab
 * webview exactly over it. Event-driven (resize observer, window resize and
 * move); there is no polling.
 */
export function useContentLayout(ref: RefObject<HTMLElement | null>, enabled: boolean): void {
  useEffect(() => {
    const el = ref.current;
    if (!el || !enabled) return;
    let frame = 0;
    let last: { content: ContentRect; viewport: Viewport } | null = null;
    let maximized = false;

    const report = () => {
      frame = 0;
      const r = el.getBoundingClientRect();
      const scale = window.devicePixelRatio || 1;
      const next = {
        content: snapRect({ x: r.left, y: r.top, width: r.width, height: r.height }, scale),
        viewport: { width: window.innerWidth, height: window.innerHeight, scale, maximized, screenX: window.screenX, screenY: window.screenY },
      };
      if (sameLayout(last, next)) return;
      last = next;
      void api.setLayout(next.content, next.viewport).catch(() => {});
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(report);
    };
    const refreshMaximized = () => void isMaximized().then((m) => {
      if (m !== maximized) {
        maximized = m;
        schedule();
      }
    }).catch(() => {});

    const ro = new ResizeObserver(schedule);
    ro.observe(el);
    const onResize = () => {
      schedule();
      refreshMaximized();
    };
    window.addEventListener("resize", onResize);
    let unMoved: (() => void) | undefined;
    let disposed = false;
    void onWindowMoved(schedule).then((u) => {
      if (disposed) u();
      else unMoved = u;
    }).catch(() => {});
    refreshMaximized();
    schedule();

    return () => {
      disposed = true;
      cancelAnimationFrame(frame);
      ro.disconnect();
      window.removeEventListener("resize", onResize);
      unMoved?.();
    };
  }, [ref, enabled]);
}
