// Development-only stand-in for the Rust core, used when the UI is opened in a
// plain browser (`npm run dev`). It lets the shell be built, screenshotted and
// tested without compiling the native app. It is never part of a production build
// (see main.tsx) and nothing here ships in the installer.

import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";

import { DEFAULT_SETTINGS } from "../lib/settings";
import type { Bookmark, BrowserState, Download, PerfSample, Settings, Tab } from "../lib/types";

const now = Date.now();
const MIN = 60_000;

function tab(id: number, url: string, title: string, extra: Partial<Tab> = {}): Tab {
  return {
    id, url, title, favicon: null, pinned: false, private: false, loading: false, canGoBack: false, canGoForward: false, zoom: 1,
    audible: false, muted: false, lifecycle: url.startsWith("browser://") ? "unloaded" : "live", security: url.startsWith("https") ? "secure" : "internal",
    error: null, heavy: false, bookmarked: false, lastActive: now - id * MIN, ...extra,
  };
}

export function installMockBackend(): void {
  const params = new URLSearchParams(location.search);
  const startUrl = params.get("page") ? `browser://${params.get("page")}` : "browser://newtab";

  let settings: Settings = { ...DEFAULT_SETTINGS, gamingMode: params.get("gaming") === "1", accent: params.get("accent") ?? "ion" };
  let nextId = 10;
  let state: BrowserState = {
    tabs: [
      tab(1, "https://github.com/", "GitHub", { pinned: true, bookmarked: true }),
      tab(2, "https://www.youtube.com/watch?v=dQw4w9WgXcQ", "Never Gonna Give You Up - YouTube", { audible: true, heavy: true }),
      tab(3, "https://store.steampowered.com/", "Welcome to Steam", { lifecycle: "sleeping" }),
      tab(4, "https://en.wikipedia.org/wiki/Rust_(programming_language)", "Rust (programming language) - Wikipedia", { loading: params.get("loading") === "1" }),
      tab(5, startUrl, startUrl === "browser://newtab" ? "New Tab" : startUrl.replace("browser://", "")),
    ],
    activeId: params.get("web") === "1" ? 4 : 5,
    closedCount: 2,
    fullscreen: false,
    browserFullscreen: false,
    gaming: settings.gamingMode,
  };
  let bookmarks: Bookmark[] = [
    { id: 1, parentId: null, kind: "folder", title: "Bookmarks bar", url: null, position: 0, createdAt: now },
    { id: 2, parentId: null, kind: "folder", title: "Other bookmarks", url: null, position: 1, createdAt: now },
    { id: 3, parentId: 1, kind: "url", title: "GitHub", url: "https://github.com/", position: 0, createdAt: now },
    { id: 4, parentId: 1, kind: "url", title: "Steam", url: "https://store.steampowered.com/", position: 1, createdAt: now },
    { id: 5, parentId: 1, kind: "url", title: "Twitch", url: "https://www.twitch.tv/", position: 2, createdAt: now },
    { id: 6, parentId: 1, kind: "folder", title: "Dev", url: null, position: 3, createdAt: now },
    { id: 7, parentId: 6, kind: "url", title: "MDN Web Docs", url: "https://developer.mozilla.org/", position: 0, createdAt: now },
    { id: 8, parentId: 2, kind: "url", title: "Hacker News", url: "https://news.ycombinator.com/", position: 0, createdAt: now },
  ];
  const sites = ["github.com", "youtube.com", "store.steampowered.com", "twitch.tv", "reddit.com", "wikipedia.org", "news.ycombinator.com", "discord.com"];
  const history = Array.from({ length: 42 }, (_, i) => {
    const host = sites[i % sites.length] ?? "example.com";
    return { visitId: i + 1, urlId: i + 1, url: `https://${host}/page/${i}`, title: `${host} — page ${i}`, visitedAt: now - i * 47 * MIN };
  });
  let downloads: Download[] = [
    { id: 1, url: "https://example.com/setup.exe", path: "C:\\Users\\me\\Downloads\\game-launcher-setup.exe", filename: "game-launcher-setup.exe", mime: null, totalBytes: 180_000_000, receivedBytes: 96_000_000, state: "inprogress", danger: null, error: null, startedAt: now - 2 * MIN, endedAt: null, private: false, pausedByGaming: false },
    { id: 2, url: "https://example.com/mod.exe", path: "C:\\Users\\me\\Downloads\\mod-installer.exe", filename: "mod-installer.exe", mime: null, totalBytes: 8_000_000, receivedBytes: 8_000_000, state: "completed", danger: "This type of file can run code on your computer. Only open it if you trust the site.", error: null, startedAt: now - 30 * MIN, endedAt: now - 29 * MIN, private: false, pausedByGaming: false },
    { id: 3, url: "https://example.com/wallpaper.png", path: "C:\\Users\\me\\Downloads\\wallpaper.png", filename: "wallpaper.png", mime: "image/png", totalBytes: 3_400_000, receivedBytes: 3_400_000, state: "completed", danger: null, error: null, startedAt: now - 3 * 3600_000, endedAt: now - 3 * 3600_000, private: false, pausedByGaming: false },
  ];
  let subscribers = 0;
  let tick = 0;

  const sample = (): PerfSample => {
    tick++;
    const wave = (a: number, p: number) => a + Math.sin(tick / p) * a * 0.25;
    return {
      timestamp: Date.now(), totalMemory: wave(1_150_000_000, 7), shellMemory: 190_000_000, webviewMemory: wave(960_000_000, 7), totalCpu: wave(6, 3), gpu: wave(11, 5),
      processes: [
        { pid: 4100, kind: "host", label: "Browser host process", memory: 190_000_000, cpu: 0.8, tabIds: [] },
        { pid: 4200, kind: "browser", label: "WebView2 browser process", memory: 120_000_000, cpu: 0.4, tabIds: [] },
        { pid: 4300, kind: "gpu", label: "GPU process", memory: 210_000_000, cpu: 2.1, tabIds: [] },
        { pid: 4400, kind: "renderer", label: "Tab: Never Gonna Give You Up - YouTube", memory: 610_000_000, cpu: wave(3, 4), tabIds: [2] },
        { pid: 4500, kind: "renderer", label: "Tab: GitHub", memory: 160_000_000, cpu: 0.2, tabIds: [1] },
      ],
      tabs: [{ id: 2, memory: 610_000_000, cpu: 3, shared: false, heavy: true }, { id: 1, memory: 160_000_000, cpu: 0.2, shared: false, heavy: false }],
      tabCount: state.tabs.length, liveTabs: state.tabs.filter((t) => t.lifecycle === "live").length, sleepingTabs: state.tabs.filter((t) => t.lifecycle === "sleeping").length, unloadedTabs: state.tabs.filter((t) => t.lifecycle === "unloaded").length, tabAttribution: params.get("noattr") !== "1",
      startup: { processStart: now - 5000, shellReadyMs: 412, firstPageMs: 1380 }, gaming: settings.gamingMode,
    };
  };

  const pushState = () => void emit("pb:state", state);
  const setActive = (id: number) => {
    state = { ...state, activeId: id };
    pushState();
  };
  setInterval(() => {
    if (subscribers > 0) void emit("pb:perf", sample());
  }, 2000);

  mockIPC(
    (cmd, payload) => {
      const a = (payload ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "shell_ready":
          return {
            state, settings, profile: { id: "default", name: "Hamza", color: "#3fc8ff", createdAt: now }, firstRun: params.get("firstrun") === "1",
            downloads, prompts: params.get("prompt") === "1" ? [{ id: 1, tabId: 5, origin: "https://meet.example.com", kind: "camera" }] : [], version: "0.1.0",
          };
        case "overlay_open":
          return null;
        case "tab_activate":
          return setActive(a.id as number);
        case "tab_new": {
          const id = nextId++;
          state = { ...state, tabs: [...state.tabs, tab(id, "browser://newtab", "New Tab")], activeId: id };
          return pushState();
        }
        case "tab_close": {
          const left = state.tabs.filter((t) => t.id !== a.id);
          state = { ...state, tabs: left.length ? left : [tab(nextId++, "browser://newtab", "New Tab")], activeId: left.at(-1)?.id ?? null };
          return pushState();
        }
        case "tab_pin":
          state = { ...state, tabs: state.tabs.map((t) => (t.id === a.id ? { ...t, pinned: a.pinned as boolean } : t)) };
          return pushState();
        case "tab_mute":
          state = { ...state, tabs: state.tabs.map((t) => (t.id === a.id ? { ...t, muted: a.muted as boolean } : t)) };
          return pushState();
        case "tab_sleep":
          state = { ...state, tabs: state.tabs.map((t) => (t.id === a.id ? { ...t, lifecycle: a.unload ? "unloaded" : "sleeping" } : t)) };
          return pushState();
        case "navigate": {
          const input = String(a.input);
          const internal = /^browser:\/\/(\w+)/.exec(input);
          const url = internal ? input : input.startsWith("http") ? input : `https://${input}`;
          state = { ...state, tabs: state.tabs.map((t) => (t.id === state.activeId ? { ...t, url, title: internal ? internal[1] ?? "" : url, lifecycle: internal ? "unloaded" : "live", security: internal ? "internal" : "secure" } : t)) };
          return pushState();
        }
        case "run_command": {
          const c = a.command as { type: string };
          const page = { openSettings: "settings", openHistory: "history", openBookmarks: "bookmarks", openDownloads: "downloads", openPerformance: "performance", openGaming: "gaming" }[c.type];
          if (page) {
            state = { ...state, tabs: state.tabs.map((t) => (t.id === state.activeId ? { ...t, url: `browser://${page}`, title: page, lifecycle: "unloaded" } : t)) };
            pushState();
          } else if (c.type === "toggleGamingMode") {
            settings = { ...settings, gamingMode: !settings.gamingMode };
            void emit("pb:settings", settings);
          } else if (c.type === "find") void emit("pb:ui", { kind: "openFind" });
          return null;
        }
        case "settings_get":
          return settings;
        case "settings_set":
          settings = a.settings as Settings;
          void emit("pb:settings", settings);
          return settings;
        case "gaming_set":
          settings = { ...settings, gamingMode: a.on as boolean };
          void emit("pb:settings", settings);
          return settings;
        case "perf_subscribe":
          subscribers += a.on ? 1 : -1;
          return a.on ? sample() : null;
        case "ntp_data":
          return {
            topSites: history.slice(0, 8).map((h, i) => ({ url: `https://${sites[i]}/`, title: sites[i] ?? "", visitCount: 20 - i, lastVisit: h.visitedAt })),
            recentBookmarks: bookmarks.filter((b) => b.kind === "url").slice(0, 5),
            recentlyClosed: [{ url: "https://reddit.com/r/pcgaming", title: "r/pcgaming", favicon: null }],
            continueBrowsing: history.slice(0, 5).map((h) => ({ url: h.url, title: h.title, visitCount: 1, lastVisit: h.visitedAt })),
          };
        case "suggest": {
          const q = String(a.input);
          return [
            { kind: "search", title: q, url: `https://www.google.com/search?q=${q}`, tabId: null },
            { kind: "tab", title: "GitHub", url: "https://github.com/", tabId: 1 },
            { kind: "bookmark", title: "Steam", url: "https://store.steampowered.com/", tabId: null },
            { kind: "history", title: "r/pcgaming", url: "https://reddit.com/r/pcgaming", tabId: null },
          ];
        }
        case "history_query": {
          const before = (a.before as number | undefined) ?? Infinity;
          const text = String(a.text ?? "").toLowerCase();
          return history.filter((h) => h.visitedAt < before && (!text || h.url.includes(text) || h.title.toLowerCase().includes(text))).slice(0, (a.limit as number) ?? 100);
        }
        case "bookmarks_all":
          return bookmarks;
        case "bookmark_create": {
          bookmarks = [...bookmarks, { id: nextId++, parentId: a.parentId as number, kind: a.url ? "url" : "folder", title: String(a.title), url: (a.url as string | undefined) ?? null, position: 99, createdAt: Date.now() }];
          void emit("pb:bookmarks", null);
          return nextId;
        }
        case "bookmark_delete":
          bookmarks = bookmarks.filter((b) => b.id !== a.id && b.parentId !== a.id);
          void emit("pb:bookmarks", null);
          return null;
        case "downloads_list":
          return downloads;
        case "download_command":
          if (a.command === "remove") downloads = downloads.filter((d) => d.id !== a.id);
          void emit("pb:downloads", downloads);
          return null;
        case "site_permissions":
          return [{ origin: "https://meet.example.com", kind: "camera", decision: "allow", updatedAt: now }, { origin: "https://meet.example.com", kind: "microphone", decision: "block", updatedAt: now }];
        case "profiles_list":
          return { profiles: [{ id: "default", name: "Hamza", color: "#3fc8ff", createdAt: now }, { id: "work", name: "Work", color: "#9d7bff", createdAt: now }], current: "default" };
        case "import_detect":
          return params.get("noimport") === "1" ? [] : [
            { browser: "chrome", browserLabel: "Google Chrome", profileDir: "Default", name: "Person 1", items: [
              { kind: "bookmarks", label: "Bookmarks", supported: true, present: true, detail: "312 bookmarks found" },
              { kind: "history", label: "Browsing history", supported: true, present: true, detail: "Up to 100,000 most recent pages" },
              { kind: "searchEngines", label: "Default search engine", supported: true, present: true, detail: "Default: DuckDuckGo" },
              { kind: "passwords", label: "Saved passwords", supported: false, present: true, detail: "Not imported. Google Chrome protects saved passwords with encryption that only it can unlock. We don't bypass that protection." },
              { kind: "autofill", label: "Autofill (addresses, cards)", supported: false, present: true, detail: "Not imported. Addresses and payment cards are protected by the source browser and are not read." },
              { kind: "cookies", label: "Cookies and site sign-ins", supported: false, present: true, detail: "Not imported. Cookies are encrypted per browser; you will sign in to sites again." },
            ] },
            { browser: "edge", browserLabel: "Microsoft Edge", profileDir: "Default", name: "Profile 1", items: [
              { kind: "bookmarks", label: "Bookmarks", supported: true, present: false, detail: "No bookmarks file" },
              { kind: "history", label: "Browsing history", supported: true, present: true, detail: "Up to 100,000 most recent pages" },
              { kind: "searchEngines", label: "Default search engine", supported: true, present: false, detail: "No custom default search engine set in this profile" },
              { kind: "passwords", label: "Saved passwords", supported: false, present: false, detail: "Not imported." },
            ] },
          ];
        case "import_run":
          return { bookmarksImported: 312, bookmarksSkipped: 0, historyImported: 18200, historySkipped: 0, searchEngine: a.searchEngine ? "DuckDuckGo" : null, errors: [] };
        case "ai_config_get":
          return { provider: "openAiCompatible", baseUrl: "http://localhost:11434/v1", model: params.get("ai") === "1" ? "llama3.2" : "", hasKey: false, keyMigrationPending: params.get("keymigration") === "1" };
        case "ai_chat": {
          const id = String(a.requestId);
          const text = "Here's a **quick summary**:\n\n- The page introduces the topic\n- It lists three key ideas\n- It ends with `references`\n\n```rust\nfn main() { println!(\"hi\"); }\n```";
          let i = 0;
          const timer = setInterval(() => {
            void emit("pb:ai", { requestId: id, kind: "delta", text: text.slice(i, i + 6) });
            i += 6;
            if (i >= text.length) {
              clearInterval(timer);
              void emit("pb:ai", { requestId: id, kind: "done", text: "" });
            }
          }, 60);
          return null;
        }
        case "app_info":
          return { version: "0.1.1", identifier: "com.projectbrowser.desktop", dataDir: "C:\\Users\\me\\AppData\\Local\\com.projectbrowser.desktop", profileDir: "C:\\Users\\me\\AppData\\Local\\com.projectbrowser.desktop\\profiles\\default", profileName: "Hamza", webview2Version: "131.0.2903.86", updateConfigured: true };
        case "update_check": {
          const mode = params.get("update");
          if (mode === "error") throw "Couldn't reach the update server. Check your internet connection and try again.";
          if (mode === "available") return { status: "available", current: "0.1.1", version: "0.1.2", notes: "- Faster startup\n- Fixes for sleeping tabs", date: null };
          return { status: "upToDate", current: "0.1.1" };
        }
        case "update_install": {
          const total = 40_000_000;
          let sent = 0;
          const timer = setInterval(() => {
            sent += 4_000_000;
            void emit("pb:update", sent >= total ? { kind: "installing" } : { kind: "downloading", downloaded: sent, total });
            if (sent >= total) clearInterval(timer);
          }, 250);
          return null;
        }
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}
