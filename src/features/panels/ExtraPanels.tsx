import { useCallback, useEffect, useMemo, useState } from "react";

import { Favicon } from "../../components/ui/Favicon";
import { Icon } from "../../components/ui/Icon";
import { EmptyState } from "../../components/ui/Section";
import { Button, IconButton, cx } from "../../components/ui/controls";
import { hostOf } from "../../lib/format";
import { api, errorMessage } from "../../lib/ipc";
import { readLocal, writeLocal } from "../../lib/storage";
import { activeTab, attempt, toast, useStore } from "../../lib/store";
import type { MediaStatus, Tab } from "../../lib/types";
import { isInternalUrl } from "../../lib/urls";
import { dayKey, endSession, formatDuration, loadConfig, loadUsage, saveConfig, sessionActive, startSession, topSites, totalFor } from "../focus/focus";
import type { FocusConfig } from "../focus/focus";
import { exportText, noteEntries, searchNotes } from "./notes";

// ---- Notes ---------------------------------------------------------------------------------------

type NoteScope = "site" | "all" | "list";

/** Quick notes: one for the current site, and one scratchpad for everything. Saved on this computer as you type. */
export function NotesPanel() {
  const tab = useStore(activeTab);
  const site = tab && !isInternalUrl(tab.url) && !tab.private ? hostOf(tab.url) : null;
  const [scope, setScope] = useState<NoteScope>(site ? "site" : "all");
  const key = scope === "site" && site ? `notes.site.${site}` : "notes.all";
  const [text, setText] = useState(() => readLocal<string>(key, ""));
  const [loadedKey, setLoadedKey] = useState(key);
  if (loadedKey !== key) {
    setLoadedKey(key);
    setText(readLocal<string>(key, ""));
  }
  useEffect(() => {
    const t = window.setTimeout(() => writeLocal(key, text), 300);
    return () => window.clearTimeout(t);
  }, [key, text]);

  return (
    <>
      <div className="panel-tabs" role="tablist">
        <button type="button" role="tab" aria-selected={scope === "site"} className={cx(scope === "site" && "is-on")} disabled={!site} onClick={() => setScope("site")}>
          {site ?? "This site"}
        </button>
        <button type="button" role="tab" aria-selected={scope === "all"} className={cx(scope === "all" && "is-on")} onClick={() => setScope("all")}>
          Scratchpad
        </button>
        <button type="button" role="tab" aria-selected={scope === "list"} className={cx(scope === "list" && "is-on")} onClick={() => setScope("list")}>
          All notes
        </button>
      </div>
      {scope === "list" ? <AllNotes /> : (
      <>
      <div className="panel-body notes-body">
        <textarea
          className="notes-area"
          value={text}
          placeholder={scope === "site" ? `Notes for ${site} — they come back whenever you visit it.` : "Anything you want to keep handy."}
          onChange={(e) => setText(e.target.value)}
          spellCheck
          aria-label="Notes"
        />
      </div>
      <div className="panel-foot">
        <span className="dim small">{text.trim() ? `${text.trim().split(/\s+/).length} words · saved on this computer` : "Saved on this computer"}</span>
        <Button size="sm" variant="ghost" icon="copy" disabled={!text} onClick={() => void navigator.clipboard.writeText(text).then(() => toast("Notes copied"))}>
          Copy
        </Button>
      </div>
      </>
      )}
    </>
  );
}

function storeSnapshot(): Record<string, string> {
  const out: Record<string, string> = {};
  try {
    for (const k of Object.keys(localStorage)) out[k] = localStorage.getItem(k) ?? "";
  } catch {
    /* storage unavailable */
  }
  return out;
}

/** Every saved note: search, delete one, export all as a text file. */
function AllNotes() {
  const [query, setQuery] = useState("");
  const [rev, setRev] = useState(0);
  // `rev` changes after a delete, so the list is read again.
  const entries = useMemo(() => (rev >= 0 ? noteEntries(storeSnapshot()) : []), [rev]);
  const shown = searchNotes(entries, query);
  return (
    <>
      <div className="panel-body notes-list">
        <input className="input" placeholder="Search notes" aria-label="Search notes" value={query} onChange={(e) => setQuery(e.target.value)} />
        {shown.length === 0 && <p className="dim small">{entries.length === 0 ? "No notes yet. Notes for a site come back whenever you visit it." : "No note matches."}</p>}
        {shown.map((e) => (
          <div key={e.key} className="note-item">
            <div className="note-head">
              <strong className="truncate">{e.site ?? "Scratchpad"}</strong>
              <Button
                size="sm"
                variant="ghost"
                icon="trash"
                onClick={() => {
                  try {
                    localStorage.removeItem(`pb:${e.key}`);
                  } catch {
                    /* ignore */
                  }
                  setRev((r) => r + 1);
                  toast(`Note for ${e.site ?? "the scratchpad"} deleted`);
                }}
              >
                Delete
              </Button>
            </div>
            <p className="dim small note-text">{e.text.slice(0, 280)}</p>
          </div>
        ))}
      </div>
      <div className="panel-foot">
        <span className="dim small">{entries.length} {entries.length === 1 ? "note" : "notes"} · on this computer</span>
        <Button
          size="sm"
          variant="ghost"
          icon="download"
          disabled={entries.length === 0}
          onClick={() => void attempt(() => api.saveTextFile("Project Browser notes.txt", exportText(entries))).then((p) => p && toast(`Notes saved to ${p}`))}
        >
          Export
        </Button>
      </div>
    </>
  );
}

// ---- Media hub -------------------------------------------------------------------------------------

function fmtTime(s: number): string {
  if (!Number.isFinite(s) || s <= 0) return "0:00";
  const m = Math.floor(s / 60);
  return `${m}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
}

function ActiveMedia({ tab }: { tab: Tab }) {
  const [status, setStatus] = useState<MediaStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const run = useCallback(
    (action: Parameters<typeof api.mediaControl>[1]) =>
      api
        .mediaControl(tab.id, action)
        .then((s) => {
          setStatus(s);
          setError(null);
        })
        .catch((e) => setError(errorMessage(e))),
    [tab.id],
  );
  useEffect(() => {
    void run({ action: "status" });
    const t = window.setInterval(() => void run({ action: "status" }), 2000);
    return () => window.clearInterval(t);
  }, [run]);

  if (error) return <p className="dim small">{error}</p>;
  if (!status) return <p className="dim small">Looking for media…</p>;
  if (!status.media) return <p className="dim small">Nothing to play on this page.</p>;
  const rates = [0.75, 1, 1.25, 1.5, 2, 3];
  return (
    <div className="media-card">
      <div className="media-now">
        <Favicon src={tab.favicon} fallback={hostOf(tab.url).charAt(0).toUpperCase()} size={18} />
        <span className="truncate">{tab.title || tab.url}</span>
      </div>
      <div className="media-progress" aria-label="Position">
        <div style={{ width: `${status.duration ? Math.min(100, ((status.time ?? 0) / status.duration) * 100) : 0}%` }} />
      </div>
      <div className="media-times dim small">
        <span>{fmtTime(status.time ?? 0)}</span>
        <span>{status.duration ? fmtTime(status.duration) : "live"}</span>
      </div>
      <div className="media-controls">
        <IconButton icon="back" label="Back 10 seconds" onClick={() => void run({ action: "seek", seconds: -10 })} />
        <button type="button" className="media-play" aria-label={status.paused ? "Play" : "Pause"} onClick={() => void run({ action: "toggle" })}>
          <Icon name={status.paused ? "play" : "pause"} size={20} />
        </button>
        <IconButton icon="forward" label="Forward 10 seconds" onClick={() => void run({ action: "seek", seconds: 10 })} />
        <IconButton icon="loop" label={status.loop ? "Loop is on" : "Loop"} active={!!status.loop} onClick={() => void run({ action: "loop" })} />
        {status.video && <IconButton icon="pip" label="Picture in picture" active={!!status.pip} onClick={() => void run({ action: "pictureInPicture" })} />}
      </div>
      <div className="media-rates" role="group" aria-label="Playback speed">
        {rates.map((r) => (
          <button key={r} type="button" className={cx("chip", Math.abs((status.rate ?? 1) - r) < 0.01 && "is-on")} onClick={() => void run({ action: "speed", rate: r })}>
            {r}×
          </button>
        ))}
      </div>
    </div>
  );
}

/** Every tab making sound, with controls for the page in front. */
export function MediaPanel() {
  const tabs = useStore((s) => s.browser.tabs);
  const tab = useStore(activeTab);
  const playing = tabs.filter((t) => t.audible || t.muted);
  const web = !!tab && !isInternalUrl(tab.url) && tab.lifecycle === "live";
  return (
    <>
      <div className="panel-body">
        <h3 className="panel-subhead">This page</h3>
        {web ? <ActiveMedia key={tab.id} tab={tab} /> : <p className="dim small">Open a page with audio or video.</p>}
        <h3 className="panel-subhead">Playing in this window</h3>
        {playing.length === 0 ? (
          <EmptyState icon="headphones" title="Nothing is playing">Tabs that make sound show up here.</EmptyState>
        ) : (
          playing.map((t) => (
            <div key={t.id} className="panel-row">
              <button type="button" className="panel-link" onClick={() => void attempt(() => api.tabActivate(t.id))}>
                <Favicon src={t.favicon} fallback={hostOf(t.url).charAt(0).toUpperCase()} size={16} />
                <span className="panel-text">
                  <span className="truncate">{t.title || t.url}</span>
                  <span className="panel-sub truncate">{hostOf(t.url)}</span>
                </span>
              </button>
              <IconButton icon={t.muted ? "mute" : "sound"} label={t.muted ? "Unmute" : "Mute"} size={14} onClick={() => void attempt(() => api.tabMute(t.id, !t.muted))} />
            </div>
          ))
        )}
      </div>
    </>
  );
}

// ---- Focus -----------------------------------------------------------------------------------------

export function useFocusConfig(): [FocusConfig, (c: FocusConfig) => void] {
  const [cfg, setCfg] = useState<FocusConfig>(loadConfig);
  useEffect(() => {
    const reload = () => setCfg(loadConfig());
    window.addEventListener("pb-focus", reload);
    window.addEventListener("storage", reload);
    return () => {
      window.removeEventListener("pb-focus", reload);
      window.removeEventListener("storage", reload);
    };
  }, []);
  return [cfg, (c) => saveConfig(c)];
}

export function useNow(ms = 1000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), ms);
    return () => window.clearInterval(t);
  }, [ms]);
  return now;
}

export function FocusTimer({ cfg, now }: { cfg: FocusConfig; now: number }) {
  if (cfg.session && sessionActive(cfg, now)) {
    const left = Math.max(0, cfg.session.until - now);
    const total = cfg.session.until - cfg.session.startedAt;
    return (
      <div className="focus-timer is-on">
        <div className="focus-ring" style={{ ["--p" as string]: `${(1 - left / total) * 100}%` }}>
          <span>{`${Math.floor(left / 60000)}:${String(Math.floor((left % 60000) / 1000)).padStart(2, "0")}`}</span>
        </div>
        <p className="small">Distracting sites are blocked until the timer ends.</p>
        <Button size="sm" variant="ghost" onClick={() => endSession()}>
          End early
        </Button>
      </div>
    );
  }
  return (
    <div className="focus-timer">
      <p className="small dim">Block distracting sites for a while:</p>
      <div className="focus-presets">
        {[15, 25, 50, 90].map((m) => (
          <Button key={m} size="sm" variant={m === 25 ? "primary" : "secondary"} onClick={() => (startSession(m), toast(`Focus session started: ${m} minutes`))}>
            {m} min
          </Button>
        ))}
      </div>
    </div>
  );
}

export function FocusPanel() {
  const [cfg] = useFocusConfig();
  const now = useNow();
  const usage = loadUsage();
  const today = dayKey(now);
  const top = topSites(usage, today, 6);
  const max = top[0]?.seconds ?? 1;
  return (
    <>
      <div className="panel-body">
        <FocusTimer cfg={cfg} now={now} />
        <h3 className="panel-subhead">Today · {formatDuration(totalFor(usage, today))}</h3>
        {top.length === 0 ? (
          <p className="dim small">Time on each site shows up here as you browse.</p>
        ) : (
          top.map((t) => (
            <div key={t.site} className="usage-row">
              <span className="truncate">{t.site}</span>
              <span className="usage-bar">
                <span style={{ width: `${(t.seconds / max) * 100}%` }} />
              </span>
              <span className="dim small">{formatDuration(t.seconds)}</span>
            </div>
          ))
        )}
      </div>
      <div className="panel-foot">
        <Button size="sm" variant="ghost" icon="hourglass" onClick={() => void attempt(() => api.tabNew("browser://focus"))}>
          Dashboard and limits
        </Button>
      </div>
    </>
  );
}
