# Feature matrix (0.7.0 audit)

Every significant browser feature, with its real status. Written from the code on `main` plus the evidence listed at
the bottom. A row only says **Verified** when something actually exercised it: an automated test (logic), a CI run on a
real Windows machine (native behaviour), or a headless-Chrome run of the exact page script that ships. Nothing in this
table has been used by a person in a real Project Browser window during this sprint; `docs/VERIFY-WINDOWS.md` is the
list for that.

| Status | Meaning |
|---|---|
| **Verified (CI)** | Seen working on a real Windows runner: the real executable, real WebView2, real input or real install. |
| **Verified (logic)** | Unit, contract or headless-browser tests prove the logic. The native glue around it is unverified. |
| **Implemented, unverified** | Built and compiled; nobody has watched it work. |
| **Partial** | Works for part of what the name promises; the row says which part. |
| **Broken** | Known not to work. |
| **Missing** | Not built. |
| **Blocked** | Needs something WebView2 (or Windows, or a third party) does not provide; the row says what. |

## Tabs and windows

| Feature | Status | Notes / evidence |
|---|---|---|
| New, close, reopen, duplicate, pin, mute, reorder tabs | Verified (logic) | Tab model tests (`tabs.rs`). Ctrl+T verified in CI by pixels (UI probe). |
| Multiple windows, private windows | Implemented, unverified | Per-window state swapping (`window_set.rs`), tested model. |
| Tab tear-off into a new window | See CI evidence | 0.6.0: the new window's page area was **black** in the CI capture. 0.7.0 re-hosts the moved WebView2 (`native::rehost`); the probe now checks paint and input by pixels. |
| Drag a tab onto another window's strip | Implemented, unverified | `WindowFromPoint` + drop marker; not driven by the probe. |
| Split view | Verified (logic) | Placement, ratio clamping, partner selection, drift rules and the shell's op format are unit-tested. Native layout unverified. |
| Tab groups | Verified (logic) | Model tests; saved with the session. |
| Tab tidy-up (duplicates, sort, group by site, close to the right) | Verified (logic) | `tabtools.test.ts`. |
| Workspaces | Implemented, unverified | Stored as bookmark folders through the normal bookmark commands. |
| Session restore, recently closed tabs and windows | Verified (logic) | Restore planning tests; real restarts unverified. Back/forward history of restored tabs is not restored (WebView2 has no API). |
| Crash recovery | Partial | Session saved 0.8 s after every change; a crashed renderer shows the error page with Reload. No "restore after crash" prompt. |
| Startup behaviour (restore / new tab) | Verified (logic) | |
| Keyboard shortcuts | Verified (logic) | `shortcuts.rs` tests; F1 lists them. |

## Navigation and pages

| Feature | Status | Notes / evidence |
|---|---|---|
| Address bar, search, suggestions | Verified (logic) | Input resolution tests; suggestions from history, bookmarks, tabs. |
| Address-bar calculator, unit conversion, bangs | Verified (logic) | `instant.test.ts` (precedence, refusal of non-maths, conversions, 22 bangs). |
| Command Center (Ctrl+K) | Verified (logic) | Ranking tests; rendered in the dev preview (screenshot). |
| Back, forward, reload, stop, hard reload | Verified (CI) partially | Page load on the runner; buttons' states unverified by a person. |
| Find in page | Implemented, unverified | WebView2 find API. |
| Zoom, per-site zoom memory | Implemented, unverified | Zoom steps tested; per-site memory is shell storage. |
| Reader view and Listen | Verified (logic) | `reader_extract.js` run in headless Chrome on Wikipedia (3,468 words, starts at the lead paragraph), MDN, BBC News; index pages correctly give "no article". Listen uses Windows voices via the Web Speech API: unverified in WebView2. |
| Screenshots (visible / whole page) | Implemented, unverified | DevTools `Page.captureScreenshot`; file naming tested. |
| Printing | Partial | Ctrl+P uses WebView2's print dialog; no menu item. |
| Save page as | Partial | Offered in the right-click menu (WebView2 command). |
| PDF viewing | Partial | WebView2's built-in viewer (the new PDF UI is turned off by a browser flag). Downloading a PDF works as a download. |
| Full screen | Implemented, unverified | F11 and page full screen. |
| Right-click menu (back, forward, reload, links, images, copy/paste, PiP) | Verified (CI) | The probe right-clicks a real page and sees our menu (pixels), Escape closes it (pixels). Individual commands are WebView2's. |
| Pop-up blocking, sign-in pop-ups | Implemented, unverified | |
| Permission prompts (camera, mic, location, notifications) | Implemented, unverified | Decisions stored per site; never in private tabs. |
| Downloads: progress, pause, resume, cancel, open, show in folder | Implemented, unverified | 0.4.0 fixed the stuck-download bug found in a real log. |
| Default zoom setting | Missing | |

## Privacy, data and profiles

| Feature | Status | Notes / evidence |
|---|---|---|
| Project Shield network blocking | Verified (logic) | Rule engine tests; lists downloaded on the runner (110k rules, see CI log). Real-page behaviour unverified by a person. |
| Shield element hiding | Verified (logic) | Site-specific rules only. |
| Shield YouTube ad script | Implemented, unverified | Not shown working on youtube.com. YouTube changes often; this can break without notice. |
| SponsorSkip | Verified (logic) | SHA-256, prefix privacy, response parsing (`sponsor.rs` tests); the live API answers in the expected format (79 videos share one prefix); the in-page skipper passes play-through, seek-into, Undo and wrong-video tests with real video playback in headless Chrome. On youtube.com inside WebView2: unverified. |
| Link Cleaner | Verified (logic) | `linkclean.rs` tests (exact query preservation, per-site rules, loop guard). Native cancel-and-reload unverified. |
| Focus Mode, Screen Time, daily limits | Verified (logic) | `focus.test.ts`. Blocking in a real window unverified. |
| Private browsing | Implemented, unverified | WebView2 private mode; private tabs never saved, never counted by Screen Time, no SponsorSkip lookups. |
| Clear browsing data | Implemented, unverified | |
| Site permissions list | Implemented, unverified | |
| Password import (Chrome CSV), origin-exact fill, DPAPI vault | Verified (logic) | Fill scripts tested in headless Chrome (0.2.0). |
| Chrome import (bookmarks, history, open tabs, search engine, addresses) | Verified (logic) | Real-profile run unverified. |
| Active Chrome sign-ins | Blocked | Chrome encrypts them for itself (app-bound); copying them would bypass that protection. |
| Profiles | Implemented, unverified | Switching restarts the app (WebView2 binds one data folder per process). |
| Notes panel | Implemented, unverified | Shell storage on this computer. |
| Proxy (Project Tunnel) | Implemented, unverified | A browser proxy; not a VPN. |

## Windows integration

| Feature | Status | Notes / evidence |
|---|---|---|
| Listed in Default apps, HTTP/HTTPS/.htm/.html associations | Verified (CI) | Installer registration checked value by value on the runner (18/18 in 0.6.0). |
| Links from other apps reach the running browser | Verified (CI) | Registered command run cold, then a second link forwarded, then a local file. |
| Choosing it as default in Windows Settings | Blocked (needs a person) | Windows' protected user choice; never written by the app. |
| Uninstall removes only its own registration | Verified (CI) | |
| Settings > Default browser status | Implemented, unverified | Reads UserChoice and UserChoiceLatest. |

## Extensions

| Feature | Status | Notes / evidence |
|---|---|---|
| Load unpacked MV3 extension, list, enable/disable, remove | Implemented, unverified | `ICoreWebView2Profile7`. Content scripts and declarativeNetRequest rules are what WebView2 runs. |
| Extension toolbar buttons and pop-ups | Blocked | WebView2 has no browser-action API. Needs the CEF Chrome-style engine (`docs/ENGINE.md`). |
| Chrome Web Store, `.crx` install, extension updates | Blocked | Not offered by WebView2. |
| Extensions in private tabs | Blocked by design | WebView2 does not run them there. |

## AI (NVIDIA)

| Feature | Status | Notes / evidence |
|---|---|---|
| Key storage (Windows Credential Manager), connection test, model list | Verified (logic) | Real NVIDIA key never available during development: the test with a real key is unverified. |
| Chat, summaries, research, claim checks, page restyle | Verified (logic) | Context limits enforced in the core (tested). Answers from a real model unverified. |
| AI Actions (allowlisted, plan review, confirmation) | Verified (logic) | No shell, file or credential access exists to grant. |

## Engine

| Feature | Status | Notes / evidence |
|---|---|---|
| Pages rendered by WebView2 (Microsoft Edge runtime) | Verified (CI) | This is the engine today. It is not bundled with the app. |
| Bundled Chromium (CEF) | Partial (prototype) | `.github/workflows/cef-prototype.yml` builds CEF 152 and renders real pages on a runner. Not integrated into the browser; see `docs/ENGINE.md`. |

## Evidence (CI, this sprint)

Filled from the runs this release was built from; see the release notes for links.
