import { useRef, useState } from "react";

import { api } from "../../lib/ipc";
import { attempt, useStore } from "../../lib/store";

/** Must match `split::GAP` in the core. */
export const SPLIT_GAP = 6;

/**
 * The handle between the two halves of split view. The pages themselves are native views placed by the core; this
 * handle lives in the gap the core leaves between them. Dragging it resizes the halves, double-clicking evens them.
 */
export function SplitDivider({ width }: { width: number }) {
  const split = useStore((s) => s.browser.split);
  const activeId = useStore((s) => s.browser.activeId);
  const [drag, setDrag] = useState<number | null>(null);
  const pending = useRef<number | null>(null);
  const timer = useRef<number | undefined>(undefined);
  if (!split || (activeId !== split.left && activeId !== split.right) || width <= 0) return null;

  const ratio = drag ?? split.ratio;
  const left = Math.round((width - SPLIT_GAP) * ratio);
  const send = (r: number) => {
    pending.current = r;
    if (timer.current !== undefined) return;
    timer.current = window.setTimeout(() => {
      timer.current = undefined;
      if (pending.current !== null) void attempt(() => api.splitView({ op: "ratio", ratio: pending.current as number }));
    }, 40);
  };

  return (
    <div
      className={`split-divider${drag !== null ? " is-dragging" : ""}`}
      style={{ left }}
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize split view (double-click to even out)"
      aria-valuenow={Math.round(ratio * 100)}
      tabIndex={0}
      onDoubleClick={() => void attempt(() => api.splitView({ op: "ratio", ratio: 0.5 }))}
      onKeyDown={(e) => {
        if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
          e.preventDefault();
          void attempt(() => api.splitView({ op: "ratio", ratio: split.ratio + (e.key === "ArrowLeft" ? -0.05 : 0.05) }));
        }
      }}
      onPointerDown={(e) => {
        e.currentTarget.setPointerCapture(e.pointerId);
        setDrag(split.ratio);
      }}
      onPointerMove={(e) => {
        if (drag === null) return;
        const host = e.currentTarget.parentElement?.getBoundingClientRect();
        if (!host) return;
        const r = Math.min(0.8, Math.max(0.2, (e.clientX - host.left) / host.width));
        setDrag(r);
        send(r);
      }}
      onPointerUp={(e) => {
        e.currentTarget.releasePointerCapture(e.pointerId);
        if (drag !== null) void attempt(() => api.splitView({ op: "ratio", ratio: drag }));
        setDrag(null);
      }}
    >
      <span className="split-grip" />
    </div>
  );
}
