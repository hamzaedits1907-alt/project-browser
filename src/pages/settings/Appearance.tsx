import { Toggle } from "../../components/ui/controls";
import { Card, SettingRow } from "../../components/ui/Section";
import { ACCENTS, NEW_TAB_BACKGROUNDS, accentOrDefault, backgroundOrDefault } from "../../lib/settings";
import { useSettings } from "../../lib/useSettings";

export function Appearance() {
  const { settings, update } = useSettings();
  const accent = accentOrDefault(settings.accent);
  const bg = backgroundOrDefault(settings.newTabBackground);
  return (
    <>
      <Card title="Accent colour" subtitle="Used for the active tab, focus rings and highlights.">
        <div className="swatches" role="radiogroup" aria-label="Accent colour">
          {ACCENTS.map((a) => (
            <button
              key={a.id}
              type="button"
              role="radio"
              aria-checked={a.id === accent}
              className={`swatch${a.id === accent ? " is-on" : ""}`}
              style={{ "--sw": a.swatch } as React.CSSProperties}
              onClick={() => void update({ accent: a.id })}
            >
              <span className="swatch-dot" />
              {a.label}
            </button>
          ))}
        </div>
      </Card>

      <Card title="New tab page">
        <div className="bg-picker" role="radiogroup" aria-label="New tab background">
          {NEW_TAB_BACKGROUNDS.map((b) => (
            <button key={b.id} type="button" role="radio" aria-checked={b.id === bg} className={`bg-option${b.id === bg ? " is-on" : ""}`} onClick={() => void update({ newTabBackground: b.id })}>
              <span className="bg-preview" data-bg={b.id} />
              {b.label}
            </button>
          ))}
        </div>
        <SettingRow title="Show memory and CPU on the new tab page" description="Takes one reading when the page opens, then stops.">
          <Toggle label="Show resource usage" checked={settings.showRamOnNewTab} onChange={(showRamOnNewTab) => void update({ showRamOnNewTab })} />
        </SettingRow>
      </Card>

      <Card title="Motion">
        <SettingRow title="Reduce motion" description="Turns off interface animations and background effects. Gaming Mode can do this automatically.">
          <Toggle label="Reduce motion" checked={settings.reduceMotion} onChange={(reduceMotion) => void update({ reduceMotion })} />
        </SettingRow>
      </Card>
    </>
  );
}
