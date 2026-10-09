# Verifying the native app on Windows

A green GitHub Actions run proves the code **compiles, passes its unit tests and packages**. It does not prove that
WebView2 draws web pages in the right place, that tabs switch correctly, or that the importer works on your real
Chrome profile. Nobody (including the tool that wrote this code) has run the native window yet. This checklist is
how that gets verified. Record the result of every line; do not tick what you did not see.

## What you need

* The installer from the **project-browser-windows-installer** artifact of a green run (or the bare
  `project-browser.exe` from **project-browser-portable-exe**).
* A Windows 10/11 PC that is allowed to run an unsigned program: a second PC, a virtual machine, or a PC where
  Smart App Control is off. **Smart App Control blocks unsigned installers and executables** (this is the same policy
  that blocks the local Rust build, `os error 4551`). Do not turn it off on your main PC for this; Windows does not let
  you turn it back on without reinstalling. Either test on another machine, or sign the build (see
  [RELEASING.md](RELEASING.md)).
* The WebView2 runtime (already part of Windows 11; the installer adds it on Windows 10).

## Where the evidence is

* Log file: `%LOCALAPPDATA%\com.projectbrowser.desktop\logs\project-browser.log` (previous run: `.log.old`).
  It records webview creation/destruction, native hook installation, failed navigations (error code only) and panics.
  Pages are logged as scheme + host only, and not at all for private tabs.
* Settings → About shows the data folder and the WebView2 version.

Lines to look for after opening `example.com`:

```text
tab 1: webview created (https://example.com), visible=true, bounds=<w>x<h> at <x>,<y>
tab 1: native hooks installed, first navigation started
```

`bounds` must be the size of the page area (window size minus title bar, toolbar and bookmarks bar), not `0x0`.

## Checklist

| # | Do this | Expected | Result |
| --- | --- | --- | --- |
| 1 | Start the app | Window opens within a few seconds; New Tab page; no error screen | |
| 2 | Type `example.com`, Enter | Page renders in the area **below** the toolbar; toolbar, tabs and title bar stay visible | |
| 3 | Type `rust lang`, Enter | Search results page of the chosen engine | |
| 4 | Click the address bar and the main menu while a page is showing | Menus appear over the page (the page is dimmed behind them), then the page returns | |
| 5 | Open 3 tabs with different sites; switch, close the middle one, `Ctrl+Shift+T` | Each tab shows its own page; the closed tab comes back where it was | |
| 6 | Back, forward, reload, stop on a slow page | Buttons enable/disable correctly; address bar follows | |
| 7 | `http://github.com` | Redirects to https; lock icon is shown | |
| 8 | `https://nonexistent.invalid` | Branded error page with a "Try again" button, not a blank area | |
| 9 | Click a `target=_blank` link; Ctrl+click a link | New foreground tab; new background tab | |
| 10 | Sign in to a site that uses a popup (e.g. "Sign in with Google" on a demo site) | Popup opens, completes, the opener page is signed in | |
| 11 | `Ctrl+F`, zoom `Ctrl+=`, F11, video fullscreen | Find highlights; zoom changes the page only; fullscreen hides the chrome and the page fills the window | |
| 12 | Resize, maximize/restore, snap, move to a monitor with a different scaling | Page area always matches the space under the toolbar; no gap, no overlap, no blur | |
| 13 | Pin a tab, mute a playing tab, put a tab to sleep from its menu, switch back | Pinned stays first; audio icon works; sleeping tab wakes with its page (not blank) | |
| 14 | Download a file; pause/resume; download an `.exe` | Progress shown; pause works; the `.exe` needs a second confirmation | |
| 15 | Visit a camera test page | Permission bar appears; "Remember" is honoured; revocable in Settings | |
| 16 | Open a private tab, sign in somewhere, close it | Nothing in history; signing in does not persist | |
| 17 | Sign in to a site, bookmark a page, close the app completely, start it again | Same tabs and selected tab, pinned order kept, still signed in, bookmark and history present | |
| 18 | Check Task Manager after exiting | No `msedgewebview2.exe` left behind within ~5 s | |
| 19 | Settings → Profiles: create "Test", switch (the app restarts) | The new profile is not signed in to the site from step 17; switching back restores it | |
| 20 | Import wizard (Chrome, with Chrome open and again closed) | Counts match what the wizard showed; history is searchable; running it again says "Already up to date"; Chrome is unchanged; passwords/autofill/cookies listed as not imported | |
| 21 | Settings → AI: save an API key | Entry `ProjectBrowser/<profile>/ai-api-key` appears in Credential Manager; the key does not appear in `browser.db` (search the file for it) | |

## Known behaviour to expect (not defects)

* A restored tab starts with only its last URL: **back/forward history is not restored**. WebView2 has no API to rebuild
  a navigation history, and replaying every page would defeat lazy loading.
* Switching profiles restarts the app: WebView2 binds one user-data folder to the running browser process.
* The very first run of an unsigned build shows SmartScreen (or is blocked by Smart App Control).
* Saved passwords, autofill, cards and cookies are not imported from other browsers.
* Memory numbers on the Performance page are real measurements of this app's WebView2 processes. They say nothing about
  other browsers until someone measures both under comparable conditions.

## Reporting a failure

Send the last ~50 lines of `project-browser.log` and the checklist row number. A compile or test failure from CI is
reported by pasting the failing step's output from the Actions run.
