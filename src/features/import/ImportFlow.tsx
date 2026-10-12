import { useEffect, useState } from "react";

import { Badge, Button, Spinner } from "../../components/ui/controls";
import { Icon } from "../../components/ui/Icon";
import { EmptyState, Notice } from "../../components/ui/Section";
import { api, errorMessage } from "../../lib/ipc";
import { toast } from "../../lib/store";
import { buildChecklist, chosenKinds, defaultChoices, effectiveChoices, GOOGLE_SIGN_IN_URL, hasAnyChoice, importPlan, onlyChoice, profileKey, recordReport, summarizeReport } from "./importPlan";
import type { ImportPlan, KindReports } from "./importPlan";
import { ImportChecklist } from "./ImportChecklist";
import { PasswordImport } from "./PasswordImport";
import type { DetectedProfile, ImportChoices, ImportReport, PasswordImportReport } from "../../lib/types";

type Phase =
  | { name: "detecting" }
  | { name: "choose"; profiles: DetectedProfile[] }
  | { name: "running" }
  | { name: "done"; report: ImportReport; profile: DetectedProfile; plan: ImportPlan }
  | { name: "passwords"; profile: DetectedProfile; after: Phase }
  | { name: "failed"; message: string };

const OPEN_TABS_LIMIT = 20;

const CHOICE_ROWS: { key: keyof ImportChoices; title: string; plan: (p: ImportPlan) => { available: boolean; detail: string }; note?: string }[] = [
  { key: "bookmarks", title: "Bookmarks", plan: (p) => p.bookmarks },
  { key: "history", title: "Browsing history", plan: (p) => p.history },
  { key: "searchEngine", title: "Default search engine", plan: (p) => p.searchEngine },
  { key: "addresses", title: "Saved addresses", plan: (p) => p.addresses },
  { key: "session", title: "Open tabs", plan: (p) => p.session },
];

/**
 * The migration wizard body. Every kind of data gets a visible status: what will be imported, what is done outside
 * the wizard (passwords: the user exports a file), and what can never be imported (with the reason). The result
 * screen reports each kind separately, including what was already there. The other browser's files are only read.
 */
export function ImportFlow({ onClose, onDone, cancelLabel = "Cancel" }: { onClose: () => void; onDone?: (report: ImportReport) => void; cancelLabel?: string }) {
  const [phase, setPhase] = useState<Phase>({ name: "detecting" });
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [choices, setChoices] = useState<ImportChoices>({ bookmarks: true, history: true, searchEngine: true, addresses: true, session: false });
  const [attempt, setAttempt] = useState(0);
  // The latest report for each kind of data, so a retry of one item replaces only that item's result.
  const [kindReports, setKindReports] = useState<KindReports>({});
  const [passwordReport, setPasswordReport] = useState<PasswordImportReport | null>(null);
  const [sentChoices, setSentChoices] = useState<ImportChoices>({ bookmarks: false, history: false, searchEngine: false, addresses: false, session: false });
  const [retrying, setRetrying] = useState(false);

  useEffect(() => {
    let live = true;
    api
      .importDetect()
      .then((profiles) => {
        if (!live) return;
        setPhase({ name: "choose", profiles });
        const first = profiles[0];
        if (first) {
          setSelectedKey(profileKey(first));
          setChoices(defaultChoices(importPlan(first)));
        }
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

  if (phase.name === "passwords") {
    const { profile, after } = phase;
    return <PasswordImport browserLabel={profile.browserLabel} onFinished={setPasswordReport} onClose={() => setPhase(after)} />;
  }

  if (phase.name === "done") {
    const { report, profile, plan } = phase;
    const s = summarizeReport(report);
    const sessionUrls = kindReports.session?.sessionUrls ?? report.sessionUrls;
    const urls = sessionUrls.slice(0, OPEN_TABS_LIMIT);
    const checklist = buildChecklist({ plan, chosen: sentChoices, reports: kindReports, passwords: passwordReport, browserLabel: profile.browserLabel });
    const retry = (key: keyof ImportChoices) => {
      void (async () => {
        setRetrying(true);
        try {
          const next = await api.importRun(profile.browser, profile.profileDir, onlyChoice(key));
          setKindReports((cur) => recordReport(cur, [key], next));
          setSentChoices((cur) => ({ ...cur, [key]: true }));
          onDone?.(next);
        } catch (e) {
          toast(errorMessage(e));
        } finally {
          setRetrying(false);
        }
      })();
    };
    const act = (action: "passwords" | "google") => {
      if (action === "passwords") setPhase({ name: "passwords", profile, after: phase });
      else {
        void api.tabNew(GOOGLE_SIGN_IN_URL).catch((e) => toast(errorMessage(e)));
        onClose();
      }
    };
    const openTabs = () => {
      void (async () => {
        for (const url of urls) await api.tabNew(url).catch(() => undefined);
        toast(`Opened ${urls.length} tab${urls.length === 1 ? "" : "s"}`);
        onClose();
      })();
    };
    return (
      <>
        <div className={`import-result tone-${s.tone}`}>
          <Icon name={s.tone === "bad" ? "warning" : "check"} size={26} />
          <div>
            <h3>{s.title}</h3>
            {s.rows.length === 0 && <ul className="plain-list">{s.lines.map((l) => <li key={l}>{l}</li>)}</ul>}
          </div>
        </div>
        <ImportChecklist rows={checklist} busy={retrying} onRetry={retry} onAction={act} />
        {report.errors.length > 0 && (
          <Notice tone="warn">
            <strong>Not everything could be imported:</strong>
            {report.errors.map((e) => <div key={e}>{e}</div>)}
          </Notice>
        )}
        {urls.length > 0 && (
          <div className="next-step">
            <div>
              <strong>{sessionUrls.length > OPEN_TABS_LIMIT ? `Open the first ${OPEN_TABS_LIMIT} of ${sessionUrls.length} tabs` : `Open the ${urls.length} tab${urls.length === 1 ? "" : "s"} you had open`}</strong>
              <small>They are also saved under Other bookmarks, in “Open tabs”.</small>
            </div>
            <Button icon="tabs" onClick={openTabs}>Open now</Button>
          </div>
        )}
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
        <div className="wizard-actions"><Button onClick={onClose}>{cancelLabel === "Cancel" ? "Close" : "Continue"}</Button></div>
      </>
    );
  }

  const choose = (p: DetectedProfile) => {
    setSelectedKey(profileKey(p));
    setChoices(defaultChoices(importPlan(p)));
  };

  const run = async () => {
    if (!chosen || !plan) return;
    const sent = effectiveChoices(plan, choices);
    const here = phase;
    setPhase({ name: "running" });
    try {
      const report = await api.importRun(chosen.browser, chosen.profileDir, sent);
      setKindReports(recordReport({}, chosenKinds(sent), report));
      setSentChoices(sent);
      setPasswordReport(null);
      onDone?.(report);
      setPhase({ name: "done", report, profile: chosen, plan });
    } catch (e) {
      setPhase(here);
      toast(errorMessage(e));
    }
  };

  return (
    <>
      <p className="dim">Choose where to import from. Your other browser's files are only read, never changed, and nothing leaves this computer.</p>
      <div className="import-profiles" role="radiogroup" aria-label="Browser profile to import from">
        {profiles.map((p) => {
          const key = profileKey(p);
          return (
            <button key={key} type="button" role="radio" aria-checked={key === selectedKey} className={`import-profile${key === selectedKey ? " is-on" : ""}`} onClick={() => choose(p)}>
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
          <h4>What to bring over</h4>
          {CHOICE_ROWS.map((row) => {
            const p = row.plan(plan);
            return (
              <label key={row.key} className={`check-row${p.available ? "" : " is-disabled"}`}>
                <input type="checkbox" checked={choices[row.key] && p.available} disabled={!p.available} onChange={(e) => setChoices({ ...choices, [row.key]: e.target.checked })} />
                <span><strong>{row.title}</strong><small>{p.detail}</small></span>
              </label>
            );
          })}

          <h4>Passwords</h4>
          <div className={`next-step${plan.passwords.available ? "" : " is-off"}`}>
            <div>
              <strong>Saved passwords {plan.passwords.available && <Badge tone="accent">Needs your export</Badge>}</strong>
              <small>{plan.passwords.available ? plan.passwords.detail : "No saved passwords were found in this profile."}</small>
            </div>
            <Button icon="lock" disabled={!plan.passwords.available} onClick={() => setPhase({ name: "passwords", profile: chosen, after: phase })}>
              Import passwords…
            </Button>
          </div>

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
        <Button variant="primary" icon="import" disabled={!chosen || !plan || !hasAnyChoice(plan, choices)} onClick={() => void run()}>
          Import
        </Button>
      </div>
    </>
  );
}
