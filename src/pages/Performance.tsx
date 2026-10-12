import { MetricCard } from "../components/perf/Gauges";
import { Badge, Button } from "../components/ui/controls";
import { Icon } from "../components/ui/Icon";
import { Card, EmptyState, Notice, PageHeader } from "../components/ui/Section";
import { formatBytes, formatDuration, formatPercent } from "../lib/format";
import { api } from "../lib/ipc";
import { usePerf } from "../lib/perf";
import { attempt, useStore } from "../lib/store";
import { monogram, tabTitle } from "../lib/urls";
import { Favicon } from "../components/ui/Favicon";

export default function Performance() {
  const { sample, history } = usePerf("performance-page");
  const tabs = useStore((s) => s.browser.tabs);
  const activeId = useStore((s) => s.browser.activeId);

  const stat = (id: number) => sample?.tabs.find((t) => t.id === id);
  const sleepable = tabs.filter((t) => t.id !== activeId && t.lifecycle === "live");

  return (
    <div className="page">
      <PageHeader
        icon="gauge"
        title="Performance"
        subtitle="Real readings from the browser's own processes. Sampled every 2 seconds while this page is open, then it stops."
        actions={
          <Button icon="moon" disabled={sleepable.length === 0} onClick={() => sleepable.forEach((t) => void attempt(() => api.tabSleep(t.id, false)))}>
            Sleep {sleepable.length} background tab{sleepable.length === 1 ? "" : "s"}
          </Button>
        }
      />

      <div className="metric-grid">
        <MetricCard title="Memory" icon={<Icon name="memory" size={15} />} value={sample ? formatBytes(sample.totalMemory) : "…"} sub={sample ? `Browser ${formatBytes(sample.shellMemory)} · Web content ${formatBytes(sample.webviewMemory)}` : "Sampling…"} values={history.map((h) => h.totalMemory)} />
        <MetricCard title="CPU" icon={<Icon name="cpu" size={15} />} value={sample ? formatPercent(sample.totalCpu, 1) : "…"} sub="Share of all cores, whole browser" values={history.map((h) => h.totalCpu)} max={100} />
        <MetricCard title="GPU" icon={<Icon name="gpu" size={15} />} value={sample ? (sample.gpu == null ? "n/a" : formatPercent(sample.gpu, 1)) : "…"} sub={sample && sample.gpu == null ? "GPU counters unavailable on this system" : "Engine utilisation, browser processes"} values={history.map((h) => h.gpu)} max={100} />
        <MetricCard
          title="Tabs"
          icon={<Icon name="tabs" size={15} />}
          value={String(sample?.tabCount ?? tabs.length)}
          sub={sample ? `${sample.liveTabs} active · ${sample.sleepingTabs} sleeping · ${sample.unloadedTabs} not loaded` : undefined}
        />
      </div>

      <p className="dim small metric-note">
        <strong>What “memory” means here:</strong> each process's <em>private working set</em> (the figure in Task Manager's Memory column), added up over the browser's own
        processes. Pages shared between processes are not counted twice, so this is neither the larger “working set” nor the “commit size”. “n/a” means Windows did not
        provide a reading; nothing on this page is estimated.
      </p>

      <Card title="Tabs" subtitle="Memory is per renderer process. Tabs that share a process show the same figure.">
        {tabs.length === 0 ? (
          <EmptyState icon="tabs" title="No tabs open" />
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>Tab</th>
                <th>State</th>
                <th className="num">Memory</th>
                <th className="num">CPU</th>
                <th aria-label="Actions" />
              </tr>
            </thead>
            <tbody>
              {tabs.map((t) => {
                const s = stat(t.id);
                return (
                  <tr key={t.id} className={t.id === activeId ? "is-current" : undefined}>
                    <td>
                      <button type="button" className="row-link tab-cell" onClick={() => void attempt(() => api.tabActivate(t.id))}>
                        <Favicon src={t.favicon} fallback={monogram(t)} size={16} />
                        <span className="truncate">{tabTitle(t)}</span>
                      </button>
                    </td>
                    <td>
                      {t.url.startsWith("browser://") ? <Badge>Browser page</Badge> : t.lifecycle === "sleeping" ? <Badge tone="accent">Sleeping</Badge> : t.lifecycle === "unloaded" ? <Badge>Not loaded</Badge> : t.id === activeId ? <Badge tone="good">Active</Badge> : <Badge>Running</Badge>}
                      {(s?.heavy || t.heavy) && <Badge tone="warn">Heavy</Badge>}
                    </td>
                    <td className="num">{s ? `${formatBytes(s.memory)}${s.shared ? " *" : ""}` : "—"}</td>
                    <td className="num">{s ? formatPercent(s.cpu, 1) : "—"}</td>
                    <td className="actions">
                      {t.id !== activeId && t.lifecycle === "live" && <Button size="sm" variant="ghost" onClick={() => void attempt(() => api.tabSleep(t.id, false))}>Sleep</Button>}
                      <Button size="sm" variant="ghost" onClick={() => void attempt(() => api.tabClose(t.id))}>Close</Button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
        {sample && !sample.tabAttribution && (
          <Notice tone="warn">
            Per-tab memory is unavailable: this computer's WebView2 runtime crashed while matching processes to tabs, so that lookup is switched off. The
            totals and the process list below are still real measurements.
          </Notice>
        )}
        <p className="dim small">* Shared process: figure includes other tabs and frames in the same process.</p>
      </Card>

      <Card title="Processes">
        {!sample ? (
          <p className="dim">Collecting the first sample…</p>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>Process</th>
                <th>Type</th>
                <th className="num">PID</th>
                <th className="num">Memory</th>
                <th className="num">CPU</th>
              </tr>
            </thead>
            <tbody>
              {[...sample.processes].sort((a, b) => b.memory - a.memory).map((p) => (
                <tr key={p.pid}>
                  <td className="truncate">{p.label}</td>
                  <td><Badge>{p.kind}</Badge></td>
                  <td className="num mono">{p.pid}</td>
                  <td className="num">{formatBytes(p.memory)}</td>
                  <td className="num">{formatPercent(p.cpu, 1)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>

      {sample && (
        <Card title="Startup">
          <dl className="about-list">
            <dt>Interface ready</dt>
            <dd>{formatDuration(sample.startup.shellReadyMs)} after launch</dd>
            <dt>First web page loaded</dt>
            <dd>{formatDuration(sample.startup.firstPageMs)} after launch</dd>
          </dl>
        </Card>
      )}
    </div>
  );
}
