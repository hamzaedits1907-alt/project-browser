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
| Rust core (tabs, navigation, history, bookmarks, downloads, permissions, gaming, perf, import, profiles) | compiles and passes its unit tests in CI (`cargo test`, Clippy with warnings as errors, `cargo fmt`) |
| Startup | 0.1.0 crashed at launch on some PCs; root cause and fix: [docs/CRASH-0.1.0.md](docs/CRASH-0.1.0.md). A startup smoke test launches the real exe in CI |
| Native page rendering (WebView2 child views under the toolbar) | seen working: a real site rendered in the app's own window (screenshot of that window only, on the development PC). The full checklist in [docs/VERIFY-WINDOWS.md](docs/VERIFY-WINDOWS.md) is not complete |
| Sleeping tabs, Gaming Mode, memory limit | rules unit-tested; measured memory results in [docs/BENCHMARK.md](docs/BENCHMARK.md) |
| Updates | in-app, signed, user-confirmed: [docs/UPDATES.md](docs/UPDATES.md) (a full update from one published version to the next is documented there with its status) |
| React shell (all pages, menus, wizards, AI panel) | type-checked, linted, unit/contract-tested, rendered and inspected against a mock core |
| Windows installer (NSIS, per-user, no admin rights) | produced by CI and by the Release workflow |
| Code signing | not configured, so Windows SmartScreen warns on first run (see [docs/RELEASING.md](docs/RELEASING.md)) |

Install **0.1.1** once from the [latest release](https://github.com/hamzaedits1907-alt/project-browser/releases/latest);
after that, updates install from inside the app (**Settings → About**). The Rust side cannot be compiled on a PC where
Smart App Control blocks Cargo build scripts; CI does that work (see [docs/BUILDING.md](docs/BUILDING.md)).

**Blank page area in `npm run dev`?** That is expected: `npm run dev` shows only the interface against a mock core, with
no web engine. The page area then shows a "Development preview" notice. Websites render only in the native app.

## Get the installer (no terminal needed)

Download the `…_x64-setup.exe` from the [latest release](https://github.com/hamzaedits1907-alt/project-browser/releases/latest)
and double-click it. (Test builds from every push are the **project-browser-windows-installer** artifact of the
**Windows build** workflow.) Details and troubleshooting: [docs/BUILDING.md](docs/BUILDING.md).

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
* [docs/RELEASING.md](docs/RELEASING.md): versions, installers, signing.
* [docs/UPDATES.md](docs/UPDATES.md): the in-app updater, the release pipeline, the signing key, what is tested.
* [docs/CRASH-0.1.0.md](docs/CRASH-0.1.0.md): the 0.1.0 startup crash, how it was found and fixed.
* [docs/BENCHMARK.md](docs/BENCHMARK.md): the repeatable memory/CPU benchmark and its measured results.
