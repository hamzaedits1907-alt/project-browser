import { useCallback, useEffect, useRef, useState } from "react";
import type { KeyboardEvent } from "react";

import { api } from "../../lib/ipc";
import { useOverlay } from "../../lib/overlay";
import { activeTab, attempt, onBus, useStore } from "../../lib/store";
import type { FillOptions, Suggestion, Tab } from "../../lib/types";
import { idleAddress, splitUrl } from "../../lib/urls";
import { useLoad } from "../../lib/useLoad";
import { canOfferFill } from "../../lib/vault";
import { FillPanel } from "../menus/FillPanel";
import { SitePanel } from "../menus/SitePanel";
import { Icon } from "../ui/Icon";
import type { IconName } from "../ui/Icon";

const SUGGEST_DELAY_MS = 60;

const KIND_ICON: Record<Suggestion["kind"], IconName> = {
  search: "search",
  url: "globe",
  tab: "tabs",
  bookmark: "star",
  history: "history",
};

const KIND_LABEL: Record<Suggestion["kind"], string> = {
  search: "Search",
  url: "Go to",
  tab: "Switch to tab",
  bookmark: "Bookmark",
  history: "History",
};

function securityIcon(tab: Tab | null): { icon: IconName; label: string; tone: string } {
  switch (tab?.security) {
    case "secure":
      return { icon: "lock", label: "Secure", tone: "good" };
    case "insecure":
      return { icon: "warning", label: "Not secure", tone: "warn" };
    case "certerror":
      return { icon: "warning", label: "Certificate error", tone: "bad" };
    case "local":
      return { icon: "file", label: "File", tone: "neutral" };
    default:
      return { icon: "shieldCheck", label: "Project Browser", tone: "brand" };
  }
}

export function Omnibox() {
  const tab = useStore(activeTab);
  const inputRef = useRef<HTMLInputElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const [focused, setFocused] = useState(false);
  const [text, setText] = useState("");
  const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
  const [selected, setSelected] = useState(-1);
  const [siteOpen, setSiteOpen] = useState(false);
  const [fillOpen, setFillOpen] = useState(false);
  const keyRef = useRef<HTMLButtonElement>(null);
  const seq = useRef(0);
  const timer = useRef<number | undefined>(undefined);

  // The page is replaced by a snapshot while the address is being edited, so the
  // suggestion list can be drawn over it.
  const overlayReady = useOverlay(focused);

  const idle = idleAddress(tab);
  const shown = focused ? text : idle;

  // Saved logins and addresses for this exact page. The result is tagged with the page it was asked for, so a slow
  // answer for the previous tab is never offered on the new one.
  const fillTab = canOfferFill(tab) ? tab : null;
  const fillKey = fillTab ? `${fillTab.id}|${fillTab.url}` : null;
  const fill = useLoad<{ key: string; options: FillOptions }>(
    () => api.fillOptions(fillTab?.id ?? 0).then((options) => ({ key: fillKey ?? "", options })),
    fillKey,
  );
  const fillOptions = fillKey && fill.data?.key === fillKey && (fill.data.options.logins.length > 0 || fill.data.options.profileCount > 0) ? fill.data.options : null;

  const focusInput = useCallback(() => {
    const el = inputRef.current;
    if (!el) return;
    el.focus();
    el.select();
  }, []);

  useEffect(() => onBus("focusOmnibox", focusInput), [focusInput]);

  const query = useCallback((value: string) => {
    window.clearTimeout(timer.current);
    const mine = ++seq.current;
    if (!value.trim()) {
      setSuggestions([]);
      setSelected(-1);
      return;
    }
    timer.current = window.setTimeout(() => {
      api
        .suggest(value)
        .then((list) => {
          if (mine !== seq.current) return;
          setSuggestions(list);
          setSelected(-1);
        })
        .catch(() => {
          if (mine === seq.current) setSuggestions([]);
        });
    }, SUGGEST_DELAY_MS);
  }, []);

  useEffect(() => () => window.clearTimeout(timer.current), []);

  const commit = (value: string, suggestion?: Suggestion) => {
    const input = inputRef.current;
    setSuggestions([]);
    if (suggestion?.tabId != null) {
      const id = suggestion.tabId;
      void attempt(() => api.tabActivate(id));
    } else if (value.trim()) {
      void attempt(() => api.navigate(value));
    }
    seq.current++;
    input?.blur();
  };

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    const count = suggestions.length;
    if (e.key === "ArrowDown" && count) {
      e.preventDefault();
      const next = (selected + 1) % count;
      setSelected(next);
      const s = suggestions[next];
      if (s && s.kind !== "tab") setText(s.kind === "search" ? text : s.url);
    } else if (e.key === "ArrowUp" && count) {
      e.preventDefault();
      const next = selected <= 0 ? count - 1 : selected - 1;
      setSelected(next);
    } else if (e.key === "Enter") {
      e.preventDefault();
      const s = selected >= 0 ? suggestions[selected] : undefined;
      commit(s && s.kind !== "tab" && s.kind !== "search" ? s.url : text, s);
    } else if (e.key === "Escape") {
      e.preventDefault();
      if (suggestions.length) {
        setSuggestions([]);
        setText(idle);
        inputRef.current?.select();
      } else {
        inputRef.current?.blur();
      }
    }
  };

  const sec = securityIcon(tab);
  const parts = splitUrl(idle);
  const showList = focused && overlayReady && suggestions.length > 0;
  const canBookmark = !!tab && !tab.url.startsWith("browser://") && !tab.error;

  return (
    <div ref={wrapRef} className={`omnibox${focused ? " is-focused" : ""}${tab?.private ? " is-private" : ""}`}>
      <button
        type="button"
        className={`omni-site tone-${sec.tone}`}
        aria-label={`Site information: ${sec.label}`}
        title={sec.label}
        onClick={() => setSiteOpen((o) => !o)}
      >
        <Icon name={sec.icon} size={16} />
        {tab?.security === "insecure" && <span className="omni-site-text">Not secure</span>}
        {tab?.security === "certerror" && <span className="omni-site-text">Certificate error</span>}
      </button>

      <div className="omni-field">
        <input
          ref={inputRef}
          className="omni-input"
          type="text"
          role="combobox"
          aria-label="Address and search bar"
          aria-expanded={showList}
          aria-controls="omni-suggestions"
          autoComplete="off"
          spellCheck={false}
          placeholder="Search or enter address"
          value={shown}
          style={!focused && idle ? { color: "transparent" } : undefined}
          onFocus={(e) => {
            setFocused(true);
            setText(idle);
            e.currentTarget.select();
          }}
          onBlur={() => {
            setFocused(false);
            setSuggestions([]);
            seq.current++;
          }}
          onChange={(e) => {
            setText(e.target.value);
            query(e.target.value);
          }}
          onKeyDown={onKeyDown}
        />
        {!focused && idle && (
          <div className="omni-display" aria-hidden="true" onClick={focusInput}>
            <span className="omni-scheme">{parts.scheme && tab?.security !== "secure" ? parts.scheme : ""}</span>
            <span className="omni-host">{parts.host}</span>
            <span className="omni-rest">{parts.rest}</span>
          </div>
        )}
      </div>

      {tab && tab.zoom !== 1 && (
        <button type="button" className="omni-chip" title="Reset zoom" onClick={() => void attempt(() => api.zoom("reset", undefined, tab.id))}>
          <Icon name="zoomIn" size={13} />
          {Math.round(tab.zoom * 100)}%
        </button>
      )}
      {canBookmark && (
        <button
          type="button"
          className={`omni-star${tab.bookmarked ? " is-on" : ""}`}
          aria-label={tab.bookmarked ? "Remove bookmark" : "Bookmark this page"}
          title={tab.bookmarked ? "Remove bookmark (Ctrl+D)" : "Bookmark this page (Ctrl+D)"}
          onClick={() => void attempt(() => api.bookmarkToggle(tab.id))}
        >
          <Icon name="star" size={16} filled={tab.bookmarked} />
        </button>
      )}
      {fillOptions && fillTab && (
        <button
          ref={keyRef}
          type="button"
          className={`omni-key${fillOpen ? " is-on" : ""}`}
          aria-label="Fill a saved login or address"
          aria-haspopup="dialog"
          title={fillOptions.logins.length > 0 ? "Fill a saved login" : "Fill a saved address"}
          onClick={() => setFillOpen((o) => !o)}
        >
          <Icon name="key" size={16} />
        </button>
      )}
      {tab?.loading && <div className="omni-progress" role="progressbar" aria-label="Loading page" />}

      {showList && (
        <ul id="omni-suggestions" className="omni-list" role="listbox" onMouseDown={(e) => e.preventDefault()}>
          {suggestions.map((s, i) => (
            <li
              key={`${s.kind}-${s.url}-${i}`}
              role="option"
              aria-selected={i === selected}
              className={i === selected ? "is-selected" : undefined}
              onMouseMove={() => selected !== i && setSelected(i)}
              onClick={() => commit(s.kind === "search" ? text : s.url, s)}
            >
              <Icon name={KIND_ICON[s.kind]} size={16} />
              <span className="sugg-title truncate">{s.kind === "search" ? text : s.title || s.url}</span>
              {s.kind !== "search" && s.title && s.title !== s.url && <span className="sugg-url truncate">{s.url}</span>}
              <span className="sugg-kind">{KIND_LABEL[s.kind]}</span>
            </li>
          ))}
        </ul>
      )}

      <SitePanel tab={tab} open={siteOpen} onClose={() => setSiteOpen(false)} anchor={wrapRef} />
      {fillOptions && fillTab && <FillPanel tab={fillTab} options={fillOptions} open={fillOpen} onClose={() => setFillOpen(false)} anchor={keyRef} />}
    </div>
  );
}
