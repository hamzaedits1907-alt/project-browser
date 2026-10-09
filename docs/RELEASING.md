# Releasing

## Versioning

Bump `version` in all three places, then tag:

* `package.json`
* `src-tauri/Cargo.toml`
* `src-tauri/tauri.conf.json`

```text
git tag v0.2.0
git push origin v0.2.0
```

The tag triggers the **Windows build** workflow, which attaches the installer to a **draft** GitHub Release.
Review the draft and publish it.

## The installer

* Format: NSIS `.exe`, `installMode: currentUser`: installs under the user's profile, no administrator prompt.
* Uninstall: Windows Settings → Apps. User data (profiles) lives under `%LOCALAPPDATA%\com.projectbrowser.desktop`
  and is kept unless you tick the uninstaller's "delete application data" option.
* WebView2: the installer downloads the Evergreen runtime if it is missing (Windows 11 already includes it).

## SmartScreen and signing

**Smart App Control.** On PCs where Windows 11's Smart App Control is on (it is on this development PC), an unsigned
installer or app is *blocked outright*, with no "Run anyway" button, and so are locally compiled Rust build tools
(`os error 4551`). To run a build on such a PC it must be signed with a certificate Microsoft trusts (for example
Azure Trusted Signing or a purchased code-signing certificate). Turning Smart App Control off is the only other way
and is deliberately not done here.

The installer is **not code-signed** yet. Where Smart App Control is off, Windows SmartScreen shows "Windows protected your PC": choose *More info → Run
anyway*. A signed installer removes the warning once it builds reputation. To sign, buy a code-signing certificate
(or use Azure Trusted Signing), add it as repository secrets and set `bundle.windows.certificateThumbprint` /
`signCommand` in `tauri.conf.json` (see the Tauri v2 "Windows code signing" guide). This repository does not do that
for you.

## Automatic updates (optional)

Update code is in place (`src-tauri/src/updates.rs`, `tauri-plugin-updater`) but dormant:

1. Generate a signing key pair: `npx tauri signer generate -w updater.key` (keep the private key secret; `*.key` is
   git-ignored).
2. Put the **public** key in `plugins.updater.pubkey` of `tauri.conf.json` (replace the placeholder there).
3. Set `bundle.createUpdaterArtifacts` to `true`, add repository secrets `TAURI_SIGNING_PRIVATE_KEY` and
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, and pass them as `env` to the `tauri build` step.
4. Host a `latest.json` over https and build with `PROJECT_BROWSER_UPDATE_ENDPOINT=https://…/latest.json`.

Without an endpoint, **Settings → About** states that updates are not configured instead of pretending to check.
