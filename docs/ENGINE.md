# Rendering engine: WebView2 today, a bundled Chromium (CEF) tomorrow

**Status for 0.7.0: websites are rendered by Microsoft Edge WebView2**, the Chromium-based runtime that ships with
Windows and is updated by Microsoft. Project Browser does **not** bundle its own engine yet. The goal is a bundled
Chromium engine; 0.7.0 adds the first step, a reproducible prototype, and this page records what it showed and what
the migration still needs.

## What runs today

| Part | Engine |
|---|---|
| Browser interface (tabs, toolbar, settings, AI panel) | React app in a WebView2 webview (Tauri 2) |
| Every website tab and popup | WebView2 (the Evergreen runtime installed with Windows, Chromium-based, updated by Microsoft) |
| Native glue | `src-tauri/src/native/` talks to WebView2's COM API (`ICoreWebView2*`) directly |

## Step 1 (0.7.0): the CEF prototype

`.github/workflows/cef-prototype.yml` on a Windows runner:

1. Downloads a **pinned stable CEF build** (CEF 152.0.13, Chromium 152.0.7977.158, from Spotify's automated CEF builds)
   and checks it against the SHA-1 the build index publishes.
2. Compiles CEF's minimal embedding sample, `cefsimple`, with CMake and the runner's MSVC: a real native program that
   embeds the bundled `libcef.dll` (no system browser involved).
3. Runs it on an engine test page (`prototype/cef/engine-test.html`, served over `http://127.0.0.1`) and on real sites
   (example.com, Wikipedia, YouTube). For each it records the window title the page set, a capture of the window, its
   brightness (a black window is a failure), the processes CEF started and their memory (`scripts/cef-probe.ps1`).
4. Measures the size of the runnable engine on disk and zipped.

Results of the run this release was built with are in "Prototype results" below; the raw evidence is the
`cef-prototype-report` artifact.

## Why the browser itself has not switched yet

1. **Every browser feature is written against WebView2's API.** About 2,500 lines in `native/` use `ICoreWebView2*`:
   request blocking (Shield's `WebResourceRequested`), scripts added at document creation (YouTube guard, SponsorSkip),
   downloads (`DownloadStarting`, pause/resume), permissions, the right-click menu (`ContextMenuRequested` +
   `SelectedCommandId`), page snapshots and screenshots (`CapturePreview`, DevTools protocol), find, zoom, printing,
   audio state, process metrics, extensions (`ICoreWebView2Profile7`), private mode, focus and re-parenting. Each one
   must be re-implemented against CEF's handlers (`CefRequestHandler`, `CefResourceRequestHandler`,
   `CefDownloadHandler`, `CefContextMenuHandler`, `CefDisplayHandler`, `CefFocusHandler`, …).
2. **Tauri has no production CEF runtime.** Tauri's windowing and IPC sit on wry, which uses WebView2 on Windows. The
   Tauri project's CEF work (`cef-rs` and a CEF runtime for Tauri) is not production-ready. Hosting CEF browsers next
   to the WebView2 shell means a second message loop (or `multi_threaded_message_loop`), CEF's subprocess executable,
   and a bridge between the React shell and CEF browsers.
3. **Distribution and security updates.** The runnable engine is roughly 300 MB on disk and well over 100 MB zipped
   (measured below); today's installer is about 4 MB. Every Chromium security release (several a month) would have to
   be rebuilt, signed and shipped by this project, where WebView2 is patched by Microsoft through Windows Update.
   Without a fast, automated, signed update channel for the engine, a bundled Chromium is **less** safe than WebView2.
4. **Media.** The standard CEF builds do not include proprietary codecs (H.264/AAC) or the Widevine DRM module; the
   engine test records this. YouTube works with VP9/AV1, but Netflix, Spotify Web and some streaming sites do not.
   Shipping H.264 requires a codec licence; Widevine requires a licence from Google.
5. **It could not be verified here.** Native code cannot be compiled or run on the development PC (Smart App Control
   stays on), so every step is checked only through CI runs. A wholesale switch, released without anyone having used it,
   would put working features at risk.

## The migration plan

1. ~~Pinned CEF build in CI, checksum-verified, rendering real pages~~ (0.7.0, this prototype).
2. An `Engine` trait in the core with the operations the controller uses today (create a view, navigate, show/hide,
   bounds, events), implemented for WebView2 first so nothing changes for users.
3. A CEF implementation behind it, hosted as child windows of the Tauri window (CEF windowed rendering, its own
   message-loop thread), one hook at a time: navigation events, downloads, request blocking, context menu, permissions.
   WebView2 stays the default; CEF is opt-in and labelled experimental.
4. Packaging: CEF files in the NSIS installer (or a separate downloadable engine package), the subprocess executable
   signed with the app, a size budget.
5. An engine update channel: rebuilt within days of every Chromium security release, signed, delivered through the
   existing updater, with CI that refuses to release an engine older than the current stable Chromium.
6. Extensions: CEF's Chrome-style browsers can run Chrome extensions with toolbar actions; WebView2 cannot. That is the
   main feature reason to finish the move.

Until step 3 ships, the About page reports the WebView2 runtime version that renders pages, and release notes say so.

## Prototype results

CI run [38297085815](https://github.com/hamzaedits1907-alt/project-browser/actions/runs/38297085815) (windows-latest,
2026-10-12), artifact `cef-prototype-report`. **Passed: the bundled engine rendered every page.**

| Page | Title the page set | Painted (brightness) | Processes | Private memory |
|---|---|---|---|---|
| Engine test (127.0.0.1) | `PBTEST {…}` | yes (234) | browser 1, GPU 1, renderer 3, utility 4 | 189 MB |
| example.com | Example Domain | yes (233) | browser 1, GPU 1, renderer 3, utility 2 | 130 MB |
| Wikipedia article | Chromium (web browser) - Wikipedia | yes (245) | browser 1, GPU 1, renderer 3, utility 2 | 175 MB |
| youtube.com | YouTube | yes (250) | browser 1, GPU 1, renderer 4, utility 4 | 412 MB |

Engine capabilities (`Chrome/152.0.0.0`): modern JavaScript, CSS grid / `:has()` / container queries, localStorage,
cookies, IndexedDB, WebGL 2, WebGPU, Web Audio, service workers, WebAssembly, VP9 and AV1 video (incl. Media Source):
**all yes**. **H.264 video: no. Widevine DRM: no.** (As expected for standard CEF builds; see point 4 above.)

Size: **399.9 MB on disk, 179.5 MB zipped** for the runnable engine. Today's installer is about 4 MB.

Conclusion: the bundled engine works and is current (Chromium 152 stable). Shipping it needs the engine abstraction,
re-implemented hooks, the update channel and a codec decision (steps 2–5), not a different engine.
