import { useCallback, useEffect, useRef, useState } from "react";

import { Button } from "../components/ui/controls";
import { Favicon } from "../components/ui/Favicon";
import { Icon } from "../components/ui/Icon";
import { EmptyState, Notice, PageHeader } from "../components/ui/Section";
import { formatTime, groupByDay, hostOf } from "../lib/format";
import { api, errorMessage } from "../lib/ipc";
import { attempt } from "../lib/store";
import { openClearData } from "../lib/ui";
import type { HistoryEntry } from "../lib/types";

const PAGE = 100;

export default function History() {
  const [text, setText] = useState("");
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [done, setDone] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const seq = useRef(0);

  const load = useCallback(async (query: string, before?: number) => {
    const mine = ++seq.current;
    setLoading(true);
    try {
      const rows = await api.historyQuery(query, before, PAGE);
      if (mine !== seq.current) return;
      setEntries((prev) => (before ? [...prev, ...rows] : rows));
      setDone(rows.length < PAGE);
      setError(null);
    } catch (e) {
      if (mine === seq.current) setError(errorMessage(e));
    } finally {
      if (mine === seq.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    const t = setTimeout(() => {
      setSelected(new Set());
      void load(text);
    }, text ? 200 : 0);
    return () => clearTimeout(t);
  }, [text, load]);

  const toggle = (id: number) =>
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  const removeSelected = async () => {
    const ids = [...selected];
    const ok = await attempt(() => api.historyDelete(ids), "Could not delete");
    if (ok !== undefined) {
      setEntries((e) => e.filter((x) => !selected.has(x.visitId)));
      setSelected(new Set());
    }
  };

  const groups = groupByDay(entries, (e) => e.visitedAt);
  const last = entries[entries.length - 1];

  return (
    <div className="page">
      <PageHeader
        icon="history"
        title="History"
        subtitle="Pages you've visited. Private tabs are never recorded."
        actions={<Button icon="broom" onClick={openClearData}>Clear browsing data…</Button>}
      />
      <div className="page-toolbar">
        <label className="search-field">
          <Icon name="search" size={16} />
          <input value={text} onChange={(e) => setText(e.target.value)} placeholder="Search history" aria-label="Search history" spellCheck={false} />
        </label>
        {selected.size > 0 && (
          <div className="selection-bar">
            <span>{selected.size} selected</span>
            <Button size="sm" variant="danger" icon="trash" onClick={() => void removeSelected()}>Delete</Button>
            <Button size="sm" variant="ghost" onClick={() => setSelected(new Set())}>Cancel</Button>
          </div>
        )}
      </div>

      {error && <Notice tone="bad">{error}</Notice>}
      {!loading && entries.length === 0 && !error && (
        <EmptyState icon="history" title={text ? "No matching pages" : "No history yet"}>
          {text ? "Try a different search." : "Pages you visit will be listed here."}
        </EmptyState>
      )}

      {groups.map((g) => (
        <section key={g.label} className="day-group">
          <h2 className="day-label">{g.label}</h2>
          <div className="list">
            {g.items.map((e) => (
              <div key={e.visitId} className={`list-row${selected.has(e.visitId) ? " is-selected" : ""}`}>
                <input type="checkbox" aria-label={`Select ${e.title || e.url}`} checked={selected.has(e.visitId)} onChange={() => toggle(e.visitId)} />
                <span className="row-time">{formatTime(e.visitedAt)}</span>
                <Favicon src={null} fallback={(hostOf(e.url).charAt(0) || "•").toUpperCase()} size={16} />
                <button type="button" className="row-link truncate" onClick={() => void attempt(() => api.navigate(e.url))} title={e.url}>
                  {e.title || e.url}
                </button>
                <span className="row-host truncate">{hostOf(e.url)}</span>
                <button type="button" className="icon-btn row-del" aria-label="Remove from history" title="Remove from history" onClick={() => void attempt(() => api.historyDelete([e.visitId])).then(() => setEntries((x) => x.filter((y) => y.visitId !== e.visitId)))}>
                  <Icon name="close" size={14} />
                </button>
              </div>
            ))}
          </div>
        </section>
      ))}

      {!done && last && (
        <div className="load-more">
          <Button disabled={loading} onClick={() => void load(text, last.visitedAt)}>{loading ? "Loading…" : "Load older history"}</Button>
        </div>
      )}
    </div>
  );
}
