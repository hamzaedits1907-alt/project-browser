import { dismissToast, useStore } from "../../lib/store";
import { Icon } from "../ui/Icon";

/** Notifications slide in over the toolbar row (the page area belongs to the native webview). */
export function Toasts() {
  const toasts = useStore((s) => s.toasts);
  if (toasts.length === 0) return null;
  return (
    <div className="toasts" role="status" aria-live="polite">
      {toasts.map((t) => (
        <button key={t.id} type="button" className="toast" onClick={() => dismissToast(t.id)}>
          <Icon name="info" size={15} />
          <span>{t.message}</span>
        </button>
      ))}
    </div>
  );
}
