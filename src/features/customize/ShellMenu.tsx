import { MenuHeading, MenuItem, MenuSeparator, Popover } from "../../components/ui/Popover";
import { api } from "../../lib/ipc";
import { THEME_DEFS } from "../../lib/settings";
import { attempt } from "../../lib/store";
import { openCustomize, openSettingsSection } from "../../lib/ui";
import { useSettings } from "../../lib/useSettings";

/** The right-click menu of the browser's own chrome (toolbar, sidebar, title bar): layout and look. */
export function ShellMenu({ at, onClose }: { at: { x: number; y: number } | null; onClose: () => void }) {
  const { settings, update } = useSettings();
  return (
    <Popover open={!!at} onClose={onClose} point={at} width={262} label="Customize" className="menu">
      <div role="menu">
        <MenuItem icon="sliders" label="Customize toolbar…" onSelect={() => openCustomize("toolbar")} />
        <MenuItem icon="sidebar" label="Customize sidebar…" onSelect={() => openCustomize("sidebar")} />
        <MenuItem icon="tabsVertical" label="Vertical tabs" checked={settings.verticalTabs} onSelect={() => void update({ verticalTabs: !settings.verticalTabs })} />
        <MenuItem icon="star" label="Bookmarks bar" hint="Ctrl+Shift+B" checked={settings.showBookmarksBar} onSelect={() => void attempt(() => api.runCommand({ type: "toggleBookmarksBar" }))} />
        <MenuItem
          icon="sidebar"
          label={settings.sidebar === "hidden" ? "Show the sidebar" : "Hide the sidebar"}
          onSelect={() => void update({ sidebar: settings.sidebar === "hidden" ? "rail" : "hidden" })}
        />
        <MenuSeparator />
        <MenuHeading>Theme</MenuHeading>
        {THEME_DEFS.filter((t) => t.id !== "custom").map((t) => (
          <MenuItem
            key={t.id}
            icon="palette"
            label={t.label}
            checked={settings.theme === t.id}
            trailing={<span className="theme-dot" style={{ background: `hsl(${t.palette.accent[0]} ${t.palette.accent[1]}% ${t.palette.accent[2]}%)` }} />}
            onSelect={() => void update({ theme: t.id })}
          />
        ))}
        <MenuSeparator />
        <MenuItem icon="settings" label="Appearance settings…" onSelect={() => openSettingsSection("appearance")} />
      </div>
    </Popover>
  );
}
