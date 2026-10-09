// Reference list of the browser's keyboard shortcuts. The keys themselves are
// handled natively (src-tauri/src/shortcuts.rs) so they work whichever webview
// has focus; this table only documents them and labels menu items.

export interface ShortcutGroup {
  title: string;
  items: { keys: string[]; label: string }[];
}

export const SHORTCUT_GROUPS: ShortcutGroup[] = [
  {
    title: "Tabs",
    items: [
      { keys: ["Ctrl", "T"], label: "New tab" },
      { keys: ["Ctrl", "Shift", "N"], label: "New private tab" },
      { keys: ["Ctrl", "W"], label: "Close tab" },
      { keys: ["Ctrl", "Shift", "T"], label: "Reopen closed tab" },
      { keys: ["Ctrl", "Tab"], label: "Next tab" },
      { keys: ["Ctrl", "Shift", "Tab"], label: "Previous tab" },
      { keys: ["Ctrl", "1-8"], label: "Go to tab by position" },
      { keys: ["Ctrl", "9"], label: "Go to last tab" },
    ],
  },
  {
    title: "Navigation",
    items: [
      { keys: ["Ctrl", "L"], label: "Focus address bar" },
      { keys: ["Alt", "←"], label: "Back" },
      { keys: ["Alt", "→"], label: "Forward" },
      { keys: ["F5"], label: "Reload" },
      { keys: ["Ctrl", "Shift", "R"], label: "Hard reload" },
      { keys: ["Ctrl", "F"], label: "Find in page" },
      { keys: ["F3"], label: "Find next" },
      { keys: ["Shift", "F3"], label: "Find previous" },
    ],
  },
  {
    title: "Pages",
    items: [
      { keys: ["Ctrl", "D"], label: "Bookmark this page" },
      { keys: ["Ctrl", "Shift", "B"], label: "Show or hide bookmarks bar" },
      { keys: ["Ctrl", "H"], label: "History" },
      { keys: ["Ctrl", "J"], label: "Downloads" },
      { keys: ["Ctrl", "Shift", "O"], label: "Bookmarks" },
      { keys: ["Ctrl", ","], label: "Settings" },
    ],
  },
  {
    title: "Gaming & performance",
    items: [
      { keys: ["Ctrl", "Shift", "G"], label: "Toggle Gaming Mode" },
      { keys: ["Ctrl", "Shift", "D"], label: "Gaming dashboard" },
      { keys: ["Shift", "Esc"], label: "Performance monitor" },
    ],
  },
  {
    title: "View",
    items: [
      { keys: ["Ctrl", "+"], label: "Zoom in" },
      { keys: ["Ctrl", "-"], label: "Zoom out" },
      { keys: ["Ctrl", "0"], label: "Reset zoom" },
      { keys: ["F11"], label: "Full screen" },
    ],
  },
];

/** Display string for a menu hint, e.g. "Ctrl+Shift+T". */
export function hint(...keys: string[]): string {
  return keys.join("+");
}
