# Chrome feature parity (0.6.0)

Project Browser is **not** equivalent to Chrome and does not claim to be. This matrix says, feature by feature, what exists,
what has actually been checked, and what is missing or impossible on this engine (WebView2). Use it to decide what to build
next instead of adding what already works.

**Status words**

| Status | Meaning |
|---|---|
| **Tested** | Covered by automated tests that run in CI (logic, state machines, contracts). It still needs a person to try it in a real window (see `docs/VERIFY-WINDOWS.md`). |
| **Built, unverified** | Implemented and compiled, but its native behaviour has not been seen in a real window by anyone. |
| **Missing, feasible** | Not built; WebView2 or the platform allows it. |
| **Platform limit** | WebView2 does not offer the needed API. |
| **Google / third party** | Needs a Google service or an outside provider; not available. |

"Tested" never means "works in a real window": no real window has been observed during development on the build PC (Smart
App Control blocks unsigned test builds there). The only real-world evidence so far is the user's own log from 0.3.0, which
found the download bug fixed in 0.4.0.

## Windows and tabs

| Capability | Status | Notes |
|---|---|---|
| Several independent windows (Ctrl+N) | Built, unverified | One tab strip, layout, find bar and focus per window; see `controller/window_set.rs`. Tab ids are unique across windows (unit-tested). |
| Private window (Ctrl+Shift+N) | Built, unverified | Private tabs only, never saved. Uses WebView2's private mode: separate from normal cookies and storage. |
| Close window / last window quits | Built, unverified | Other windows keep running. Closing a window with a running download keeps that download's webview alive. |
| Reopen closed window (Ctrl+Shift+T after closing one) | Built, unverified | Last 5 closed windows. |
| Window position and size restored | Built, unverified | Saved per window with the session. |
| Restore all windows after a restart | Built, unverified | `session_windows`; old single-window sessions still load (tested). |
| Move a tab to another or a new window | Built, unverified | The native webview is re-parented, so the page keeps running. Tab-model move is unit-tested. |
| Drag a tab out into a new window / into another window | Built, CI-probed | **0.5.0**: pointer capture in the shell plus `WindowFromPoint` in the core (no HTML5 drag types). Torn-off tabs open a window at the pointer; other windows show a drop marker and take the tab at that position. The CI UI probe tears a tab off with real mouse input. |
| New tab, close, reopen closed tab | Tested | |
| Pin / unpin, reorder | Tested | |
| Duplicate, mute, loading indicator, tab search | Built, unverified | |
| Tab groups and group colours | Tested (model) | **0.5.0**: name, nine colours, collapse, add/remove, ungroup, close; saved with the session. |
| Sleeping tabs / unload | Tested | Beyond Chrome's defaults; see `docs/ARCHITECTURE.md`. |
| Restore tabs after a crash | Built, unverified | The session is saved after changes (debounced 0.8 s) and on exit. |

## Navigation and pages

| Capability | Status | Notes |
|---|---|---|
| Back / forward / reload / stop, address bar, suggestions | Tested (logic) | |
| Bookmarks, bookmarks bar, history search | Tested (logic) | |
| Find in page | Built, unverified | |
| Zoom (per tab, Chrome's steps) | Tested (steps) | A default zoom setting is missing. |
| Fullscreen (F11, page fullscreen) | Built, unverified | |
| Print (Ctrl+P) | Platform default | WebView2's own print dialog; not wired to a menu item. |
| Save page as, print, copy/paste, save image, picture in picture | Built, CI-probed | **0.5.0**: offered in Project Browser's own right-click menu; executed by WebView2 through `SelectedCommandId`. |
| Downloads manager with progress, pause, resume, cancel | Built, unverified | **0.4.0 fixes the stuck download found in the 0.3.0 log**: a download is tied to the tab that started it, and closing that tab used to kill it and leave a row stuck on "in progress". Now the webview stays alive until the download ends, tabs opened only to download are closed, and a dead transfer is reported as interrupted. Every download is logged with its interrupt reason. |
| Safe handling of downloaded programs | Tested | Executables and scripts need a second confirmation to open; Windows SmartScreen still applies. |
| Picture in picture | Platform default | Chromium's own button on video pages. |
| Media controls, tab audio indicator | Built, unverified | |
| Permission prompts (camera, microphone, location, notifications…) | Built, unverified | Docked bar; remembered decisions per site; never remembered in private tabs. |
| Website notifications | Platform limit (partly) | Permission can be granted; there is no Windows notification centre integration beyond WebView2's. |
| External protocol links (mailto:, etc.) | Platform default | WebView2's confirmation. |
| File dialogs for uploads | Platform default | |
| Error pages | Tested (logic) | |
| Pop-up handling | Built, unverified | **0.4.0**: the native `NewWindowRequested` event reports whether a click caused the request. Scripted pop-ups nobody asked for are blocked, with a bar to open it once or always allow the site. Sign-in pop-ups that a click opens (window.open with a size) get a real window that shares the profile (or the private session) and `window.opener`; closing it returns focus to the opener. A setting opens other new-window requests as a tab or a window. |
| Search engine choice | Tested | Six engines and a custom address. |
| Developer tools (F12) | Platform default | |
| Accessibility | Not audited | Keyboard use works through standard controls; no screen-reader audit has been done. |

## Passwords, autofill, privacy

| Capability | Status | Notes |
|---|---|---|
| Passwords imported from a Chrome export, one-click autofill | Tested (logic) | Never auto-submitted; exact-origin matching; sealed with Windows DPAPI. |
| Active Chrome sign-ins (cookies) | Platform limit | Chrome encrypts them for itself. Copying them would bypass that protection and is not done. Sign in once in Project Browser. |
| Staying signed in across restarts | Built, unverified | Normal windows and popups share one WebView2 user-data folder, which persists cookies and storage. Session cookies (no expiry) are not kept; see `docs/SESSIONS.md`. |
| Address autofill | Tested (logic) | Separate from passwords. |
| Clear browsing data | Built, unverified | |
| Per-site permissions list | Built, unverified | |
| Cookie inspection per site | Missing, feasible | |
| Tracking prevention | Built, unverified | **Project Shield** (below). WebView2's own tracking prevention level is not exposed in the settings. |
| Do-not-track header | Missing, feasible | |
| Ad and tracker blocking | Tested (engine), unverified in a window | `docs/SHIELD.md`. Not compared with uBlock Origin or Brave Shields. |
| VPN | Partly | `docs/TUNNEL.md`: a browser proxy you supply. No system-wide VPN, no provider, no servers. |

## Profiles, import, extensions, sync

| Capability | Status | Notes |
|---|---|---|
| Multiple profiles | Built, unverified | Separate databases and WebView2 folders; one is open at a time. |
| Import bookmarks, history, search engine, open tabs, addresses, passwords (via export) | Tested (logic) | Exact status per data type is shown after an import. |
| Browser extensions | Built, unverified (0.6.0) | Unpacked Manifest V3 extensions through WebView2 (`ICoreWebView2Profile7`): load, list, enable/disable, remove (Settings > Extensions). No Chrome Web Store, no `.crx`, no toolbar pop-ups (platform limit). |
| Chrome Sync, Google account services, Google Pay, Safe Browsing reporting | Google | Not available. |
| Themes from the Chrome Web Store | Google | Not available; the browser has its own accent colours. |

## Speed

See `docs/BENCHMARK.md` for the method. **No new benchmark numbers are published for 0.4.0**: nothing could be run on the
build PC. What changed for speed is listed in `docs/SHIELD.md` (the rule index) and the release notes; they are design
facts, not measurements.

## 0.5.0 additions

| Capability | Status | Notes |
|---|---|---|
| Own right-click menu (page, link, image, selection, field, media) | Built, CI-probed | `native/context_menu.rs`; WebView2's menu never shows. |
| Side panels (bookmarks, history, downloads, Shield) | Tested (frontend) | Docked next to the page. |
| Element hiding (cosmetic filters) | Tested (engine) | Site-specific rules only; generic/procedural counted, not applied. |
| Vertical tabs, themes, density, customizable toolbar and sidebar | Tested (frontend) | |
| Extensions | Superseded | See 0.6.0 below. |
| Installable web apps (PWA) | Platform limit | WebView2 has no install API; pinned sidebar sites cover quick access. |

## 0.6.0 additions

| Capability | Status | Notes |
|---|---|---|
| Listed in Windows Default apps / selectable as default browser | Installer registration CI-verified | `windows/hooks.nsh` writes ProgIDs, `StartMenuInternet` + Capabilities and `RegisteredApplications` (HKCU, per-user install). CI installs the real installer and checks every value. Choosing it in Windows Settings needs a person and has not been observed. |
| Opening http/https links and .html files from other apps | CI-verified (registered command) | CI runs the registered `open` command: cold start, a second link forwarded to the running browser, a local file. `launch.rs` refuses `javascript:`/`data:` targets (unit-tested). |
| Default browser status + button to Windows settings | Built, unverified | Settings > General. Reads the real registry; never writes the user's choice. |
| Extensions | Built, unverified | See "Browser extensions" above. |
| Own rendering engine (bundled Chromium / CEF) | Not done | Pages are still rendered by WebView2. See `docs/ENGINE.md` for why and what it would take. |
