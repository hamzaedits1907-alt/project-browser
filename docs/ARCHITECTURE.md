# Architecture

```
┌──────────────────────────── one Tauri window ("main") ────────────────────────────┐
│  Shell webview  (React, src/)      local origin, has IPC                           │
│  ┌ title bar + tabs ┐                                                              │
│  ├ toolbar / omnibox ┤   ← drawn by the shell                                       │
│  ├ bookmarks, find, permission bars (docked rows) ┤                                │
│  └ content area ─────┐                                                             │
│       browser:// page → rendered by the shell (React)                              │
│       web page        → native child WebView2 ("tab-N") placed over this rectangle │
└────────────────────────────────────────────────────────────────────────────────────┘
        ▲ commands (invoke)                    │ events (pb:*)
        │                                      ▼
   src-tauri/src/commands.rs  ──►  controller thread (single owner of all state)
```

## Rust core (`src-tauri/src`)

* **controller/** owns every piece of browser state on one thread: tab strip, webview handles, settings,
  downloads, permission prompts, find state, performance sampling. Commands post closures to it and await the
  result; WebView2 event handlers post and never block. The UI thread never waits on the controller.
* **tabs.rs** is the pure tab-strip model (ordering, pinning, reopen stack), unit-tested without a webview.
* **navigation.rs** resolves omnibox input (URL vs search, `browser://` pages, dangerous schemes).
* **native/** is the only code that touches WebView2 COM objects (permissions, downloads, suspend, find,
  preview capture, page-text extraction).
* **db/** is SQLite (history, bookmarks, downloads, site permissions, key-value store) per profile.
* **import.rs** reads Chrome/Edge/Brave bookmarks, history and the default search engine. `collect` only reads files
  (run on a blocking thread, never on the controller); `apply` writes to the profile database in one quick step. Both
  database writes are idempotent.
* **secrets.rs** secret storage: `SecretStore` trait, Windows Credential Manager implementation, opaque `Secret` type.
* **ai.rs** the NVIDIA backend: model catalog, key check, SSE streaming with a responsive Stop, deadlines and length bounds, the
  user's context limit (`check_context`), page-read rules, page-style checks. [AI.md](AI.md) has the feature-level view.
* **db/memory.rs** Project Memory (schema v3): user-saved records, Unicode-aware search, links between records.
* **window.rs** builds the shell window and tab webviews.
* **updates.rs** in-app updates (`tauri-plugin-updater`, GitHub Releases, signature-checked, user-confirmed): [UPDATES.md](UPDATES.md).
* **controller/sleep_policy.rs** the pure rules for sleeping/unloading tabs; `controller/gaming.rs` applies them. `controller/consistency.rs` checks the tab model against the native views after every step.
* **logging.rs** a small file logger (`<data>/logs/project-browser.log`) so native problems leave evidence.

## The shell (`src/`)

| Path | Purpose |
| --- | --- |
| `lib/ipc.ts` | the only place that calls `invoke`; typed wrappers and the `COMMANDS` list |
| `lib/store.ts` | app state, fed by `pb:*` events; no polling anywhere |
| `lib/overlay.ts` | hides the native page and shows a snapshot while a popup is open (see below) |
| `lib/layout.ts` | reports the content rectangle to the core (resize observer, no polling) |
| `components/chrome/` | title bar, tabs, toolbar, omnibox, bookmarks bar, find/permission bars, content host |
| `components/menus/` | main menu, site panel, downloads popover, tab search |
| `components/ui/` | icons and primitives |
| `pages/` | New Tab, Settings (+ `pages/settings/*`), History, Bookmarks, Downloads, Performance, Gaming, Memory, Research |
| `features/` | import wizard and checklist, first run, clear-data dialog, AI panel, `research/` (page comparison and claim check, quote verification), `actions/` (plan allowlist, confirmation, history), `memory/` |
| `dev/mockBackend.ts` | dev-only mock core (never shipped) |

### Development preview (plain `npm run dev`)

Outside the Tauri window there is no Rust core and no WebView2. `main.tsx` installs a mock core
(`src/dev/mockBackend.ts`, development builds only) so the interface can be developed, and `ContentHost` shows a
**development-preview notice** in place of the page. Nothing there renders a website, and no iframe is used as a
substitute engine (a test forbids it). If the page area is blank in this mode, that is expected: it is the empty
slot where the native webview would sit.

### Lifecycle of a web tab

1. The controller creates the tab in `TabStrip` (pure model) and, when it becomes visible, `create_view` adds a child
   webview (`tab-N`) to the main window at the rectangle the shell reported with `set_layout`.
2. The webview starts on `about:blank`; `native::install_tab` then hooks the WebView2 events and navigates to the
   real URL. Events for the initial blank document are ignored if they arrive late.
3. Only the active tab's webview is shown (`relayout`); others are hidden, and may be suspended (sleeping) or destroyed
   (unloaded). Waking is just becoming visible again; an unloaded tab gets a new webview from its stored URL.
4. Closing a tab destroys its webview and drops the native handles (`native::remove_tab`).

### Sleeping, unloading and the memory limit

* **Sleep** = WebView2 `TrySuspend` on a hidden tab: the page keeps all its state and wakes instantly when shown.
* **Unload** = the webview is destroyed (nothing hidden is kept); the page reloads when opened and the site may lose
  transient state. Before unloading, the page is asked whether it holds typed-in data (changed fields, rich-text boxes); if so,
  or if it does not answer in 1.5 s, it is put to sleep instead.
* Never slept automatically: the active tab, tabs playing audio, loading, waiting on a permission prompt, with a running
  download, on the never-sleep list, and (setting, on by default) pinned tabs. A running download also blocks unloading.
* Idle timer (default 30 min; 10 s in Gaming Mode), Gaming Mode (background tabs sleep or unload, downloads are held
  and resumed afterwards) and an optional **memory limit** (one least-recently-used tab per sample, sleeping first, unloading
  only when nothing is left to sleep) all use the same rules. Suspend answers carry the tab's activation epoch, so one that
  arrives after the tab was switched to is ignored (rapid switching cannot mark a running page as asleep).
* Restored tabs are created unloaded and only get a webview when first shown.

**Not implemented, so nothing here claims it:** automatic detection of a running game (Gaming Mode is a manual switch);
download throttling (WebView2 has no bandwidth control, so Gaming Mode can hold downloads but not slow them); unloading in
response to *Windows'* free-memory pressure (the memory limit is on the browser's own total); a configurable idle time for
Gaming Mode (fixed at 10 s; the normal idle time is configurable). No frame-rate benefit is claimed anywhere: it has not
been measured. The only memory comparisons that exist are in [BENCHMARK.md](BENCHMARK.md): Project Browser against itself
(1/5/10 tabs, normal vs Gaming/sleep vs Gaming/unload) on one PC; none against Chrome, Brave or Opera GX.

### Session restore

The session (URLs, titles, favicons, pinned flag, order, selected tab) is saved to the profile database shortly after
every change and on exit. On start `plan_restore` (pure, unit-tested) decides what to recreate; restored tabs are
**unloaded** and only get a webview when first shown. **Back/forward history is not restored**: WebView2 offers no API to
rebuild a navigation history.

### Why popups need an overlay

A native child webview is always drawn above the shell webview, so a shell menu opened over a web page would be
invisible. While any popup is open the shell calls `overlay_open`: the core captures a snapshot of the page,
hides the webview and returns the snapshot, which the content area shows as a dimmed backdrop. `overlay_close`
brings the live page back (and returns keyboard focus to it unless a shell text box is focused). The find bar,
permission prompt and bookmarks bar are *docked rows* instead, so they never need to hide the page.

### What the performance numbers are

Memory is each process's **private working set** (`PROCESS_MEMORY_COUNTERS_EX2.PrivateWorkingSetSize`, the figure in Task
Manager's Memory column), summed over the host process and the WebView2 processes the browser started (each PID once). It is
not total working set and not commit size, and the Performance page says so. CPU is kernel+user time deltas as a share of all
logical CPUs; GPU is the PDH "GPU Engine" utilisation for those PIDs and shows `n/a` when Windows doesn't provide it. Startup
is measured from the process creation time. A value that can't be read is shown as unavailable, never filled in.
Memory before/after sleeping is measured from outside by `scripts/benchmark.ps1` ([BENCHMARK.md](BENCHMARK.md)), which is
deliberately separate from the running browser.

### Performance rules the code follows

* Event-driven only. The core pushes `pb:state`, `pb:downloads`, `pb:settings`…; the shell never polls.
* The sampler (`perf_subscribe`) runs only while a subscribed page is on screen (every 2 s), or every 5 s in Gaming
  Mode for heavy-tab detection. The New Tab page takes one reading and releases it.
* Internal pages other than New Tab are code-split and load on demand.
* Animations use `transform`/`opacity`; there are no blur or backdrop filters in the always-visible chrome.
  `data-motion="reduced"` (user setting or Gaming Mode) switches animation off globally.
* Background tabs: sleep (WebView2 `TrySuspend`) or unload (webview destroyed); restored-session tabs are created
  unloaded and only get a webview when first shown.

## Access control

See [SECURITY.md](SECURITY.md). In short: every command is registered in `build.rs`, granted only to the `main`
webview in `capabilities/shell.json`, and tab webviews (remote content) have no capability at all.

## Keeping the two sides in sync

`src/contract.test.ts` parses the Rust sources and fails `npm test` if the command list (`commands.rs`,
`build.rs`, `capabilities/shell.json`, `lib/ipc.ts`), payload shapes (`lib/types.ts`), event names, settings
defaults, shortcut commands or internal pages drift apart.
