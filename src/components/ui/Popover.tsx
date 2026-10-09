import { createContext, useContext, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { ReactNode, RefObject } from "react";

import { useOverlay } from "../../lib/overlay";
import { Icon } from "./Icon";
import type { IconName } from "./Icon";
import { cx } from "./controls";

const PopoverContext = createContext<{ close: () => void }>({ close: () => {} });

export interface PopoverProps {
  open: boolean;
  onClose: () => void;
  /** Element the popover hangs from (excluded from outside-click detection). */
  anchor?: RefObject<HTMLElement | null>;
  /** Or a fixed screen point (context menus). */
  point?: { x: number; y: number } | null;
  align?: "start" | "end";
  width?: number;
  className?: string;
  label: string;
  children: ReactNode;
}

const MARGIN = 8;

/**
 * Floating panel drawn by the shell. It hides the native page while open (see
 * lib/overlay.ts) so it never ends up underneath the web content.
 */
export function Popover({ open, onClose, anchor, point, align = "start", width, className, label, children }: PopoverProps) {
  const ready = useOverlay(open);
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ top: number; left: number; maxHeight: number } | null>(null);

  useLayoutEffect(() => {
    if (!ready) return;
    const el = ref.current;
    if (!el) return;
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const w = el.offsetWidth;
    const h = el.offsetHeight;
    let left: number;
    let top: number;
    if (point) {
      left = point.x;
      top = point.y;
    } else {
      const r = anchor?.current?.getBoundingClientRect();
      left = r ? (align === "end" ? r.right - w : r.left) : MARGIN;
      top = r ? r.bottom + 6 : MARGIN;
    }
    left = Math.max(MARGIN, Math.min(left, vw - w - MARGIN));
    if (top + h > vh - MARGIN) top = Math.max(MARGIN, point ? vh - h - MARGIN : vh - h - MARGIN);
    setPos({ top, left, maxHeight: vh - top - MARGIN });
  }, [ready, anchor, point, align]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      const t = e.target as Node;
      if (ref.current?.contains(t) || anchor?.current?.contains(t)) return;
      onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    document.addEventListener("pointerdown", onDown, true);
    document.addEventListener("keydown", onKey, true);
    window.addEventListener("blur", onClose);
    return () => {
      document.removeEventListener("pointerdown", onDown, true);
      document.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", onClose);
    };
  }, [open, onClose, anchor]);

  if (!ready) return null;
  return (
    <PopoverContext.Provider value={{ close: onClose }}>
      <div
        ref={ref}
        role="dialog"
        aria-label={label}
        className={cx("popover", className)}
        style={{ top: pos?.top ?? 0, left: pos?.left ?? 0, width, maxHeight: pos?.maxHeight, visibility: pos ? "visible" : "hidden" }}
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
}

export function MenuItem({ icon, filledIcon, label, hint, onSelect, danger, disabled, checked, keepOpen, trailing }: MenuItemProps) {
  const close = usePopoverClose();
  return (
    <button
      type="button"
      role="menuitem"
      className={cx("menu-item", danger && "is-danger")}
      disabled={disabled}
      onClick={() => {
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
