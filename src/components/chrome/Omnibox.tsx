import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent } from "react";

import { instantAnswer, resolveBang } from "../../features/omnibox/instant";
import { api } from "../../lib/ipc";
import { useOverlay } from "../../lib/overlay";
import { SEARCH_ENGINES, engineLabel } from "../../lib/settings";
import { activeTab, attempt, onBus, toast, useStore } from "../../lib/store";
import type { FillOptions, SearchEngine, Suggestion, Tab } from "../../lib/types";
import { idleAddress, splitUrl } from "../../lib/urls";
import { useLoad } from "../../lib/useLoad";
import { useSettings } from "../../lib/useSettings";
import { canOfferFill } from "../../lib/vault";
import { FillPanel } from "../menus/FillPanel";
import { SitePanel } from "../menus/SitePanel";
import { Icon } from "../ui/Icon";
import type { IconName } from "../ui/Icon";
import { MenuItem, Popover } from "../ui/Popover";
import { cx } from "../ui/controls";

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

export function securityIcon(tab: Tab | null): { icon: IconName; label: string; tone: string } {
  switch (tab?.security) {
    case "secure":
      return { icon: "lock", label: "Connection is secure", tone: "good" };
    case "insecure":
      return { icon: "warning", label: "Not secure", tone: "warn" };
    case "certerror":
      return { icon: "warning", label: "Certificate error", tone: "bad" };
    case "local":
      return { icon: "file", label: "File on this computer", tone: "neutral" };
    default:
      return { icon: "shieldCheck", label: "Project Browser page", tone: "brand" };
  }
}

function EngineMenu({ open, onClose, anchor }: { open: boolean; onClose: () => void; anchor: React.RefObject<HTMLElement | null> }) {
  const { settings, update } = useSettings();
  return (
    <Popover open={open} onClose={onClose} anchor={anchor} width={240} label="Search engine" className="menu">
      <div role="menu">
        <div className="menu-heading">Search with</div>
        {SEARCH_ENGINES.map((e) => (
          <MenuItem
            key={e.id}
            icon="search"
            label={e.label}
            checked={!settings.customSearchTemplate && settings.searchEngine === e.id}
            onSelect={() => void update({ searchEngine: e.id as SearchEngine, customSearchTemplate: null })}
          />
        ))}
        {settings.customSearchTemplate && <MenuItem icon="sliders" label="Custom search address" checked onSelect={() => {}} disabled />}
      </div>
    </Popover>
  );
}

export function Omnibox() {
  const tab = useStore(activeTab);
  const settings = useStore((s) => s.settings);
  const inputRef = useRef<HTMLInputElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const engineRef = useRef<HTMLButtonElement>(null);
  const [focused, setFocused] = useState(false);
  const [text, setText] = useState("");
  const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
  const [selected, setSelected] = useState(-1);
  const [siteOpen, setSiteOpen] = useState(false);
  const [fillOpen, setFillOpen] = useState(false);
  const [engineOpen, setEngineOpen] = useState(false);
  const keyRef = useRef<HTMLButtonElement>(null);
  const seq = useRef(0);
  const timer = useRef<number | undefined>(undefined);

  // Calculator / unit answers and bang shortcuts are computed locally as the user types.
  const instant = useMemo(() => (focused ? instantAnswer(text) : null), [focused, text]);
  const bang = useMemo(() => (focused ? resolveBang(text) : null), [focused, text]);
  const extras = (instant ? 1 : 0) + (bang ? 1 : 0);

  // The page is replaced by a snapshot while the address is being edited, so the suggestion list can be drawn over it.
  const overlayReady = useOverlay(focused && suggestions.length + extras > 0);

  const idle = idleAddress(tab);
  const shown = focused ? text : idle;

  // Saved logins and addresses for this exact page. The result is tagged with the page it was asked for, so a slow
  // answer for the previous tab is never offered on the new one.
  const fillTab = canOfferFill(tab) ? tab : null;
  const fillKey = fillTab ? `${fillTab.id}|${fillTab.url}` : null;
  const fill = useLoad<{ key: string; options: FillOptions }>(() => api.fillOptions(fillTab?.id ?? 0).then((options) => ({ key: fillKey ?? "", options })), fillKey);
  const fillOptions = fillKey && fill.data?.key === fillKey && (fill.data.options.logins.length > 0 || fill.data.options.profileCount > 0) ? fill.data.options : null;

  const focusInput = useCallback(() => {
    const el = inputRef.current;
    if (!el) return;
    el.focus();
    el.select();
  }, []);

  useEffect(() => onBus("focusOmnibox", focusInput), [focusInput]);
  useEffect(() => onBus("openSiteInfo", () => setSiteOpen(true)), []);
  useEffect(() => onBus("openFill", () => setFillOpen(true)), []);

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

  const commit = (value: string, suggestion?: Suggestion, newTab = false) => {
    const input = inputRef.current;
    setSuggestions([]);
    const viaBang = !suggestion ? resolveBang(value) : null;
    if (suggestion?.tabId != null) {
      const id = suggestion.tabId;
      void attempt(() => api.tabActivate(id));
    } else if (viaBang) {
      void attempt(() => (newTab ? api.tabNew(viaBang.url) : api.navigate(viaBang.url)));
    } else if (value.trim()) {
      void attempt(() => (newTab ? api.tabNew(value) : api.navigate(value)));
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
      setSelected(selected <= 0 ? count - 1 : selected - 1);
    } else if (e.key === "Enter" && e.ctrlKey && instant) {
      e.preventDefault();
      copyAnswer();
    } else if (e.key === "Enter") {
      e.preventDefault();
      const s = selected >= 0 ? suggestions[selected] : undefined;
      commit(s && s.kind !== "tab" && s.kind !== "search" ? s.url : text, s, e.altKey);
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

  const copyAnswer = () => {
    if (!instant) return;
    void navigator.clipboard
      .writeText(instant.value)
      .then(() => toast(`Copied ${instant.value}`))
      .catch(() => toast("Could not copy"));
  };

  const sec = securityIcon(tab);
  const parts = splitUrl(idle);
  const showList = focused && overlayReady && suggestions.length + extras > 0;
  const canBookmark = !!tab && !tab.url.startsWith("browser://") && !tab.error;

  return (
    <div ref={wrapRef} className={cx("omnibox", focused && "is-focused", tab?.private && "is-private", showList && "has-list")}>
      {focused ? (
        <button
          ref={engineRef}
          type="button"
          className="omni-engine"
          title={`Searching with ${engineLabel(settings)}: click to change`}
          aria-label={`Search engine: ${engineLabel(settings)}`}
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => setEngineOpen((o) => !o)}
        >
          <Icon name="search" size={14} />
          <span>{engineLabel(settings)}</span>
          <Icon name="chevronDown" size={12} />
        </button>
      ) : (
        <button type="button" className={`omni-site tone-${sec.tone}`} aria-label={`Site information: ${sec.label}`} title={sec.label} onClick={() => setSiteOpen((o) => !o)}>
          <Icon name={sec.icon} size={15} />
          {tab?.security === "insecure" && <span className="omni-site-text">Not secure</span>}
          {tab?.security === "certerror" && <span className="omni-site-text">Certificate error</span>}
        </button>
      )}

      <div className="omni-field">
        <input
          ref={inputRef}
          className="omni-input"
          type="text"
          role="combobox"
          aria-label="Address and search bar"
          aria-expanded={showList}
          aria-controls="omni-suggestions"
          aria-autocomplete="list"
          autoComplete="off"
          spellCheck={false}
          placeholder={`Search ${engineLabel(settings)} or enter address`}
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

      {focused && text.trim() && <kbd className="omni-hint" title="Alt+Enter opens it in a new tab">Enter</kbd>}
      {!focused && tab && tab.zoom !== 1 && (
        <button type="button" className="omni-chip" title="Reset zoom" onClick={() => void attempt(() => api.zoom("reset", undefined, tab.id))}>
          <Icon name="zoomIn" size={13} />
          {Math.round(tab.zoom * 100)}%
        </button>
      )}
      {!focused && fillOptions && fillTab && (
        <button
          ref={keyRef}
          type="button"
          className={cx("omni-btn omni-key", fillOpen && "is-on")}
          aria-label="Fill a saved login or address"
          aria-haspopup="dialog"
          title={fillOptions.logins.length > 0 ? "Fill a saved login" : "Fill a saved address"}
          onClick={() => setFillOpen((o) => !o)}
        >
          <Icon name="key" size={15} />
        </button>
      )}
      {!focused && canBookmark && (
        <button
          type="button"
          className={cx("omni-btn omni-star", tab.bookmarked && "is-on")}
          aria-label={tab.bookmarked ? "Remove bookmark" : "Bookmark this page"}
          title={tab.bookmarked ? "Remove bookmark (Ctrl+D)" : "Bookmark this page (Ctrl+D)"}
          onClick={() => void attempt(() => api.bookmarkToggle(tab.id))}
        >
          <Icon name="star" size={15} filled={tab.bookmarked} />
        </button>
      )}
      {tab?.loading && <div className="omni-progress" role="progressbar" aria-label="Loading page" />}

      {showList && (
        <ul id="omni-suggestions" className="omni-list" role="listbox" onMouseDown={(e) => e.preventDefault()}>
          {instant && (
            <li className="omni-answer" role="option" aria-selected={false} title="Click or Ctrl+Enter to copy" onClick={copyAnswer}>
              <span className="sugg-icon">
                <Icon name="calculator" size={15} />
              </span>
              <span className="omni-answer-q truncate">{instant.question}</span>
              <span className="omni-answer-a">{instant.answer}</span>
              <span className="sugg-kind">Copy · Ctrl+Enter</span>
            </li>
          )}
          {bang && (
            <li className="omni-answer is-bang" role="option" aria-selected={selected < 0} onClick={(e) => commit(text, undefined, e.altKey)}>
              <span className="sugg-icon">
                <Icon name="zap" size={15} />
              </span>
              <span className="sugg-title truncate">{bang.query}</span>
              <span className="sugg-kind">Search {bang.bang.name}</span>
            </li>
          )}
          {suggestions.map((s, i) => (
            <li
              key={`${s.kind}-${s.url}-${i}`}
              role="option"
              aria-selected={i === selected}
              className={cx(i === selected && "is-selected", i === 0 && "is-primary")}
              onMouseMove={() => selected !== i && setSelected(i)}
              onClick={(e) => commit(s.kind === "search" ? text : s.url, s, e.altKey)}
            >
              <span className="sugg-icon">
                <Icon name={KIND_ICON[s.kind]} size={15} />
              </span>
              <span className="sugg-title truncate">{s.kind === "search" ? text : s.title || s.url}</span>
              {s.kind !== "search" && s.title && s.title !== s.url && <span className="sugg-url truncate">{s.url}</span>}
              <span className="sugg-kind">{s.kind === "search" ? `Search ${engineLabel(settings)}` : KIND_LABEL[s.kind]}</span>
            </li>
          ))}
          <li className="omni-foot" aria-hidden="true">
            <span><kbd>↑</kbd><kbd>↓</kbd> choose</span>
            <span><kbd>Enter</kbd> open</span>
            <span><kbd>Alt</kbd>+<kbd>Enter</kbd> new tab</span>
            <span><kbd>Esc</kbd> close</span>
          </li>
        </ul>
      )}

      <EngineMenu open={engineOpen} onClose={() => setEngineOpen(false)} anchor={engineRef} />
      <SitePanel tab={tab} open={siteOpen} onClose={() => setSiteOpen(false)} anchor={wrapRef} />
      {fillOptions && fillTab && <FillPanel tab={fillTab} options={fillOptions} open={fillOpen} onClose={() => setFillOpen(false)} anchor={keyRef} />}
    </div>
  );
}
