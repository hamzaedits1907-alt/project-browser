import { useState } from "react";

import { Button, Slider, Toggle } from "../../components/ui/controls";
import { Card, SettingRow } from "../../components/ui/Section";
import { parseHostList } from "../../lib/settings";
import { api } from "../../lib/ipc";
import { attempt } from "../../lib/store";
import { useSettings } from "../../lib/useSettings";

export function PerformanceSettings() {
  const { settings, update, updateSoon } = useSettings();
  const [hosts, setHosts] = useState(settings.neverSleepHosts.join("\n"));

  const saveHosts = () => void update({ neverSleepHosts: parseHostList(hosts) }).then(() => setHosts(parseHostList(hosts).join("\n")));

  return (
    <>
      <Card title="Sleeping tabs" subtitle="Idle background tabs are suspended so they stop using CPU and give memory back. They wake instantly when you click them.">
        <SettingRow title="Put inactive tabs to sleep">
          <Toggle label="Sleeping tabs" checked={settings.sleepingTabsEnabled} onChange={(sleepingTabsEnabled) => void update({ sleepingTabsEnabled })} />
        </SettingRow>
        <SettingRow title="Sleep after" description="How long a background tab can sit unused.">
          <Slider
            label="Minutes before a tab sleeps"
            min={1}
            max={180}
            value={settings.sleepAfterMinutes}
            format={(m) => (m >= 60 ? `${Math.floor(m / 60)} hr${m % 60 ? ` ${m % 60} min` : ""}` : `${m} min`)}
            onChange={(sleepAfterMinutes) => updateSoon({ sleepAfterMinutes })}
          />
        </SettingRow>
        <SettingRow title="Never put these sites to sleep" description="One address per line, for example music.youtube.com. Tabs that are playing audio are never put to sleep." stacked>
          <textarea className="input textarea" rows={4} value={hosts} placeholder="music.youtube.com" spellCheck={false} aria-label="Sites that never sleep" onChange={(e) => setHosts(e.target.value)} onBlur={saveHosts} />
          <div className="row-actions">
            <Button onClick={saveHosts}>Save list</Button>
          </div>
        </SettingRow>
      </Card>

      <Card title="Heavy tab warning">
        <SettingRow title="Flag tabs using more than" description="Tabs above this memory (or sustained high CPU) get a marker on the tab strip and in the Performance monitor.">
          <Slider
            label="Heavy tab memory threshold in megabytes"
            min={100}
            max={4000}
            step={50}
            value={settings.heavyTabMemoryMb}
            format={(v) => `${v} MB`}
            onChange={(heavyTabMemoryMb) => updateSoon({ heavyTabMemoryMb })}
          />
        </SettingRow>
      </Card>

      <Card title="Monitor">
        <SettingRow title="Performance monitor" description="Live memory, CPU and GPU per tab and process. It only samples while the page is open.">
          <Button icon="gauge" onClick={() => void attempt(() => api.runCommand({ type: "openPerformance" }))}>Open monitor</Button>
        </SettingRow>
      </Card>
    </>
  );
}
