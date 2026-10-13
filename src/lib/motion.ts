// Motion helpers. All timings follow the user's motion setting (the --m scale on <html>); with motion off every
// helper resolves immediately, so nothing ever waits on an animation.

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { RefObject } from "react";

/** The current motion scale (0 when motion is off). */
export function motionScale(): number {
  if (typeof document === "undefined") return 0;
  if (typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) return 0;
  const raw = getComputedStyle(document.documentElement).getPropertyValue("--m").trim();
  const m = Number.parseFloat(raw);
  return Number.isFinite(m) ? m : 1;
}

/** A duration in ms scaled by the motion setting. */
export function motionMs(base: number): number {
  return Math.round(base * motionScale());
}

/**
 * Keeps an element mounted while it plays its exit animation.
 * `mounted` stays true for `exitMs` (scaled) after `open` turns false; `leaving` is true during that time.
 */
export function usePresence(open: boolean, exitMs = 110): { mounted: boolean; leaving: boolean } {
  const [mounted, setMounted] = useState(open);
  const [prevOpen, setPrevOpen] = useState(open);
  // Derived during render (not in an effect): opening mounts at once.
  if (open !== prevOpen) {
    setPrevOpen(open);
    if (open) setMounted(true);
  }
  useEffect(() => {
    if (open || !mounted) return;
    const t = window.setTimeout(() => setMounted(false), Math.max(0, motionMs(exitMs)));
    return () => window.clearTimeout(t);
  }, [open, mounted, exitMs]);
  return { mounted: open || mounted, leaving: !open && mounted };
}

/**
 * FLIP: children with `data-flip` that moved since the last render glide from their old place to the new one
 * (transform only). Elements with `data-flip-skip` (the tab being dragged) are left alone.
 */
export function useFlip(container: RefObject<HTMLElement | null>, key: unknown, axis: "x" | "y" = "x", paused = false): () => void {
  const last = useRef(new Map<string, number>());
  useLayoutEffect(() => {
    const root = container.current;
    // While paused (a tab drag in progress: tabs are moved by transforms) nothing is measured or animated.
    if (!root || paused) return;
    const scale = motionScale();
    const next = new Map<string, number>();
    const items = root.querySelectorAll<HTMLElement>("[data-flip]");
    for (const el of items) {
      const id = el.dataset.flip as string;
      const r = el.getBoundingClientRect();
      const pos = axis === "x" ? r.left : r.top;
      next.set(id, pos);
      const prev = last.current.get(id);
      if (scale <= 0 || prev === undefined || el.dataset.flipSkip !== undefined) continue;
      const delta = prev - pos;
      if (Math.abs(delta) < 1) continue;
      el.style.transition = "none";
      el.style.transform = axis === "x" ? `translateX(${delta}px)` : `translateY(${delta}px)`;
      // Force the start position, then let the stylesheet's transition carry it home.
      void el.offsetWidth;
      el.style.transition = `transform ${Math.round(220 * scale)}ms cubic-bezier(0.16, 1, 0.3, 1)`;
      el.style.transform = "";
      const clear = () => {
        el.style.transition = "";
        el.removeEventListener("transitionend", clear);
      };
      el.addEventListener("transitionend", clear);
    }
    last.current = next;
  });
  // Reading `key` keeps the hook honest about what drives it; layout is measured after every render anyway.
  void key;
  // Records where the items are drawn right now (transforms included), so the next render glides from there.
  return () => {
    const root = container.current;
    if (!root) return;
    const now = new Map<string, number>();
    for (const el of root.querySelectorAll<HTMLElement>("[data-flip]")) {
      const r = el.getBoundingClientRect();
      now.set(el.dataset.flip as string, axis === "x" ? r.left : r.top);
    }
    last.current = now;
  };
}
