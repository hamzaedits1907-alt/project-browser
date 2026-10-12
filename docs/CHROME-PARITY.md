# Chrome feature parity (0.4.0)

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
| Drag a tab out into a new window / into another window | Built, unverified | Uses HTML5 drag and drop. Whether Chromium passes a custom drag type between two WebView2 windows has not been observed; the tab menu ("Move tab to…") does the same without dragging. |
| New tab, close, reopen closed tab | Tested | |
| Pin / unpin, reorder | Tested | |
| Duplicate, mute, loading indicator, tab search | Built, unverified | |
| Tab groups and group colours | Missing, feasible | Not built. |
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
| Save page as | Platform default | Via the page's context menu (WebView2). |
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
| Browser extensions | Platform limit (mostly) | WebView2 can load some unpacked Chromium extensions through an experimental API, but not the Chrome Web Store, and not reliably. Not offered; no fake button. |
| Chrome Sync, Google account services, Google Pay, Safe Browsing reporting | Google | Not available. |
| Themes from the Chrome Web Store | Google | Not available; the browser has its own accent colours. |

## Speed

See `docs/BENCHMARK.md` for the method. **No new benchmark numbers are published for 0.4.0**: nothing could be run on the
build PC. What changed for speed is listed in `docs/SHIELD.md` (the rule index) and the release notes; they are design
facts, not measurements.
