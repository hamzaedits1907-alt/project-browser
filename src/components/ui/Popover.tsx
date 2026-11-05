import { createContext, useContext, useEffect, useEffectEvent, useLayoutEffect, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent, ReactNode, RefObject } from "react";

import { usePresence } from "../../lib/motion";
import { newHoleId, setHole } from "../../lib/overlay";
import { cue } from "../../lib/sounds";
import { trace } from "../../lib/trace";
import { windowIsActive } from "../../lib/window";
import { Icon } from "./Icon";
import type { IconName } from "./Icon";
import { cx } from "./controls";

export const PopoverContext = createContext<{ close: () => void }>({ close: () => {} });

export interface PopoverProps {
  open: boolean;
  onClose: () => void;
  /** Element the popover hangs from (excluded from outside-click detection). */
  anchor?: RefObject<HTMLElement | null>;
  /** Or a fixed point (context menus). */
  point?: { x: number; y: number } | null;
  align?: "start" | "end";
  width?: number;
  className?: string;
  label: string;
  /** Move keyboard focus into the popover when it opens (menus opened from the keyboard). */
  focusFirst?: boolean;
  /** Keep the page visible behind it (the page is not dimmed). */
  clearBackdrop?: boolean;
  /** Called once the popover has finished closing and the page behind it is being given back. */
  onExited?: () => void;
  children: ReactNode;
}

const MARGIN = 8;

function focusables(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>('[role="menuitem"]:not(:disabled), [role="menuitemcheckbox"]:not(:disabled), [role="menuitemradio"]:not(:disabled)')];
}

/**
 * Floating panel drawn by the shell. Native pages are drawn above the shell, so while a popover is open a hole is cut
 * into the page exactly where the popover is (lib/overlay.ts, `setHole`): the popover shows, and the page stays visible
 * and running everywhere else. It animates in from its anchor and out again.
 */
export function Popover({ open, onClose, anchor, point, align = "start", width, className, label, focusFirst, onExited, children }: PopoverProps) {
  const { mounted, leaving } = usePresence(open, 110);
  // Drawn over the live page: a hole is cut into the page where the popover is (see `setHole` below), so the page is
  // never frozen or hidden while a popover is open.
  const ready = mounted;
  const [holeId] = useState(newHoleId);
  const wasMounted = useRef(false);
  const fireExited = useEffectEvent(() => onExited?.());

  // Declared after useOverlay on purpose: its cleanup (giving the page back) has been queued by the time this runs.
  useEffect(() => {
    if (mounted) wasMounted.current = true;
    else if (wasMounted.current) {
      wasMounted.current = false;
      fireExited();
    }
  }, [mounted]);
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ top: number; left: number; maxHeight: number; origin: string } | null>(null);

  useLayoutEffect(() => {
    if (!ready || !open) return;
    const el = ref.current;
    if (!el) return;
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const w = el.offsetWidth;
    const h = el.offsetHeight;
    let left: number;
    let top: number;
    let originX = align === "end" ? "right" : "left";
    let originY = "top";
    if (point) {
      left = point.x;
      top = point.y;
      if (left + w > vw - MARGIN) {
        left = Math.max(MARGIN, point.x - w);
        originX = "right";
      }
      if (top + h > vh - MARGIN) {
        top = Math.max(MARGIN, point.y - h);
        originY = "bottom";
      }
    } else {
      const r = anchor?.current?.getBoundingClientRect();
      left = r ? (align === "end" ? r.right - w : r.left) : MARGIN;
      top = r ? r.bottom + 6 : MARGIN;
      // A popup hanging from a toolbar button stays below it (it scrolls when taller than the room left); it never
      // slides up over its own button and the toolbar.
    }
    left = Math.max(MARGIN, Math.min(left, vw - w - MARGIN));
    setPos({ top, left, maxHeight: vh - top - MARGIN, origin: `${originY} ${originX}` });
  }, [ready, open, anchor, point, align]);

  useEffect(() => {
    if (!open) return;
    trace(`popover "${label}" opening`);
    cue("open");
    const onDown = (e: PointerEvent) => {
      const t = e.target as Node;
      if (ref.current?.contains(t) || anchor?.current?.contains(t)) return;
      trace(`popover "${label}" closed: click outside`);
      onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    // Focus leaving the shell closes the popover: either the page under it was clicked or another window came forward.
    let frame = 0;
    const onBlur = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        void windowIsActive().then((active) => {
          // The page stays where it is under a popover, so the shell losing focus inside the window means the page
          // was clicked: that is a click outside the popover too.
          trace(`popover "${label}" closed: ${active ? "the page was clicked" : "another window came to the front"}`);
          onClose();
        });
      });
    };
    document.addEventListener("pointerdown", onDown, true);
    document.addEventListener("keydown", onKey, true);
    window.addEventListener("blur", onBlur);
    return () => {
      cancelAnimationFrame(frame);
      document.removeEventListener("pointerdown", onDown, true);
      document.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", onBlur);
    };
  }, [open, onClose, anchor, label]);

  // Keyboard users start inside the menu; mouse users keep focus where it was until they press an arrow key.
  useEffect(() => {
    if (!ready || !open || !pos) return;
    const el = ref.current;
    if (!el) return;
    if (focusFirst) focusables(el)[0]?.focus();
    else if (!el.contains(document.activeElement)) el.focus({ preventScroll: true });
  }, [ready, open, pos, focusFirst]);

  const onKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    const el = ref.current;
    if (!el || !["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) return;
    const items = focusables(el);
    if (items.length === 0) return;
    e.preventDefault();
    const at = items.indexOf(document.activeElement as HTMLElement);
    const next =
      e.key === "Home" ? 0 : e.key === "End" ? items.length - 1 : e.key === "ArrowDown" ? (at + 1) % items.length : at <= 0 ? items.length - 1 : at - 1;
    items[next]?.focus();
  };

  useEffect(() => {
    if (ready && open) trace(`popover "${label}" shown`);
  }, [ready, open, label]);

  // The hole in the page follows the popover's box (its content may grow, for example when data arrives).
  useLayoutEffect(() => {
    const el = ref.current;
    if (!mounted || !pos || !el) return;
    const report = () => setHole(holeId, { x: pos.left, y: pos.top, width: el.offsetWidth, height: el.offsetHeight });
    report();
    const ro = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(report);
    ro?.observe(el);
    return () => ro?.disconnect();
  }, [mounted, pos, holeId]);
  useEffect(() => {
    if (!mounted) setHole(holeId, null);
  }, [mounted, holeId]);
  useEffect(() => () => setHole(holeId, null), [holeId]);

  if (!ready) return null;
  return (
    <PopoverContext.Provider value={{ close: onClose }}>
      <div
        ref={ref}
        role="dialog"
        aria-label={label}
        tabIndex={-1}
        className={cx("popover", className, leaving && "is-leaving")}
        onKeyDown={onKeyDown}
        style={{
          top: pos?.top ?? 0,
          left: pos?.left ?? 0,
          width,
          maxHeight: pos?.maxHeight,
          visibility: pos ? "visible" : "hidden",
          ["--origin" as string]: pos?.origin ?? "top left",
        }}
      >
        {children}
      </div>
    </PopoverContext.Provider>
  );
}

export function usePopoverClose(): () => void {
  return useContext(PopoverContext).close;
}

interface MenuItemProps {
  icon?: IconName;
  filledIcon?: boolean;
  label: string;
  hint?: string;
  onSelect: () => void;
  danger?: boolean;
  disabled?: boolean;
  checked?: boolean;
  keepOpen?: boolean;
  trailing?: ReactNode;
  title?: string;
}

export function MenuItem({ icon, filledIcon, label, hint, onSelect, danger, disabled, checked, keepOpen, trailing, title }: MenuItemProps) {
  const close = usePopoverClose();
  return (
    <button
      type="button"
      role={checked === undefined ? "menuitem" : "menuitemcheckbox"}
      aria-checked={checked}
      className={cx("menu-item", danger && "is-danger")}
      disabled={disabled}
      title={title}
      onClick={() => {
        cue("tap");
        if (!keepOpen) close();
        onSelect();
      }}
    >
      <span className="menu-icon">{icon && <Icon name={icon} size={17} filled={filledIcon} />}</span>
      <span className="menu-label truncate">{label}</span>
      {checked && <Icon name="check" size={15} className="menu-check" />}
      {trailing}
      {hint && <span className="menu-hint">{hint}</span>}
    </button>
  );
}

export function MenuSeparator() {
  return <div className="menu-sep" role="separator" />;
}

export function MenuHeading({ children }: { children: ReactNode }) {
  return <div className="menu-heading">{children}</div>;
}
