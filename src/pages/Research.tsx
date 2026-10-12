import { useEffect } from "react";

import { Badge, Button, Segmented, Spinner } from "../components/ui/controls";
import { Favicon } from "../components/ui/Favicon";
import { Icon } from "../components/ui/Icon";
import { Card, EmptyState, Notice, PageHeader } from "../components/ui/Section";
import { ai, loadAiConfig } from "../features/ai/aiStore";
import { draftFromResearch } from "../features/memory/memory";
import type { ResearchMode } from "../features/research/prompts";
import { unreadableReason } from "../features/research/readable";
import type { ClaimBattleResult, ComparisonResult, Statement } from "../features/research/provenance";
import { cancelResearch, research, resetResearch, runBlocker, runResearch, setMode, setQuestion, toggleTab } from "../features/research/researchStore";
import { estimateTokens, perSourceChars, PROMPT_OVERHEAD_CHARS } from "../features/research/sources";
import { api } from "../lib/ipc";
import { writeLocal } from "../lib/storage";
import { attempt, toast, useStore } from "../lib/store";
import { monogram, tabTitle } from "../lib/urls";

const LEANING: Record<ClaimBattleResult["leaning"], { tone: "good" | "warn" | "bad" | "neutral"; text: string }> = {
  supports: { tone: "good", text: "The checked evidence leans towards the claim" },
  contradicts: { tone: "bad", text: "The checked evidence leans against the claim" },
  mixed: { tone: "warn", text: "The checked evidence points both ways" },
  insufficient: { tone: "neutral", text: "There is no checked evidence either way" },
};

function openSource(url: string) {
  void attempt(() => api.tabNew(url, false, true));
}

function StatementView({ s }: { s: Statement }) {
  return (
    <li className={`statement${s.grounded ? "" : " is-ungrounded"}`}>
      <div>
        <span data-selectable>{s.text}</span>{" "}
        <Badge tone={s.basis === "inferred" ? "warn" : s.grounded ? "good" : "neutral"}>
          {s.basis === "inferred" ? "Model's inference" : s.grounded ? "Page says" : "Unverified"}
        </Badge>
      </div>
      {s.citations.map((c) => (
        <div key={c.sourceId} className="citation">
          <button type="button" className="chip" title={`Open ${c.url} in a new tab`} onClick={() => openSource(c.url)}>
            {c.sourceId}: {c.title}
          </button>
          {c.quote && (
            <span className={c.verified ? "quote" : "quote is-unverified"} data-selectable>
              {c.verified ? "✓ " : "✗ not found: "}“{c.quote}”
            </span>
          )}
        </div>
      ))}
      {s.problems.length > 0 && <small className="dim">{s.problems.join(" ")}</small>}
    </li>
  );
}

function Group({ title, items }: { title: string; items: Statement[] }) {
  if (items.length === 0) return null;
  return (
    <section className="research-group">
      <h4>{title}</h4>
      <ul className="statement-list">{items.map((s, i) => <StatementView key={i} s={s} />)}</ul>
    </section>
  );
}

function Lines({ title, items }: { title: string; items: string[] }) {
  if (items.length === 0) return null;
  return (
    <section className="research-group">
      <h4>{title}</h4>
      <ul className="plain-list">{items.map((m, i) => <li key={i} data-selectable>{m}</li>)}</ul>
    </section>
  );
}

function ComparisonView({ r }: { r: ComparisonResult }) {
  return (
    <>
      {r.summary && <p data-selectable>{r.summary}</p>}
      <Group title="Claims by source" items={r.claims} />
      <Group title="Where the pages agree" items={r.agreements} />
      {r.contradictions.length > 0 && (
        <section className="research-group">
          <h4>Where they contradict each other</h4>
          {r.contradictions.map((c, i) => (
            <div key={i} className="contradiction">
              <strong>{c.topic}</strong>
              <ul className="statement-list">{c.positions.map((p, j) => <StatementView key={j} s={p} />)}</ul>
            </div>
          ))}
        </section>
      )}
      {r.table && (
        <section className="research-group">
          <h4>Table</h4>
          <div className="table-scroll">
            <table className="data-table">
              <thead><tr>{r.table.columns.map((c, i) => <th key={i}>{c}</th>)}</tr></thead>
              <tbody>{r.table.rows.map((row, i) => <tr key={i}>{row.map((cell, j) => <td key={j} data-selectable>{cell}</td>)}</tr>)}</tbody>
            </table>
          </div>
        </section>
      )}
      <Lines title="Missing information" items={r.missing} />
      <Lines title="Open questions" items={r.openQuestions} />
    </>
  );
}

function ClaimView({ r }: { r: ClaimBattleResult }) {
  const lean = LEANING[r.leaning];
  return (
    <>
      <Notice tone={lean.tone === "good" ? "good" : lean.tone === "neutral" ? "info" : "warn"}>
        <strong>{lean.text}.</strong> This is a reading of the quotes that were found in your pages, not a verdict.
        <ul className="plain-list">{r.caveats.map((c, i) => <li key={i}>{c}</li>)}</ul>
      </Notice>
      {r.summary && <p data-selectable>{r.summary}</p>}
      <Group title="Supporting evidence" items={r.supporting} />
      <Group title="Contradicting evidence" items={r.contradicting} />
      <Group title="Uncertain or unverified" items={r.uncertain} />
      <Lines title="Missing information" items={r.missing} />
      {r.reliability.length > 0 && (
        <section className="research-group">
          <h4>How much weight each source deserves</h4>
          <ul className="plain-list">
            {r.reliability.map((x) => (
              <li key={x.sourceId} data-selectable><strong>{x.sourceId}: {x.title}.</strong> {x.note}</li>
            ))}
          </ul>
          <small className="dim">These notes are the model's judgement from the page text, not a fact-check.</small>
        </section>
      )}
    </>
  );
}

const MODES: { value: ResearchMode; label: string }[] = [
  { value: "compare", label: "Compare pages" },
  { value: "claim", label: "Claim Battle" },
];

/** Research Constellation and Claim Battle: the user picks pages, the model's answer is checked against those pages. */
export default function Research() {
  const state = research.use((s) => s);
  const config = ai.use((s) => s.config);
  const tabs = useStore((s) => s.browser.tabs);
  const maxContext = useStore((s) => s.settings.aiMaxContextChars);

  useEffect(() => {
    void loadAiConfig();
  }, []);

  const blocker = runBlocker(state.mode, state.question, state.selected.length);
  const busy = state.phase === "reading" || state.phase === "asking";
  const ready = !!config?.hasKey && config.model.trim().length > 0;
  const upTo = estimateTokens(state.selected.length * perSourceChars(maxContext, state.selected.length) + PROMPT_OVERHEAD_CHARS);

  const save = () => {
    if (!state.result) return;
    void attempt(() => api.memorySave(draftFromResearch(state.result!, state.sources)), "Could not save").then((saved) => saved && toast("Saved to Project Memory"));
  };

  const openAiSettings = () => {
    writeLocal("settings.section", "ai");
    void attempt(() => api.runCommand({ type: "openSettings" }));
  };

  return (
    <div className="page">
      <PageHeader
        icon="search"
        title="Research"
        subtitle="Compare the pages you choose, or weigh them against a claim. Only the pages you tick are read, and every quote is checked against the page it came from."
      />

      {config && !ready && (
        <Notice tone="warn">
          Research uses your NVIDIA model. {config.hasKey ? "Choose a model" : "Add your API key"} in AI settings first.{" "}
          <Button size="sm" onClick={openAiSettings}>Open AI settings</Button>
        </Notice>
      )}

      <Card>
        <div className="research-mode">
          <Segmented label="What to do" value={state.mode} options={MODES} onChange={setMode} />
          <p className="dim small">
            {state.mode === "compare"
              ? "Lists what each page claims, where they agree and contradict each other, and what is missing."
              : "Sorts the evidence on a question into supporting, contradicting and uncertain. Agreement between pages is not treated as proof."}
          </p>
        </div>
        <div className="field">
          <label htmlFor="research-question">{state.mode === "claim" ? "Question or claim to check" : "Focus (optional)"}</label>
          <input id="research-question" className="input" value={state.question} maxLength={600} placeholder={state.mode === "claim" ? "e.g. Does this card really have 16 GB of memory?" : "e.g. memory size and price"} onChange={(e) => setQuestion(e.target.value)} />
        </div>

        <h4 className="research-pick">Pages to include</h4>
        {tabs.length === 0 ? (
          <p className="dim">No tabs are open.</p>
        ) : (
          <div className="list research-tabs">
            {tabs.map((t) => {
              const why = unreadableReason(t);
              const selected = state.selected.includes(t.id);
              return (
                <label key={t.id} className={`list-row${why ? " is-disabled" : ""}`}>
                  <input type="checkbox" checked={selected && !why} disabled={!!why || busy} onChange={() => toggleTab(t.id)} />
                  <Favicon src={t.favicon} fallback={monogram(t)} size={16} />
                  <span className="row-link">{tabTitle(t)}</span>
                  {why ? <span className="dim small">{why}</span> : selected && <span className="dim small">S{state.selected.indexOf(t.id) + 1}</span>}
                </label>
              );
            })}
          </div>
        )}
        <p className="dim small">
          {state.selected.length === 0
            ? "Nothing is read until you press Run."
            : `${state.selected.length} page${state.selected.length === 1 ? "" : "s"} will be read and sent to NVIDIA: up to about ${upTo.toLocaleString("en-US")} tokens (your limit is about ${estimateTokens(maxContext).toLocaleString("en-US")}).`}
        </p>

        <div className="row-actions">
          {busy ? (
            <Button icon="stop" onClick={cancelResearch}>Cancel</Button>
          ) : (
            <Button variant="primary" icon="sparkle" disabled={!ready || !!blocker} title={blocker ?? undefined} onClick={() => void runResearch(tabs, maxContext)}>
              Run
            </Button>
          )}
          {blocker && !busy && <span className="dim small">{blocker}</span>}
          {(state.result || state.error) && !busy && <Button variant="ghost" onClick={resetResearch}>Clear</Button>}
        </div>
      </Card>

      {busy && (
        <div className="ai-status" role="status">
          <Spinner size={16} /> <span>{state.progress}</span>
        </div>
      )}

      {state.skipped.length > 0 && (
        <Notice tone="warn">
          <strong>Left out:</strong>
          {state.skipped.map((s) => <div key={s.tabId}>{s.title}: {s.reason}</div>)}
        </Notice>
      )}

      {state.phase === "idle" && state.progress && <p className="dim">{state.progress}</p>}

      {state.error && (
        <Notice tone="bad">
          {state.error}
          {state.raw && (
            <details>
              <summary>Show what the model said (unchecked, not cited)</summary>
              <pre className="update-notes" data-selectable>{state.raw}</pre>
            </details>
          )}
        </Notice>
      )}

      {state.result && (
        <Card
          title={state.result.mode === "claim" ? state.result.question : "Comparison"}
          subtitle={`${state.sources.length} pages · about ${state.estimatedTokens?.toLocaleString("en-US") ?? "?"} tokens sent`}
          actions={<Button icon="bookmarks" onClick={save}>Save to memory</Button>}
        >
          {state.result.dropped > 0 && (
            <Notice tone="info">
              {state.result.dropped} statement{state.result.dropped === 1 ? " was" : "s were"} removed because {state.result.dropped === 1 ? "it" : "they"} didn't cite any of your pages.
            </Notice>
          )}
          {state.result.mode === "compare" ? <ComparisonView r={state.result} /> : <ClaimView r={state.result} />}
          <div className="research-sources">
            <h4>Pages used</h4>
            <ul className="plain-list">
              {state.sources.map((s) => (
                <li key={s.id}>
                  <button type="button" className="row-link" onClick={() => openSource(s.url)}>{s.id}: {s.title}</button>{" "}
                  <span className="dim small">{s.truncated ? `first ${s.text.length.toLocaleString("en-US")} of ${s.totalChars.toLocaleString("en-US")} characters` : "whole page"}</span>
                </li>
              ))}
            </ul>
          </div>
        </Card>
      )}

      {state.phase === "idle" && !state.result && !state.error && state.selected.length === 0 && (
        <EmptyState icon="layers" title="Pick two or more open pages">
          Open the pages you want to compare in tabs, come back here and tick them. Pages that are asleep, private or browser pages can't be read.
        </EmptyState>
      )}
      <p className="dim small"><Icon name="info" size={13} /> The model can be wrong. Quotes marked ✓ were found word for word in the page text that was read; everything else is the model's own reading.</p>
    </div>
  );
}
