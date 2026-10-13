import { MetricCard, RingGauge } from "../components/perf/Gauges";
import { Badge, Button, Toggle } from "../components/ui/controls";
import { Favicon } from "../components/ui/Favicon";
import { Icon } from "../components/ui/Icon";
import { Card, EmptyState, PageHeader, SettingRow } from "../components/ui/Section";
import { formatBytes, formatPercent } from "../lib/format";
import { api } from "../lib/ipc";
import { usePerf } from "../lib/perf";
import { attempt, useStore } from "../lib/store";
import { summarizeTabs } from "../lib/tabs";
import { monogram, tabTitle } from "../lib/urls";
import { useSettings } from "../lib/useSettings";

export default function Gaming() {
  const { settings, update } = useSettings();
  const tabs = useStore((s) => s.browser.tabs);
  const activeId = useStore((s) => s.browser.activeId);
  const downloads = useStore((s) => s.downloads);
  const { sample, history } = usePerf("gaming-dashboard");

  const gaming = settings.gamingMode;
  const states = summarizeTabs(tabs);
  const background = tabs.filter((t) => t.id !== activeId && t.lifecycle === "live");
  const heavy = tabs.filter((t) => t.heavy);
  const held = downloads.filter((d) => d.pausedByGaming).length;
  const cpu = sample?.totalCpu ?? null;

  return (
    <div className="page">
      <PageHeader icon="gamepad" title="Gaming dashboard" subtitle="Keep the browser light while you play. Everything here is a real reading or a real action." />

      <section className={`gaming-hero${gaming ? " is-on" : ""}`}>
        <div className="gh-glow" aria-hidden="true" />
        <div className="gh-text">
          <Badge tone={gaming ? "accent" : "neutral"}>{gaming ? "Active" : "Off"}</Badge>
          <h2>{gaming ? "Gaming Mode is on" : "Gaming Mode is off"}</h2>
          <p className="dim">
            {gaming
              ? `Background tabs ${settings.gamingBackgroundAction === "unload" ? "are unloaded" : "go to sleep"} after 10 seconds, ${settings.gamingPauseDownloads ? "downloads are on hold, " : ""}${settings.gamingReduceAnimations ? "animations are subtler, " : ""}and sampling slows to every 5 seconds.`
              : "Turn it on before you launch a game. Your tabs stay safe: sleeping tabs wake instantly."}
          </p>
        </div>
        <Button variant={gaming ? "secondary" : "primary"} icon="bolt" onClick={() => void attempt(() => api.gamingSet(!gaming))}>
          {gaming ? "Turn off" : "Turn on"}
        </Button>
      </section>

      <div className="gauge-row">
        <Card className="gauge-card">
          <RingGauge percent={cpu} label="CPU" value={sample ? formatPercent(cpu, 0) : "…"} />
          <RingGauge percent={sample?.gpu ?? null} label="GPU" value={sample ? (sample.gpu == null ? "n/a" : formatPercent(sample.gpu, 0)) : "…"} />
        </Card>
        <MetricCard title="Browser memory" icon={<Icon name="memory" size={15} />} value={sample ? formatBytes(sample.totalMemory) : "…"} sub={sample ? `${sample.liveTabs} active · ${sample.sleepingTabs} sleeping · ${sample.unloadedTabs} not loaded` : "Sampling…"} values={history.map((h) => h.totalMemory)} />
      </div>

      <Card title="Free up resources" subtitle="One-click actions on your open tabs. The tab you're using is never touched.">
        <div className="action-row">
          <Button icon="moon" disabled={background.length === 0} onClick={() => background.forEach((t) => void attempt(() => api.tabSleep(t.id, false)))}>
            Sleep {background.length} background tab{background.length === 1 ? "" : "s"}
          </Button>
          <Button icon="trash" disabled={background.length === 0} onClick={() => background.forEach((t) => void attempt(() => api.tabSleep(t.id, true)))}>
            Unload them completely
          </Button>
          <span className="dim small">Tabs playing audio or showing a permission request are skipped.</span>
        </div>
        <div className="stat-chips">
          <Badge tone="good">{states.live} active</Badge>
          <Badge tone="accent">{states.sleeping} sleeping</Badge>
          <Badge>{states.unloaded} not loaded</Badge>
          {held > 0 && <Badge tone="warn">{held} download{held === 1 ? "" : "s"} on hold</Badge>}
        </div>
      </Card>

      <Card title="Heaviest tabs" subtitle={`Flagged above ${settings.heavyTabMemoryMb} MB of memory or sustained high CPU.`}>
        {heavy.length === 0 ? (
          <EmptyState icon="check" title="Nothing heavy right now">No tab is using an unusual amount of memory or CPU.</EmptyState>
        ) : (
          <div className="list">
            {heavy.map((t) => {
              const s = sample?.tabs.find((x) => x.id === t.id);
              return (
                <div key={t.id} className="list-row">
                  <Favicon src={t.favicon} fallback={monogram(t)} size={16} />
                  <button type="button" className="row-link truncate" onClick={() => void attempt(() => api.tabActivate(t.id))}>{tabTitle(t)}</button>
                  <span className="row-host">{s ? `${formatBytes(s.memory)} · ${formatPercent(s.cpu, 0)} CPU` : ""}</span>
                  <span className="row-actions-inline">
                    {t.id !== activeId && t.lifecycle === "live" && <Button size="sm" onClick={() => void attempt(() => api.tabSleep(t.id, false))}>Sleep</Button>}
                    <Button size="sm" variant="ghost" onClick={() => void attempt(() => api.tabClose(t.id))}>Close</Button>
                  </span>
                </div>
              );
            })}
          </div>
        )}
      </Card>

      <Card title="Gaming Mode options">
        <SettingRow title="Background tabs" description="What happens to tabs you aren't looking at.">
          <div className="seg-inline">
            <Button size="sm" variant={settings.gamingBackgroundAction === "sleep" ? "primary" : "secondary"} onClick={() => void update({ gamingBackgroundAction: "sleep" })}>Sleep</Button>
            <Button size="sm" variant={settings.gamingBackgroundAction === "unload" ? "primary" : "secondary"} onClick={() => void update({ gamingBackgroundAction: "unload" })}>Unload</Button>
          </div>
        </SettingRow>
        <SettingRow title="Hold downloads"><Toggle label="Hold downloads" checked={settings.gamingPauseDownloads} onChange={(v) => void update({ gamingPauseDownloads: v })} /></SettingRow>
        <SettingRow title="Reduce animations"><Toggle label="Reduce animations" checked={settings.gamingReduceAnimations} onChange={(v) => void update({ gamingReduceAnimations: v })} /></SettingRow>
        <SettingRow title="Lower memory priority for background tabs"><Toggle label="Low memory target" checked={settings.gamingLowMemoryTarget} onChange={(v) => void update({ gamingLowMemoryTarget: v })} /></SettingRow>
      </Card>
    </div>
  );
}
