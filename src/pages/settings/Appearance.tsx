import type { CSSProperties } from "react";

import { Button, Segmented, Slider, Toggle } from "../../components/ui/controls";
import { Card, SettingRow } from "../../components/ui/Section";
import { ACCENTS, NEW_TAB_BACKGROUNDS, THEME_DEFS, accentCss, accentOrDefault, backgroundOrDefault } from "../../lib/settings";
import { cue } from "../../lib/sounds";
import { openCustomize } from "../../lib/ui";
import type { CornerStyle, DarkPages, Density, MotionLevel, SidebarMode, TabStyle } from "../../lib/types";
import { useSettings } from "../../lib/useSettings";

/** A miniature browser window painted in a theme's own colours. */
function ThemePreview({ id }: { id: string }) {
  const t = THEME_DEFS.find((x) => x.id === id);
  if (!t) return null;
  const p = t.palette;
  const accent = accentCss(p.accent);
  const style = { "--p0": p.bg[0], "--p1": p.bg[1], "--p2": p.bg[2], "--p3": p.bg[3], "--pa": accent, "--pt": p.text[1] } as CSSProperties;
  return (
    <span className="theme-preview" style={style} aria-hidden="true">
      <span className="tp-bar">
        <span className="tp-tab is-on" />
        <span className="tp-tab" />
      </span>
      <span className="tp-tool">
        <span className="tp-omni" />
      </span>
      <span className="tp-body">
        <span className="tp-rail" />
        <span className="tp-page">
          <span className="tp-line" />
          <span className="tp-line is-short" />
          <span className="tp-btn" />
        </span>
      </span>
    </span>
  );
}

export function Appearance() {
  const { settings, update } = useSettings();
  const accent = accentOrDefault(settings.accent);
  const bg = backgroundOrDefault(settings.newTabBackground);
  return (
    <>
      <Card title="Theme" subtitle="Each theme sets the surfaces, the glow and the accent. You can still pick your own accent below.">
        <div className="theme-grid" role="radiogroup" aria-label="Theme">
          {THEME_DEFS.map((t) => (
            <button key={t.id} type="button" role="radio" aria-checked={settings.theme === t.id} className={`theme-card${settings.theme === t.id ? " is-on" : ""}`} onClick={() => void update({ theme: t.id })}>
              <ThemePreview id={t.id} />
              <span className="theme-name">{t.label}</span>
              <span className="theme-blurb">{t.blurb}</span>
            </button>
          ))}
        </div>
      </Card>

      <Card title="Accent colour" subtitle="Used for the active tab, focus rings, highlights and glow.">
        <div className="swatches" role="radiogroup" aria-label="Accent colour">
          <button type="button" role="radio" aria-checked={accent === "auto"} className={`swatch${accent === "auto" ? " is-on" : ""}`} style={{ "--sw": "var(--accent)" } as CSSProperties} onClick={() => void update({ accent: "auto" })}>
            <span className="swatch-dot" />
            Theme
          </button>
          {ACCENTS.map((a) => (
            <button key={a.id} type="button" role="radio" aria-checked={a.id === accent} className={`swatch${a.id === accent ? " is-on" : ""}`} style={{ "--sw": accentCss(a.hsl) } as CSSProperties} onClick={() => void update({ accent: a.id })}>
              <span className="swatch-dot" />
              {a.label}
            </button>
          ))}
          <button type="button" role="radio" aria-checked={accent === "custom"} className={`swatch${accent === "custom" ? " is-on" : ""}`} style={{ "--sw": `hsl(${settings.customHue} 100% 60%)` } as CSSProperties} onClick={() => void update({ accent: "custom" })}>
            <span className="swatch-dot" />
            Custom
          </button>
        </div>
        {(accent === "custom" || settings.theme === "custom") && (
          <SettingRow title="Custom hue" description="Drag to pick any colour on the wheel.">
            <Slider label="Accent hue" value={settings.customHue} min={0} max={359} onChange={(customHue) => void update({ customHue })} format={(v) => `${v}°`} />
          </SettingRow>
        )}
      </Card>

      <Card title="Shape and layout">
        <SettingRow title="Corners" description="Angular is the Project Browser look. Rounded and soft are calmer.">
          <Segmented<CornerStyle>
            label="Corner style"
            value={settings.cornerStyle}
            options={[
              { value: "angular", label: "Angular" },
              { value: "rounded", label: "Rounded" },
              { value: "soft", label: "Soft" },
            ]}
            onChange={(cornerStyle) => void update({ cornerStyle })}
          />
        </SettingRow>
        <SettingRow title="Density" description="How much room the toolbar, tabs and lists take.">
          <Segmented<Density>
            label="Density"
            value={settings.density}
            options={[
              { value: "compact", label: "Compact" },
              { value: "comfortable", label: "Comfortable" },
              { value: "spacious", label: "Spacious" },
            ]}
            onChange={(density) => void update({ density })}
          />
        </SettingRow>
        <SettingRow title="Tabs" description="Shaped tabs join the toolbar; pills float; minimal is text with an underline.">
          <Segmented<TabStyle>
            label="Tab style"
            value={settings.tabStyle}
            options={[
              { value: "shaped", label: "Shaped" },
              { value: "pill", label: "Pills" },
              { value: "minimal", label: "Minimal" },
            ]}
            onChange={(tabStyle) => void update({ tabStyle })}
          />
        </SettingRow>
        <SettingRow title="Vertical tabs" description="Tabs in a column next to the page; good with many tabs.">
          <Toggle label="Vertical tabs" checked={settings.verticalTabs} onChange={(verticalTabs) => void update({ verticalTabs })} />
        </SettingRow>
        <SettingRow title="Compact title bar" description="A slimmer title bar with the product name.">
          <Toggle label="Compact title bar" checked={settings.compactTitleBar} onChange={(compactTitleBar) => void update({ compactTitleBar })} />
        </SettingRow>
        <SettingRow title="Sidebar">
          <Segmented<SidebarMode>
            label="Sidebar"
            value={settings.sidebar}
            options={[
              { value: "rail", label: "Icons" },
              { value: "expanded", label: "Labels" },
              { value: "hidden", label: "Hidden" },
            ]}
            onChange={(sidebar) => void update({ sidebar })}
          />
          <Button size="sm" onClick={() => openCustomize("sidebar")}>
            Customize…
          </Button>
        </SettingRow>
        <SettingRow title="Toolbar buttons" description="Choose and order the optional buttons next to the address bar.">
          <Button size="sm" onClick={() => openCustomize("toolbar")}>
            Customize…
          </Button>
        </SettingRow>
      </Card>

      <Card title="Motion and sound">
        <SettingRow title="Animation" description="Subtle keeps transitions short; expressive adds travel and spring. Interaction never waits for an animation.">
          <Segmented<MotionLevel>
            label="Animation intensity"
            value={settings.motion}
            options={[
              { value: "subtle", label: "Subtle" },
              { value: "standard", label: "Standard" },
              { value: "expressive", label: "Expressive" },
            ]}
            onChange={(motion) => void update({ motion })}
          />
        </SettingRow>
        <SettingRow
          title="Dark pages"
          description="Auto asks websites for their own dark theme. Force also darkens pages that are still bright (pictures and video keep their colours; pages that are already dark are left alone). The moon button in the toolbar (add it with Customize) switches a single site on or off."
        >
          <Segmented<DarkPages>
            label="Dark pages"
            value={settings.darkPages}
            options={[
              { value: "off", label: "Off" },
              { value: "auto", label: "Auto" },
              { value: "force", label: "Force" },
            ]}
            onChange={(darkPages) => void update({ darkPages })}
          />
        </SettingRow>
        <SettingRow title="Reduce motion" description="Turns interface animation off. The Performance theme and Gaming Mode (if set) do this too; Windows' own setting is always respected.">
          <Toggle label="Reduce motion" checked={settings.reduceMotion} onChange={(reduceMotion) => void update({ reduceMotion })} />
        </SettingRow>
        <SettingRow title="Interface sounds" description="Short, quiet clicks for tabs, menus and Gaming Mode. Off by default.">
          {settings.uiSounds && (
            <Button size="sm" variant="ghost" onClick={() => cue("power")}>
              Play a sample
            </Button>
          )}
          <Toggle label="Interface sounds" checked={settings.uiSounds} onChange={(uiSounds) => void update({ uiSounds })} />
        </SettingRow>
        {settings.uiSounds && (
          <SettingRow title="Sound volume" description="Only the browser's own sounds; websites keep their own volume.">
            <input
              type="range"
              min={0}
              max={100}
              step={5}
              aria-label="Interface sound volume"
              value={settings.uiSoundVolume}
              onChange={(e) => void update({ uiSoundVolume: Number(e.target.value) })}
              onPointerUp={() => cue("tap")}
            />
            <span className="num dim">{settings.uiSoundVolume}%</span>
          </SettingRow>
        )}
      </Card>

      <Card title="New Tab page">
        <SettingRow title="Layout">
          <Segmented
            label="New Tab layout"
            value={settings.ntpLayout}
            options={[
              { value: "full", label: "Full" },
              { value: "minimal", label: "Minimal" },
            ]}
            onChange={(ntpLayout) => void update({ ntpLayout })}
          />
        </SettingRow>
        <div className="bg-picker" role="radiogroup" aria-label="New tab background">
          {NEW_TAB_BACKGROUNDS.map((b) => (
            <button key={b.id} type="button" role="radio" aria-checked={b.id === bg} className={`bg-option${b.id === bg ? " is-on" : ""}`} onClick={() => void update({ newTabBackground: b.id })}>
              <span className="bg-preview" data-bg={b.id} />
              {b.label}
            </button>
          ))}
        </div>
        <SettingRow title="Show memory and CPU" description="The System widget takes one reading when the page opens, then stops.">
          <Toggle label="Show resource usage" checked={settings.showRamOnNewTab} onChange={(showRamOnNewTab) => void update({ showRamOnNewTab })} />
        </SettingRow>
        <p className="dim small">More widgets: use Customize on the New Tab page itself.</p>
      </Card>
    </>
  );
}
