import { useEffect, useId, useMemo, useRef, useState } from "react";
import type { KeyboardEvent } from "react";

import { Badge, Spinner } from "../../components/ui/controls";
import { Icon } from "../../components/ui/Icon";
import type { ModelInfo } from "../../lib/types";
import { KIND_LABEL, groupByPublisher, visibleModels } from "./aiModels";

interface ModelPickerProps {
  models: readonly ModelInfo[];
  /** The saved model id (may not be in the list). */
  value: string;
  onChange: (id: string) => void;
  /** Models NVIDIA refused this session: still selectable, but marked. */
  unavailable?: readonly string[];
  disabled?: boolean;
  /** The list is being (re)loaded. */
  busy?: boolean;
  label?: string;
  compact?: boolean;
}

/**
 * A searchable drop-down for NVIDIA's model list. The user picks from what NVIDIA reports; there is no free
 * text entry. Type to filter, arrow keys to move, Enter to choose, Escape to close.
 */
export function ModelPicker({ models, value, onChange, unavailable = [], disabled, busy, label = "Model", compact }: ModelPickerProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [showAll, setShowAll] = useState(false);
  const [cursor, setCursor] = useState(0);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const listId = useId();

  const { shown, hiddenNonChat } = useMemo(() => visibleModels(models, { query, showAll }), [models, query, showAll]);
  const groups = useMemo(() => groupByPublisher(shown), [shown]);
  const flat = useMemo(() => groups.flatMap((g) => g.models), [groups]);
  const rowOf = useMemo(() => new Map(flat.map((m, i) => [m.id, i])), [flat]);
  const selected = models.find((m) => m.id === value) ?? null;
  // The highlighted row can never point past the end of a filtered list.
  const active = Math.min(cursor, Math.max(0, flat.length - 1));

  const close = (refocus: boolean) => {
    setOpen(false);
    setQuery("");
    if (refocus) triggerRef.current?.focus();
  };

  const toggle = () => {
    if (open) return close(false);
    // The highlight starts on the saved model.
    const all = visibleModels(models, { query: "", showAll });
    setCursor(Math.max(0, all.shown.findIndex((m) => m.id === value)));
    setOpen(true);
  };

  const choose = (m: ModelInfo) => {
    close(true);
    if (m.id !== value) onChange(m.id);
  };

  useEffect(() => {
    if (!open) return;
    document.getElementById(`${listId}-${active}`)?.scrollIntoView({ block: "nearest" });
  }, [open, active, listId]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) {
        setOpen(false);
        setQuery("");
      }
    };
    document.addEventListener("pointerdown", onDown, true);
    return () => document.removeEventListener("pointerdown", onDown, true);
  }, [open]);

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setCursor(flat.length ? (active + 1) % flat.length : 0);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setCursor(flat.length ? (active - 1 + flat.length) % flat.length : 0);
    } else if (e.key === "Home") {
      e.preventDefault();
      setCursor(0);
    } else if (e.key === "End") {
      e.preventDefault();
      setCursor(Math.max(0, flat.length - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const m = flat[active];
      if (m) choose(m);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close(true);
    }
  };

  const triggerText = selected ? `${selected.publisher} / ${selected.name}` : value || "Choose a model";
  const empty = models.length === 0;

  return (
    <div ref={rootRef} className={`model-picker${compact ? " is-compact" : ""}`}>
      <button
        ref={triggerRef}
        type="button"
        className="mp-trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={`${label}: ${triggerText}`}
        disabled={disabled || empty}
        onClick={toggle}
      >
        {busy ? <Spinner size={14} /> : <Icon name="sparkle" size={15} />}
        <span className={`mp-current truncate${selected || value ? "" : " is-placeholder"}`}>{empty && !busy ? "No models loaded" : triggerText}</span>
        <Icon name="chevronDown" size={14} />
      </button>

      {open && (
        <div className="mp-panel" onKeyDown={onKey}>
          <div className="mp-search">
            <Icon name="search" size={15} />
            <input
              autoFocus
              type="text"
              role="combobox"
              aria-expanded="true"
              aria-controls={listId}
              aria-activedescendant={flat.length ? `${listId}-${active}` : undefined}
              aria-label="Search models"
              placeholder={`Search ${models.length} models`}
              autoComplete="off"
              spellCheck={false}
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
                setCursor(0);
              }}
            />
          </div>
          <ul id={listId} role="listbox" aria-label="Models" className="mp-list">
            {groups.map((g) => (
              <li key={g.publisher} role="presentation" className="mp-group">
                <div className="mp-group-name" aria-hidden="true">{g.publisher}</div>
                <ul role="group" aria-label={g.publisher}>
                  {g.models.map((m) => {
                    const i = rowOf.get(m.id) ?? 0;
                    return (
                      <li
                        key={m.id}
                        id={`${listId}-${i}`}
                        role="option"
                        aria-selected={m.id === value}
                        className={`mp-item${i === active ? " is-active" : ""}${m.id === value ? " is-selected" : ""}`}
                        onMouseMove={() => active !== i && setCursor(i)}
                        onClick={() => choose(m)}
                      >
                        <span className="mp-item-name truncate">{m.name}</span>
                        <span className="mp-item-meta">
                          {m.hints.vision === "yes" && <Badge tone="accent">Vision</Badge>}
                          {m.kind !== "chat" && m.kind !== "vision" && <Badge>{KIND_LABEL[m.kind]}</Badge>}
                          {m.hints.chat === "no" && <Badge tone="warn">Can't chat</Badge>}
                          {unavailable.includes(m.id) && <Badge tone="bad">Unavailable</Badge>}
                        </span>
                        {m.id === value && <Icon name="check" size={15} />}
                      </li>
                    );
                  })}
                </ul>
              </li>
            ))}
            {flat.length === 0 && (
              <li role="presentation" className="mp-empty">
                {query ? `No model matches “${query}”.` : "No chat models in the list."}
              </li>
            )}
          </ul>
          {(hiddenNonChat > 0 || showAll) && (
            <button type="button" className="mp-toggle" onClick={() => setShowAll((v) => !v)}>
              {showAll ? "Hide models that can't chat" : `Show ${hiddenNonChat} more that can't chat (embeddings, safety, documents)`}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
