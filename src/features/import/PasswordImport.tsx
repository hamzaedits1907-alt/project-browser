import { useState } from "react";

import { Button, Spinner } from "../../components/ui/controls";
import { Icon } from "../../components/ui/Icon";
import { Notice } from "../../components/ui/Section";
import { api, errorMessage } from "../../lib/ipc";
import type { ExportPreview, PasswordImportReport } from "../../lib/types";
import { summarizePasswordReport } from "./importPlan";

type Phase =
  | { name: "intro" }
  | { name: "picking" }
  | { name: "preview"; file: ExportPreview }
  | { name: "importing" }
  | { name: "done"; report: PasswordImportReport }
  | { name: "failed"; message: string };

/** Where each browser keeps its "export passwords" command. */
function exportSteps(browserLabel: string): string[] {
  const where =
    browserLabel === "Microsoft Edge"
      ? "Open the menu, then Settings → Passwords and autofill → Microsoft Edge Password Manager → ⋯ → Export passwords."
      : browserLabel === "Brave"
        ? "Open the menu, then Settings → Autofill and passwords → Password Manager → ⋯ → Export passwords."
        : "Open the menu, then Settings → Autofill and passwords → Google Password Manager → Settings (gear) → Export passwords.";
  return [
    `In ${browserLabel}: ${where} Windows will ask for your PIN or password.`,
    "Save the file where you can find it, for example in Downloads. It is a CSV file.",
    "Choose that file here.",
  ];
}

/**
 * Brings saved passwords over from a file the user exported from their old browser. The old browser's own password
 * store cannot be read by anyone but that browser (it is encrypted for it), so the user hands over a deliberate
 * export instead. This component only ever sees counts: the file is read by the core, which stores every password
 * encrypted for this Windows account and can overwrite and delete the file afterwards.
 */
export function PasswordImport({ browserLabel = "your old browser", onFinished, onClose }: { browserLabel?: string; onFinished?: (report: PasswordImportReport) => void; onClose: () => void }) {
  const [phase, setPhase] = useState<Phase>({ name: "intro" });
  const [deleteFile, setDeleteFile] = useState(true);

  const pick = async () => {
    setPhase({ name: "picking" });
    try {
      const file = await api.passwordsPickFile();
      setPhase(file ? { name: "preview", file } : { name: "intro" });
    } catch (e) {
      setPhase({ name: "failed", message: errorMessage(e) });
    }
  };

  const run = async () => {
    setPhase({ name: "importing" });
    try {
      const report = await api.passwordsImport(deleteFile);
      onFinished?.(report);
      setPhase({ name: "done", report });
    } catch (e) {
      setPhase({ name: "failed", message: errorMessage(e) });
    }
  };

  if (phase.name === "picking" || phase.name === "importing") {
    return (
      <div className="import-center">
        <Spinner size={22} />
        <p>{phase.name === "picking" ? "Waiting for you to choose the file…" : "Importing passwords…"}</p>
      </div>
    );
  }

  if (phase.name === "failed") {
    return (
      <>
        <Notice tone="bad">{phase.message}</Notice>
        <div className="wizard-actions">
          <Button variant="ghost" onClick={onClose}>Close</Button>
          <Button variant="primary" onClick={() => setPhase({ name: "intro" })}>Try again</Button>
        </div>
      </>
    );
  }

  if (phase.name === "done") {
    const s = summarizePasswordReport(phase.report);
    return (
      <>
        <div className={`import-result tone-${s.tone}`}>
          <Icon name={s.tone === "bad" ? "warning" : "check"} size={26} />
          <div>
            <h3>{s.title}</h3>
            {s.rows.length > 0 && (
              <ul className="result-rows">
                {s.rows.map((r) => (
                  <li key={r.key} className={`tone-${r.tone}`}>
                    <strong>{r.label}</strong>
                    <span>{r.text}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>
        <Notice tone={s.fileNote.tone === "good" ? "good" : "warn"}>{s.fileNote.text}</Notice>
        <p className="dim small">
          Your passwords are stored encrypted for this Windows account. When you visit one of these sites, a key button appears in the address bar; it fills the
          login only when you click it.
        </p>
        <div className="wizard-actions"><Button variant="primary" onClick={onClose}>Done</Button></div>
      </>
    );
  }

  if (phase.name === "preview") {
    const f = phase.file;
    return (
      <>
        <div className="file-card">
          <Icon name="file" size={22} />
          <div>
            <strong data-selectable>{f.name}</strong>
            <div className="dim small">
              {f.logins.toLocaleString()} login{f.logins === 1 ? "" : "s"} for {f.sites.toLocaleString()} site{f.sites === 1 ? "" : "s"}
              {f.skipped > 0 && ` · ${f.skipped.toLocaleString()} entr${f.skipped === 1 ? "y" : "ies"} can't be imported`}
            </div>
          </div>
        </div>
        {f.logins === 0 && <Notice tone="warn">There are no logins in this file that can be imported.</Notice>}
        <label className="check-row">
          <input type="checkbox" checked={deleteFile} onChange={(e) => setDeleteFile(e.target.checked)} />
          <span>
            <strong>Overwrite and delete the file afterwards</strong>
            <small>Recommended. The file holds your passwords as plain text. This replaces its contents with zeros, then deletes it (it does not go to the Recycle Bin).</small>
          </span>
        </label>
        <div className="wizard-actions">
          <Button variant="ghost" onClick={() => void pick()}>Choose a different file</Button>
          <Button variant="primary" icon="lock" disabled={f.logins === 0} onClick={() => void run()}>
            Import {f.logins.toLocaleString()} login{f.logins === 1 ? "" : "s"}
          </Button>
        </div>
      </>
    );
  }

  return (
    <>
      <p className="dim">
        {browserLabel} locks your saved passwords so that only it can open them, and this browser does not try to get around that. Instead you export them from {browserLabel} yourself,
        then choose the file here.
      </p>
      <ol className="plain-list steps-list">
        {exportSteps(browserLabel).map((step) => <li key={step}>{step}</li>)}
      </ol>
      <Notice tone="warn">
        <strong>Know the risk.</strong> The exported file contains every password in plain text. Any program on this PC, and any cloud folder that syncs it (such as OneDrive), can read it
        while it exists. This browser reads it once, stores each password encrypted for your Windows account, and can overwrite and delete the file right away. Nothing is uploaded anywhere.
      </Notice>
      <div className="wizard-actions">
        <Button variant="ghost" onClick={onClose}>Not now</Button>
        <Button variant="primary" icon="file" onClick={() => void pick()}>Choose the exported file…</Button>
      </div>
    </>
  );
}
