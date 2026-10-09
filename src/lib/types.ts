// Mirrors the serde types of the Rust core (src-tauri/src). Field names are camelCase on the wire.

export type TabId = number;

export type Lifecycle = "unloaded" | "live" | "sleeping";
export type Security = "internal" | "secure" | "insecure" | "certerror" | "local";

export interface NavError {
  url: string;
  code: string;
  description: string;
}

export interface Tab {
  id: TabId;
  url: string;
  title: string;
  favicon: string | null;
  pinned: boolean;
  private: boolean;
  loading: boolean;
  canGoBack: boolean;
  canGoForward: boolean;
  zoom: number;
  audible: boolean;
  muted: boolean;
  lifecycle: Lifecycle;
  security: Security;
  error: NavError | null;
  heavy: boolean;
  bookmarked: boolean;
  lastActive: number;
}

export interface BrowserState {
  tabs: Tab[];
  activeId: TabId | null;
  closedCount: number;
  fullscreen: boolean;
  browserFullscreen: boolean;
  gaming: boolean;
}

export type SearchEngine = "google" | "bing" | "duckduckgo" | "brave" | "startpage" | "ecosia";
export type StartupBehavior = "restoreSession" | "newTab";
export type BackgroundTabAction = "sleep" | "unload";

export interface Settings {
  searchEngine: SearchEngine;
  customSearchTemplate: string | null;
  startup: StartupBehavior;
  showBookmarksBar: boolean;
  downloadDir: string | null;
  accent: string;
  newTabBackground: string;
  reduceMotion: boolean;
  showRamOnNewTab: boolean;
  sleepingTabsEnabled: boolean;
  sleepAfterMinutes: number;
  neverSleepHosts: string[];
  gamingMode: boolean;
  gamingBackgroundAction: BackgroundTabAction;
  gamingPauseDownloads: boolean;
  gamingReduceAnimations: boolean;
  gamingLowMemoryTarget: boolean;
  savePasswords: boolean;
  autofill: boolean;
  heavyTabMemoryMb: number;
}

export interface ProfileMeta {
  id: string;
  name: string;
  color: string;
  createdAt: number;
}

export type DownloadState = "inprogress" | "paused" | "completed" | "cancelled" | "interrupted";

export interface Download {
  id: number;
  url: string;
  path: string;
  filename: string;
  mime: string | null;
  totalBytes: number | null;
  receivedBytes: number;
  state: DownloadState;
  danger: string | null;
  error: string | null;
  startedAt: number;
  endedAt: number | null;
  private: boolean;
  pausedByGaming: boolean;
}

export type DownloadCommand = "pause" | "resume" | "cancel" | "open" | "openConfirmed" | "showInFolder" | "remove";

export interface PermissionPrompt {
  id: number;
  tabId: TabId;
  origin: string;
  kind: string;
}

export interface FindState {
  tabId: TabId;
  query: string;
  matches: number;
  active: number;
  unavailable: boolean;
}

export interface Bootstrap {
  state: BrowserState;
  settings: Settings;
  profile: ProfileMeta;
  firstRun: boolean;
  downloads: Download[];
  prompts: PermissionPrompt[];
  version: string;
}

export type UiEvent = { kind: "focusOmnibox" } | { kind: "openFind" } | { kind: "toast"; message: string };

export interface Suggestion {
  kind: "search" | "url" | "tab" | "bookmark" | "history";
  title: string;
  url: string;
  tabId: TabId | null;
}

export interface UrlRow {
  url: string;
  title: string;
  visitCount: number;
  lastVisit: number;
}

export interface Bookmark {
  id: number;
  parentId: number | null;
  kind: "url" | "folder";
  title: string;
  url: string | null;
  position: number;
  createdAt: number;
}

export interface HistoryEntry {
  visitId: number;
  urlId: number;
  url: string;
  title: string;
  visitedAt: number;
}

export interface ClosedTabInfo {
  url: string;
  title: string;
  favicon: string | null;
}

export interface NtpData {
  topSites: UrlRow[];
  recentBookmarks: Bookmark[];
  recentlyClosed: ClosedTabInfo[];
  continueBrowsing: UrlRow[];
}

export interface SitePermission {
  origin: string;
  kind: string;
  decision: "allow" | "block";
  updatedAt: number;
}

export interface ClearOptions {
  history: boolean;
  downloads: boolean;
  cookies: boolean;
  cache: boolean;
  passwords: boolean;
  autofill: boolean;
  since: number | null;
}

export interface ProcessStat {
  pid: number;
  kind: string;
  label: string;
  memory: number;
  cpu: number | null;
  tabIds: TabId[];
}

export interface TabStat {
  id: TabId;
  memory: number;
  cpu: number | null;
  shared: boolean;
  heavy: boolean;
}

export interface StartupMetrics {
  processStart: number | null;
  shellReadyMs: number | null;
  firstPageMs: number | null;
}

export interface PerfSample {
  timestamp: number;
  totalMemory: number;
  shellMemory: number;
  webviewMemory: number;
  totalCpu: number | null;
  gpu: number | null;
  processes: ProcessStat[];
  tabs: TabStat[];
  tabCount: number;
  liveTabs: number;
  sleepingTabs: number;
  unloadedTabs: number;
  startup: StartupMetrics;
  gaming: boolean;
}

export interface ProfilesInfo {
  profiles: ProfileMeta[];
  current: string;
}

export type SourceBrowser = "chrome" | "edge" | "brave";

export interface ImportItem {
  kind: string;
  label: string;
  supported: boolean;
  present: boolean;
  detail: string;
}

export interface DetectedProfile {
  browser: SourceBrowser;
  browserLabel: string;
  profileDir: string;
  name: string;
  items: ImportItem[];
}

export interface ImportReport {
  bookmarksImported: number;
  /** Bookmarks that were already there (or not web addresses), so were not added again. */
  bookmarksSkipped: number;
  historyImported: number;
  historySkipped: number;
  /** Name of the default search engine that was adopted, if one was imported. */
  searchEngine: string | null;
  errors: string[];
}

export interface AppInfo {
  version: string;
  identifier: string;
  dataDir: string;
  profileDir: string;
  profileName: string;
  webview2Version: string | null;
  updateConfigured: boolean;
}

export type UpdateStatus =
  | { status: "notConfigured" }
  | { status: "upToDate"; current: string }
  | { status: "available"; current: string; version: string; notes: string | null; date: string | null };

export type AiProvider = "openAiCompatible" | "anthropic";

export interface AiConfigView {
  provider: AiProvider;
  baseUrl: string;
  model: string;
  hasKey: boolean;
  /** A key from an older build is still in the profile database because moving it to Credential Manager failed. */
  keyMigrationPending: boolean;
}

export interface AiChatMessage {
  role: "user" | "assistant";
  content: string;
}

export interface AiEvent {
  requestId: string;
  kind: "delta" | "done" | "error";
  text: string;
}

export interface ContentRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Viewport {
  width: number;
  height: number;
  scale: number;
  maximized: boolean;
  screenX: number;
  screenY: number;
}

/** Serialised `shortcuts::Command` (tag = "type", content = "value"). */
export type BrowserCommand =
  | { type: "newTab" }
  | { type: "newPrivateTab" }
  | { type: "closeTab" }
  | { type: "reopenTab" }
  | { type: "nextTab" }
  | { type: "prevTab" }
  | { type: "selectTab"; value: number }
  | { type: "focusOmnibox" }
  | { type: "reload" }
  | { type: "hardReload" }
  | { type: "back" }
  | { type: "forward" }
  | { type: "stop" }
  | { type: "find" }
  | { type: "findNext" }
  | { type: "findPrev" }
  | { type: "bookmarkPage" }
  | { type: "toggleBookmarksBar" }
  | { type: "openHistory" }
  | { type: "openDownloads" }
  | { type: "openBookmarks" }
  | { type: "openSettings" }
  | { type: "openPerformance" }
  | { type: "openGaming" }
  | { type: "zoomIn" }
  | { type: "zoomOut" }
  | { type: "zoomReset" }
  | { type: "fullscreen" }
  | { type: "toggleGamingMode" };
