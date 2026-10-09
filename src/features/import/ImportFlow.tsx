import { useEffect, useState } from "react";

import { Badge, Button, Spinner } from "../../components/ui/controls";
import { Icon } from "../../components/ui/Icon";
import { EmptyState, Notice } from "../../components/ui/Section";
import { api, errorMessage } from "../../lib/ipc";
import { importPlan, profileKey, summarizeReport } from "./importPlan";
import type { DetectedProfile, ImportReport } from "../../lib/types";

type Phase = { name: "detecting" } | { name: "choose"; profiles: DetectedProfile[] } | { name: "running" } | { name: "done"; report: ImportReport } | { name: "failed"; message: string };

/**
 * The migration wizard body. It lists exactly what each data type's status is:
 * bookmarks, history and the default search engine are imported; everything else
 * (passwords, autofill, cookies) is shown as not imported with the reason, never
 * silently skipped.
 */
export function ImportFlow({ onClose, onDone, cancelLabel = "Cancel" }: { onClose: () => void; onDone?: (report: ImportReport) => void; cancelLabel?: string }) {
  const [phase, setPhase] = useState<Phase>({ name: "detecting" });
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [bookmarks, setBookmarks] = useState(true);
  const [history, setHistory] = useState(true);
  const [searchEngine, setSearchEngine] = useState(true);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let live = true;
    api
      .importDetect()
      .then((profiles) => {
        if (!live) return;
        setPhase({ name: "choose", profiles });
        const first = profiles[0];
        if (first) setSelectedKey(profileKey(first));
      })
      .catch((e) => live && setPhase({ name: "failed", message: errorMessage(e) }));
    return () => {
      live = false;
    };
  }, [attempt]);

  if (phase.name === "detecting") {
    return (
      <div className="import-center">
        <Spinner size={22} />
        <p className="dim">Looking for Chrome, Edge and Brave on this computer…</p>
      </div>
    );
  }

  if (phase.name === "failed") {
    return (
      <>
        <Notice tone="bad">{phase.message}</Notice>
        <div className="wizard-actions">
          <Button variant="ghost" onClick={onClose}>Close</Button>
          <Button
            variant="primary"
            onClick={() => {
              setPhase({ name: "detecting" });
              setAttempt((n) => n + 1);
            }}
          >
            Try again
          </Button>
        </div>
      </>
    );
  }

  if (phase.name === "running") {
    return (
      <div className="import-center">
        <Spinner size={22} />
        <p>Importing…</p>
        <p className="dim small">Large histories can take a few seconds. Importing is safe to repeat: nothing is added twice.</p>
      </div>
    );
  }

  if (phase.name === "done") {
    const s = summarizeReport(phase.report);
    return (
      <>
        <div className={`import-result tone-${s.tone}`}>
          <Icon name={s.tone === "bad" ? "warning" : "check"} size={26} />
          <div>
            <h3>{s.title}</h3>
            <ul className="plain-list">{s.lines.map((l) => <li key={l}>{l}</li>)}</ul>
          </div>
        </div>
        {phase.report.errors.length > 0 && <Notice tone="warn">{phase.report.errors.map((e) => <div key={e}>{e}</div>)}</Notice>}
        <div className="wizard-actions"><Button variant="primary" onClick={onClose}>Done</Button></div>
      </>
    );
  }

  const profiles = phase.profiles;
  const chosen = profiles.find((p) => profileKey(p) === selectedKey) ?? null;
  const plan = chosen ? importPlan(chosen) : null;

  if (profiles.length === 0) {
    return (
      <>
        <EmptyState icon="import" title="No other browser found">
          We looked for Google Chrome, Microsoft Edge and Brave profiles in your Windows user folder and didn't find any. Portable or
          custom-location installs can't be detected in this version.
        </EmptyState>
        <div className="wizard-actions"><Button onClick={onClose}>Close</Button></div>
      </>
    );
  }

  const run = async () => {
    if (!chosen) return;
    setPhase({ name: "running" });
    try {
      const report = await api.importRun(
        chosen.browser,
        chosen.profileDir,
        bookmarks && !!plan?.bookmarks.available,
        history && !!plan?.history.available,
        searchEngine && !!plan?.searchEngine.available,
      );
      onDone?.(report);
      setPhase({ name: "done", report });
    } catch (e) {
      setPhase({ name: "failed", message: errorMessage(e) });
    }
  };

  return (
    <>
      <p className="dim">Choose where to import from. Your other browser's files are only read, never changed, and nothing leaves this computer.</p>
      <div className="import-profiles" role="radiogroup" aria-label="Browser profile to import from">
        {profiles.map((p) => {
          const key = profileKey(p);
          return (
            <button key={key} type="button" role="radio" aria-checked={key === selectedKey} className={`import-profile${key === selectedKey ? " is-on" : ""}`} onClick={() => setSelectedKey(key)}>
              <span className={`browser-dot b-${p.browser}`}>{p.browserLabel.replace("Google ", "").replace("Microsoft ", "").charAt(0)}</span>
              <span className="ip-text">
                <strong>{p.browserLabel}</strong>
                <span className="dim">{p.name}</span>
              </span>
              {key === selectedKey && <Icon name="check" size={18} />}
            </button>
          );
        })}
      </div>

      {chosen && plan && (
        <div className="import-items">
          <h4>What to import</h4>
          <label className={`check-row${plan.bookmarks.available ? "" : " is-disabled"}`}>
            <input type="checkbox" checked={bookmarks && plan.bookmarks.available} disabled={!plan.bookmarks.available} onChange={(e) => setBookmarks(e.target.checked)} />
            <span><strong>Bookmarks</strong><small>{plan.bookmarks.detail}</small></span>
          </label>
          <label className={`check-row${plan.history.available ? "" : " is-disabled"}`}>
            <input type="checkbox" checked={history && plan.history.available} disabled={!plan.history.available} onChange={(e) => setHistory(e.target.checked)} />
            <span><strong>Browsing history</strong><small>{plan.history.detail}</small></span>
          </label>
          <label className={`check-row${plan.searchEngine.available ? "" : " is-disabled"}`}>
            <input type="checkbox" checked={searchEngine && plan.searchEngine.available} disabled={!plan.searchEngine.available} onChange={(e) => setSearchEngine(e.target.checked)} />
            <span><strong>Default search engine</strong><small>{plan.searchEngine.detail}</small></span>
          </label>

          <h4>Not imported</h4>
          <ul className="unsupported-list">
            {plan.unsupported.map((u) => (
              <li key={u.kind}>
                <Icon name="lock" size={15} />
                <span>
                  <strong>{u.label}</strong> {u.present && <Badge tone="warn">found, but can't be imported</Badge>}
                  <small>{u.detail}</small>
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}

      <div className="wizard-actions">
        <Button variant="ghost" onClick={onClose}>{cancelLabel}</Button>
        <Button variant="primary" icon="import" disabled={!chosen || !((bookmarks && plan?.bookmarks.available) || (history && plan?.history.available) || (searchEngine && plan?.searchEngine.available))} onClick={() => void run()}>
          Import
        </Button>
      </div>
    </>
  );
}
