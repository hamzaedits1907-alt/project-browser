import { useEffect, useRef } from "react";
import type { ReactNode } from "react";

import { useOverlay } from "../../lib/overlay";
import { IconButton } from "./controls";

interface ModalProps {
  open: boolean;
  title: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  width?: number;
  /** Disables Escape / backdrop dismissal (e.g. while an import is running). */
  locked?: boolean;
}

export function Modal({ open, title, onClose, children, footer, width = 520, locked }: ModalProps) {
  const ready = useOverlay(open);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!ready) return;
    ref.current?.querySelector<HTMLElement>("[data-autofocus], input, button:not(.modal-close)")?.focus();
  }, [ready]);

  useEffect(() => {
    if (!open || locked) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, locked, onClose]);

  if (!ready) return null;
  return (
    <div className="modal-backdrop" onPointerDown={(e) => e.target === e.currentTarget && !locked && onClose()}>
      <div ref={ref} className="modal" role="dialog" aria-modal="true" aria-label={title} style={{ width }}>
        <header className="modal-head">
          <h2>{title}</h2>
          {!locked && <IconButton className="modal-close" icon="close" label="Close" size={16} onClick={onClose} />}
        </header>
        <div className="modal-body">{children}</div>
        {footer && <footer className="modal-foot">{footer}</footer>}
      </div>
    </div>
  );
}
