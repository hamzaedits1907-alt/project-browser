# Project Browser

A gaming-first desktop browser for Windows. Rust core (Tauri 2 + WebView2) with a React shell.
It installs with a normal Windows installer and starts by double-clicking it.

* **Dark, fast, restrained.** Premium dark UI, accent colours, no heavy visual effects.
* **Tabs that cost nothing.** Idle tabs sleep (WebView2 suspend) or unload; they wake instantly.
* **Gaming Mode** (`Ctrl+Shift+G`). Background tabs sleep after 10 s, downloads are held, animations reduced, sampling slows down.
* **Honest performance tools.** Real per-tab and per-process memory/CPU, GPU utilisation, startup timings. Sampling runs only while a page that shows it is open.
* **Chrome / Edge / Brave import** for bookmarks (including signed-in Chrome's `AccountBookmarks`), history and the default search engine. Safe to repeat. Passwords, autofill, cards and cookies are **not** imported, and the wizard says so.
* **AI assistant** (optional). Your own local model (Ollama, LM Studio) or your own API key (OpenAI-compatible, Anthropic). Nothing is sent until you ask; page text only when you opt in. The API key lives in the Windows Credential Manager, never in the browser's database.

## Status

| Area | State |
| --- | --- |
| Rust core (tabs, navigation, history, bookmarks, downloads, permissions, gaming, perf, import, profiles) | written; unit tests written; **never compiled or run yet** (see below) |
| Native page rendering (WebView2 child views placed under the toolbar) | implemented; **not yet verified in a real window**: follow [docs/VERIFY-WINDOWS.md](docs/VERIFY-WINDOWS.md) |
| React shell (all pages, menus, wizards, AI panel) | complete; type-checked, linted, unit/contract-tested, rendered and inspected against a mock core |
| Windows installer (NSIS, per-user, no admin rights) | produced by CI: see [docs/BUILDING.md](docs/BUILDING.md) |
| Code signing | not configured, so Windows SmartScreen warns on first run (see [docs/RELEASING.md](docs/RELEASING.md)) |

The Rust side could not be compiled locally (Smart App Control blocks Cargo build scripts). Everything native is
therefore verified only by the GitHub Actions build, and that proves compilation and unit tests only. If the first CI
run reports compile errors, those are the next thing to fix. Seeing real websites render needs a person to run the
installer on a Windows PC that may run unsigned programs (see [docs/VERIFY-WINDOWS.md](docs/VERIFY-WINDOWS.md)); Smart App
Control blocks the unsigned installer on the development PC.

**Blank page area in `npm run dev`?** That is expected: `npm run dev` shows only the interface against a mock core, with
no web engine. The page area then shows a "Development preview" notice. Websites render only in the native app.

## Get the installer (no terminal needed)

1. Push this repository to GitHub.
2. Open **Actions → Windows build** and wait for the run to finish (about 10–15 minutes the first time).
3. Download the **project-browser-windows-installer** artifact, unzip it, double-click the `.exe`.

Details and troubleshooting: [docs/BUILDING.md](docs/BUILDING.md).

## Develop

```text
npm install
npm run dev          # the UI alone in a browser, against a built-in mock core (http://localhost:1420); NO web pages render
npm run check:web    # typecheck + lint + tests + production build; needs no Rust
npm run app:dev      # the real app (needs the Rust toolchain; cannot run where build scripts are blocked)
```

`npm run dev` opens the shell with a **development-only** mock of the Rust core (`src/dev/mockBackend.ts`, excluded
from production builds). Query parameters select states, e.g. `?page=gaming`, `?web=1&prompt=1`, `?firstrun=1`,
`?gaming=1&accent=violet`.

## Keyboard shortcuts

Handled natively so they work whichever view has focus. Full list in **Settings → Shortcuts**.

`Ctrl+T` new tab · `Ctrl+W` close · `Ctrl+Shift+T` reopen · `Ctrl+L` address bar · `Ctrl+F` find · `Ctrl+D` bookmark ·
`Ctrl+H` history · `Ctrl+J` downloads · `Ctrl+,` settings · `Ctrl+Shift+G` Gaming Mode · `Shift+Esc` performance monitor.

## Documentation

* [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): how the core, shell and webviews fit together.
* [docs/BUILDING.md](docs/BUILDING.md): CI build, local build, tests.
* [docs/VERIFY-WINDOWS.md](docs/VERIFY-WINDOWS.md): the manual checklist that proves rendering, tabs, import and profiles on real Windows.
* [docs/SECURITY.md](docs/SECURITY.md): trust boundaries, permissions, what the browser does and does not do.
* [docs/RELEASING.md](docs/RELEASING.md): versions, installers, signing, updates.
