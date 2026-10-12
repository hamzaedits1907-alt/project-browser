import { useEffect, useRef, useState } from "react";
import type { CSSProperties, FormEvent, ReactNode } from "react";

import { Favicon } from "../components/ui/Favicon";
import { Icon } from "../components/ui/Icon";
import type { IconName } from "../components/ui/Icon";
import { Logo } from "../components/ui/Logo";
import { Popover } from "../components/ui/Popover";
import { Segmented, Toggle } from "../components/ui/controls";
import { DownloadRow } from "../components/chrome/DownloadRow";
import { sendMessage, setAiOpen } from "../features/ai/aiStore";
import { formatBytes, formatPercent, hostOf } from "../lib/format";
import { api } from "../lib/ipc";
import { usePerfOnce } from "../lib/perf";
import { NEW_TAB_BACKGROUNDS, NTP_WIDGETS, backgroundOrDefault, engineLabel, normalizeSiteUrl, toggleItem } from "../lib/settings";
import { attempt, useStore } from "../lib/store";
import type { NtpData } from "../lib/types";
import { openImportWizard } from "../lib/ui";
import { useSettings } from "../lib/useSettings";

function greeting(now = new Date()): string {
  const h = now.getHours();
  if (h < 5) return "Late session";
  if (h < 12) return "Good morning";
  if (h < 18) return "Good afternoon";
  return "Good evening";
}

const go = (url: string) => void attempt(() => api.navigate(url));

/** The time, updated on the minute (a single timer while the page is open). */
function useClock(): Date {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    let t = 0;
    const tick = () => {
      setNow(new Date());
      t = window.setTimeout(tick, 60_000 - (Date.now() % 60_000) + 50);
    };
    t = window.setTimeout(tick, 60_000 - (Date.now() % 60_000) + 50);
    return () => window.clearTimeout(t);
  }, []);
  return now;
}

const WIDGET_LABEL: Record<string, string> = {
  shortcuts: "Shortcuts",
  continue: "Continue browsing",
  closed: "Recently closed",
  bookmarks: "Recent bookmarks",
  downloads: "Recent downloads",
  system: "System (memory and CPU)",
  gaming: "Gaming Mode",
  ai: "Ask AI",
};

export function NewTab() {
  const { settings, update } = useSettings();
  const profile = useStore((s) => s.profile);
  const bookmarksRev = useStore((s) => s.bookmarksRev);
  const closedCount = useStore((s) => s.browser.closedCount);
  const downloads = useStore((s) => s.downloads);
  const [data, setData] = useState<NtpData | null>(null);
  const [query, setQuery] = useState("");
  const [custOpen, setCustOpen] = useState(false);
  const custRef = useRef<HTMLButtonElement>(null);
  const now = useClock();
  const minimal = settings.ntpLayout === "minimal";
  const has = (w: string) => settings.ntpWidgets.includes(w);
  const sample = usePerfOnce("ntp", !minimal && has("system") && settings.showRamOnNewTab);

  useEffect(() => {
    let live = true;
    api.ntpData().then((d) => live && setData(d)).catch(() => {});
    return () => {
      live = false;
    };
  }, [bookmarksRev, closedCount]);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (query.trim()) go(query);
  };

  const shortcuts = [
    ...settings.ntpShortcuts.map((s) => ({ ...s, own: true })),
    ...(data?.topSites ?? [])
      .filter((s) => !settings.ntpHiddenSites.includes(s.url) && !settings.ntpShortcuts.some((o) => o.url === s.url))
      .map((s) => ({ title: s.title || hostOf(s.url), url: s.url, own: false })),
  ].slice(0, 10);

  const removeShortcut = (url: string, own: boolean) =>
    void update(own ? { ntpShortcuts: settings.ntpShortcuts.filter((s) => s.url !== url) } : { ntpHiddenSites: [...settings.ntpHiddenSites, url] });

  let order = 0;
  const stagger = () => ({ "--i": order++ }) as CSSProperties;
  const empty = !!data && data.topSites.length === 0 && data.continueBrowsing.length === 0 && data.recentBookmarks.length === 0;

  return (
    <div className={`ntp${minimal ? " is-minimal" : ""}`} data-bg={backgroundOrDefault(settings.newTabBackground)}>
      <div className="ntp-bg" aria-hidden="true" />
      <div className="ntp-inner">
        <header className="ntp-hero" style={stagger()}>
          <Logo size={46} glow className="ntp-mark" />
          <div className="ntp-clock num" aria-label="Time">
            {now.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })}
          </div>
          <p className="ntp-greeting">
            {greeting(now)}
            {profile ? `, ${profile.name}` : ""} · {now.toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric" })}
          </p>
          <form className="ntp-search" onSubmit={submit} role="search">
            <span className="ntp-engine">
              <Icon name="search" size={16} />
              {engineLabel(settings)}
            </span>
            <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search the web or type an address" aria-label="Search the web or type an address" spellCheck={false} autoFocus />
            <kbd>Enter</kbd>
          </form>
        </header>

        {has("shortcuts") && (
          <section className="ntp-tiles" aria-label="Shortcuts" style={stagger()}>
            {shortcuts.map((s) => (
              <div key={s.url} className="ntp-tile">
                <button type="button" className="ntp-tile-main" onClick={() => go(s.url)} title={s.url}>
                  <span className="tile-icon">
                    <Favicon src={null} fallback={(hostOf(s.url).charAt(0) || "•").toUpperCase()} seed={hostOf(s.url)} size={30} />
                  </span>
                  <span className="tile-title truncate">{s.title}</span>
                  <span className="tile-host truncate">{hostOf(s.url)}</span>
                </button>
                <button type="button" className="tile-remove" aria-label={`Remove ${s.title}`} title="Remove from New Tab" onClick={() => removeShortcut(s.url, s.own)}>
                  <Icon name="close" size={12} />
                </button>
              </div>
            ))}
            {shortcuts.length < 10 && <AddShortcut onAdd={(title, url) => void update({ ntpShortcuts: [...settings.ntpShortcuts, { title, url }] })} />}
          </section>
        )}

        {!minimal && (
          <div className="ntp-widgets">
            {has("continue") && data && data.continueBrowsing.length > 0 && (
              <Widget title="Continue browsing" icon="history" style={stagger()}>
                {data.continueBrowsing.map((r) => <Row key={r.url} url={r.url} title={r.title || hostOf(r.url)} />)}
              </Widget>
            )}
            {has("closed") && data && data.recentlyClosed.length > 0 && (
              <Widget title="Recently closed" icon="tabs" style={stagger()} action={<button type="button" className="link-btn" onClick={() => void attempt(() => api.tabReopen())}>Reopen last</button>}>
                {data.recentlyClosed.map((c, i) => <Row key={`${c.url}-${i}`} url={c.url} title={c.title || hostOf(c.url)} />)}
              </Widget>
            )}
            {has("bookmarks") && data && data.recentBookmarks.length > 0 && (
              <Widget title="Recent bookmarks" icon="star" style={stagger()}>
                {data.recentBookmarks.filter((b) => b.url).map((b) => <Row key={b.id} url={b.url ?? ""} title={b.title || (b.url ?? "")} />)}
              </Widget>
            )}
            {has("downloads") && downloads.length > 0 && (
              <Widget title="Recent downloads" icon="download" style={stagger()}>
                {downloads.slice(0, 3).map((d) => <DownloadRow key={d.id} d={d} compact />)}
              </Widget>
            )}
            {has("system") && settings.showRamOnNewTab && sample && (
              <Widget title="System" icon="gauge" style={stagger()} action={<button type="button" className="link-btn" onClick={() => void attempt(() => api.runCommand({ type: "openPerformance" }))}>Details</button>}>
                <div className="ntp-stats">
                  <Stat label="Browser memory" value={formatBytes(sample.totalMemory)} />
                  <Stat label="CPU" value={formatPercent(sample.totalCpu)} />
                  <Stat label="Tabs running" value={String(sample.liveTabs)} />
                  <Stat label="Sleeping" value={String(sample.sleepingTabs + sample.unloadedTabs)} />
                </div>
                <p className="dim small">One reading taken when this page opened (private working set of the browser's processes).</p>
              </Widget>
            )}
            {has("gaming") && <GamingWidget style={stagger()} />}
            {has("ai") && <AiWidget style={stagger()} />}
          </div>
        )}

        {empty && (
          <section className="ntp-welcome" style={stagger()}>
            <h2>Your start page fills in as you browse</h2>
            <p className="dim">Sites you visit often and your bookmarks appear here. Coming from Chrome, Edge or Brave? Bring your bookmarks and history across.</p>
            <button type="button" className="btn btn-primary" onClick={openImportWizard}>
              <Icon name="import" size={16} /> Import from another browser
            </button>
          </section>
        )}
      </div>

      <button
        ref={custRef}
        type="button"
        className="ntp-customize"
        onClick={() => setCustOpen((o) => !o)}
      >
        <Icon name="sliders" size={15} /> Customize
      </button>
      <Popover open={custOpen} onClose={() => setCustOpen(false)} anchor={custRef} align="end" width={320} label="Customize New Tab" className="ntp-custom-pop">
        <div className="pop-head">
          <h3>New Tab</h3>
        </div>
        <div className="ntp-custom">
          <Segmented
            label="Layout"
            value={settings.ntpLayout}
            options={[
              { value: "full", label: "Full" },
              { value: "minimal", label: "Minimal" },
            ]}
            onChange={(ntpLayout) => void update({ ntpLayout })}
          />
          <div className="eyebrow">Background</div>
          <div className="bg-picker">
            {NEW_TAB_BACKGROUNDS.map((b) => (
              <button key={b.id} type="button" className={`bg-option${settings.newTabBackground === b.id ? " is-on" : ""}`} onClick={() => void update({ newTabBackground: b.id })}>
                <span className="bg-preview" data-bg={b.id} />
                {b.label}
              </button>
            ))}
          </div>
          <div className="eyebrow">Widgets</div>
          {NTP_WIDGETS.map((w) => (
            <label key={w} className="ntp-custom-row">
              <span>{WIDGET_LABEL[w]}</span>
              <Toggle label={WIDGET_LABEL[w] ?? w} checked={has(w)} onChange={(on) => void update({ ntpWidgets: toggleItem(settings.ntpWidgets, w, on, NTP_WIDGETS) })} />
            </label>
          ))}
        </div>
      </Popover>
    </div>
  );
}

function AddShortcut({ onAdd }: { onAdd: (title: string, url: string) => void }) {
  const [open, setOpen] = useState(false);
  const [url, setUrl] = useState("");
  const [title, setTitle] = useState("");
  const [error, setError] = useState<string | null>(null);
  if (!open) {
    return (
      <button type="button" className="ntp-tile ntp-add" onClick={() => setOpen(true)} title="Add a shortcut">
        <span className="tile-icon">
          <Icon name="plus" size={20} />
        </span>
        <span className="tile-title">Add shortcut</span>
      </button>
    );
  }
  return (
    <form
      className="ntp-tile ntp-add-form"
      onSubmit={(e) => {
        e.preventDefault();
        const u = normalizeSiteUrl(url);
        if (!u) {
          setError("Enter a web address");
          return;
        }
        onAdd(title.trim() || hostOf(u), u);
        setOpen(false);
        setUrl("");
        setTitle("");
        setError(null);
      }}
    >
      <input className="input" placeholder="Address" value={url} autoFocus onChange={(e) => setUrl(e.target.value)} aria-label="Shortcut address" />
      <input className="input" placeholder="Name (optional)" value={title} onChange={(e) => setTitle(e.target.value)} aria-label="Shortcut name" />
      {error && <span className="field-error">{error}</span>}
      <div className="row-actions">
        <button type="submit" className="btn btn-primary btn-sm">Add</button>
        <button type="button" className="btn btn-ghost btn-sm" onClick={() => setOpen(false)}>Cancel</button>
      </div>
    </form>
  );
}

function Widget({ title, icon, action, style, children }: { title: string; icon: IconName; action?: ReactNode; style?: CSSProperties; children: ReactNode }) {
  return (
    <section className="ntp-widget" aria-label={title} style={style}>
      <header>
        <h2>
          <Icon name={icon} size={15} /> {title}
        </h2>
        {action}
      </header>
      <div className="ntp-widget-body">{children}</div>
    </section>
  );
}

function Row({ url, title }: { url: string; title: string }) {
  return (
    <button type="button" className="ntp-row" onClick={() => go(url)} title={url}>
      <Favicon src={null} fallback={(hostOf(url).charAt(0) || "•").toUpperCase()} seed={hostOf(url)} size={18} />
      <span className="truncate row-title">{title}</span>
      <span className="truncate row-sub">{hostOf(url)}</span>
    </button>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="stat">
      <div className="stat-value num">{value}</div>
      <div className="stat-label">{label}</div>
    </div>
  );
}

function GamingWidget({ style }: { style?: CSSProperties }) {
  const gaming = useStore((s) => s.settings.gamingMode);
  return (
    <section className={`ntp-widget ntp-gaming${gaming ? " is-on" : ""}`} style={style}>
      <header>
        <h2>
          <Icon name="bolt" size={15} filled={gaming} /> Gaming Mode
        </h2>
        <Toggle label="Gaming Mode" checked={gaming} onChange={(on) => void attempt(() => api.gamingSet(on))} />
      </header>
      <p className="dim">
        {gaming
          ? "On: background tabs sleep, animations are reduced and downloads wait until you're done."
          : "Frees memory for your game: background tabs sleep, animations calm down, downloads wait."}
      </p>
      <button type="button" className="link-btn" onClick={() => void attempt(() => api.runCommand({ type: "openGaming" }))}>
        Open the dashboard
      </button>
    </section>
  );
}

function AiWidget({ style }: { style?: CSSProperties }) {
  const [q, setQ] = useState("");
  return (
    <section className="ntp-widget ntp-ai" style={style}>
      <header>
        <h2>
          <Icon name="sparkle" size={15} /> Ask AI
        </h2>
      </header>
      <form
        className="ntp-ai-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (!q.trim()) return;
          setAiOpen(true);
          void sendMessage(q, false);
          setQ("");
        }}
      >
        <input className="input" value={q} onChange={(e) => setQ(e.target.value)} placeholder="Ask anything (no page is shared)" aria-label="Ask the AI assistant" />
        <button type="submit" className="btn btn-primary btn-sm" disabled={!q.trim()}>
          <Icon name="send" size={14} />
        </button>
      </form>
    </section>
  );
}
