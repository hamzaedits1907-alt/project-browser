import { useEffect, useMemo, useRef, useState } from "react";

import { Favicon } from "../../components/ui/Favicon";
import { Icon } from "../../components/ui/Icon";
import type { IconName } from "../../components/ui/Icon";
import { cx } from "../../components/ui/controls";
import { hostOf } from "../../lib/format";
import { api } from "../../lib/ipc";
import { useOverlay } from "../../lib/overlay";
import { engineLabel } from "../../lib/settings";
import { activeTab, attempt, toast, useStore } from "../../lib/store";
import type { Bookmark, HistoryEntry } from "../../lib/types";
import { closeDialog, ui } from "../../lib/ui";
import { instantAnswer, resolveBang } from "../omnibox/instant";
import { buildCommands } from "./commands";
import { rank } from "./rank";

interface Item {
  key: string;
  section: "Answer" | "Tabs" | "Commands" | "Bookmarks" | "History" | "Go";
  title: string;
  subtitle?: string;
  icon?: IconName;
  favicon?: { src: string | null; host: string };
  hint?: string;
  keywords?: string;
  boost?: number;
  run: () => void;
}

const SECTION_ORDER: Item["section"][] = ["Answer", "Go", "Tabs", "Commands", "Bookmarks", "History"];

/**
 * The Command Center (Ctrl+K): one box that finds open tabs, browser commands, bookmarks, history and settings, and
 * answers sums and conversions. Prefix ">" for commands only, "@" for tabs only.
 */
export function CommandPalette() {
  const open = ui.use((s) => s.dialog === "palette");
  const ready = useOverlay(open);
  if (!ready) return null;
  return <PaletteBody />;
}

function PaletteBody() {
  const tab = useStore(activeTab);
  const tabs = useStore((s) => s.browser.tabs);
  const settings = useStore((s) => s.settings);
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const [bookmarks, setBookmarks] = useState<Bookmark[]>([]);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
    api.bookmarksAll().then(setBookmarks, () => {});
  }, []);

  const mode = q.startsWith(">") ? "commands" : q.startsWith("@") ? "tabs" : "all";
  const query = mode === "all" ? q.trim() : q.slice(1).trim();

  useEffect(() => {
    if (mode !== "all" || query.length < 2) return;
    const t = window.setTimeout(() => {
      api.historyQuery(query, undefined, 12).then(setHistory, () => setHistory([]));
    }, 90);
    return () => window.clearTimeout(t);
  }, [mode, query]);

  const items = useMemo(() => {
    const out: Item[] = [];
    const run = (fn: () => void) => () => {
      closeDialog();
      // Let the overlay close (the page comes back) before the action runs.
      window.setTimeout(fn, 30);
    };
    if (mode === "all" && query) {
      const answer = instantAnswer(query);
      if (answer) {
        out.push({
          key: "answer",
          section: "Answer",
          title: answer.answer,
          subtitle: `${answer.question} · Enter copies`,
          icon: "calculator",
          run: run(() => void navigator.clipboard.writeText(answer.value).then(() => toast(`Copied ${answer.value}`))),
        });
      }
      const bang = resolveBang(query);
      out.push(
        bang
          ? { key: "go", section: "Go", title: `Search ${bang.bang.name} for “${bang.query}”`, icon: "zap", run: run(() => void attempt(() => api.tabNew(bang.url))) }
          : /^[\w-]+(\.[\w-]+)+(\/\S*)?$/.test(query) || /^https?:\/\//.test(query)
            ? { key: "go", section: "Go", title: `Open ${query}`, icon: "globe", run: run(() => void attempt(() => api.tabNew(query))) }
            : { key: "go", section: "Go", title: `Search ${engineLabel(settings)} for “${query}”`, icon: "search", run: run(() => void attempt(() => api.tabNew(query))) },
      );
    }
    if (mode !== "commands") {
      const tabItems: Item[] = tabs.map((t) => ({
        key: `tab-${t.id}`,
        section: "Tabs",
        title: t.title || t.url,
        subtitle: t.id === tab?.id ? "Current tab" : hostOf(t.url),
        keywords: t.url,
        favicon: { src: t.favicon, host: hostOf(t.url) },
        boost: 300,
        run: run(() => void attempt(() => api.tabActivate(t.id))),
      }));
      out.push(...(query ? rank(tabItems, query, 8) : tabItems.slice(0, 6)));
    }
    if (mode !== "tabs") {
      const cmdItems: Item[] = buildCommands(tab, tabs, settings).map((c) => ({
        key: `cmd-${c.id}`,
        section: "Commands",
        title: c.title,
        icon: c.icon,
        hint: c.hint,
        keywords: c.keywords,
        boost: 200,
        run: run(c.run),
      }));
      out.push(...(query ? rank(cmdItems, query, mode === "commands" ? 40 : 8) : cmdItems.slice(0, mode === "commands" ? 60 : 8)));
    }
    if (mode === "all" && query) {
      const bm: Item[] = bookmarks
        .filter((b) => b.kind === "url" && b.url)
        .map((b) => ({
          key: `bm-${b.id}`,
          section: "Bookmarks",
          title: b.title || (b.url as string),
          subtitle: hostOf(b.url as string),
          keywords: b.url as string,
          favicon: { src: null, host: hostOf(b.url as string) },
          boost: 80,
          run: run(() => void attempt(() => api.tabNew(b.url as string))),
        }));
      out.push(...rank(bm, query, 5));
      const seen = new Set<string>();
      for (const h of query.length >= 2 ? history : []) {
        if (seen.has(h.url) || out.some((o) => o.keywords === h.url)) continue;
        seen.add(h.url);
        out.push({
          key: `h-${h.visitId}`,
          section: "History",
          title: h.title || h.url,
          subtitle: hostOf(h.url),
          favicon: { src: null, host: hostOf(h.url) },
          run: run(() => void attempt(() => api.tabNew(h.url))),
        });
        if (seen.size >= 6) break;
      }
    }
    return SECTION_ORDER.flatMap((s) => out.filter((i) => i.section === s));
  }, [mode, query, tabs, tab, settings, bookmarks, history]);

  const clamped = Math.min(sel, Math.max(0, items.length - 1));
  useEffect(() => {
    listRef.current?.querySelector<HTMLElement>(`[data-index="${clamped}"]`)?.scrollIntoView({ block: "nearest" });
  }, [clamped]);

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSel((clamped + 1) % Math.max(1, items.length));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSel(clamped <= 0 ? items.length - 1 : clamped - 1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      items[clamped]?.run();
    } else if (e.key === "Escape") {
      e.preventDefault();
      closeDialog();
    } else if (e.key === "Tab") {
      e.preventDefault();
      setQ(mode === "commands" ? `@${query}` : mode === "tabs" ? query : `>${query}`);
    }
  };

  return (
    <div className="palette-backdrop" onPointerDown={(e) => e.target === e.currentTarget && closeDialog()}>
      <div className="palette" role="dialog" aria-modal="true" aria-label="Command Center">
        <div className="palette-input">
          <Icon name={mode === "commands" ? "command" : mode === "tabs" ? "tabs" : "search"} size={18} />
          <input
            ref={inputRef}
            value={q}
            onChange={(e) => {
              setQ(e.target.value);
              setSel(0);
            }}
            onKeyDown={onKey}
            placeholder="Search tabs, commands, bookmarks, history — or type a sum"
            aria-label="Command Center"
            aria-controls="palette-list"
            role="combobox"
            aria-expanded="true"
            spellCheck={false}
          />
          <kbd>Esc</kbd>
        </div>
        <div className="palette-list" id="palette-list" role="listbox" ref={listRef}>
          {items.length === 0 && <p className="palette-empty dim">Nothing matches “{query}”.</p>}
          {items.map((item, i) => {
            const header = item.section !== items[i - 1]?.section ? item.section : null;
            return (
              <div key={item.key}>
                {header && <div className="palette-section">{header}</div>}
                <div
                  role="option"
                  aria-selected={i === clamped}
                  data-index={i}
                  className={cx("palette-item", i === clamped && "is-selected", item.section === "Answer" && "is-answer")}
                  onMouseMove={() => i !== clamped && setSel(i)}
                  onClick={item.run}
                >
                  <span className="palette-icon">
                    {item.favicon ? <Favicon src={item.favicon.src} fallback={item.favicon.host.charAt(0).toUpperCase()} seed={item.favicon.host} size={16} /> : <Icon name={item.icon ?? "dots"} size={16} />}
                  </span>
                  <span className="palette-text">
                    <span className="truncate">{item.title}</span>
                    {item.subtitle && <span className="palette-sub truncate">{item.subtitle}</span>}
                  </span>
                  {item.hint && <kbd className="palette-hint">{item.hint}</kbd>}
                </div>
              </div>
            );
          })}
        </div>
        <div className="palette-foot">
          <span>
            <kbd>↑</kbd>
            <kbd>↓</kbd> choose
          </span>
          <span>
            <kbd>Enter</kbd> run
          </span>
          <span>
            <kbd>Tab</kbd> commands / tabs
          </span>
          <span>
            <kbd>&gt;</kbd> commands <kbd>@</kbd> tabs
          </span>
        </div>
      </div>
    </div>
  );
}
