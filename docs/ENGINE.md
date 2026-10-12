# Rendering engine: WebView2 today, and what moving to bundled Chromium (CEF) would take

Status for 0.6.0: **websites are still rendered by Microsoft Edge WebView2.** The move to a bundled Chromium engine was
investigated in the 0.6.0 sprint and **not shipped**. This page explains why, so nobody mistakes the current
architecture for something it is not.

## What runs today

| Part | Engine |
|---|---|
| Browser interface (tabs, toolbar, settings, AI panel) | React app in a WebView2 webview (Tauri 2) |
| Every website tab and popup | WebView2 (the Evergreen runtime installed with Windows, Chromium-based, updated by Microsoft) |
| Native glue | `src-tauri/src/native/` talks to WebView2's COM API (`ICoreWebView2*`) directly |

WebView2 is Chromium underneath, but it is Microsoft's runtime, shared with the OS, and versioned by Microsoft. Project
Browser does not have its own engine and does not claim to.

## Why CEF was not shipped in 0.6.0

1. **Every browser feature is written against WebView2's API.** About 2,200 lines in `native/` use `ICoreWebView2*`
   directly: request blocking (Project Shield's `WebResourceRequested`), the YouTube script
   (`AddScriptToExecuteOnDocumentCreated`), downloads (`DownloadStarting`, pause/resume), permissions, the right-click
   menu (`ContextMenuRequested` with `SelectedCommandId`), page snapshots (`CapturePreview`), find, zoom, printing, audio
   state, process metrics (`GetProcessExtendedInfos`), extensions (`ICoreWebView2Profile7`) and private mode. Moving to
   CEF means re-implementing each of these against CEF's C API (`CefRequestHandler`, `CefResourceRequestHandler`,
   `CefDownloadHandler`, `CefContextMenuHandler`, …), not swapping a library.
2. **Tauri has no production CEF runtime.** Tauri's windowing and IPC are tied to wry, which uses WebView2 on Windows.
   The Tauri project's CEF work (`cef-rs`, a CEF runtime for Tauri) is experimental. Embedding CEF next to wry means a
   second message loop, a second set of child windows to lay out and focus, and a hand-written bridge between the React
   shell and CEF browsers.
3. **Distribution.** CEF's Windows binaries (libcef.dll, resources, locales, the GPU/renderer subprocess executable)
   are roughly 250–350 MB unpacked and about 100 MB compressed. The installer would grow from about 4 MB to about
   100 MB, and every Chromium security release would have to be rebuilt and shipped by this project (WebView2 is patched
   by Microsoft through Windows Update today).
4. **It could not be verified here.** Native code cannot be compiled or run on the development PC (Smart App Control
   stays on), so every step would be checked only through CI runs of 15–20 minutes each. A migration this size, built
   that way and released without a person having used it, would put working features at risk.

## What it would take (estimate, not a commitment)

1. A CEF build pinned to a Chromium version (Spotify's automated CEF builds), downloaded and cached in CI.
2. A `cef` engine module behind the same interface `native/` exposes today (create tab, navigate, events), so the
   controller and the React shell do not change.
3. Re-implementing the hooks listed above, one at a time, keeping WebView2 as the fallback until each is proven.
4. Packaging: CEF's files in the NSIS installer and a subprocess executable signed with the app.
5. An update channel for Chromium security releases (at least monthly).

That is a multi-release project. Until it is done, the About page reports the WebView2 runtime version that renders
pages, and the release notes say so plainly.
