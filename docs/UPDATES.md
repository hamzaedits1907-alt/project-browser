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

**Not yet done: an update installed from inside a running older version.** That needs a second published release to
update *to*, and a person (or UI automation) to press the buttons in the older, installed version. Everything it consists
of is covered separately above (discovering a newer version, downloading, rejecting tampered or wrongly signed packages,
version comparison, "up to date", the UI states), but the whole chain has not been run end to end.
To do it: install 0.1.1; when a later version is published, open **Settings → About → Check now**, press **Download and
install…** and confirm. The app should close, install and reopen on the new version with its tabs.
