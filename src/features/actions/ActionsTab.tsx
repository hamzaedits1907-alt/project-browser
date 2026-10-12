import { useState } from "react";

import { Badge, Button, Spinner } from "../../components/ui/controls";
import { Notice } from "../../components/ui/Section";
import { useStore, activeTab } from "../../lib/store";
import { EFFECT_LABEL, describeStep, effectOf, stepBlocker } from "./actions";
import type { Effect, StepOutcome } from "./actions";
import { actions, cancelActions, clearActionHistory, confirmPlan, contextOf, discardPlan, planRequest, toggleStep } from "./actionsStore";

const EFFECT_TONE: Record<Effect, "neutral" | "accent" | "warn" | "good"> = {
  "loads-web": "warn",
  "sends-page": "warn",
  "changes-view": "accent",
  "saves-memory": "neutral",
  none: "neutral",
};

const STATUS_TEXT: Record<StepOutcome["status"], string> = { done: "Done", failed: "Failed", skipped: "Skipped", cancelled: "Stopped" };

const EXAMPLES = [
  "Open the Rust book and the Tauri docs in new tabs",
  "Summarize this page and save it to my memory",
  "Find all route names on this page",
  "Make this page dark with bigger text",
];

/**
 * Natural-language browser actions. The assistant proposes a plan; nothing runs until the user has read it and pressed
 * Run, and only from a fixed list of things the browser can do (see actions.ts). Results are shown as plain text.
 */
export function ActionsTab({ maxContext, ready }: { maxContext: number; ready: boolean }) {
  const s = actions.use((x) => x);
  const tab = useStore(activeTab);
  const [draft, setDraft] = useState("");
  const context = contextOf(tab);
  const text = draft;
  const busy = s.phase === "planning" || s.phase === "running";

  const plan = () => {
    const request = text.trim();
    if (request) void planRequest(request);
  };

  return (
    <div className="actions-tab">
      {!ready && <Notice tone="warn">Add your NVIDIA key and choose a model in AI settings to use actions.</Notice>}
      <p className="dim small">
        Say what you want done. You'll see the plan first and nothing happens until you press Run. The assistant can open pages, search your bookmarks and memory, read or restyle the page you're on, and save to Project Memory. It can't run commands, touch files, fill or submit forms, or download anything.
      </p>

      {(s.phase === "idle" || s.phase === "error" || s.phase === "done" || s.phase === "planning") && (
        <>
          <textarea
            className="input textarea actions-input"
            rows={3}
            value={draft}
            placeholder="e.g. Open five pages about GPU benchmarks"
            aria-label="What should the assistant do?"
            disabled={busy || !ready}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                plan();
              }
            }}
          />
          <div className="row-actions">
            {s.phase === "planning" ? (
              <>
                <Button icon="stop" onClick={cancelActions}>Cancel</Button>
                <span className="ai-status"><Spinner size={14} /> Planning…</span>
              </>
            ) : (
              <Button variant="primary" icon="sparkle" disabled={!ready || text.trim().length < 3} onClick={plan}>Make a plan</Button>
            )}
          </div>
          {s.phase !== "planning" && !text && (
            <div className="ai-suggestions">
              {EXAMPLES.map((e) => (
                <button key={e} type="button" className="chip" disabled={!ready} onClick={() => setDraft(e)}>{e}</button>
              ))}
            </div>
          )}
        </>
      )}

      {s.error && s.phase === "error" && <Notice tone="bad">{s.error}</Notice>}

      {s.plan && (s.phase === "review" || s.phase === "running" || s.phase === "done") && (
        <div className="plan">
          <h4>{s.phase === "review" ? "Review the plan" : s.phase === "running" ? "Running…" : "Finished"}</h4>
          {s.plan.summary && <p data-selectable>{s.plan.summary}</p>}
          {s.plan.steps.length === 0 && <Notice tone="info">The assistant couldn't plan anything with the actions available. Try rephrasing.</Notice>}
          <ol className="plan-steps">
            {s.plan.steps.map((step, i) => {
              const blocked = stepBlocker(step, context);
              const outcome = s.outcomes[i];
              const effect = effectOf(step.tool);
              return (
                <li key={i} className={`plan-step${s.enabled[i] ? "" : " is-off"}`}>
                  <label className="plan-line">
                    <input type="checkbox" checked={!!s.enabled[i]} disabled={s.phase !== "review" || !!blocked} onChange={() => toggleStep(i)} />
                    <span data-selectable>{describeStep(step)}</span>
                  </label>
                  <div className="plan-meta">
                    <Badge tone={EFFECT_TONE[effect]}>{EFFECT_LABEL[effect]}</Badge>
                    {step.why && <span className="dim small">{step.why}</span>}
                  </div>
                  {step.tool === "open_pages" && (
                    <ul className="plan-urls">{(step.args.urls as string[]).map((u) => <li key={u} className="mono small" data-selectable>{u}</li>)}</ul>
                  )}
                  {blocked && s.phase === "review" && <small className="dim">Can't run: {blocked}.</small>}
                  {outcome && (
                    <div className={`plan-outcome is-${outcome.status}`}>
                      <strong>{STATUS_TEXT[outcome.status]}</strong>
                      {(outcome.error || outcome.text) && <pre data-selectable>{outcome.error ?? outcome.text}</pre>}
                    </div>
                  )}
                </li>
              );
            })}
          </ol>
          {s.phase === "review" && (
            <div className="row-actions">
              <Button variant="primary" icon="play" disabled={!s.enabled.some(Boolean)} onClick={() => void confirmPlan(maxContext)}>
                Run {s.enabled.filter(Boolean).length} step{s.enabled.filter(Boolean).length === 1 ? "" : "s"}
              </Button>
              <Button variant="ghost" onClick={discardPlan}>Discard</Button>
            </div>
          )}
          {s.phase === "running" && <div className="row-actions"><Button icon="stop" onClick={cancelActions}>Stop</Button></div>}
          {s.phase === "done" && <div className="row-actions"><Button onClick={() => { setDraft(""); discardPlan(); }}>New request</Button></div>}
        </div>
      )}

      {s.history.length > 0 && (
        <details className="action-history">
          <summary>History ({s.history.length})</summary>
          <ul className="plain-list">
            {s.history.slice(0, 20).map((h) => (
              <li key={h.id}>
                <span data-selectable>{h.request}</span>
                <small className="dim"> · {new Date(h.at).toLocaleString([], { dateStyle: "short", timeStyle: "short" })}</small>
                <ul className="plain-list">
                  {h.steps.map((st, i) => (
                    <li key={i}><small>{STATUS_TEXT[st.status]}: {st.label}{st.error ? ` (${st.error})` : ""}</small></li>
                  ))}
                </ul>
              </li>
            ))}
          </ul>
          <Button size="sm" variant="ghost" onClick={clearActionHistory}>Clear history</Button>
        </details>
      )}
    </div>
  );
}
