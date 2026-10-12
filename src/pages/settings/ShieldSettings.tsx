import { useState } from "react";

import { Badge, Button, Spinner, Toggle } from "../../components/ui/controls";
import { Card, Notice, SettingRow } from "../../components/ui/Section";
import { useSettings } from "../../lib/useSettings";
import { formatCount, listStatus, ruleLines, updateMessage } from "../../features/shield/shieldModel";
import { api } from "../../lib/ipc";
import { attempt, toast } from "../../lib/store";
import { useLoad } from "../../lib/useLoad";
import type { ShieldList } from "../../lib/types";

const TONE = { good: "good", warn: "warn", bad: "bad", neutral: "neutral" } as const;

function ListRow({ list, now, busy, onToggle }: { list: ShieldList; now: number; busy: boolean; onToggle: (on: boolean) => void }) {
  const status = listStatus(list, now);
  return (
    <div className="shield-list-row">
      <div>
        <strong>{list.name}</strong> <span className="dim">{list.description}</span>
      </div>
      <Toggle label={`Use ${list.name}`} checked={list.enabled} disabled={busy} onChange={onToggle} />
      <div className="dim small">
        <Badge tone={TONE[status.tone]}>{list.state === "updating" ? "Updating" : list.enabled ? "On" : "Off"}</Badge> {status.text}
        {list.version && list.enabled ? ` · version ${list.version}` : ""}
      </div>
      <div className="dim small">
        {list.license}. <a href={list.homepage} target="_blank" rel="noreferrer noopener" data-selectable>{list.homepage}</a>
      </div>
    </div>
  );
}

export function ShieldSettings() {
  const loaded = useLoad(() => api.shieldState(), 0);
  const view = loaded.data;
  const [busy, setBusy] = useState(false);
  const [rulesDraft, setRulesDraft] = useState<string | null>(null);
  const [newSite, setNewSite] = useState("");
  const [testUrl, setTestUrl] = useState("");
  const [testPage, setTestPage] = useState("");
  const [testResult, setTestResult] = useState<string | null>(null);

  const run = async <T,>(action: () => Promise<T>, done?: string): Promise<T | undefined> => {
    setBusy(true);
    const r = await attempt(action);
    setBusy(false);
    if (r !== undefined) {
      if (done) toast(done);
      loaded.reload();
    }
    return r;
  };

  if (loaded.error && !view) return <Notice tone="bad">{loaded.error}</Notice>;
  if (!view) {
    return (
      <div className="ai-status" role="status">
        <Spinner size={16} /> <span>Loading…</span>
      </div>
    );
  }

  const draft = rulesDraft ?? view.userRules;
  const dirty = rulesDraft !== null && rulesDraft !== view.userRules;
  const stats = view.engine.stats;

  const update = async () => {
    setBusy(true);
    const summary = await attempt(() => api.shieldUpdate(true));
    setBusy(false);
    if (summary) toast(updateMessage(summary));
    loaded.reload();
  };

  const addSite = async () => {
    const value = newSite.trim();
    if (!value) return;
    if (await run(() => api.shieldSite(value, false), "Shield is off for that site")) setNewSite("");
  };

  const test = async () => {
    const r = await attempt(() => api.shieldTest(testUrl, testPage));
    if (r !== undefined) setTestResult(r);
  };

  return (
    <>
      <SponsorSkipCard />
      <Card
        title="Project Shield"
        subtitle="Blocks advertising and tracking requests before they are sent, using public filter lists. Switch it off for a site from the shield button next to the address bar."
      >
        <SettingRow title="Block ads and trackers" description="Applies to every tab, including private tabs. Pages on this computer are never filtered.">
          <Toggle label="Block ads and trackers" checked={view.enabled} disabled={busy} onChange={(enabled) => void run(() => api.shieldSet({ enabled }))} />
        </SettingRow>
        <p className="dim small">
          {formatCount(view.blockedSession)} requests blocked since the browser started · {formatCount(view.blockedLifetime)} in total (private tabs are not counted).
          Only counts are kept, never the addresses.
        </p>
        <Notice tone="info">
          What this does: it cancels matching scripts, images, frames and other sub-resources of a page and hides ad slots with the lists' site-specific element
          rules. It never blocks the page you are visiting, and it has not been compared with uBlock Origin or Brave Shields, so don't expect the same results.
        </Notice>
      </Card>

      <Card
        title="Filter lists"
        subtitle="Downloaded over HTTPS, checked, and only then put to use. If a download fails or looks wrong, the previous copy stays in use."
        actions={
          <Button icon="refresh" disabled={busy || view.updating} onClick={() => void update()}>
            {busy || view.updating ? "Updating…" : "Update lists"}
          </Button>
        }
      >
        <div className="shield-list">
          {view.lists.map((l) => (
            <ListRow key={l.id} list={l} now={view.now} busy={busy} onToggle={(on) => void run(() => api.shieldSet({ listId: l.id, listEnabled: on }))} />
          ))}
        </div>
        <p className="dim small">
          Rules in use: {formatCount(view.engine.rules)} ({formatCount(stats.blockRules)} block, {formatCount(stats.exceptionRules)} exceptions). Skipped because this
          engine does not implement them: {formatCount(stats.cosmeticSkipped)} element-hiding, {formatCount(stats.unsupportedSkipped)} using options such as popup, redirect or csp,
          {" "}{formatCount(stats.malformedSkipped)} unusable. A small built-in starter set of well-known ad networks is always active, even before any list has downloaded.
        </p>
      </Card>

      <Card title="Sites where the Shield is off" subtitle="The page and everything it loads are left alone on these sites (and their subdomains).">
        {view.disabledSites.length === 0 ? (
          <p className="dim">No exceptions.</p>
        ) : (
          <ul className="shield-sites">
            {view.disabledSites.map((host) => (
              <li key={host}>
                <span data-selectable>{host}</span>
                <Button size="sm" variant="ghost" disabled={busy} onClick={() => void run(() => api.shieldSite(host, true))}>
                  Turn on again
                </Button>
              </li>
            ))}
          </ul>
        )}
        <div className="row-actions">
          <input
            className="input"
            aria-label="Site to leave alone"
            placeholder="example.com"
            spellCheck={false}
            value={newSite}
            onChange={(e) => setNewSite(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void addSite()}
          />
          <Button disabled={busy || !newSite.trim()} onClick={() => void addSite()}>Add site</Button>
        </div>
      </Card>

      <Card
        title="Your rules"
        subtitle="One rule per line, in the same syntax as the public lists. Start a line with @@ to make an exception, for example @@||example.com^ lets everything from example.com through."
      >
        <textarea
          className="input shield-rules"
          aria-label="Your own filter rules"
          spellCheck={false}
          value={draft}
          placeholder={"||ads.example.com^\n@@||cdn.example.com^$script"}
          onChange={(e) => setRulesDraft(e.target.value)}
        />
        <div className="row-actions">
          <Button
            variant="primary"
            disabled={busy || !dirty}
            onClick={() => void run(() => api.shieldSet({ userRules: draft }), "Rules saved").then((r) => r && setRulesDraft(null))}
          >
            Save rules
          </Button>
          <span className="dim small">
            {formatCount(ruleLines(draft).length)} lines{!dirty ? ` · ${formatCount(view.userRulesAccepted)} understood` : ""}. Lines that can't be understood are ignored.
          </span>
        </div>
      </Card>

      <Card title="Test a request" subtitle="Asks the rule engine what it would do with one request. Nothing is loaded.">
        <div className="shield-test">
          <label className="field">
            <span>Request address</span>
            <input className="input" placeholder="https://ads.example.com/banner.js" spellCheck={false} value={testUrl} onChange={(e) => setTestUrl(e.target.value)} />
          </label>
          <label className="field">
            <span>On the page</span>
            <input className="input" placeholder="news.example" spellCheck={false} value={testPage} onChange={(e) => setTestPage(e.target.value)} />
          </label>
          <Button disabled={!testUrl.trim()} onClick={() => void test()}>Test</Button>
        </div>
        {testResult && <Notice tone={testResult.startsWith("Blocked") ? "warn" : "good"}>{testResult}</Notice>}
      </Card>

      <Card title="Attribution">
        <p className="dim">
          The public filter lists are maintained by the EasyList authors (easylist.to) and used under their licences. This browser does not modify them. Project
          Shield's matching engine is written for this browser; it reads the network-filter part of the Adblock Plus syntax.
        </p>
      </Card>
    </>
  );
}

/** SponsorSkip lives with the Shield: both remove things from pages the user did not come for. */
function SponsorSkipCard() {
  const { settings, update } = useSettings();
  return (
    <Card title="SponsorSkip for YouTube" subtitle="Jumps over sponsor reads, self-promotion and subscribe reminders inside videos, using SponsorBlock's community database.">
      <SettingRow
        title="Skip sponsored segments"
        description="For each video only the first four characters of a hash of its id are sent to sponsor.ajay.app, so the service cannot tell which video you watch. Off in private tabs. A notice with Undo appears whenever something is skipped."
      >
        <Toggle label="Skip sponsored segments" checked={settings.sponsorSkip} onChange={(sponsorSkip) => void update({ sponsorSkip })} />
      </SettingRow>
    </Card>
  );
}
