# Staying signed in

## What was audited (code review, 0.4.0)

* Every normal tab, popup and shell window uses the same WebView2 user-data folder, `profiles/<id>/webview`, with identical
  browser arguments (`window.rs`; a contract test fails if one differs). It is created once and never deleted by a normal
  exit; the only code that clears cookies or storage is `clear_data`, which runs only when the user asks.
* Private tabs and private windows use WebView2's private mode in the same folder: their cookies and storage live in
  memory and are discarded; they never write into the normal profile.
* Another profile has its own folder, so profiles stay isolated.
* Sign-in pop-ups (`window.open` with a size) are created with the opener's data directory and private flag, so a sign-in
  made in one is visible to the opener.
* A session (tabs, windows) is saved 0.8 s after a change and on exit, and restoring it only creates tabs; it does not touch
  site storage.

## What cannot be promised

* **Session cookies** (cookies without an expiry date) are dropped by WebView2 when the last browser process exits.
  Sites that sign you in with a session cookie will ask again after a full restart. Chrome's "continue where you left off"
  hides this by keeping them; this browser does not do that yet (feasible through the cookie manager, not built).
* Sites can expire or invalidate a sign-in (Discord and ChatGPT included), or tie it to a device signal.
* Google sign-in does not sign you in elsewhere. Discord and ChatGPT need their own first sign-in here.
* Active Chrome sessions are not imported, by design.

## How to check it on a real PC

See `docs/VERIFY-WINDOWS.md`, items P1–P4.
