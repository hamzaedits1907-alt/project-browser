# Security model

## Trust boundaries

* **Shell webview** (`main`): our own React code, loaded from the app bundle. It is the only webview with IPC.
* **Tab webviews** (`tab-N`): arbitrary web content. They receive **no capability**, so they cannot call any
  command, and the core never emits events to them (events go to `main` only).
* Tab webviews cannot navigate to `browser://`, `tauri://`, `ipc://`, `asset://` or the shell's own origin.

## Tauri access control

* `src-tauri/build.rs` registers every command in an app manifest; Tauri then denies a command to any webview
  without an explicit `allow-<command>` permission.
* `src-tauri/capabilities/shell.json` grants those permissions to the `main` webview only (`local: true`, no
  `remote`), plus the few `core:` window/event permissions the frameless title bar needs.
* `npm test` fails if a command exists without its manifest entry and permission, or if a permission is granted
  that no command uses.
* CSP for the shell: `script-src 'self'`, no `unsafe-eval`, `connect-src ipc: http://ipc.localhost`. The shell
  cannot make network requests itself; the AI backend does that in Rust.
* The shell never uses `innerHTML`, `dangerouslySetInnerHTML`, `eval` or `new Function` (enforced by a test).
  Assistant replies are parsed into plain data and rendered as text nodes.

## Permissions (camera, microphone, location, …)

Requests are deferred natively and surface as a docked bar in the shell. "Remember" stores the decision per
origin and kind; private tabs never read or write stored decisions. Dismissing denies once. Stored decisions are
listed, changeable and removable in **Settings → Privacy & security** and from the lock icon in the address bar.

## Downloads

Files whose type can execute code (exe, msi, scripts, shortcuts, disk images…) are flagged. Opening one requires
an explicit second confirmation. Files open through Explorer so SmartScreen and the Mark-of-the-Web still apply.

## Private tabs

Incognito WebView2 contexts: no history, no saved passwords/autofill, no remembered permissions, no session
restore, downloads kept in memory only, and page text is never shared with the AI assistant.

## Passwords and import

* Password saving and autofill are WebView2's built-in features (stored, Windows-encrypted, in the profile).
* The import wizard reads bookmarks (`Bookmarks` and, for signed-in Chrome profiles, `AccountBookmarks`), history
  and the default search engine. SQLite files are copied into a private scratch folder (with their journal) and read
  there; the source browser's files are never opened for writing. Imports are idempotent: running one twice adds
  nothing the second time.
* It **does not import passwords, cookies, autofill, cards or extensions** and says so for each one it finds.
  Chrome/Edge/Brave protect these (and `EncryptedAccountBookmarks2`) with app-bound keys that only they can use; this
  browser does not attempt to defeat that, never copies those encrypted blobs, and cannot use them anyway.
* The search-engine import only accepts an `https` template with exactly one search-terms placeholder and no
  credentials; Chromium's tracking placeholders are dropped and anything unknown is refused rather than guessed.

## AI assistant

* Disabled until a model is configured. Nothing is sent until you press Send.
* Page text is attached only when "Include this page" is on, never from private tabs, and is fenced and labelled
  as untrusted content in the prompt (prompt-injection hygiene).
* Plain-http endpoints are accepted only for localhost; remote endpoints must use https.
* The API key is a secret and is handled as one; see "Credential protection" below.

## Credential protection

The AI API key is stored in the **Windows Credential Manager** (generic credential, target
`ProjectBrowser/<profile-id>/ai-api-key`, one per profile). Windows encrypts it for the signed-in user with DPAPI;
another Windows account, or anyone who copies the profile folder or database, cannot read it.

* **Not in SQLite.** The profile database stores only provider, endpoint and model. The stored struct has no key
  field; the one legacy field is `skip_serializing`, so it can never be written back.
* **No plaintext fallback.** If the Credential Manager refuses the write, saving fails with an error. The key is
  never parked in a file or setting instead.
* **Out of the frontend.** The shell reads the key from an uncontrolled password field once, at the moment you press
  Save, and passes it to `ai_config_set`, which writes it to the Credential Manager. It is not kept in React state,
  localStorage, events or any command result. The shell only learns `hasKey: true/false`. There is no command that
  returns the key. It is read natively, at request time, to set the `Authorization` / `x-api-key` header.
* **Not loggable.** Rust holds it as `Secret`, which has no `Serialize`/`Display`, prints as `Secret(***)`, and is zeroed
  when dropped. Errors from the credential API never include it. Tests assert none of the AI code logs it.
* **Migration.** Builds before this change kept the key in the profile database. On load the key is moved into
  the Credential Manager and removed from the database once the move has succeeded. If the Credential Manager
  refuses it, the old copy is **kept** (it was already there; nothing new is written in plaintext), Settings shows a
  warning, saving other AI settings is refused until you re-enter the key, and the move is retried on every load.
  If the Credential Manager already holds a key it wins and the old copy is just erased.
* **Remove.** *Settings → AI assistant → Remove key from Credential Manager* deletes the credential. Uninstalling does
  not delete credentials automatically; remove them from *Windows Credential Manager → Windows Credentials* (entries
  starting with `ProjectBrowser/`) if you want them gone.

Limits: malware running as your Windows user can ask the Credential Manager for your credentials just like this
browser can; Windows has no per-app isolation for generic credentials. Use an API key with a spending limit.

## Updates

* The app updates itself only from the release feed in its configuration, over https, and only installs a package whose
  Tauri (minisign) signature verifies against the public key compiled into the app. A tampered file, or one signed by
  another key, is rejected before installing; this is covered by automated tests.
* An update is never installed without a confirmation, and the background check only shows a notice.
* The private updater key lives outside the repository and is stored only as an encrypted GitHub Actions secret. It is
  never printed, uploaded or placed in a release. This signature is separate from Windows code signing, which is missing.
* `PROJECT_BROWSER_UPDATE_ENDPOINT` (https, or http on loopback) and `PROJECT_BROWSER_DATA_DIR` are test switches for the
  local user; signatures are still verified.

## Logs

`%LOCALAPPDATA%\com.projectbrowser.desktop\logs\project-browser.log` records startup stages, native errors and update errors.
It never contains page addresses beyond scheme and host (nothing for private tabs), cookies, passwords, API keys or page text.

## Known limitations

* Installers are unsigned: SmartScreen warns on first run, and Smart App Control blocks them outright (see RELEASING.md).
* There is no extension support.
