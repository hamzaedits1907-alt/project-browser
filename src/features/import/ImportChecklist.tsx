import { Badge, Button } from "../../components/ui/controls";
import type { ChecklistRow, ChecklistStatus } from "./importPlan";

const STATUS: Record<ChecklistStatus, { label: string; tone: "good" | "neutral" | "warn" | "bad" | "accent" }> = {
  imported: { label: "Imported", tone: "good" },
  already: { label: "Already imported", tone: "neutral" },
  unsupported: { label: "Not supported", tone: "neutral" },
  needsBrowserClosed: { label: "Close your old browser", tone: "warn" },
  needsAction: { label: "Needs you", tone: "accent" },
  failed: { label: "Failed", tone: "bad" },
  notFound: { label: "Nothing found", tone: "neutral" },
};

/**
 * What is done, what is not possible, and what is still up to the user, one row per kind of data. A row can offer to
 * repeat just that item, so one problem never means running the whole import again.
 */
export function ImportChecklist({
  rows,
  busy,
  onRetry,
  onAction,
}: {
  rows: ChecklistRow[];
  busy: boolean;
  onRetry: (key: NonNullable<ChecklistRow["retry"]>) => void;
  onAction: (action: NonNullable<ChecklistRow["action"]>) => void;
}) {
  return (
    <ul className="import-checklist" aria-label="Import checklist">
      {rows.map((r) => (
        <li key={r.key} className={`check-item status-${r.status}`}>
          <div className="check-item-head">
            <strong>{r.label}</strong>
            <Badge tone={STATUS[r.status].tone}>{STATUS[r.status].label}</Badge>
          </div>
          <p className="dim small">{r.text}</p>
          {(r.retry || r.action) && (
            <div className="row-actions">
              {r.retry && (
                <Button size="sm" disabled={busy} onClick={() => onRetry(r.retry!)}>
                  {r.status === "needsAction" ? "Import this now" : "Try this item again"}
                </Button>
              )}
              {r.action === "passwords" && <Button size="sm" icon="lock" onClick={() => onAction("passwords")}>Import passwords…</Button>}
              {r.action === "google" && <Button size="sm" icon="globe" onClick={() => onAction("google")}>Sign in to Google</Button>}
            </div>
          )}
        </li>
      ))}
    </ul>
  );
}
