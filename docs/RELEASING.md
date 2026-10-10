# Releasing

## Versioning and publishing

Bump `version` everywhere (a unit test and the Release workflow fail if they disagree):

* `package.json` and `package-lock.json`
* `src-tauri/Cargo.toml` and the `project-browser` entry in `src-tauri/Cargo.lock`
* `src-tauri/tauri.conf.json`

Write `release-notes/v<version>.md`, commit, push, then tag:

```text
git tag v0.1.2
git push origin v0.1.2
```

The tag starts the **Release** workflow (`.github/workflows/release.yml`), which checks, builds, signs, publishes the
GitHub Release (installer, signed updater package, `latest.json`) and verifies what it published. Details, the signing
key and what is tested: [UPDATES.md](UPDATES.md). The ordinary **Windows build** workflow is for test builds only and
never publishes.

## The installer

* Format: NSIS `.exe`, `installMode: currentUser`: installs under the user's profile, no administrator prompt.
* Uninstall: Windows Settings → Apps. User data (profiles) lives under `%LOCALAPPDATA%\com.projectbrowser.desktop`
  and is kept unless you tick the uninstaller's "delete application data" option.
* WebView2: the installer downloads the Evergreen runtime if it is missing (Windows 11 already includes it).

## SmartScreen and signing

**Smart App Control.** On PCs where Windows 11's Smart App Control is on an unsigned
installer or app can be *blocked outright*, with no "Run anyway" button, and so are locally compiled Rust build tools
(`os error 4551`). To run a build on such a PC it must be signed with a certificate Microsoft trusts (for example
Azure Trusted Signing or a purchased code-signing certificate). Turning Smart App Control off is the only other way
and is deliberately not done here.

The installer is **not code-signed** yet. Where Smart App Control is off, Windows SmartScreen shows "Windows protected your PC": choose *More info → Run
anyway*. A signed installer removes the warning once it builds reputation. To sign, buy a code-signing certificate
(or use Azure Trusted Signing), add it as repository secrets and set `bundle.windows.certificateThumbprint` /
`signCommand` in `tauri.conf.json` (see the Tauri v2 "Windows code signing" guide). This repository does not do that
for you.

## Automatic updates

Live since 0.1.1: see [UPDATES.md](UPDATES.md). The updater verifies a Tauri signature (minisign) before installing; that
is not the same thing as the Windows code signing above, which is still missing.
