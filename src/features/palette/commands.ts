// Everything the Command Center can do, as data. Each command calls the same core commands as the buttons and
// menus elsewhere in the browser; nothing here is a shortcut around the core's checks.

import type { IconName } from "../../components/ui/Icon";
import { toggleAi } from "../ai/aiStore";
import { checkForUpdates } from "../update/updateStore";
import { api, errorMessage } from "../../lib/ipc";
import { SECTION_META } from "../../pages/settingsIndex";
import { THEME_DEFS } from "../../lib/settings";
import { attempt, emitBus, getState, setSettingsLocal, toast } from "../../lib/store";
import type { Settings, Tab } from "../../lib/types";
import { openClearData, openDialog, openImportWizard, openSettingsSection, togglePanel } from "../../lib/ui";
import { isInternalUrl } from "../../lib/urls";
import { startScreenshot } from "../dialogs/screenshotState";
import { endSession, loadConfig, startSession } from "../focus/focus";
import { darkFor, toggleDark } from "../sitetools/siteprefs";
import { closeDuplicates, closeToTheRight, groupBySite, linksAsMarkdown, sortBySite } from "../tabtools/tabtools";

export interface PaletteCommand {
  id: string;
  title: string;
  icon: IconName;
  keywords?: string;
  hint?: string;
  run: () => void;
}

async function saveSettings(changes: Partial<Settings>): Promise<void> {
  const before = getState().settings;
  setSettingsLocal({ ...before, ...changes });
  try {
    setSettingsLocal(await api.settingsSet({ ...before, ...changes }));
  } catch (e) {
    setSettingsLocal(before);
    toast(errorMessage(e));
  }
}

async function copy(text: string, what: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    toast(`${what} copied`);
  } catch {
    toast("Could not copy to the clipboard");
  }
}

/** Removes click-tracking parameters, mirroring the core's Link Cleaner (linkclean.rs) for the common ones. */
export function cleanLink(url: string): string {
  try {
    const u = new URL(url);
    const drop = [...u.searchParams.keys()].filter((k) =>
      /^(utm_[a-z_-]+|fbclid|gclid|gclsrc|dclid|gbraid|wbraid|msclkid|twclid|ttclid|igshid|mc_cid|mc_eid|yclid|_hsenc|_hsmi|mkt_tok|srsltid)$/i.test(k),
    );
    if (/(^|\.)(youtube\.com|youtu\.be)$/.test(u.hostname)) drop.push(...[...u.searchParams.keys()].filter((k) => k === "si" || k === "pp" || k === "feature"));
    for (const k of drop) u.searchParams.delete(k);
    return u.toString();
  } catch {
    return url;
  }
}

function webTab(tab: Tab | null): tab is Tab {
  return !!tab && !isInternalUrl(tab.url);
}

export function buildCommands(tab: Tab | null, tabs: Tab[], settings: Settings): PaletteCommand[] {
  const s = getState();
  const active = s.browser.activeId;
  const split = s.browser.split;
  const web = webTab(tab);
  const cmds: (PaletteCommand | false)[] = [
    // Tabs and windows
    { id: "new-tab", title: "New tab", icon: "plus", hint: "Ctrl+T", run: () => void attempt(() => api.runCommand({ type: "newTab" })) },
    { id: "new-window", title: "New window", icon: "windowNew", hint: "Ctrl+N", run: () => void attempt(() => api.runCommand({ type: "newWindow" })) },
    { id: "new-private", title: "New private window", icon: "incognito", keywords: "incognito", hint: "Ctrl+Shift+N", run: () => void attempt(() => api.runCommand({ type: "newPrivateWindow" })) },
    { id: "reopen", title: "Reopen closed tab", icon: "undo", keywords: "restore undo close", hint: "Ctrl+Shift+T", run: () => void attempt(() => api.tabReopen()) },
    !!tab && { id: "close-tab", title: "Close tab", icon: "close", hint: "Ctrl+W", run: () => void attempt(() => api.tabClose(tab.id)) },
    !!tab && { id: "duplicate", title: "Duplicate tab", icon: "copy", run: () => void attempt(() => api.tabDuplicate(tab.id)) },
    !!tab && { id: "pin", title: tab.pinned ? "Unpin tab" : "Pin tab", icon: "pin", run: () => void attempt(() => api.tabPin(tab.id, !tab.pinned)) },
    !!tab && web && { id: "mute", title: tab.muted ? "Unmute tab" : "Mute tab", icon: tab.muted ? "sound" : "mute", keywords: "sound audio", run: () => void attempt(() => api.tabMute(tab.id, !tab.muted)) },
    !!tab && { id: "close-others", title: "Close other tabs", icon: "close", run: () => void attempt(() => api.tabCloseOthers(tab.id)) },
    !!tab && { id: "close-right", title: "Close tabs to the right", icon: "close", run: () => void closeToTheRight(tabs, tab.id).then((n) => toast(n ? `Closed ${n} tab${n === 1 ? "" : "s"}` : "No tabs to the right")) },
    { id: "dupes", title: "Close duplicate tabs", icon: "broom", keywords: "clean up same page twice", run: () => void closeDuplicates(tabs, active).then((n) => toast(n ? `Closed ${n} duplicate tab${n === 1 ? "" : "s"}` : "No duplicate tabs")) },
    { id: "sort", title: "Sort tabs by site", icon: "list", keywords: "order arrange", run: () => void sortBySite(tabs).then(() => toast("Tabs sorted by site")) },
    { id: "group-site", title: "Group tabs by site", icon: "group", keywords: "organize tidy", run: () => void groupBySite(tabs).then((n) => toast(n ? `Made ${n} group${n === 1 ? "" : "s"}` : "No site has two loose tabs")) },
    { id: "copy-all", title: "Copy all tab links (Markdown)", icon: "link", keywords: "export list urls", run: () => void copy(linksAsMarkdown(tabs), "Links") },
    { id: "workspace", title: "Workspaces: save or open a set of tabs", icon: "workspace", keywords: "session save restore project", run: () => openDialog("workspaces") },
    // Split view
    web && !split && { id: "split", title: "Split view: show this tab next to another", icon: "columns", keywords: "side by side two pages", hint: "Ctrl+Shift+E", run: () => void attempt(() => api.splitView({ op: "open" })) },
    !!split && { id: "split-swap", title: "Split view: swap sides", icon: "swap", run: () => void attempt(() => api.splitView({ op: "swap" })) },
    !!split && { id: "split-close", title: "Split view: close", icon: "columns", hint: "Ctrl+Shift+E", run: () => void attempt(() => api.splitView({ op: "close" })) },
    // This page
    web && { id: "reader", title: "Reader view and Listen", icon: "book", keywords: "read aloud speech article clean immersive", hint: "F9", run: () => openDialog("reader") },
    web && { id: "screenshot", title: "Screenshot of what you see", icon: "crop", keywords: "capture image picture", hint: "Ctrl+Shift+S", run: () => void startScreenshot(false) },
    web && { id: "screenshot-full", title: "Screenshot of the whole page", icon: "crop", keywords: "capture image picture full page scrolling long", run: () => void startScreenshot(true) },
    web && { id: "share", title: "Share: QR code and links", icon: "qr", keywords: "phone send copy link", run: () => openDialog("share") },
    web && { id: "copy-link", title: "Copy page link", icon: "link", keywords: "url address", run: () => void copy(tab.url, "Link") },
    web && { id: "copy-clean", title: "Copy clean link (no tracking)", icon: "broom", keywords: "utm fbclid", run: () => void copy(cleanLink(tab.url), "Clean link") },
    web && { id: "copy-md", title: "Copy link as Markdown", icon: "code", run: () => void copy(`[${tab.title || tab.url}](${tab.url})`, "Markdown link") },
    web && { id: "find", title: "Find in page", icon: "search", hint: "Ctrl+F", run: () => void attempt(() => api.runCommand({ type: "find" })) },
    web && { id: "bookmark", title: tab.bookmarked ? "Remove bookmark" : "Bookmark this page", icon: "star", hint: "Ctrl+D", run: () => void attempt(() => api.bookmarkToggle(tab.id)) },
    web &&
      !tab.private && {
        id: "dark",
        title: darkFor(tab.url, getState().settings.darkPages) ? "Dark pages: turn off for this site" : "Dark pages: turn on for this site",
        icon: "moon",
        keywords: "night theme invert",
        run: () => {
          void toggleDark(tab);
        },
      },
    web && { id: "zoom-in", title: "Zoom in", icon: "zoomIn", hint: "Ctrl++", run: () => void attempt(() => api.zoom("in")) },
    web && { id: "zoom-out", title: "Zoom out", icon: "zoomOut", hint: "Ctrl+-", run: () => void attempt(() => api.zoom("out")) },
    web && { id: "zoom-reset", title: "Reset zoom", icon: "search", hint: "Ctrl+0", run: () => void attempt(() => api.zoom("reset")) },
    { id: "fullscreen", title: "Full screen", icon: "fullscreen", hint: "F11", run: () => void attempt(() => api.runCommand({ type: "fullscreen" })) },
    web && { id: "hard-reload", title: "Hard reload (skip cache)", icon: "reload", hint: "Ctrl+Shift+R", run: () => void attempt(() => api.nav("hardReload")) },
    // Media
    web && { id: "media-toggle", title: "Media: play or pause", icon: "play", keywords: "video music", run: () => void attempt(() => api.mediaControl(tab.id, { action: "toggle" })) },
    web && { id: "media-fast", title: "Media: speed 1.5×", icon: "bolt", keywords: "video playback faster", run: () => void attempt(() => api.mediaControl(tab.id, { action: "speed", rate: 1.5 })) },
    web && { id: "media-2x", title: "Media: speed 2×", icon: "bolt", keywords: "video playback faster double", run: () => void attempt(() => api.mediaControl(tab.id, { action: "speed", rate: 2 })) },
    web && { id: "media-normal", title: "Media: normal speed", icon: "play", run: () => void attempt(() => api.mediaControl(tab.id, { action: "speed", rate: 1 })) },
    web && { id: "media-loop", title: "Media: loop on or off", icon: "loop", run: () => void attempt(() => api.mediaControl(tab.id, { action: "loop" })) },
    web && { id: "pip", title: "Picture in picture", icon: "pip", keywords: "floating video", run: () => void attempt(() => api.mediaControl(tab.id, { action: "pictureInPicture" })) },
    { id: "media-panel", title: "Media hub: everything playing", icon: "headphones", keywords: "audio sound tabs", run: () => togglePanel("media") },
    // Focus
    !loadConfig().session && { id: "focus-25", title: "Focus: block distractions for 25 minutes", icon: "target", keywords: "pomodoro productivity block", run: () => (startSession(25), toast("Focus session started: 25 minutes")) },
    !loadConfig().session && { id: "focus-50", title: "Focus: block distractions for 50 minutes", icon: "target", keywords: "pomodoro productivity block", run: () => (startSession(50), toast("Focus session started: 50 minutes")) },
    !!loadConfig().session && { id: "focus-end", title: "Focus: end the session", icon: "target", run: () => (endSession(), toast("Focus session ended")) },
    { id: "screen-time", title: "Screen Time and Focus dashboard", icon: "hourglass", keywords: "usage time spent limits", run: () => void attempt(() => api.tabNew("browser://focus")) },
    { id: "notes", title: "Notes", icon: "note", keywords: "scratchpad write site notes", run: () => togglePanel("notes") },
    // Browser pages and panels
    { id: "history", title: "History", icon: "history", hint: "Ctrl+H", run: () => void attempt(() => api.runCommand({ type: "openHistory" })) },
    { id: "downloads", title: "Downloads", icon: "download", hint: "Ctrl+J", run: () => void attempt(() => api.runCommand({ type: "openDownloads" })) },
    { id: "bookmarks", title: "Bookmarks", icon: "bookmarks", hint: "Ctrl+Shift+O", run: () => void attempt(() => api.runCommand({ type: "openBookmarks" })) },
    { id: "bookmarks-bar", title: settings.showBookmarksBar ? "Hide bookmarks bar" : "Show bookmarks bar", icon: "bookmarks", hint: "Ctrl+Shift+B", run: () => void attempt(() => api.runCommand({ type: "toggleBookmarksBar" })) },
    { id: "shield", title: "Project Shield panel", icon: "shieldCheck", keywords: "ad blocker trackers", run: () => togglePanel("shield") },
    { id: "ai", title: "AI assistant", icon: "sparkle", keywords: "nvidia chat ask summarize", run: toggleAi },
    { id: "memory", title: "Project Memory", icon: "brain", run: () => void attempt(() => api.tabNew("browser://memory")) },
    { id: "research", title: "Research", icon: "flask", run: () => void attempt(() => api.tabNew("browser://research")) },
    { id: "performance", title: "Performance monitor", icon: "gauge", hint: "Shift+Esc", run: () => void attempt(() => api.runCommand({ type: "openPerformance" })) },
    { id: "gaming", title: settings.gamingMode ? "Turn Gaming Mode off" : "Turn Gaming Mode on", icon: "bolt", hint: "Ctrl+Shift+G", run: () => void attempt(() => api.gamingSet(!settings.gamingMode)) },
    { id: "gaming-page", title: "Gaming dashboard", icon: "gamepad", hint: "Ctrl+Shift+D", run: () => void attempt(() => api.runCommand({ type: "openGaming" })) },
    { id: "clear", title: "Clear browsing data", icon: "trash", keywords: "cookies cache history delete", run: openClearData },
    { id: "import", title: "Import from Chrome or Edge", icon: "import", keywords: "migrate bookmarks passwords", run: openImportWizard },
    { id: "shortcuts", title: "Keyboard shortcuts", icon: "command", keywords: "keys hotkeys help", hint: "F1", run: () => openDialog("shortcuts") },
    { id: "updates", title: "Check for updates", icon: "refresh", run: () => void checkForUpdates(false) },
    // Look
    { id: "vtabs", title: settings.verticalTabs ? "Horizontal tabs" : "Vertical tabs", icon: "tabsVertical", run: () => void saveSettings({ verticalTabs: !settings.verticalTabs }) },
    { id: "sidebar", title: settings.sidebar === "hidden" ? "Show the sidebar" : "Hide the sidebar", icon: "sidebar", run: () => void saveSettings({ sidebar: settings.sidebar === "hidden" ? "rail" : "hidden" }) },
    { id: "clean-links", title: settings.cleanLinks ? "Link Cleaner: turn off" : "Link Cleaner: turn on", icon: "broom", keywords: "tracking parameters privacy", run: () => void saveSettings({ cleanLinks: !settings.cleanLinks }) },
    { id: "sponsor", title: settings.sponsorSkip ? "SponsorSkip: turn off" : "SponsorSkip: turn on", icon: "zap", keywords: "youtube sponsor segments skip", run: () => void saveSettings({ sponsorSkip: !settings.sponsorSkip }) },
  ];
  const themes: PaletteCommand[] = THEME_DEFS.filter((t) => t.id !== "custom").map((t) => ({
    id: `theme-${t.id}`,
    title: `Theme: ${t.label}`,
    icon: "palette",
    keywords: "look colour color appearance",
    run: () => void saveSettings({ theme: t.id }),
  }));
  const sections: PaletteCommand[] = SECTION_META.map((m) => ({
    id: `settings-${m.id}`,
    title: `Settings: ${m.label}`,
    icon: m.icon,
    keywords: m.keywords,
    run: () => openSettingsSection(m.id),
  }));
  return [...cmds.filter((c): c is PaletteCommand => !!c), ...themes, ...sections];
}

/** For the shortcut handlers: the same actions as the commands above, by bus event. */
export function emitShell(kind: "openPalette" | "reader" | "screenshot" | "shortcuts"): void {
  emitBus(kind);
}
