import { useState } from "react";

import { Badge, Button, Select, Toggle } from "../../components/ui/controls";
import { Card, Notice, SettingRow } from "../../components/ui/Section";
import { api } from "../../lib/ipc";
import { attempt, toast } from "../../lib/store";
import type { ProbeResult, ProxyKind, TunnelConfig, TunnelState } from "../../lib/types";
import { useLoad } from "../../lib/useLoad";

function stateText(state: TunnelState, running: TunnelConfig | null): { tone: "good" | "warn" | "neutral"; label: string; detail: string } {
  switch (state.kind) {
    case "notSetUp":
      return { tone: "neutral", label: "Not set up", detail: "No proxy has been entered. Nothing is routed anywhere." };
    case "off":
      return { tone: "neutral", label: "Off", detail: "A proxy is saved but switched off. This browser connects directly." };
    case "restartNeeded":
      return {
        tone: "warn",
        label: "Restart needed",
        detail: state.turningOn ? "Your settings are saved, but the running browser still connects directly. Restart to route its traffic through the proxy." : "The running browser still uses the proxy. Restart to go back to direct connections.",
      };
    case "active":
      return { tone: "good", label: "Proxy active (this browser only)", detail: `This browser was started with ${running?.host}:${running?.port} as its proxy. Other programs are not affected.` };
  }
}

function probeText(r: ProbeResult): { tone: "good" | "warn" | "bad"; text: string } {
  if (r.kind === "works") return { tone: "good", text: `The proxy answered and agreed to connect (${r.millis} ms). That says it works; it does not say where its traffic comes out or who runs it.` };
  if (r.kind === "refused") return { tone: "warn", text: r.reason };
  return { tone: "bad", text: r.reason };
}

export function TunnelSettings() {
  const loaded = useLoad(() => api.tunnelState(), 0);
  const view = loaded.data;
  const [draft, setDraft] = useState<TunnelConfig | null>(null);
  const [probe, setProbe] = useState<ProbeResult | null>(null);
  const [busy, setBusy] = useState(false);

  if (loaded.error && !view) return <Notice tone="bad">{loaded.error}</Notice>;
  if (!view) return null;
  const cfg = draft ?? view.config;
  const dirty = draft !== null && JSON.stringify(draft) !== JSON.stringify(view.config);
  const status = stateText(view.state, view.running);
  const edit = (changes: Partial<TunnelConfig>) => setDraft({ ...cfg, ...changes });

  const save = async () => {
    setBusy(true);
    const next = await attempt(() => api.tunnelSet(cfg));
    setBusy(false);
    if (next) {
      setDraft(null);
      toast("Proxy settings saved");
      loaded.reload();
    }
  };
  const test = async () => {
    setBusy(true);
    const r = await attempt(() => api.tunnelProbe(cfg));
    setBusy(false);
    if (r) setProbe(r);
  };

  return (
    <>
      <Card title="Project Tunnel" subtitle="Send this browser's traffic through a proxy server that you provide.">
        <p className="setup-stage">
          <Badge tone={status.tone}>{status.label}</Badge> {status.detail}
        </p>
        <Notice tone="warn">
          This is not a system-wide VPN. It covers the pages in this browser only, and only as far as the proxy's operator can be trusted: they can see which
          sites you visit. No servers, locations or accounts come with the browser.
        </Notice>
      </Card>

      <Card title="Browser proxy">
        <SettingRow title="Type">
          <Select
            label="Proxy type"
            value={cfg.kind}
            options={[
              { value: "http", label: "HTTP or HTTPS proxy" },
              { value: "socks5", label: "SOCKS5 proxy" },
            ]}
            onChange={(kind: ProxyKind) => edit({ kind })}
          />
        </SettingRow>
        <div className="row-actions">
          <input className="input" aria-label="Proxy host" placeholder="proxy.example.com" spellCheck={false} value={cfg.host} onChange={(e) => edit({ host: e.target.value })} />
          <input
            className="input"
            aria-label="Proxy port"
            placeholder="8080"
            inputMode="numeric"
            value={cfg.port === 0 ? "" : String(cfg.port)}
            onChange={(e) => edit({ port: Math.min(65535, Number(e.target.value.replace(/\D/g, "")) || 0) })}
          />
        </div>
        <SettingRow title="Use this proxy" description="Applied when the browser starts.">
          <Toggle label="Use this proxy" checked={cfg.enabled} onChange={(enabled) => edit({ enabled })} />
        </SettingRow>
        <div className="row-actions">
          <Button variant="primary" disabled={busy || !dirty} onClick={() => void save()}>Save</Button>
          <Button disabled={busy || !cfg.host.trim() || cfg.port === 0} onClick={() => void test()}>Test the proxy</Button>
          {view.state.kind === "restartNeeded" && !dirty && (
            <Button onClick={() => void attempt(() => api.tunnelRestart())}>Restart now</Button>
          )}
        </div>
        {probe && <Notice tone={probeText(probe).tone}>{probeText(probe).text}</Notice>}
      </Card>

      <Card title="What this does and doesn't cover">
        <ul className="plain-list">
          <li>Pages, images, downloads and sign-ins in this browser go through the proxy once it is active. Other programs, Windows and updates do not.</li>
          <li>Web addresses are resolved by the proxy, not by your computer. Pages cannot use WebRTC to find your real address through a direct connection.</li>
          <li>If the proxy stops answering, pages fail to load. The browser does not fall back to a direct connection by itself; that has not been tested in this release.</li>
          <li>Proxies that ask for a user name and password are not supported: the engine reports a proxy's password prompt the same way as a web site's, so answering it could give the password to a web site.</li>
          <li>Pages on this computer (including the browser's own pages) never use the proxy.</li>
        </ul>
        <h4>Ways to tunnel traffic</h4>
        <ul className="plain-list">
          {view.providers.map((p) => (
            <li key={p.id}>
              <strong>{p.name}</strong> <Badge tone={p.availability === "available" ? "good" : "neutral"}>{p.availability === "available" ? "Available" : "Not available"}</Badge>
              <div className="dim small">{p.scope} {p.needs}</div>
            </li>
          ))}
        </ul>
      </Card>
    </>
  );
}
