import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type {
  AiChatMessage,
  AiConfigView,
  AiEvent,
  AiProvider,
  AppInfo,
  Bookmark,
  BrowserCommand,
  BrowserState,
  Bootstrap,
  ClearOptions,
  ContentRect,
  DetectedProfile,
  Download,
  DownloadCommand,
  FindState,
  HistoryEntry,
  ImportReport,
  NtpData,
  PerfSample,
  PermissionPrompt,
  ProfileMeta,
  ProfilesInfo,
  SearchEngine,
  Settings,
  SitePermission,
  SourceBrowser,
  Suggestion,
  TabId,
  UiEvent,
  UpdateStatus,
  Viewport,
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
  "ai_config_get",
  "ai_config_set",
  "ai_chat",
  "ai_cancel",
  "app_info",
  "update_check",
  "update_install",
] as const;

export type CommandName = (typeof COMMANDS)[number];

function call<T = void>(cmd: CommandName, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args);
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
  overlayOpen: () => call<string | null>("overlay_open"),
  overlayClose: (focusPage = true) => call("overlay_close", { focusPage }),
  runCommand: (command: BrowserCommand) => call("run_command", { command }),

  tabNew: (url?: string, isPrivate?: boolean) => call("tab_new", { url, private: isPrivate }),
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
  importRun: (browser: SourceBrowser, profileDir: string, bookmarks: boolean, history: boolean, searchEngine: boolean) =>
    call<ImportReport>("import_run", { browser, profileDir, bookmarks, history, searchEngine }),

  aiConfigGet: () => call<AiConfigView>("ai_config_get"),
  aiConfigSet: (provider: AiProvider, baseUrl: string, model: string, apiKey?: string) =>
    call<AiConfigView>("ai_config_set", { provider, baseUrl, model, apiKey }),
  aiChat: (requestId: string, messages: AiChatMessage[], includePage: boolean) =>
    call("ai_chat", { requestId, messages, includePage }),
  aiCancel: (requestId: string) => call("ai_cancel", { requestId }),

  appInfo: () => call<AppInfo>("app_info"),
  updateCheck: () => call<UpdateStatus>("update_check"),
  updateInstall: () => call("update_install"),
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
}

export type EventName = keyof EventMap;

export function on<K extends EventName>(name: K, handler: (payload: EventMap[K]) => void): Promise<() => void> {
  return listen<EventMap[K]>(name, (e) => handler(e.payload));
}
