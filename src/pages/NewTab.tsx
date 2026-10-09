import { useEffect, useState } from "react";
import type { FormEvent, ReactNode } from "react";

import { Icon } from "../components/ui/Icon";
import type { IconName } from "../components/ui/Icon";
import { Favicon } from "../components/ui/Favicon";
import { formatBytes, formatPercent, hostOf } from "../lib/format";
import { api } from "../lib/ipc";
import { usePerfOnce } from "../lib/perf";
import { backgroundOrDefault } from "../lib/settings";
import { attempt, useStore } from "../lib/store";
import type { NtpData } from "../lib/types";
import { openImportWizard } from "../lib/ui";

function greeting(now = new Date()): string {
  const h = now.getHours();
  if (h < 5) return "Burning the midnight oil";
  if (h < 12) return "Good morning";
  if (h < 18) return "Good afternoon";
  return "Good evening";
}

const go = (url: string) => void attempt(() => api.navigate(url));

export function NewTab() {
  const settings = useStore((s) => s.settings);
  const profile = useStore((s) => s.profile);
  const bookmarksRev = useStore((s) => s.bookmarksRev);
  const closedCount = useStore((s) => s.browser.closedCount);
  const [data, setData] = useState<NtpData | null>(null);
  const [query, setQuery] = useState("");
  const sample = usePerfOnce("ntp", settings.showRamOnNewTab);

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

  const hasHistory = !!data && (data.topSites.length > 0 || data.continueBrowsing.length > 0);

  return (
    <div className="ntp" data-bg={backgroundOrDefault(settings.newTabBackground)}>
      <div className="ntp-bg" aria-hidden="true" />
      <div className="ntp-inner">
        <header className="ntp-hero">
          <div className="ntp-mark" aria-hidden="true">
            <svg viewBox="0 0 32 32" width="44" height="44">
              <defs>
                <linearGradient id="ntp-logo" x1="0" y1="0" x2="1" y2="1">
                  <stop offset="0" stopColor="var(--accent-hi)" />
                  <stop offset="1" stopColor="var(--accent-lo)" />
                </linearGradient>
              </defs>
              <rect x="2" y="2" width="28" height="28" rx="9" fill="url(#ntp-logo)" />
              <path d="M11 22V10h6.2a3.6 3.6 0 010 7.2H11" fill="none" stroke="var(--on-accent)" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </div>
          <h1>{greeting()}{profile ? `, ${profile.name}` : ""}</h1>
          <form className="ntp-search" onSubmit={submit} role="search">
            <Icon name="search" size={18} />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search the web or type an address"
              aria-label="Search the web or type an address"
              spellCheck={false}
            />
            <kbd>Enter</kbd>
          </form>
        </header>

        {sample && (
          <div className="ntp-stats" aria-label="Browser resource usage">
            <Stat icon="memory" label="Browser memory" value={formatBytes(sample.totalMemory)} />
            <Stat icon="cpu" label="CPU" value={formatPercent(sample.totalCpu)} />
            <Stat icon="tabs" label="Tabs" value={`${sample.liveTabs} active · ${sample.sleepingTabs} sleeping`} />
            <button type="button" className="stat stat-link" onClick={() => void attempt(() => api.runCommand({ type: "openPerformance" }))}>
              <Icon name="gauge" size={16} />
              <span>Details</span>
              <Icon name="chevronRight" size={14} />
            </button>
          </div>
        )}

        {data && data.topSites.length > 0 && (
          <section className="ntp-section" aria-label="Most visited">
            <h2>Most visited</h2>
            <div className="ntp-tiles">
              {data.topSites.map((s) => (
                <button key={s.url} type="button" className="ntp-tile" onClick={() => go(s.url)} title={s.url}>
                  <span className="tile-icon">
                    <Favicon src={null} fallback={(hostOf(s.url).charAt(0) || "•").toUpperCase()} size={30} />
                  </span>
                  <span className="tile-title truncate">{s.title || hostOf(s.url)}</span>
                  <span className="tile-host truncate">{hostOf(s.url)}</span>
                </button>
              ))}
            </div>
          </section>
        )}

        <div className="ntp-columns">
          {data && data.continueBrowsing.length > 0 && (
            <ListSection title="Pick up where you left off" icon="history" items={data.continueBrowsing.map((r) => ({ key: r.url, title: r.title || hostOf(r.url), sub: hostOf(r.url), url: r.url }))} />
          )}
          {data && data.recentBookmarks.length > 0 && (
            <ListSection title="Recent bookmarks" icon="star" items={data.recentBookmarks.filter((b) => b.url).map((b) => ({ key: String(b.id), title: b.title || (b.url ?? ""), sub: hostOf(b.url ?? ""), url: b.url ?? "" }))} />
          )}
          {data && data.recentlyClosed.length > 0 && (
            <ListSection
              title="Recently closed"
              icon="tabs"
              action={<button type="button" className="link-btn" onClick={() => void attempt(() => api.tabReopen())}>Reopen last</button>}
              items={data.recentlyClosed.map((c, i) => ({ key: `${c.url}-${i}`, title: c.title || hostOf(c.url), sub: hostOf(c.url), url: c.url }))}
            />
          )}
        </div>

        {data && !hasHistory && data.recentBookmarks.length === 0 && (
          <section className="ntp-welcome">
            <h2>Your start page fills in as you browse</h2>
            <p className="dim">Sites you visit often and your bookmarks will appear here. Already use Chrome, Edge or Brave? Bring your bookmarks and history across.</p>
            <button type="button" className="btn btn-primary btn-md" onClick={openImportWizard}>
              <Icon name="import" size={16} /> Import from another browser
            </button>
          </section>
        )}

        <GamingCard />
      </div>
    </div>
  );
}

function Stat({ icon, label, value }: { icon: IconName; label: string; value: string }) {
  return (
    <div className="stat">
      <Icon name={icon} size={16} />
      <div>
        <div className="stat-value">{value}</div>
        <div className="stat-label">{label}</div>
      </div>
    </div>
  );
}

function ListSection({ title, icon, items, action }: { title: string; icon: IconName; items: { key: string; title: string; sub: string; url: string }[]; action?: ReactNode }) {
  return (
    <section className="ntp-list" aria-label={title}>
      <header>
        <h2><Icon name={icon} size={15} /> {title}</h2>
        {action}
      </header>
      {items.map((it) => (
        <button key={it.key} type="button" className="ntp-row" onClick={() => go(it.url)} title={it.url}>
          <Favicon src={null} fallback={(it.sub.charAt(0) || "•").toUpperCase()} size={18} />
          <span className="truncate row-title">{it.title}</span>
          <span className="truncate row-sub">{it.sub}</span>
        </button>
      ))}
    </section>
  );
}

function GamingCard() {
  const gaming = useStore((s) => s.settings.gamingMode);
  return (
    <section className={`ntp-gaming${gaming ? " is-on" : ""}`}>
      <div className="ng-icon"><Icon name="bolt" size={22} filled={gaming} /></div>
      <div className="ng-text">
        <h2>{gaming ? "Gaming Mode is on" : "Gaming Mode"}</h2>
        <p className="dim">
          {gaming
            ? "Background tabs are sleeping, animations are reduced and downloads wait until you're done."
            : "Frees memory for your game: puts background tabs to sleep, reduces animations and holds downloads."}
        </p>
      </div>
      <div className="ng-actions">
        <button type="button" className="btn btn-secondary btn-md" onClick={() => void attempt(() => api.runCommand({ type: "openGaming" }))}>Dashboard</button>
        <button type="button" className={`btn btn-md ${gaming ? "btn-secondary" : "btn-primary"}`} onClick={() => void attempt(() => api.gamingSet(!gaming))}>
          {gaming ? "Turn off" : "Turn on"}
        </button>
      </div>
    </section>
  );
}
