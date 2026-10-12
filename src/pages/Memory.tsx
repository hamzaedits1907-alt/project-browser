import { useEffect, useState } from "react";

import { Badge, Button, IconButton, TextField } from "../components/ui/controls";
import { Icon } from "../components/ui/Icon";
import { Modal } from "../components/ui/Modal";
import { EmptyState, Notice, PageHeader } from "../components/ui/Section";
import { formatTags, MEMORY_KIND_LABEL, parseTags } from "../features/memory/memory";
import { hostOf } from "../lib/format";
import { api, errorMessage } from "../lib/ipc";
import { attempt, toast } from "../lib/store";
import type { MemoryItem } from "../lib/types";
import { useLoad } from "../lib/useLoad";

interface Draft {
  id: number | null; // null = new
  kind: MemoryItem["kind"];
  title: string;
  body: string;
  tags: string;
  related: number[];
  sources: MemoryItem["sources"];
}

function blank(): Draft {
  return { id: null, kind: "note", title: "", body: "", tags: "", related: [], sources: [] };
}

function fromItem(m: MemoryItem): Draft {
  return { id: m.id, kind: m.kind, title: m.title, body: m.body, tags: formatTags(m.tags), related: m.related, sources: m.sources };
}

function when(ms: number): string {
  return new Date(ms).toLocaleDateString([], { year: "numeric", month: "short", day: "numeric" });
}

/** Project Memory: everything the user chose to keep, searchable, editable and deletable. Nothing is added by itself. */
export default function Memory() {
  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
  const [draft, setDraft] = useState<Draft | null>(null);
  const [draftError, setDraftError] = useState<string | null>(null);
  const [open, setOpen] = useState<number | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);

  useEffect(() => {
    const t = setTimeout(() => setDebounced(query), 200);
    return () => clearTimeout(t);
  }, [query]);

  const loaded = useLoad(() => api.memoryList(debounced, 500), debounced);
  const everything = useLoad(() => api.memoryList("", 500), 0);
  const items = loaded.data ?? [];
  const all = everything.data ?? [];
  const byId = new Map(all.map((m) => [m.id, m]));

  const refresh = () => {
    loaded.reload();
    everything.reload();
  };

  const save = async () => {
    if (!draft) return;
    setDraftError(null);
    try {
      await api.memorySave(
        { kind: draft.kind, title: draft.title, body: draft.body, tags: parseTags(draft.tags), sources: draft.sources, related: draft.related },
        draft.id ?? undefined,
      );
      setDraft(null);
      refresh();
      toast(draft.id === null ? "Saved to Project Memory" : "Changes saved");
    } catch (e) {
      setDraftError(errorMessage(e));
    }
  };

  const remove = async (m: MemoryItem) => {
    if (await attempt(() => api.memoryDelete(m.id), "Could not delete")) {
      refresh();
      toast("Deleted");
    }
  };

  const clearAll = async () => {
    setConfirmClear(false);
    const n = await attempt(() => api.memoryClear(), "Could not delete");
    if (n !== undefined) {
      refresh();
      toast(`Deleted ${n} record${n === 1 ? "" : "s"}`);
    }
  };

  return (
    <div className="page">
      <PageHeader
        icon="bookmarks"
        title="Project Memory"
        subtitle="Notes, summaries and research you chose to keep. Nothing is saved automatically, and nothing here is sent to the AI unless you put it into a request."
        actions={
          <>
            <Button icon="plus" variant="primary" onClick={() => { setDraftError(null); setDraft(blank()); }}>New note</Button>
            <Button icon="trash" variant="ghost" disabled={all.length === 0} onClick={() => setConfirmClear(true)}>Delete all</Button>
          </>
        }
      />

      <div className="page-toolbar">
        <input className="input" type="search" placeholder="Search titles, text, tags and sources" aria-label="Search Project Memory" value={query} onChange={(e) => setQuery(e.target.value)} />
        <span className="dim small">{loaded.loading ? "Searching…" : `${items.length} record${items.length === 1 ? "" : "s"}`}</span>
      </div>

      {loaded.error && <Notice tone="bad">{loaded.error}</Notice>}

      {!loaded.loading && items.length === 0 ? (
        <EmptyState icon="bookmarks" title={debounced ? "Nothing matches" : "Nothing saved yet"}>
          {debounced
            ? "Try fewer words. Every word you type has to appear in a record."
            : "Write a note here, or press “Save to memory” on an assistant answer or a research result."}
        </EmptyState>
      ) : (
        <div className="list memory-list">
          {items.map((m) => {
            const expanded = open === m.id;
            const related = m.related.map((id) => byId.get(id)).filter((x): x is MemoryItem => !!x);
            return (
              <div key={m.id} className="memory-item">
                <div className="memory-head">
                  <button type="button" className="memory-title" aria-expanded={expanded} onClick={() => setOpen(expanded ? null : m.id)}>
                    <Icon name={expanded ? "chevronDown" : "forward"} size={14} />
                    <span>{m.title}</span>
                  </button>
                  <Badge tone={m.kind === "research" ? "accent" : "neutral"}>{MEMORY_KIND_LABEL[m.kind]}</Badge>
                  <span className="dim small">{when(m.updatedAt)}</span>
                  <IconButton icon="edit" label="Edit" size={15} onClick={() => { setDraftError(null); setDraft(fromItem(m)); }} />
                  <IconButton icon="trash" label="Delete" size={15} onClick={() => void remove(m)} />
                </div>
                {m.tags.length > 0 && <div className="memory-tags">{m.tags.map((t) => <span key={t} className="tag">#{t}</span>)}</div>}
                {expanded ? (
                  <>
                    <pre className="memory-body" data-selectable>{m.body || "(no text)"}</pre>
                    {m.sources.length > 0 && (
                      <div className="memory-sources">
                        <strong>Sources</strong>
                        {m.sources.map((s) => (
                          <button key={s.url} type="button" className="row-link" title={s.url} onClick={() => void attempt(() => api.tabNew(s.url))}>
                            {s.title || hostOf(s.url)} <span className="dim small">{hostOf(s.url)}</span>
                          </button>
                        ))}
                      </div>
                    )}
                    {related.length > 0 && (
                      <div className="memory-sources">
                        <strong>Linked records</strong>
                        {related.map((r) => (
                          <button key={r.id} type="button" className="row-link" onClick={() => setOpen(r.id)}>{r.title}</button>
                        ))}
                      </div>
                    )}
                  </>
                ) : (
                  m.body && <p className="memory-preview dim">{m.body.slice(0, 220)}</p>
                )}
              </div>
            );
          })}
        </div>
      )}

      <Modal open={draft !== null} onClose={() => setDraft(null)} title={draft?.id === null ? "New note" : "Edit record"} width={560}>
        {draft && (
          <div className="memory-editor">
            <TextField label="Title" value={draft.title} maxLength={200} onChange={(e) => setDraft({ ...draft, title: e.target.value })} autoFocus />
            <div className="field">
              <label htmlFor="memory-body">Text</label>
              <textarea id="memory-body" className="input textarea memory-textarea" rows={9} value={draft.body} onChange={(e) => setDraft({ ...draft, body: e.target.value })} />
            </div>
            <TextField label="Tags" hint="Separate with commas" value={draft.tags} onChange={(e) => setDraft({ ...draft, tags: e.target.value })} />
            {all.filter((m) => m.id !== draft.id).length > 0 && (
              <div className="field">
                <label>Link to other records</label>
                <div className="memory-links">
                  {all.filter((m) => m.id !== draft.id).slice(0, 60).map((m) => (
                    <label key={m.id} className="check-row">
                      <input
                        type="checkbox"
                        checked={draft.related.includes(m.id)}
                        onChange={(e) => setDraft({ ...draft, related: e.target.checked ? [...draft.related, m.id] : draft.related.filter((r) => r !== m.id) })}
                      />
                      <span>{m.title}</span>
                    </label>
                  ))}
                </div>
              </div>
            )}
            {draft.sources.length > 0 && <p className="dim small">{draft.sources.length} source address{draft.sources.length === 1 ? "" : "es"} kept with this record.</p>}
            {draftError && <Notice tone="bad">{draftError}</Notice>}
            <div className="wizard-actions">
              <Button variant="ghost" onClick={() => setDraft(null)}>Cancel</Button>
              <Button variant="primary" onClick={() => void save()}>Save</Button>
            </div>
          </div>
        )}
      </Modal>

      <Modal open={confirmClear} onClose={() => setConfirmClear(false)} title="Delete all of Project Memory?" width={440}>
        <p>This permanently deletes all {all.length} record{all.length === 1 ? "" : "s"}. It cannot be undone.</p>
        <div className="wizard-actions">
          <Button variant="ghost" onClick={() => setConfirmClear(false)}>Cancel</Button>
          <Button variant="primary" onClick={() => void clearAll()}>Delete everything</Button>
        </div>
      </Modal>
    </div>
  );
}
