# Baseline before the 0.5.0 overhaul (Stage 0 audit)

Recorded on 2026-10-12 before any 0.5.0 change, so that nothing that worked in 0.4.0 is lost when the interface is
replaced. Every item here must still be reachable in 0.5.0 (see the "0.5.0 location" column of each table, filled in when
the redesign landed).

## Repository state

| Item | Value |
|---|---|
| Branch / working tree | `main`, clean, up to date with `origin/main` |
| Last commit | `52a0f78` "0.4.0: version bump, docs (parity matrix, Shield, Tunnel, sessions)…" |
| Last green Windows build | run 38241086882 on `52a0f78` (frontend checks, Rust fmt, 254 Rust tests + 2 ignored, Clippy `-D warnings`, NSIS installer, startup smoke test) |
| Published releases | v0.1.1, v0.1.2, v0.2.0, v0.3.0, v0.4.0 (each with `latest.json`, setup exe and `.sig`) |
| Version in every manifest | 0.4.0 (package.json, Cargo.toml, Cargo.lock, tauri.conf.json) |
| Updater | `plugins.updater` with the minisign public key `4AB7414CBFB37076`, endpoint `releases/latest/download/latest.json`, passive install; private key outside the repo, repo secret `TAURI_SIGNING_PRIVATE_KEY`. Unchanged by 0.5.0. |
| Frontend tests | 325 passing (`vitest`), typecheck and ESLint clean |
| WebView2 runtime seen on the user's PC | 154.0.4258.62 |

## Real-world evidence available at the start (the user's own log, read-only)

`%LOCALAPPDATA%\com.projectbrowser.desktop\logs\project-browser.log` shows:

* The in-app updater worked end to end four times: 0.1.2 → 0.2.0 → 0.3.0 → 0.4.0, each "package downloaded and signature
  verified … starting the installer", followed by the new version starting.
* 0.4.0 sessions: no errors, no panics. Shell UI ready 519–702 ms after process start. A private window was opened and
  closed. Four tabs were moved into new windows (`tab N moved from main to win-M`), the main window was closed while
  another stayed open, and the next start restored one window. Shield lists downloaded and compiled (109,881 rules).
* Earlier versions' defects visible in the log and already fixed in 0.4.0: omnibox char-boundary panic (`navigation.rs:90`),
  download actions failing with `0x8007139F` after the owning webview was closed.
* An old `0.1.1` installer in the Downloads folder was run once after 0.4.0 was installed (log line "Project Browser 0.1.1
  starting"); the next start was 0.4.0 again. Not a defect of the app, but the updater leaves its downloaded installers in
  `%TEMP%\Project Browser-<version>-updater-*` (eight of them, 0.1.2–0.4.0).

## Visible screens

| Screen | Where (0.4.0) |
|---|---|
| Title bar with brand mark, tab strip, tab search button, window controls, private-window badge | `components/chrome/TitleBar.tsx`, `TabStrip.tsx`, `TabItem.tsx`, `WindowControls.tsx` |
| Toolbar: back/forward/reload-stop, omnibox, Gaming toggle, Shield button with count, AI button, downloads button with count, main menu | `Toolbar.tsx`, `Omnibox.tsx` |
| Omnibox: security chip, URL display, suggestions list, zoom chip, bookmark star, saved-login key | `Omnibox.tsx`, `menus/SitePanel.tsx`, `menus/FillPanel.tsx` |
| Bookmarks bar with folder and overflow menus, bookmark context menu | `BookmarksBar.tsx` |
| Docked bars: find, permission prompt, blocked pop-up | `FindBar.tsx`, `PermissionBar.tsx`, `PopupBar.tsx` |
| Popovers: main menu, downloads, Shield, site information, saved-login fill, tab search, tab context menu | `components/menus/*`, `TabStrip.tsx` |
| Toasts | `Toasts.tsx` |
| New Tab page: greeting, search, resource stats, most visited, continue browsing, recent bookmarks, recently closed, import welcome, Gaming card | `pages/NewTab.tsx` |
| Settings with search: General, Appearance, Performance, Gaming Mode, Privacy & security, Project Shield, Project Tunnel, Passwords & autofill, Profiles, Import data, AI assistant, Shortcuts, About | `pages/Settings.tsx`, `pages/settings/*` |
| History, Bookmarks manager, Downloads, Performance monitor, Gaming dashboard, Project Memory, Research | `pages/*.tsx` |
| AI panel (Chat and Act) | `features/ai/AiPanel.tsx`, `features/actions/ActionsTab.tsx` |
| Dialogs: first run, import wizard (+ checklist, password import), clear browsing data | `features/*` |
| Error page for failed navigations (inside the tab webview) | `src-tauri/src/error_page.rs` |
| Fatal start-up screen, boot placeholder, development-preview notice | `App.tsx`, `DevPreviewNotice.tsx` |

## Keyboard shortcuts (handled natively, `src-tauri/src/shortcuts.rs`)

Ctrl+T new tab · Ctrl+Shift+T reopen tab/window · Ctrl+N new window · Ctrl+Shift+N private window · Ctrl+Shift+W close
window · Ctrl+W / Ctrl+F4 close tab · Ctrl+Tab / Ctrl+PgDn next tab · Ctrl+Shift+Tab / Ctrl+PgUp previous tab · Ctrl+1…8
tab by position, Ctrl+9 last tab · Ctrl+L / Ctrl+E / Ctrl+K / Alt+D / F6 address bar · Ctrl+R / F5 reload · Ctrl+Shift+R /
Ctrl+F5 / Shift+F5 hard reload · Alt+← back · Alt+→ forward · Ctrl+F find · F3 / Ctrl+G next match · Shift+F3 previous
match · Ctrl+D bookmark · Ctrl+Shift+B bookmarks bar · Ctrl+H history · Ctrl+J downloads · Ctrl+Shift+O bookmarks ·
Ctrl+, settings · Shift+Esc performance · Ctrl+Shift+G Gaming Mode · Ctrl+Shift+D Gaming dashboard · Ctrl+= / Ctrl+- /
Ctrl+0 zoom · F11 full screen. Shell-only keys: Escape closes popovers and dialogs; arrow keys and Enter in the omnibox list.

## Native commands (91, `src-tauri/src/commands.rs`)

shell_ready, set_layout, overlay_open, overlay_close, run_command, tab_new, tab_close, tab_close_others, tab_activate,
tab_reopen, tab_pin, tab_move, tab_mute, tab_sleep, tab_duplicate, navigate, nav, zoom, find_query, find_step, find_close,
suggest, ntp_data, history_query, history_delete, bookmarks_all, bookmark_create, bookmark_update, bookmark_move,
bookmark_delete, bookmark_toggle, downloads_list, download_command, downloads_clear, settings_get, settings_set, gaming_set,
permission_respond, site_permissions, site_permission_set, clear_data, perf_subscribe, profiles_list, profile_create,
profile_rename, profile_switch, first_run_complete, import_detect, import_run, passwords_pick_file, passwords_import,
passwords_list, passwords_delete, passwords_clear, autofill_profiles, autofill_profile_delete, fill_options, fill_login,
fill_profile, ai_config_get, ai_config_set, ai_models, ai_test, ai_probe, ai_chat, ai_ask, ai_page_snapshot, page_style,
memory_list, memory_save, memory_delete, memory_clear, ai_cancel, app_info, update_check, update_install, popup_decision,
tunnel_state, tunnel_set, tunnel_probe, tunnel_restart, windows_list, tab_to_window, shield_state, shield_tab, shield_set,
shield_site, shield_update, shield_test.

Events to the shell: pb:state, pb:downloads, pb:prompts, pb:find, pb:settings, pb:bookmarks, pb:perf, pb:ui, pb:ai,
pb:update, pb:shield.

## Persisted settings (`Settings`, one JSON document per profile)

searchEngine, customSearchTemplate, startup, showBookmarksBar, downloadDir, accent, newTabBackground, reduceMotion,
showRamOnNewTab, sleepingTabsEnabled, sleepAfterMinutes, neverSleepHosts, gamingMode, gamingBackgroundAction,
gamingPauseDownloads, gamingReduceAnimations, gamingLowMemoryTarget, savePasswords, autofill, heavyTabMemoryMb,
protectPinnedTabs, memoryLimitMb, aiMaxContextChars, checkUpdates, popupAllowedHosts, newWindowRequests.

Other persisted state: session windows (tabs, pinned, active, geometry), closed tabs (in memory), bookmarks, history with
visits, downloads, site permissions, vault (DPAPI-sealed logins and addresses), Project Memory, AI model choice (key in the
Windows Credential Manager), Shield configuration and lists (`<data>/shield`), Tunnel configuration (`<data>/tunnel.json`),
profiles (`profiles.json`), onboarding flag. UI conveniences in `localStorage` (`pb:` prefix): last settings section,
AI panel width and tab, Research state.

## Browser features (status as 0.4.0 left them; full matrix in CHROME-PARITY.md)

Tabs (new, close, reopen, pin, reorder by drag, duplicate, mute, sleep/unload, search), multiple windows and private
windows, move tab to window (menu and HTML5 drag), session restore with window geometry, omnibox with suggestions,
bookmarks and bookmarks bar, history with search, downloads with pause/resume/cancel and dangerous-file confirmation,
permission prompts with remembered decisions, find in page, zoom, full screen, pop-up blocking with user-initiated
detection, Project Shield (network blocking, lists, per-site switch, user rules), Project Tunnel (browser proxy), Gaming
Mode, performance monitor, import from Chrome/Edge/Brave, password vault with click-to-fill, NVIDIA AI (chat, page tools,
Research, Claim Battle, Actions, page restyle), Project Memory, profiles, clear browsing data, signed in-app updates.

## Known issues and gaps at the start

* Nothing in 0.4.0 has been checked against a real window by anyone except what the user's log shows (above).
* The page right-click menu is WebView2's own (Microsoft look). The shell suppresses its own context menu except in text
  fields, where the WebView2 menu appears too.
* Drag between windows relies on Chromium passing a custom drag type between two WebView2 windows: unobserved.
* No tab groups, no cosmetic filtering, no session-cookie persistence, no proxy authentication, no system VPN, no
  benchmark numbers for 0.4.0.
* Task Manager: the main process is "Project Browser" (file description, product name and icon are set by the bundle);
  the WebView2 processes are Microsoft's `msedgewebview2.exe`.
* The updater's downloaded installers accumulate in `%TEMP%`.
