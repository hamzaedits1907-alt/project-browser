# Chrome migration, saved passwords and the AI assistant

This documents what the migration wizard really does, what it deliberately does not do, and why. The evidence
under "What we found in a real Chrome profile" comes from a read-only look at the developer's own profile
(table names, row counts and byte prefixes only; no values were read or kept).

## What is imported

| Data | How | Notes |
|---|---|---|
| Bookmarks | `Bookmarks` and `AccountBookmarks` JSON | Folder hierarchy kept, under *Other bookmarks → Imported from … (profile)*. Same bookmark twice in one folder is added once. |
| History | `History` SQLite (a private copy) | Newest 100,000 pages **and** newest 250,000 individual visits, each with its original time. |
| Default search engine | `Secure Preferences` / `Preferences` | Mapped to a built-in engine, or a safe `https://…%s…` template. Unknown placeholders are refused, never guessed. |
| Saved addresses | `Web Data` → `addresses` + `address_type_tokens` | Only name, e-mail, phone, organisation and street/city/state/postcode/country. Stored sealed (below). |
| Open tabs | newest `Sessions/Session_*` file (the SNSS format) | Saved as bookmarks in an *Open tabs* folder and optionally reopened (first 20). Needs the other browser **closed**: Chrome holds its live session file exclusively. |
| Saved passwords | the CSV the user exports from the old browser | See below. Never read from the browser's own store. |

Every kind of data is reported separately on the result screen: how many were added, how many were already there
(duplicates), what failed, and what is unsupported. Running an import twice adds nothing the second time.
The source profile is only read: SQLite files are copied to a private scratch folder first and the copy is
deleted afterwards (there is a test that compares every byte of a fake profile before and after).

## What is deliberately not imported

* **Cookies / "stay signed in" sessions.** Not migrated, and not something a browser can promise. Cookie values are
  encrypted so that only the browser that made them can open them, and modern sites also bind sessions to the
  browser that created them. WebView2 additionally keeps its own, separate cookie store. Opening Chrome's
  encryption from another program would mean defeating Chrome's security, which this project does not do.
  After the import you sign in once; the imported passwords make that a single click.
* **Payment cards** (Chrome encrypts the number for itself).
* **Free-form form history** (`autofill` table). It holds whatever was ever typed into a form. In the profile we
  inspected it contained field names such as `ACCESS_TOKEN`, so copying it would copy secrets. Not copied.
* Extensions, settings other than the search engine, and non-default search engines.

## Passwords: why a CSV, and how it is handled

Chrome (and Edge, Brave) protect saved passwords with *app-bound encryption* (`v20` blobs) that only the browser
itself can open. In the profile we inspected, **all 24 account-stored logins were `v20`**. Nobody else can
decrypt them without defeating that protection, and this browser refuses to. The one legitimate route is the
user's own action: *Export passwords* in the old browser's password manager (Windows asks for the PIN).

Handling, in order:

1. The user chooses the CSV in the standard Windows file dialog. **The path stays inside the Rust core**; the UI
   never supplies a path, so it cannot ask the core to read another file.
2. The wizard shows counts only (logins, sites, skipped) and a plain statement of the risk: the export is plain
   text readable by any program on the PC and by any cloud folder that syncs it.
3. The file is read once into a buffer that is zeroed when dropped, parsed (RFC 4180, quoted fields, line breaks
   in notes, Chrome/Edge/Brave/Firefox column layouts), and every field is held in a zeroing `Secret`.
4. Each password (and note) is **sealed with Windows DPAPI** (user scope) with the site and user name as entropy,
   so a blob copied onto another row cannot be opened. SQLite only ever contains `sealed BLOB` columns plus the
   site and user name (needed to find the right login; not secrets). A unit test scans every byte of the vault
   tables for the plaintext password and note and fails if either appears.
5. The user may tick *Overwrite and delete the file afterwards* (on by default): the contents are replaced by
   zeros, flushed, and the file removed (not sent to the Recycle Bin). This is best effort: SSDs and synced
   folders may keep older copies, and the wizard says so when the file could not be removed.
6. Nothing about a password is logged, put in an event, returned from a command or shown in the UI. A test
   scans the Rust sources for log lines that interpolate secret-looking variables.

Duplicates: the same site + user name + password is reported as *already saved*; the same site + user name with a
different password *updates* the stored one and is counted separately; repeats inside the file, Android-app
logins (`android://…`), unreadable addresses and empty passwords each have their own count.

### Filling

Filling is always a click on the key button that appears in the address bar when the page has a saved login (or
address). The core checks that the tab is the active one, that the page is https (or localhost), and that the
login was saved for exactly this origin (scheme, host and port: no subdomain guessing). It then opens the
password and runs a script in the page's top frame that **re-checks `location.origin`**, fills only visible,
enabled, writable fields, and **never submits or clicks**. The shell receives only a result (`filled`,
`usernameOnly`, `noForm`, `originMismatch`) and the number of fields. Forms inside frames are not filled.
Address filling uses standard `autocomplete` attributes and leaves non-empty fields alone.

Passwords the *website* offers to save in WebView2 (Settings → Privacy) are kept by the web engine, separately
from this vault.

## NVIDIA AI

* One thing to enter: an NVIDIA API key, stored in Windows Credential Manager under
  `ProjectBrowser/<profile>/nvidia-api-key`. It is never returned by a command, never in an event, never in the
  profile database. The chosen model id *is* stored in the profile database (browser preference).
* **Findings against the live service (2026-10-10).** `GET https://integrate.api.nvidia.com/v1/models` returns
  the same list for a valid key, an invalid key and no key, so the list cannot validate a key. The key is
  therefore validated by a real one-token chat request: an invalid key answers `403 {"detail":"Authorization
  failed"}` (missing key: `401`). A retired model answers `410 Gone` with a reason; a model with no chat route
  (embeddings, CLIP) answers `404`. The list carries only `id`, `object`, `created` (a constant) and `owned_by`.
  Each of the 80 listed models was probed with an invalid key: 71 reach the auth check (chat route exists), 9
  answer 404. That table is the test fixture `src-tauri/tests/fixtures/nvidia-models-2026-10-10.json`, and a test
  asserts that every model without a chat route is classified "not for chat" (one exception the name cannot
  reveal, `bigcode/starcoder2-15b`, is handled at run time by the 404 handling).
* Model capabilities are **hints from the name** until checked. Vision and tool calling are enabled only after a
  real check of the selected model (one tiny request; the result is remembered per model). No current browser
  feature needs either, so the gate is in place but nothing is unlocked by it yet.
* States handled and shown with their own message: no key, invalid key, rate limited, network failure, timeout,
  server error, unreadable reply, empty list, model unavailable/retired, model rejected. A failed refresh keeps
  the last good list, marked as old.
* The key is only ever sent to `integrate.api.nvidia.com` over https; redirects are not followed.
* Keys left by earlier versions: a plaintext key in the database is erased (kept only if it was an NVIDIA key); a
  credential for a provider that is no longer supported is never used and can be removed from Settings.

## What we found in a real Chrome profile (structure only)

* One profile *Default*; `Bookmarks` plus `AccountBookmarks` (signed in).
* `Login Data` held 0 rows; `Login Data For Account` held 24, all `v20` (app-bound).
* `Network/Cookies` could not be opened while Chrome ran (exclusive lock).
* `Web Data`: 1 saved address, 0 cards, 642 free-form autofill rows (not imported).
* `Sessions/`: `Session_*` files are SNSS version 3; the one Chrome was writing was locked, older ones parsed into
  the expected tabs. `Tabs_*` files hold recently closed tabs and are ignored.

## What still needs a person with a real profile

See [VERIFY-WINDOWS.md](VERIFY-WINDOWS.md), section "Migration 2.0 and NVIDIA".
