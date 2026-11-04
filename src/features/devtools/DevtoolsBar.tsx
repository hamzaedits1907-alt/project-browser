// The bar above docked DevTools (controller/devtools.rs places the DevTools window itself). It names the page being
// inspected, moves DevTools to the bottom, the right or its own window, and closes it. Dragging the bar (docked at the
// bottom) or the grip beside it (docked on the right) resizes DevTools; the core does the layout, so the page and
// DevTools never overlap.

import { useCallback, useRef } from "react";
import type { CSSProperties, PointerEvent as ReactPointerEvent } from "react";

import { Icon } from "../../components/ui/Icon";
import { IconButton } from "../../components/ui/controls";
import { api } from "../../lib/ipc";
import { attempt, useStore } from "../../lib/store";
import type { ContentRect, DevtoolsDock, DevtoolsView } from "../../lib/types";

function place(r: ContentRect): CSSProperties {
  return { left: r.x, top: r.y, width: r.width, height: r.height };
}

function useResize(view: DevtoolsView) {
  const frame = useRef(0);
  const latest = useRef(0);
  return useCallback(
    (e: ReactPointerEvent<HTMLElement>) => {
      if (e.button !== 0) return;
      // Buttons on the bar keep their clicks.
      if ((e.target as HTMLElement).closest("button")) return;
      e.preventDefault();
      const el = e.currentTarget;
      el.setPointerCapture(e.pointerId);
      el.classList.add("is-dragging");
      const along = (ev: PointerEvent | ReactPointerEvent) => (view.side === "right" ? ev.clientX : ev.clientY);
      const send = () => {
        frame.current = 0;
        void api.devtoolsAction({ kind: "resize", pointer: latest.current }).catch(() => {});
      };
      const onMove = (ev: PointerEvent) => {
        latest.current = along(ev);
        if (!frame.current) frame.current = requestAnimationFrame(send);
      };
      const onUp = () => {
        if (frame.current) {
          cancelAnimationFrame(frame.current);
          send();
        }
        el.classList.remove("is-dragging");
        el.removeEventListener("pointermove", onMove);
        el.removeEventListener("pointerup", onUp);
        el.removeEventListener("pointercancel", onUp);
        void api.devtoolsAction({ kind: "resizeEnd" }).catch(() => {});
      };
      el.addEventListener("pointermove", onMove);
      el.addEventListener("pointerup", onUp);
      el.addEventListener("pointercancel", onUp);
    },
    [view.side],
  );
}

const SIDES: { side: DevtoolsDock; icon: "columns" | "panelRight" | "window"; label: string }[] = [
  { side: "bottom", icon: "columns", label: "Dock to the bottom" },
  { side: "right", icon: "panelRight", label: "Dock to the right" },
  { side: "window", icon: "window", label: "Open in a separate window" },
];

function Bar({ view }: { view: DevtoolsView }) {
  const tab = useStore((s) => s.browser.tabs.find((t) => t.id === view.tabId));
  const onResize = useResize(view);
  const dock = (side: DevtoolsDock) => void attempt(() => api.devtoolsAction({ kind: "dock", side }));
  const host = tab ? hostOf(tab.url) : "";
  return (
    <>
      {view.side === "right" && (
        <div className="devtools-grip" style={place(view.grip)} onPointerDown={onResize} role="separator" aria-orientation="vertical" aria-label="Resize developer tools">
          <span className="split-grip" />
        </div>
      )}
      <div
        className={`devtools-bar is-${view.side}`}
        style={place(view.bar)}
        onPointerDown={view.side === "bottom" ? onResize : undefined}
        role="toolbar"
        aria-label="Developer tools"
      >
        <Icon name="code" size={15} />
        <span className="devtools-title">DevTools</span>
        {host && <span className="devtools-host" title={tab?.url}>{host}</span>}
        <span className="devtools-keys">F12 to close</span>
        <span className="devtools-spacer" />
        {SIDES.map((s) => (
          <IconButton key={s.side} icon={s.icon} size={15} label={s.label} active={view.side === s.side} onClick={() => dock(s.side)} />
        ))}
        <IconButton icon="close" size={15} label="Close developer tools" onClick={() => void attempt(() => api.devtoolsAction({ kind: "close" }))} />
      </div>
    </>
  );
}

function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return "";
  }
}

/** Draws the bar of the active tab's docked DevTools, when it has one. */
export function DevtoolsBar() {
  const view = useStore((s) => s.browser.devtools);
  return view ? <Bar view={view} /> : null;
}
