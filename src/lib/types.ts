// Mirrors the serde types of the Rust core (src-tauri/src). Field names are camelCase on the wire.

export type TabId = number;
export type GroupId = number;

export const GROUP_COLORS = ["grey", "blue", "red", "yellow", "green", "pink", "purple", "cyan", "orange"] as const;
export type GroupColor = (typeof GROUP_COLORS)[number];

/** A named, coloured run of neighbouring tabs. */
export interface TabGroup {
  id: GroupId;
  title: string;
  color: string;
  collapsed: boolean;
}

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
  /** The address of the last pop-up this page opened without being asked to (it was blocked). */
  blockedPopup: string | null;
  group: GroupId | null;
}

export interface BrowserState {
  tabs: Tab[];
  groups: TabGroup[];
  activeId: TabId | null;
  closedCount: number;
  fullscreen: boolean;
  browserFullscreen: boolean;
  gaming: boolean;
  /** This window holds private tabs only and is never saved. */
  privateWindow: boolean;
  windowCount: number;
}

export type SearchEngine = "google" | "bing" | "duckduckgo" | "brave" | "startpage" | "ecosia";
export type StartupBehavior = "restoreSession" | "newTab";
export type BackgroundTabAction = "sleep" | "unload";
export type NewWindowRequests = "tab" | "window";
export type CornerStyle = "angular" | "rounded" | "soft";
export type Density = "compact" | "comfortable" | "spacious";
export type MotionLevel = "subtle" | "standard" | "expressive";
export type TabStyle = "shaped" | "pill" | "minimal";
export type SidebarMode = "rail" | "expanded" | "hidden";
export type NtpLayout = "full" | "minimal";

/** A site pinned to the sidebar or the New Tab page. */
export interface PinnedSite {
  title: string;
  url: string;
}

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
  protectPinnedTabs: boolean;
  memoryLimitMb: number;
  /** Most page text, in characters, one AI request may carry (about four characters per token). */
  aiMaxContextChars: number;
  checkUpdates: boolean;
  /** Sites that may open pop-ups without being asked to. */
  popupAllowedHosts: string[];
  newWindowRequests: NewWindowRequests;
  /** Theme preset id; `accent` overrides its colour unless "auto". */
  theme: string;
  /** Accent hue used when `accent` is "custom". */
  customHue: number;
  cornerStyle: CornerStyle;
  density: Density;
  motion: MotionLevel;
  uiSounds: boolean;
  tabStyle: TabStyle;
  verticalTabs: boolean;
  compactTitleBar: boolean;
  sidebar: SidebarMode;
  sidebarItems: string[];
  sidebarSites: PinnedSite[];
  toolbarItems: string[];
  ntpLayout: NtpLayout;
  ntpWidgets: string[];
  ntpShortcuts: PinnedSite[];
  ntpHiddenSites: string[];
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
  /** False when renderer processes could not be attributed to tabs; per-tab memory is then unavailable. */
  tabAttribution: boolean;
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
  /** Individual visits added with their original timestamps. */
  historyVisitsImported: number;
  /** Name of the default search engine that was adopted, if one was imported. */
  searchEngine: string | null;
  /** Open tabs found in the last session of the other browser. */
  sessionTabs: number;
  /** Of those, how many were saved as bookmarks. */
  sessionTabsSaved: number;
  /** Their web addresses (at most 50), so they can be reopened. */
  sessionUrls: string[];
  addressesImported: number;
  addressesSkipped: number;
  errors: string[];
}

/** What the user chose to bring over in the main import step. */
export interface ImportChoices {
  bookmarks: boolean;
  history: boolean;
  searchEngine: boolean;
  addresses: boolean;
  session: boolean;
}

/** The password export file the user picked, described by counts only. */
export interface ExportPreview {
  name: string;
  size: number;
  logins: number;
  sites: number;
  skipped: number;
}

export interface PasswordImportReport {
  rows: number;
  imported: number;
  /** Already saved with a different password; the file's password replaced it. */
  updated: number;
  /** Already saved with the same password. */
  duplicates: number;
  duplicatesInFile: number;
  skippedAppLogins: number;
  skippedInvalid: number;
  skippedEmpty: number;
  fileErased: boolean;
  fileEraseError: string | null;
  errors: string[];
}

/** A saved login as the shell sees it: the site and user name, never the password. */
export interface LoginSummary {
  id: number;
  origin: string;
  displayUrl: string;
  username: string;
  source: string;
  createdAt: number;
  lastUsed: number | null;
}

/** A saved address, described by a label to recognise it by. */
export interface ProfileSummary {
  id: number;
  label: string;
}

export interface FillOptions {
  logins: LoginSummary[];
  profileCount: number;
}

export type FillState = "filled" | "usernameOnly" | "noForm" | "originMismatch";

export interface FillResult {
  state: FillState;
  fields: number;
}

/** Windows registration and default-browser state (default_browser.rs). */
export interface DefaultBrowserStatus {
  registered: boolean;
  registeredHere: boolean;
  defaultHttp: boolean;
  defaultHttps: boolean;
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

/** Progress of an update that is being installed (`pb:update`). */
export type UpdateEvent = { kind: "downloading"; downloaded: number; total: number | null } | { kind: "installing" };

export type AiProvider = "nvidia";

/** Whether something is supported: known yes, known no, or not known yet. */
export type Support = "yes" | "no" | "unknown";

export interface AiCapabilities {
  chat: Support;
  vision: Support;
  tools: Support;
}

export interface AiConfigView {
  provider: AiProvider;
  model: string;
  hasKey: boolean;
  /** A key saved by an older version for a provider that is no longer supported is still stored. */
  staleKey: boolean;
  /** What the user's own checks proved about the selected model. */
  verified: AiCapabilities;
}

export type ModelKind = "chat" | "vision" | "code" | "embedding" | "safety" | "document";

/** One model from NVIDIA's list. `hints` come from the model's name only; they are not proof. */
export interface ModelInfo {
  id: string;
  publisher: string;
  name: string;
  kind: ModelKind;
  hints: AiCapabilities;
}

export type ConnectionState =
  | "ok"
  | "noKey"
  | "invalidKey"
  | "rateLimited"
  | "network"
  | "timeout"
  | "serverError"
  | "badResponse"
  | "empty"
  | "modelUnavailable"
  | "modelRejected"
  | "unconfirmed"
  | "other";

export interface ModelsResponse {
  state: ConnectionState;
  message: string | null;
  models: ModelInfo[];
  /** The models are from an earlier request because this one failed. */
  stale: boolean;
  fetchedAt: number;
}

export interface AiTestResult {
  state: ConnectionState;
  message: string;
  /** NVIDIA accepted the key (a chat request with it succeeded). */
  keyOk: boolean;
  modelCount: number;
  testedModel: string | null;
  latencyMs: number | null;
}

export type AiFeature = "vision" | "tools";

export interface AiProbeResult {
  supported: Support;
  message: string;
}

export interface AiChatMessage {
  role: "user" | "assistant";
  content: string;
}

export interface AiEvent {
  requestId: string;
  kind: "delta" | "done" | "error";
  text: string;
  /** For errors: what went wrong. */
  state?: ConnectionState | null;
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
  | { type: "newWindow" }
  | { type: "newPrivateWindow" }
  | { type: "closeWindow" }
  | { type: "reopenWindow" }
  | { type: "quit" }
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

// ---- Project Memory and page reading -----------------------------------------------------

export type MemoryKind = "note" | "page" | "research" | "session";

/** A web page a saved record came from: its address and title only, never its text. */
export interface MemorySource {
  url: string;
  title: string;
}

/** A record the user chose to keep. */
export interface MemoryItem {
  id: number;
  kind: MemoryKind;
  title: string;
  body: string;
  tags: string[];
  sources: MemorySource[];
  /** Ids of other records the user linked this one to. */
  related: number[];
  createdAt: number;
  updatedAt: number;
}

/** What is sent to create or replace a record. */
export interface MemoryDraft {
  kind: MemoryKind;
  title: string;
  body: string;
  tags: string[];
  sources: MemorySource[];
  related: number[];
}

/** The readable text of one open tab, already cut to the size that was asked for. */
export interface PageSnapshotView {
  tabId: TabId;
  title: string;
  url: string;
  text: string;
  selection: string;
  /** Characters of readable text the page has before it was cut. */
  totalChars: number;
  truncated: boolean;
}

// ---- Project Shield -----------------------------------------------------------------------------

export type ShieldListState = "missing" | "ready" | "updating" | "failed";

export interface ShieldList {
  id: string;
  name: string;
  description: string;
  homepage: string;
  license: string;
  enabled: boolean;
  state: ShieldListState;
  error: string | null;
  rules: number;
  fetchedAt: number | null;
  version: string | null;
}

export interface ShieldSite {
  host: string;
  enabled: boolean;
  /** Blocked on this site since the browser started (private tabs are not counted). */
  blocked: number;
}

export interface ShieldEngineStats {
  blockRules: number;
  exceptionRules: number;
  cosmeticSkipped: number;
  unsupportedSkipped: number;
  malformedSkipped: number;
}

export interface ShieldView {
  enabled: boolean;
  lists: ShieldList[];
  userRules: string;
  userRulesAccepted: number;
  disabledSites: string[];
  engine: { rules: number; unindexed: number; stats: ShieldEngineStats };
  blockedSession: number;
  blockedLifetime: number;
  updating: boolean;
  site: ShieldSite | null;
  /** When the core made this view, in ms since 1970. */
  now: number;
}

export interface ShieldTabView {
  blocked: number;
  hosts: { host: string; count: number }[];
}

export interface ShieldUpdateSummary {
  updated: number;
  unchanged: number;
  failed: number;
  alreadyRunning: boolean;
}

/** Blocked-request counts per tab id, sent a few times per second at most. */
export interface ShieldEvent {
  tabs: Record<string, number>;
  total: number;
}

// ---- Windows --------------------------------------------------------------------------------

/** One browser window, as the "move to window" menu lists it. */
export interface WindowBrief {
  label: string;
  title: string;
  private: boolean;
  tabCount: number;
  /** The window that asked. */
  current: boolean;
}

// ---- Project Tunnel -----------------------------------------------------------------------------

export type ProxyKind = "http" | "socks5";

export interface TunnelConfig {
  enabled: boolean;
  kind: ProxyKind;
  host: string;
  port: number;
}

export type TunnelState =
  | { kind: "notSetUp" }
  | { kind: "off" }
  | { kind: "restartNeeded"; turningOn: boolean }
  | { kind: "active" };

export interface TunnelProvider {
  id: string;
  name: string;
  availability: "available" | "unavailable";
  scope: string;
  needs: string;
}

export interface TunnelView {
  config: TunnelConfig;
  state: TunnelState;
  /** The proxy the running browser was started with. */
  running: TunnelConfig | null;
  providers: TunnelProvider[];
}

export type ProbeResult = { kind: "works"; millis: number } | { kind: "refused"; reason: string } | { kind: "unreachable"; reason: string };

// ---- Right-click menus ---------------------------------------------------------------------------

/** One of WebView2's own commands for this menu (run natively when chosen). */
export interface NativeMenuItem {
  commandId: number;
  name: string;
  label: string;
  kind: "command" | "checkbox" | "radio" | "separator" | "submenu";
  enabled: boolean;
  checked: boolean;
  shortcut: string;
  children: NativeMenuItem[];
}

export interface ContextTarget {
  kind: "page" | "image" | "selection" | "audio" | "video";
  editable: boolean;
  mainFrame: boolean;
  pageUrl: string;
  frameUrl: string;
  linkUrl: string | null;
  linkText: string | null;
  sourceUrl: string | null;
  selection: string | null;
}

/** `pb:contextmenu`: a right-click the shell should answer with its menu. */
export interface ContextMenuEvent {
  requestId: number;
  /** The tab whose page was clicked; null for the shell's own text fields. */
  tabId: TabId | null;
  x: number;
  y: number;
  private: boolean;
  target: ContextTarget;
  items: NativeMenuItem[];
}

// ---- Tab drag between windows ------------------------------------------------------------------

/** `pb:tabdrag`: a tab from another window is over this one, left it, or was dropped on it. */
export interface TabDragEvent {
  phase: "over" | "leave" | "drop";
  tabId: TabId;
  x: number;
  y: number;
}

export type DragOutcome = { kind: "window"; label: string } | { kind: "newWindow"; label: string } | { kind: "movedWindow" } | { kind: "cancelled" };

export type GroupOp =
  | { op: "create"; tabs: TabId[]; title: string; color: string }
  | { op: "add"; tab: TabId; group: GroupId }
  | { op: "remove"; tab: TabId }
  | { op: "update"; group: GroupId; title?: string; color?: string; collapsed?: boolean }
  | { op: "ungroup"; group: GroupId }
  | { op: "close"; group: GroupId };
