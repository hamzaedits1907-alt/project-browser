import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type {
  AiChatMessage,
  AiConfigView,
  AiEvent,
  AiFeature,
  AiProbeResult,
  AiTestResult,
  AppInfo,
  DefaultBrowserStatus,
  ExtensionInfo,
  Bookmark,
  BrowserCommand,
  BrowserState,
  Bootstrap,
  ContextMenuEvent,
  RestoreReport,
  DragOutcome,
  MediaAction,
  MediaStatus,
  ReaderArticle,
  SplitOp,
  GroupId,
  GroupOp,
  TabDragEvent,
  ClearOptions,
  ContentRect,
  DetectedProfile,
  Download,
  DownloadCommand,
  ExportPreview,
  FillOptions,
  FillResult,
  FindState,
  HistoryEntry,
  ImportChoices,
  ImportReport,
  LoginSummary,
  MemoryDraft,
  MemoryItem,
  ModelsResponse,
  NtpData,
  PageSnapshotView,
  PasswordImportReport,
  PerfSample,
  PermissionPrompt,
  ProfileMeta,
  ProfilesInfo,
  ProfileSummary,
  SearchEngine,
  Settings,
  ShieldEvent,
  ShieldTabView,
  ShieldUpdateSummary,
  ShieldView,
  SitePermission,
  SourceBrowser,
  Suggestion,
  TabId,
  UiEvent,
  UpdateEvent,
  ProbeResult,
  TunnelConfig,
  TunnelView,
  UpdateStatus,
  Viewport,
  WindowBrief,
} from "./types";

/**
 * Every Tauri command the shell may call. This list is checked by the test
 * suite against `all_commands!` (Rust), `build.rs` and `capabilities/shell.json`.
 */
export const COMMANDS = [
  "shell_ready",
  "set_layout",
  "overlay_open",
  "overlay_close",
  "run_command",
  "tab_new",
  "tab_close",
  "tab_close_others",
  "tab_activate",
  "tab_reopen",
  "tab_pin",
  "tab_move",
  "tab_mute",
  "tab_sleep",
  "tab_duplicate",
  "navigate",
  "nav",
  "zoom",
  "find_query",
  "find_step",
  "find_close",
  "suggest",
  "ntp_data",
  "history_query",
  "history_delete",
  "bookmarks_all",
  "bookmark_create",
  "bookmark_update",
  "bookmark_move",
  "bookmark_delete",
  "bookmark_toggle",
  "downloads_list",
  "download_command",
  "downloads_clear",
  "settings_get",
  "settings_set",
  "gaming_set",
  "permission_respond",
  "site_permissions",
  "site_permission_set",
  "clear_data",
  "perf_subscribe",
  "profiles_list",
  "profile_create",
  "profile_rename",
  "profile_switch",
  "first_run_complete",
  "import_detect",
  "import_run",
  "passwords_pick_file",
  "passwords_import",
  "passwords_list",
  "passwords_delete",
  "passwords_clear",
  "autofill_profiles",
  "autofill_profile_delete",
  "fill_options",
  "fill_login",
  "fill_profile",
  "ai_config_get",
  "ai_config_set",
  "ai_models",
  "ai_test",
  "ai_probe",
  "ai_chat",
  "ai_ask",
  "ai_page_snapshot",
  "page_style",
  "memory_list",
  "memory_save",
  "memory_delete",
  "memory_clear",
  "ai_cancel",
  "app_info",
  "default_browser_status",
  "extensions_list",
  "extension_add",
  "extension_remove",
  "extension_set_enabled",
  "default_browser_open_settings",
  "update_check",
  "update_install",
  "popup_decision",
  "tunnel_state",
  "tunnel_set",
  "tunnel_probe",
  "tunnel_restart",
  "windows_list",
  "tab_to_window",
  "tab_drag_hover",
  "tab_drag_end",
  "window_new",
  "tab_group",
  "context_menu_choose",
  "backup_export",
  "backup_restore",
  "shell_trace",
  "link_clean_undo",
  "save_text_file",
  "menu_surface_ready",
  "menu_present",
  "menu_dismiss",
  "window_active",
  "shield_state",
  "shield_tab",
  "shield_set",
  "shield_site",
  "shield_update",
  "shield_test",
  "split_view",
  "page_screenshot",
  "screenshot_save",
  "media_control",
  "reader_view",
] as const;

export type CommandName = (typeof COMMANDS)[number];

function call<T = void>(cmd: CommandName, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args);
}

/** The page snapshot arrives as raw JPEG bytes; a blob URL avoids a multi-megabyte base64 string. Empty means none. */
function snapshotUrl(data: ArrayBuffer | Uint8Array | string | null): string | null {
  if (typeof data === "string" || data === null) return data || null;
  if (data.byteLength === 0) return null;
  return URL.createObjectURL(new Blob([data as BlobPart], { type: "image/jpeg" }));
}

/** Commands return their errors as plain strings. */
export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "Something went wrong";
}

export const api = {
  shellReady: () => call<Bootstrap>("shell_ready"),
  setLayout: (content: ContentRect, viewport: Viewport) => call("set_layout", { content, viewport }),
  overlayOpen: () => call<ArrayBuffer | Uint8Array | string | null>("overlay_open").then(snapshotUrl),
  overlayClose: (focusPage = true) => call("overlay_close", { focusPage }),
  runCommand: (command: BrowserCommand) => call("run_command", { command }),

  tabNew: (url?: string, isPrivate?: boolean, background?: boolean) => call("tab_new", { url, private: isPrivate, background }),
  tabClose: (id: TabId) => call("tab_close", { id }),
  tabCloseOthers: (id: TabId) => call("tab_close_others", { id }),
  tabActivate: (id: TabId) => call("tab_activate", { id }),
  tabReopen: () => call("tab_reopen"),
  tabPin: (id: TabId, pinned: boolean) => call("tab_pin", { id, pinned }),
  tabMove: (id: TabId, index: number) => call("tab_move", { id, index }),
  tabMute: (id: TabId, muted: boolean) => call("tab_mute", { id, muted }),
  tabSleep: (id: TabId, unload = false) => call("tab_sleep", { id, unload }),
  tabDuplicate: (id: TabId) => call("tab_duplicate", { id }),

  navigate: (input: string, id?: TabId) => call("navigate", { id, input }),
  nav: (action: "back" | "forward" | "reload" | "hardReload" | "stop", id?: TabId) => call("nav", { id, action }),
  zoom: (mode: "in" | "out" | "reset" | "set", factor?: number, id?: TabId) => call("zoom", { id, mode, factor }),

  findQuery: (query: string, caseSensitive: boolean) => call("find_query", { query, caseSensitive }),
  findStep: (forward: boolean) => call("find_step", { forward }),
  findClose: () => call("find_close"),

  suggest: (input: string) => call<Suggestion[]>("suggest", { input }),
  ntpData: () => call<NtpData>("ntp_data"),

  historyQuery: (text: string, before?: number, limit?: number) => call<HistoryEntry[]>("history_query", { text, before, limit }),
  historyDelete: (visitIds: number[]) => call("history_delete", { visitIds }),

  bookmarksAll: () => call<Bookmark[]>("bookmarks_all"),
  bookmarkCreate: (parentId: number, title: string, url?: string) => call<number>("bookmark_create", { parentId, title, url }),
  bookmarkUpdate: (id: number, title: string, url?: string) => call("bookmark_update", { id, title, url }),
  bookmarkMove: (id: number, parentId: number, index: number) => call("bookmark_move", { id, parentId, index }),
  bookmarkDelete: (id: number) => call("bookmark_delete", { id }),
  bookmarkToggle: (tabId: TabId) => call("bookmark_toggle", { tabId }),

  downloadsList: () => call<Download[]>("downloads_list"),
  downloadCommand: (id: number, command: DownloadCommand) => call("download_command", { id, command }),
  downloadsClear: () => call("downloads_clear"),

  settingsGet: () => call<Settings>("settings_get"),
  settingsSet: (settings: Settings) => call<Settings>("settings_set", { settings }),
  gamingSet: (on: boolean) => call<Settings>("gaming_set", { on }),

  permissionRespond: (id: number, allow: boolean, remember: boolean) => call("permission_respond", { id, allow, remember }),
  sitePermissions: () => call<SitePermission[]>("site_permissions"),
  sitePermissionSet: (origin: string, kind: string, decision: "allow" | "block" | null) =>
    call("site_permission_set", { origin, kind, decision }),
  clearData: (options: ClearOptions) => call("clear_data", { options }),

  perfSubscribe: (key: string, on: boolean) => call<PerfSample | null>("perf_subscribe", { key, on }),

  profilesList: () => call<ProfilesInfo>("profiles_list"),
  profileCreate: (name: string) => call<ProfileMeta>("profile_create", { name }),
  profileRename: (id: string, name: string) => call("profile_rename", { id, name }),
  profileSwitch: (id: string) => call("profile_switch", { id }),
  firstRunComplete: (profileName?: string, searchEngine?: SearchEngine) =>
    call("first_run_complete", { profileName, searchEngine }),

  importDetect: () => call<DetectedProfile[]>("import_detect"),
  importRun: (browser: SourceBrowser, profileDir: string, choices: ImportChoices) =>
    call<ImportReport>("import_run", { browser, profileDir, ...choices }),

  /** Opens the Windows file dialog; the chosen path stays in the core. `null` when the user cancels. */
  passwordsPickFile: () => call<ExportPreview | null>("passwords_pick_file"),
  passwordsImport: (deleteFile: boolean) => call<PasswordImportReport>("passwords_import", { deleteFile }),
  passwordsList: () => call<LoginSummary[]>("passwords_list"),
  passwordsDelete: (id: number) => call("passwords_delete", { id }),
  passwordsClear: () => call<number>("passwords_clear"),
  autofillProfiles: () => call<ProfileSummary[]>("autofill_profiles"),
  autofillProfileDelete: (id: number) => call("autofill_profile_delete", { id }),
  fillOptions: (tabId: TabId) => call<FillOptions>("fill_options", { tabId }),
  fillLogin: (tabId: TabId, loginId: number) => call<FillResult>("fill_login", { tabId, loginId }),
  fillProfile: (tabId: TabId, profileId: number) => call<FillResult>("fill_profile", { tabId, profileId }),

  aiConfigGet: () => call<AiConfigView>("ai_config_get"),
  /** Omitted fields are kept. `apiKey: ""` removes the saved key; the key is never returned. */
  aiConfigSet: (changes: { model?: string; apiKey?: string; removeStaleKey?: boolean }) => call<AiConfigView>("ai_config_set", changes),
  aiModels: (refresh = false) => call<ModelsResponse>("ai_models", { refresh }),
  /** Tests the saved key, or a key that was just typed (which is not saved by this call). */
  aiTest: (apiKey?: string) => call<AiTestResult>("ai_test", { apiKey }),
  aiProbe: (model: string, feature: AiFeature) => call<AiProbeResult>("ai_probe", { model, feature }),
  aiChat: (requestId: string, messages: AiChatMessage[], includePage: boolean) =>
    call("ai_chat", { requestId, messages, includePage }),
  aiCancel: (requestId: string) => call("ai_cancel", { requestId }),
  /**
   * A streamed reply to a request the shell built (comparison, claim check, plan). Text arrives as `pb:ai` events for
   * `requestId`. The core refuses requests above the user's context limit.
   */
  aiAsk: (requestId: string, system: string, messages: AiChatMessage[], maxTokens?: number) =>
    call("ai_ask", { requestId, system, messages, maxTokens }),
  /** The readable text of one open tab. The core refuses private tabs, browser pages and tabs that are not running. */
  aiPageSnapshot: (tabId: TabId, maxChars?: number) => call<PageSnapshotView>("ai_page_snapshot", { tabId, maxChars }),
  /** Adds (a string) or removes (`null`) the assistant's style in the active page. Local to that view. */
  /** `onlyIfLight`: leave pages that are already dark alone (Dark pages). */
  pageStyle: (tabId: TabId, css: string | null, onlyIfLight = false) => call<boolean>("page_style", { tabId, css, onlyIfLight }),
  /** Link Cleaner's Undo: loads the original address in that tab once, uncleaned. */
  /** Saves text to a file the user picks (Save dialog). Null when cancelled. */
  saveTextFile: (suggested: string, text: string) => call<string | null>("save_text_file", { suggested, text }),
  linkCleanUndo: (tabId: TabId, url: string) => call("link_clean_undo", { tabId, url }),

  memoryList: (query?: string, limit?: number) => call<MemoryItem[]>("memory_list", { query, limit }),
  /** Creates a record, or replaces record `id`. */
  memorySave: (draft: MemoryDraft, id?: number) => call<MemoryItem>("memory_save", { draft, id }),
  memoryDelete: (id: number) => call<boolean>("memory_delete", { id }),
  memoryClear: () => call<number>("memory_clear"),

  appInfo: () => call<AppInfo>("app_info"),
  defaultBrowserStatus: () => call<DefaultBrowserStatus>("default_browser_status"),
  defaultBrowserOpenSettings: () => call("default_browser_open_settings"),
  extensionsList: () => call<ExtensionInfo[]>("extensions_list"),
  extensionAdd: () => call<ExtensionInfo | null>("extension_add"),
  extensionRemove: (id: string) => call("extension_remove", { id }),
  extensionSetEnabled: (id: string, enabled: boolean) => call("extension_set_enabled", { id, enabled }),
  updateCheck: () => call<UpdateStatus>("update_check"),
  updateInstall: () => call("update_install"),

  /** `site` (an address or host) adds that site's state to the answer. */
  shieldState: (site?: string) => call<ShieldView>("shield_state", { site }),
  shieldTab: (tabId: TabId) => call<ShieldTabView>("shield_tab", { tabId }),
  /** Omitted fields are kept. */
  shieldSet: (patch: { enabled?: boolean; listId?: string; listEnabled?: boolean; userRules?: string }) => call<ShieldView>("shield_set", { patch }),
  shieldSite: (host: string, enabled: boolean) => call<ShieldView>("shield_site", { host, enabled }),
  shieldUpdate: (force: boolean) => call<ShieldUpdateSummary>("shield_update", { force }),
  tunnelState: () => call<TunnelView>("tunnel_state"),
  /** Saves the proxy settings; they take effect after a restart. */
  tunnelSet: (config: TunnelConfig) => call<TunnelView>("tunnel_set", { config }),
  tunnelProbe: (config: TunnelConfig) => call<ProbeResult>("tunnel_probe", { config }),
  tunnelRestart: () => call("tunnel_restart"),
  /** What the user chose for a blocked pop-up: open it once, always allow this site, or dismiss. */
  popupDecision: (tabId: TabId, action: "open" | "allow" | "dismiss") => call("popup_decision", { tabId, action }),
  /** All browser windows (the asking window is marked `current`). */
  windowsList: () => call<WindowBrief[]>("windows_list"),
  /** Moves a tab to the window `target` (at `index` of its strip), or to a new window. The page keeps running. */
  tabToWindow: (id: TabId, target?: string, index?: number) => call<string>("tab_to_window", { id, target, index }),
  /** The tab is being dragged outside its strip: the window under the pointer is told (it shows a drop marker). */
  tabDragHover: (id: TabId) => call<string | null>("tab_drag_hover", { id }),
  /** The drag ended; a tab pulled out of its strip joins the window under the pointer or becomes a new window. */
  tabDragEnd: (id: TabId, detached: boolean, cancel = false) => call<DragOutcome>("tab_drag_end", { id, detached, cancel }),
  windowNew: (url?: string, isPrivate?: boolean) => call("window_new", { url, private: isPrivate }),
  tabGroup: (op: GroupOp) => call<GroupId | null>("tab_group", { op }),
  /** Closes right-click menu `request`, running WebView2's command `command` if one was chosen. */
  contextMenuChoose: (request: number, command: number | null) => call("context_menu_choose", { request, command }),
  /** The menu window (only) has loaded. */
  /** Writes a backup file (the user picks where); `shell` is the shell's own preferences. Null when cancelled. */
  backupExport: (shell: Record<string, unknown>) => call<string | null>("backup_export", { shell }),
  /** Restores a backup file the user picks. Null when cancelled. */
  backupRestore: () => call<RestoreReport | null>("backup_restore"),
  /** One short diagnostic line into the log (lib/trace.ts). */
  shellTrace: (message: string) => call("shell_trace", { message }),
  menuSurfaceReady: () => call("menu_surface_ready"),
  /** The menu window (only): place the rendered menu of this CSS size at the click and show it. */
  menuPresent: (width: number, height: number) => call("menu_present", { width, height }),
  /** The menu window (only): hide. */
  menuDismiss: () => call("menu_dismiss"),
  /** This browser window is the one Windows has in front. */
  windowActive: () => call<boolean>("window_active"),
  shieldTest: (url: string, page: string) => call<string>("shield_test", { url, page }),
  /** Opens, closes, swaps or resizes this window's split view. */
  splitView: (op: SplitOp) => call("split_view", { op }),
  /** A PNG of the tab (visible part or the whole page) as bytes, for the clipboard or a preview. */
  pageScreenshot: (tabId: TabId, full: boolean) =>
    call<ArrayBuffer | Uint8Array>("page_screenshot", { tabId, full }).then((d) => new Blob([d as BlobPart], { type: "image/png" })),
  /** Saves a PNG of the tab in the download folder; returns its path. */
  screenshotSave: (tabId: TabId, full: boolean) => call<string>("screenshot_save", { tabId, full }),
  mediaControl: (tabId: TabId, action: MediaAction) => call<MediaStatus>("media_control", { tabId, action }),
  readerView: (tabId: TabId) => call<ReaderArticle>("reader_view", { tabId }),
};

/** Events emitted by the controller. Only the shell webview ever receives them. */
export interface EventMap {
  "pb:state": BrowserState;
  "pb:downloads": Download[];
  "pb:prompts": PermissionPrompt[];
  "pb:find": FindState | null;
  "pb:settings": Settings;
  "pb:bookmarks": null;
  "pb:perf": PerfSample;
  "pb:ui": UiEvent;
  "pb:ai": AiEvent;
  "pb:update": UpdateEvent;
  "pb:shield": ShieldEvent;
  "pb:contextmenu": ContextMenuEvent;
  "pb:tabdrag": TabDragEvent;
}

export type EventName = keyof EventMap;

export function on<K extends EventName>(name: K, handler: (payload: EventMap[K]) => void): Promise<() => void> {
  return listen<EventMap[K]>(name, (e) => handler(e.payload));
}
