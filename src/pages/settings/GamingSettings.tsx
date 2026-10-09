import { Button, Segmented, Toggle } from "../../components/ui/controls";
import { Card, SettingRow } from "../../components/ui/Section";
import { api } from "../../lib/ipc";
import { attempt } from "../../lib/store";
import type { BackgroundTabAction } from "../../lib/types";
import { useSettings } from "../../lib/useSettings";

export function GamingSettings() {
  const { settings, update } = useSettings();
  return (
    <>
      <Card title="Gaming Mode" subtitle="Keeps the browser out of your game's way. Toggle it any time with Ctrl+Shift+G.">
        <SettingRow title="Gaming Mode">
          <Toggle label="Gaming Mode" checked={settings.gamingMode} onChange={(on) => void attempt(() => api.gamingSet(on))} />
        </SettingRow>
        <SettingRow title="Background tabs" description="Sleep keeps the page state and stops its activity. Unload closes the page completely for the biggest memory saving; it reloads when you return.">
          <Segmented<BackgroundTabAction>
            label="Background tab action"
            value={settings.gamingBackgroundAction}
            options={[
              { value: "sleep", label: "Sleep" },
              { value: "unload", label: "Unload" },
            ]}
            onChange={(gamingBackgroundAction) => void update({ gamingBackgroundAction })}
          />
        </SettingRow>
        <SettingRow title="Lower memory priority for background tabs" description="Asks the web engine to trim memory used by tabs you aren't looking at.">
          <Toggle label="Low memory target" checked={settings.gamingLowMemoryTarget} onChange={(gamingLowMemoryTarget) => void update({ gamingLowMemoryTarget })} />
        </SettingRow>
        <SettingRow title="Hold downloads" description="Downloads pause while Gaming Mode is on and resume automatically afterwards.">
          <Toggle label="Pause downloads" checked={settings.gamingPauseDownloads} onChange={(gamingPauseDownloads) => void update({ gamingPauseDownloads })} />
        </SettingRow>
        <SettingRow title="Reduce animations" description="Switches off interface animations and the new tab page's moving background.">
          <Toggle label="Reduce animations" checked={settings.gamingReduceAnimations} onChange={(gamingReduceAnimations) => void update({ gamingReduceAnimations })} />
        </SettingRow>
      </Card>
      <Card title="Dashboard">
        <SettingRow title="Gaming dashboard" description="Live resource use, tab states and one-click memory tools.">
          <Button icon="gamepad" onClick={() => void attempt(() => api.runCommand({ type: "openGaming" }))}>Open dashboard</Button>
        </SettingRow>
      </Card>
    </>
  );
}
