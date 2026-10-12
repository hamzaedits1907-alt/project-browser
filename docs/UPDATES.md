# Updates

## For you (the user)

1. **Once:** install **0.1.1** by running its installer (from the GitHub Release page). Version 0.1.0 cannot update itself:
   it shipped without an update address and it crashes at startup anyway.
2. **From then on:** open **Settings → About → Check now**. When a newer version exists it shows the version and what is
   new; press **Download and install…**, confirm, and the app downloads the update, closes, installs it and reopens with
   your tabs restored. The browser also looks once, quietly, 20 seconds after it starts (switch it off in the same
   panel); that only shows a notice and a menu entry, never installs anything.

## How it works

* Feed: `https://github.com/hamzaedits1907-alt/project-browser/releases/latest/download/latest.json` (set in
  `src-tauri/tauri.conf.json` → `plugins.updater.endpoints`). The Release workflow publishes that file with each release.
* Every update package is **signed** with the updater key. The matching public key is in `tauri.conf.json` and compiled
  into the app, which refuses a package whose signature does not verify (wrong key, tampered file). This is the Tauri
  update signature; it is separate from Windows Authenticode code signing, which the project does not have yet, so
  Windows SmartScreen still warns on the very first manual install and Smart App Control blocks unsigned installers.
* `check` only reports. `install` runs after the confirmation dialog, once at a time (a second attempt is refused).
  The app saves its session first; the NSIS installer runs in passive mode and starts the app again.
* Errors are explained in words (cannot reach the server, download failed, signature rejected, Windows blocked the
  installer, no update for this Windows).

## For the maintainer: publishing a release

1. Bump the version in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` (and the `project-browser` entry in
   `Cargo.lock`), `package.json` and `package-lock.json`. A unit test and the release workflow fail if they disagree.
2. Write `release-notes/v<version>.md` (it becomes the release text and the "what's new" in the app).
3. Commit and push, and wait for the **Windows build** workflow of that commit to be green (it includes the startup smoke test
   of the real exe). Only then `git tag v<version>` and `git push origin v<version>`.
4. The **Release** workflow runs the checks (typecheck, lint, tests, build, `cargo fmt`, Clippy with warnings as errors,
   Rust unit and updater end-to-end tests), builds the installer and the signed updater package with `tauri-action`,
   publishes the Release with `latest.json`, and then **independently verifies** the published files
   (`scripts/verify-release.py`: version matches the tag, installer downloads, signature verifies against the public key
   in the config, using a separate Ed25519 implementation). It stops before building if the signing secret is missing.
5. It also installs the published installer silently on the runner and starts it (smoke test, reported not enforced).

### The signing key

* Private key: `~/.project-browser-keys/updater.key` on the maintainer's PC (outside the repository, ignored by `*.key`).
  It was generated without a password. It is stored as the repository secret `TAURI_SIGNING_PRIVATE_KEY`; the workflow
  sets `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` to an empty value. Neither is printed, uploaded or put in a release.
* **Back the key up.** If it is lost, installed copies cannot verify a package signed with a new key, and everyone has to
  install once by hand again. Do not rotate it without that in mind.
* To set the secret by hand: GitHub → Settings → Secrets and variables → Actions → *New repository secret* →
  name `TAURI_SIGNING_PRIVATE_KEY`, value = the full text of `updater.key`.

## What has been tested, and what has not

| Check | How | Status |
| --- | --- | --- |
| Config agrees with itself (endpoint, public key, `createUpdaterArtifacts`, one version everywhere) | unit test | automated, passing |
| The configured public key matches the real private key | signed a file with the private key, verified it against `tauri.conf.json` with an independent verifier | done once, locally |
| A newer version is discovered, with release notes | integration test against a local server, real `tauri-plugin-updater` | automated, passing |
| Same or older version → "up to date" | same | automated, passing |
| A correctly signed package downloads and verifies | same | automated, passing |
| A tampered package, or one signed by another key, is **rejected** | same | automated, passing |
| A missing package (404), an unreachable server, a broken feed | same | automated, passing |
| Only one update operation at a time; progress throttling; update UI states | unit tests (Rust and TypeScript) | automated, passing |
| The published release's `latest.json`, installer and signature agree | `scripts/verify-release.py` in the Release workflow | runs on every release |
| **Installing an update from inside a running older version and coming back up as the newer one** | needs two published releases | see the record below |

`PROJECT_BROWSER_UPDATE_ENDPOINT` (https, or http on loopback only) points a build at another feed for experiments.
Signatures are verified regardless.

## Record

**v0.1.1, 2026-10-10** (<https://github.com/hamzaedits1907-alt/project-browser/releases/tag/v0.1.1>)

* Published by the Release workflow: `Project.Browser_0.1.1_x64-setup.exe` (3,653,144 bytes, SHA-256
  `4e786bc1a5955cc4448ee10608b9b51f42a1eda3f20e96f44f1e9b3139ee455f`), `Project.Browser_0.1.1_x64-setup.exe.sig`, `latest.json`.
* `scripts/verify-release.py --tag v0.1.1`, run by the workflow and again by hand from another machine: `latest.json` announces
  0.1.1, the installer downloads, and its signature verifies against the public key in `tauri.conf.json` (signed
  comment `version:0.1.1`). Result: `verified`.
* The published installer was installed silently on a hosted Windows runner and started: alive after 30 s, startup stages
  logged, first process sample completed, no crash event.
* The same feed URL answers `latest.json` with `windows-x86_64` and `windows-x86_64-nsis` entries pointing at that installer.

**v0.1.2, 2026-10-10** (<https://github.com/hamzaedits1907-alt/project-browser/releases/tag/v0.1.2>), published by the same workflow.

* `scripts/verify-release.py --tag v0.1.2` from this PC: version 0.1.2, installer `Project.Browser_0.1.2_x64-setup.exe`
  (3,656,116 bytes, SHA-256 `50941c16aac6d2fa16ced0a87cc0a3ffafe66b85dcf9b79ce58932b5bf317c60`), signature verified against the key in
  the config, signed comment `version:0.1.2`.
* **A real 0.1.1 client discovered it.** A 0.1.1 build was started against the live feed (throwaway profile); about 20 s after start
  it showed the toast "Version 0.1.2 is available. Open Settings → About to install it." (captured from the app's own window).
  This exercises the live `latest.json`, the version comparison and the update notice in the real application.

**Not yet done: the install step, run from inside an installed older version.** Everything it consists of is covered
separately (downloading, rejecting tampered or wrongly signed packages, the UI states, the published installer installing
silently and starting on a hosted runner), but pressing *Download and install…* in an installed 0.1.1 and watching it come back as 0.1.2 has not been
done: running it on the development PC would replace the installed copy there, so it was left for you.
To do it: install 0.1.1 (or keep it), then **Settings → About → Check now**, **Download and install…**, confirm. The app should
close, install and reopen on 0.1.2 with its tabs restored; the log (`logs\project-browser.log`) then shows
`update install: package downloaded and signature verified` in 0.1.2's own later runs.

**v0.2.0, 2026-10-12.** Published by the same workflow. The developer reported that 0.1.2 updated to 0.2.0 from inside the installed app
(user-reported; not independently observed in this record).

**v0.3.0, 2026-10-12** (<https://github.com/hamzaedits1907-alt/project-browser/releases/tag/v0.3.0>), commit `2271a78`, Release run 38232441835
(checks, build, signing and publishing all green).

* `scripts/verify-release.py --tag v0.3.0` from this PC: `latest.json` announces 0.3.0, installer
  `Project.Browser_0.3.0_x64-setup.exe` (3,848,419 bytes, SHA-256 `9029a2642445848eaf235de704aa6903aaf47f032b5f700704ad94653e9cc1fc`),
  signature verified against the key in the config, signed comment `version:0.3.0`. Result: `verified`.
* The existing signing key and secret were reused; nothing about the updater configuration changed. Two new updater tests
  (numeric version comparison, non-version feed) ran in CI.
* Not done: installing 0.3.0 from inside a running 0.2.0 (that would replace the installed copy); the Settings → About button does it.
