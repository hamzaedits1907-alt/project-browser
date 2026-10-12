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

Full detail, with evidence from a real profile: [CHROME-MIGRATION.md](CHROME-MIGRATION.md).

* Password saving and autofill *offered by websites* are WebView2's built-in features (stored, Windows-encrypted,
  in the profile). Separately, logins and addresses the user **imports** live in the browser's own **vault**.
* The import wizard reads bookmarks (`Bookmarks` and, for signed-in Chrome profiles, `AccountBookmarks`), history
  (pages and every visit with its time), the default search engine, saved address profiles and the open tabs of the
  last session. SQLite files are copied into a private scratch folder (with their journal) and read there; the
  source browser's files are never opened for writing (a test compares every byte before and after). Imports are
  idempotent: running one twice adds nothing the second time.
* It **does not import cookies, payment cards, free-form form history or extensions** and says so, with the
  reason. Chrome/Edge/Brave protect saved passwords, cookies (and `EncryptedAccountBookmarks2`) with app-bound keys
  that only they can use; this browser does not attempt to defeat that and never copies those encrypted blobs.
* **Passwords come only from a file the user exports** from the old browser and chooses in the Windows file
  dialog. The chosen path stays in the Rust core (the UI cannot name a file to read). The file is read once into a
  zeroed-on-drop buffer, parsed, and each password is **sealed with Windows DPAPI** (user scope, site and user name
  as entropy). SQLite stores only `sealed` blobs next to the site and user name. The user can have the export
  overwritten with zeros and deleted right after (best effort on SSDs and synced folders; the wizard says if it
  failed). No password is logged, put in an event, returned from a command or sent to the UI.
* **Filling** is a click on the address bar's key button. The core requires the active tab, https (or localhost), and
  an exact match of scheme, host and port with the login's site; the script re-checks `location.origin`, fills
  visible writable fields in the top frame only, and never submits or clicks. Passwords never go to the AI assistant.
* The search-engine import only accepts an `https` template with exactly one search-terms placeholder and no
  credentials; Chromium's tracking placeholders are dropped and anything unknown is refused rather than guessed.

## AI assistant

* NVIDIA's hosted API is the only provider. Disabled until a key and a model are set. Nothing is sent until you press
  Send, Connect or Test connection.
* The key is sent only to `https://integrate.api.nvidia.com/v1`; redirects are not followed.
* Page text is attached only when "Include this page" is on, never from private tabs, and is fenced and labelled
  as untrusted content in the prompt (prompt-injection hygiene).
* Features that need image input or tool calling stay off until a real check of the selected model confirmed them.
* Research, Actions, Project Memory and page restyling add no new network destination and no new secret: they use the same
  request path. [AI.md](AI.md) describes each. What matters for security:
  * **What may be read.** The core refuses to read private tabs, browser pages, files and tabs that aren't running
    (`ai::page_read_blocker`), whatever the shell asks. Page text never includes form field contents. Cookies, saved
    passwords, files and the API key are never part of a request.
  * **How much.** Every request is checked against the user's context limit in the core (`ai::check_context`).
  * **Pages are untrusted.** In every prompt page text, titles and addresses are fenced, the fence markers are neutralised
    inside them, and the model is told they are data. Replies and page text are rendered and saved as text and are never
    parsed as a plan, so a page cannot add a step or grant itself a permission. The planner is not shown any page.
  * **What may be done.** Plans can only contain the nine tools in AI.md, each with its own argument check; a plan with
    anything else is rejected whole. Nothing runs until the user confirms, and every step can be unticked. There is no
    shell, file, form, download, password or cookie tool. Addresses must be public http/https; in a private tab, opened
    pages are private tabs.
  * **Page styles** are CSS only, checked in the shell and again in the core (no `url()`, `@import`, escapes or markup),
    inserted as text content of one `<style>` element, local to the view, removable.
  * **Citations are checked, not trusted.** A quote counts only if it is found in the page text that was sent.
* The API key is a secret and is handled as one; see "Credential protection" below.

## Credential protection

The NVIDIA API key is stored in the **Windows Credential Manager** (generic credential, target
`ProjectBrowser/<profile-id>/nvidia-api-key`, one per profile). Windows encrypts it for the signed-in user with DPAPI;
another Windows account, or anyone who copies the profile folder or database, cannot read it.

* **Not in SQLite.** The profile database stores only the chosen model id. The stored struct has no key field.
* **No plaintext fallback.** If the Credential Manager refuses the write, saving fails with an error. The key is
  never parked in a file or setting instead.
* **Out of the frontend.** The shell reads the key from an uncontrolled password field once, when you press
  Connect, and passes it to `ai_test` (to check it) and then `ai_config_set` (to store it). It is not kept in React
  state, localStorage, events or any command result. The shell only learns `hasKey: true/false`. There is no command
  that returns the key. It is read natively, at request time, to set the `Authorization` header.
* **Not loggable.** Rust holds it as `Secret`, which has no `Serialize`/`Display`, prints as `Secret(***)`, and is zeroed
  when dropped. Errors from the credential API never include it. Tests assert none of the AI, vault or import code
  logs a secret-looking value.
* **Older versions.** A key a build before 0.1.1 left in the profile database is erased from it (kept only when it was
  an NVIDIA key). The credential used when any provider could be chosen (`.../ai-api-key`) is never used or sent
  anywhere; Settings shows it and can remove it.
* **Remove.** *Settings → AI assistant → Remove key from Credential Manager* deletes the credential. Uninstalling does
  not delete credentials automatically; remove them from *Windows Credential Manager → Windows Credentials* (entries
  starting with `ProjectBrowser/`) if you want them gone.

Limits: malware running as your Windows user can ask the Credential Manager, or DPAPI, for your secrets just like this
browser can; Windows has no per-app isolation for either. Use an API key with a spending limit.

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
