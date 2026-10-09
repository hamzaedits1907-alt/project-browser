import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent } from "react";

import { api } from "../../lib/ipc";
import { attempt, onBus, useStore } from "../../lib/store";
import { IconButton } from "../ui/controls";

/**
 * Docked (not floating) so the page stays visible while searching. The parent
 * keys it by tab so each tab starts with a closed find bar.
 */
export function FindBar() {
  const find = useStore((s) => s.find);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [caseSensitive, setCaseSensitive] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const timer = useRef<number | undefined>(undefined);

  useEffect(
    () =>
      onBus("openFind", () => {
        setOpen(true);
        setTimeout(() => {
          input.current?.focus();
          input.current?.select();
        }, 0);
      }),
    [],
  );

  const close = () => {
    window.clearTimeout(timer.current);
    setOpen(false);
    setQuery("");
    void attempt(() => api.findClose());
  };

  const search = (q: string, cs: boolean) => {
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => void attempt(() => api.findQuery(q, cs)), 90);
  };

  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      void attempt(() => api.findStep(!e.shiftKey));
    } else if (e.key === "Escape") {
      e.preventDefault();
      close();
    }
  };

  if (!open) return null;
  const status = find && find.query === query && query ? (find.unavailable ? "Unavailable" : find.matches === 0 ? "No results" : `${find.active} of ${find.matches}`) : "";
  return (
    <div className="findbar" role="search">
      <input
        ref={input}
        className="input find-input"
        value={query}
        placeholder="Find in page"
        aria-label="Find in page"
        spellCheck={false}
        onChange={(e) => {
          setQuery(e.target.value);
          search(e.target.value, caseSensitive);
        }}
        onKeyDown={onKey}
      />
      <span className={`find-status${status === "No results" ? " is-none" : ""}`} aria-live="polite">{status}</span>
      <button
        type="button"
        className={`find-case${caseSensitive ? " is-on" : ""}`}
        aria-pressed={caseSensitive}
        title="Match case"
        onClick={() => {
          setCaseSensitive(!caseSensitive);
          if (query) search(query, !caseSensitive);
        }}
      >
        Aa
      </button>
      <IconButton icon="chevronUp" label="Previous match (Shift+Enter)" size={16} disabled={!query} onClick={() => void attempt(() => api.findStep(false))} />
      <IconButton icon="chevronDown" label="Next match (Enter)" size={16} disabled={!query} onClick={() => void attempt(() => api.findStep(true))} />
      <IconButton icon="close" label="Close find (Esc)" size={16} onClick={close} />
    </div>
  );
}
