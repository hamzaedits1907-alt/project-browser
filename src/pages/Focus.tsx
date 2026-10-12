import { useState } from "react";

import { Icon } from "../components/ui/Icon";
import { PageHeader } from "../components/ui/Section";
import { Button, IconButton } from "../components/ui/controls";
import { FocusTimer, useFocusConfig, useNow } from "../features/panels/ExtraPanels";
import { dayKey, formatDuration, loadUsage, siteOf, topSites, totalFor } from "../features/focus/focus";
import { api } from "../lib/ipc";
import { readLocal, writeLocal } from "../lib/storage";
import { attempt } from "../lib/store";

interface BlockedNote {
  host: string;
  reason: "focus" | "limit";
  at: number;
}

/** browser://focus: Screen Time, the Focus timer, blocked sites and daily limits. Also shown in place of a blocked site. */
export default function Focus() {
  const [cfg, save] = useFocusConfig();
  const now = useNow();
  const usage = loadUsage();
  const today = dayKey(now);
  const [blocked] = useState<BlockedNote | null>(() => {
    const b = readLocal<BlockedNote | null>("focus.blocked", null);
    writeLocal("focus.blocked", null);
    return b && now - b.at < 15_000 ? b : null;
  });
  const [newSite, setNewSite] = useState("");
  const [limitSite, setLimitSite] = useState("");
  const [limitMin, setLimitMin] = useState(30);

  const days = Array.from({ length: 7 }, (_, i) => dayKey(now - (6 - i) * 86_400_000));
  const dayTotals = days.map((d) => totalFor(usage, d));
  const maxDay = Math.max(60, ...dayTotals);
  const top = topSites(usage, today, 10);
  const maxSite = top[0]?.seconds ?? 1;

  const addBlocked = () => {
    const s = siteOf(newSite.trim().replace(/^https?:\/\//, "").split("/")[0] ?? "");
    if (!s || cfg.blocked.includes(s)) return;
    save({ ...cfg, blocked: [...cfg.blocked, s] });
    setNewSite("");
  };
  const addLimit = () => {
    const s = siteOf(limitSite.trim().replace(/^https?:\/\//, "").split("/")[0] ?? "");
    if (!s || limitMin <= 0) return;
    save({ ...cfg, limits: { ...cfg.limits, [s]: Math.min(24 * 60, Math.round(limitMin)) } });
    setLimitSite("");
  };

  return (
    <div className="page focus-page">
      <PageHeader icon="target" title="Focus & Screen Time" subtitle="Time on each site is counted only while this browser window is in front. It stays on this computer." />

      {blocked && (
        <div className="focus-blocked" role="alert">
          <Icon name="target" size={28} />
          <div>
            <h2>{blocked.host} is blocked {blocked.reason === "focus" ? "during your focus session" : "for the rest of today"}</h2>
            <p className="dim">{blocked.reason === "focus" ? "It opens again when the timer ends." : "You've used up the daily limit you set for it."}</p>
          </div>
        </div>
      )}

      <div className="focus-grid">
        <section className="card">
          <h3>Focus timer</h3>
          <FocusTimer cfg={cfg} now={now} />
        </section>

        <section className="card">
          <h3>Last 7 days</h3>
          <div className="week-chart" role="img" aria-label="Browsing time over the last seven days">
            {days.map((d, i) => (
              <div key={d} className="week-col" title={`${d}: ${formatDuration(dayTotals[i] ?? 0)}`}>
                <div className="week-bar" style={{ height: `${((dayTotals[i] ?? 0) / maxDay) * 100}%` }} />
                <span className="dim small">{new Date(`${d}T12:00:00`).toLocaleDateString(undefined, { weekday: "short" })}</span>
              </div>
            ))}
          </div>
          <p className="dim small">Today: {formatDuration(totalFor(usage, today))}</p>
        </section>

        <section className="card">
          <h3>Today by site</h3>
          {top.length === 0 && <p className="dim small">Nothing yet today.</p>}
          {top.map((t) => (
            <div key={t.site} className="usage-row">
              <span className="truncate">{t.site}</span>
              <span className="usage-bar">
                <span style={{ width: `${(t.seconds / maxSite) * 100}%` }} />
              </span>
              <span className="dim small">{formatDuration(t.seconds)}</span>
              {cfg.limits[t.site] ? <span className="chip is-on small">{cfg.limits[t.site]} min/day</span> : null}
            </div>
          ))}
        </section>

        <section className="card">
          <h3>Blocked during focus</h3>
          <div className="chips">
            {cfg.blocked.map((s) => (
              <span key={s} className="chip">
                {s}
                <IconButton icon="close" label={`Unblock ${s}`} size={12} onClick={() => save({ ...cfg, blocked: cfg.blocked.filter((b) => b !== s) })} />
              </span>
            ))}
          </div>
          <div className="inline-form">
            <input className="input" placeholder="Add a site, e.g. news.example.com" value={newSite} onChange={(e) => setNewSite(e.target.value)} onKeyDown={(e) => e.key === "Enter" && addBlocked()} aria-label="Site to block" />
            <Button onClick={addBlocked}>Add</Button>
          </div>
        </section>

        <section className="card">
          <h3>Daily limits</h3>
          <p className="dim small">When a site's time for the day is used up, it is blocked until tomorrow.</p>
          {Object.entries(cfg.limits).map(([s, m]) => (
            <div key={s} className="usage-row">
              <span className="truncate">{s}</span>
              <span className="dim small">
                {formatDuration((usage[today] ? Object.entries(usage[today]).filter(([k]) => k === s || k.endsWith(`.${s}`)).reduce((a, [, v]) => a + v, 0) : 0))} of {m} min
              </span>
              <IconButton
                icon="trash"
                label={`Remove the limit for ${s}`}
                size={14}
                onClick={() => {
                  const limits = { ...cfg.limits };
                  delete limits[s];
                  save({ ...cfg, limits });
                }}
              />
            </div>
          ))}
          <div className="inline-form">
            <input className="input" placeholder="Site" value={limitSite} onChange={(e) => setLimitSite(e.target.value)} aria-label="Site to limit" />
            <input className="input narrow" type="number" min={5} max={1440} step={5} value={limitMin} onChange={(e) => setLimitMin(Number(e.target.value))} aria-label="Minutes per day" />
            <span className="dim small">min/day</span>
            <Button onClick={addLimit}>Set</Button>
          </div>
        </section>
      </div>
      {blocked && (
        <p>
          <Button variant="ghost" icon="back" onClick={() => void attempt(() => api.nav("back"))}>
            Go back
          </Button>
        </p>
      )}
    </div>
  );
}
