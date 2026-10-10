# Building

## Option A: GitHub Actions (recommended; no local compiler needed)

The workflow `.github/workflows/windows-build.yml` builds on a GitHub-hosted Windows machine, so Smart App Control
on your PC is irrelevant.

1. Create an empty repository on GitHub and push this project to it (`main` or `master`).
2. Open the repository → **Actions** → **Windows build** (it also starts by itself on every push).
3. When the run is green, open it and download the artifact **project-browser-windows-installer**.
4. Unzip and double-click `Project Browser_0.1.0_x64-setup.exe`. It installs per-user (no administrator prompt),
   adds a Start menu entry and installs the WebView2 runtime if the PC lacks it.

The workflow runs, in order: frontend typecheck, lint, tests and build → `cargo fmt --check` → Rust unit tests and the
updater end-to-end tests (`--locked`) → (informational) Credential Manager round-trip → `clippy -D warnings` → `tauri build`
→ **startup smoke test** (launches the built exe on the runner, see below). It uploads the installer, the bare
`project-browser.exe`, the Rust test log and the smoke-test report. Test builds do not make the signed updater package; only
the Release workflow does (see [UPDATES.md](UPDATES.md)). A tag such as `v0.1.0` additionally
attaches the installer to a draft GitHub Release.

**A green run proves compilation, unit tests and packaging only.** It does not prove that WebView2 renders pages; follow
[VERIFY-WINDOWS.md](VERIFY-WINDOWS.md) on a Windows PC that may run unsigned programs (not one with Smart App Control on).

### If the run fails

The Rust core has never been compiled on the development machine, so the first CI run may report compile errors.
Open the failed step, copy the error output, and fix those errors; they are in the `Rust unit tests` step. The
frontend jobs are independent and are known to pass (`npm run check:web`).

## Option B: build on a PC that can run Cargo build scripts

Requirements: Windows 10/11, Node 22, Rust stable (MSVC toolchain), Visual Studio Build Tools with the
"Desktop development with C++" workload.

```text
npm install
npm run check:all      # typecheck, lint, tests, rustfmt, clippy, cargo test
npm run app:build      # installer in src-tauri\target\release\bundle\nsis\
```

`npm run app:dev` starts the real app with hot reload.

## What can be checked on this PC without compiling

```text
npm run check:web                                  # frontend: typecheck, lint, tests, build
cargo fmt --manifest-path src-tauri/Cargo.toml --check   # parses every Rust file; needs no build scripts
```

`cargo check`, `cargo test` and `tauri dev` cannot run where Smart App Control blocks Cargo build scripts. Do not retry them
there; use the workflow.

## What can be checked without Rust

```text
npm run check:web
```

* `tsc` strict typecheck, ESLint (React hooks rules), Vitest, production build.
* The contract tests read the Rust source files, so they also catch drift between the Rust API and the UI.

## Tests

| Where | What |
| --- | --- |
| `src/**/*.test.ts` | formatting, URL/tab/bookmark helpers, drag-reorder maths, overlay logic, import wording, assistant markdown safety |
| `src/contract.test.ts` | Rust ↔ TypeScript contract (commands, ACL, payload shapes, events, defaults, CSP) |
| `src-tauri/src/**` `#[cfg(test)]` | tab strip, navigation, shortcuts, settings, databases, import parsing, AI request/stream parsing, secret storage and key migration |
| `secrets::windows_credential_manager_roundtrip` | the real Credential Manager; `#[ignore]`d, run by CI as an informational step (`cargo test secrets -- --ignored`) |

## Visual check of the UI without the native app

`npm run dev` and open `http://localhost:1420/` in any Chromium browser: the shell runs against a mock core.
Use the query parameters listed in the README to reach specific states.

## Startup smoke test

`scripts/smoke-startup.ps1 -Exe <project-browser.exe>` starts the real executable against a throwaway data folder
(`PROJECT_BROWSER_DATA_DIR`), waits, and checks: still alive, startup stages logged, the first process-memory sample
completed (the step that crashed 0.1.0), no Windows crash event, no panic. It writes `smoke-summary.json` and the log.
It proves the process starts and survives; it does not prove pages render (that was checked separately by screenshotting
the app's own window, see [VERIFY-WINDOWS.md](VERIFY-WINDOWS.md)).

## Diagnosing a crash

* Log: `%LOCALAPPDATA%\com.projectbrowser.desktop\logs\project-browser.log` (each startup stage is a line).
* **Diagnose (symbols build)** workflow: rebuilds a given commit through the same `tauri build` path with symbols and
  uploads the exe and PDB, so addresses from a Windows crash event can be mapped to function names. How the 0.1.0 crash was
  found this way: [CRASH-0.1.0.md](CRASH-0.1.0.md).
