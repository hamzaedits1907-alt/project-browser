// Settings > Keyboard shortcuts > Your own shortcuts: give any common browser command a key of your choosing. The core
// checks these before its built-in keys (shortcuts.rs); built-in keys keep working too.

import { useState } from "react";
import type { KeyboardEvent } from "react";

import { Button, IconButton, Kbd } from "../../components/ui/controls";
import { Card, Notice } from "../../components/ui/Section";
import { useSettings } from "../../lib/useSettings";
import type { ShortcutOverride } from "../../lib/types";

/** Command names (settings.rs / shortcuts.rs CUSTOMIZABLE) with their labels. */
export const CUSTOM_COMMANDS: { command: string; label: string }[] = [
  { command: "newTab", label: "New tab" },
  { command: "newPrivateTab", label: "New private tab" },
  { command: "newWindow", label: "New window" },
  { command: "newPrivateWindow", label: "New private window" },
  { command: "closeTab", label: "Close tab" },
  { command: "reopenTab", label: "Reopen closed tab" },
  { command: "nextTab", label: "Next tab" },
  { command: "prevTab", label: "Previous tab" },
  { command: "commandPalette", label: "Command Center" },
  { command: "focusOmnibox", label: "Go to the address bar" },
  { command: "reload", label: "Reload" },
  { command: "hardReload", label: "Reload without cache" },
  { command: "back", label: "Back" },
  { command: "forward", label: "Forward" },
  { command: "find", label: "Find in page" },
  { command: "bookmarkPage", label: "Bookmark this page" },
  { command: "toggleBookmarksBar", label: "Show or hide the bookmarks bar" },
  { command: "openHistory", label: "History" },
  { command: "openDownloads", label: "Downloads" },
  { command: "openBookmarks", label: "Bookmarks" },
  { command: "openSettings", label: "Settings" },
  { command: "openPerformance", label: "Performance monitor" },
  { command: "openGaming", label: "Gaming dashboard" },
  { command: "toggleGamingMode", label: "Gaming Mode on or off" },
  { command: "screenshot", label: "Screenshot" },
  { command: "readerMode", label: "Reader view" },
  { command: "toggleSplit", label: "Split view" },
  { command: "devTools", label: "Developer tools" },
  { command: "fullscreen", label: "Full screen" },
  { command: "zoomIn", label: "Zoom in" },
  { command: "zoomOut", label: "Zoom out" },
  { command: "zoomReset", label: "Reset zoom" },
];

const KEY_NAMES: Record<string, string> = { ArrowLeft: "Left", ArrowRight: "Right", ArrowUp: "Up", ArrowDown: "Down", PageUp: "PageUp", PageDown: "PageDown", ",": ",", "-": "-", "=": "=", ".": ".", "/": "/" };

/** "Ctrl+Shift+K" for a key press, or null while only modifiers are held or the combination can't be a shortcut. */
export function keysOf(e: { key: string; ctrlKey: boolean; shiftKey: boolean; altKey: boolean }): string | null {
  if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return null;
  const fn = /^F([1-9]|1[0-2])$/.test(e.key);
  let name: string | null;
  if (fn) name = e.key;
  else if (/^[a-z0-9]$/i.test(e.key)) name = e.key.toUpperCase();
  else name = KEY_NAMES[e.key] ?? null;
  if (!name || (!fn && !e.ctrlKey && !e.altKey)) return null;
  return [e.ctrlKey && "Ctrl", e.shiftKey && "Shift", e.altKey && "Alt", name].filter(Boolean).join("+");
}

export function CustomShortcuts() {
  const { settings, update } = useSettings();
  const [recording, setRecording] = useState<string | null>(null);
  const list = settings.shortcutOverrides;
  const keysFor = (command: string) => list.find((o) => o.command === command)?.keys ?? "";
  const save = (next: ShortcutOverride[]) => void update({ shortcutOverrides: next.filter((o) => o.keys) });
  const set = (command: string, keys: string) => save([...list.filter((o) => o.command !== command && o.keys !== keys), { command, keys }]);
  const onKey = (command: string, e: KeyboardEvent<HTMLButtonElement>) => {
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") return setRecording(null);
    if (e.key === "Backspace" || e.key === "Delete") {
      save(list.filter((o) => o.command !== command));
      return setRecording(null);
    }
    const k = keysOf(e);
    if (!k) return;
    set(command, k);
    setRecording(null);
  };
  return (
    <Card
      title="Your own shortcuts"
      subtitle="Click a command, then press the keys you want (with Ctrl or Alt, or a function key). Keys the browser already uses can't be recorded; pick a free combination. Built-in keys keep working. Backspace removes a shortcut."
      actions={list.length > 0 ? <Button variant="ghost" onClick={() => save([])}>Reset all</Button> : undefined}
    >
      {list.length > 0 && <Notice tone="info">{list.length} shortcut{list.length === 1 ? "" : "s"} of your own.</Notice>}
      <ul className="shortcut-list custom-shortcuts">
        {CUSTOM_COMMANDS.map((c) => {
          const keys = keysFor(c.command);
          const rec = recording === c.command;
          return (
            <li key={c.command}>
              <span>{c.label}</span>
              <span className="custom-shortcut-keys">
                <button type="button" className={rec ? "key-recorder is-recording" : "key-recorder"} onClick={() => setRecording(rec ? null : c.command)} onKeyDown={(e) => rec && onKey(c.command, e)} onBlur={() => rec && setRecording(null)}>
                  {rec ? "Press keys…" : keys ? <Kbd keys={keys.split("+")} /> : <span className="dim">Add</span>}
                </button>
                {keys && !rec && <IconButton icon="close" label={`Remove the shortcut for ${c.label}`} size={13} onClick={() => save(list.filter((o) => o.command !== c.command))} />}
              </span>
            </li>
          );
        })}
      </ul>
    </Card>
  );
}
