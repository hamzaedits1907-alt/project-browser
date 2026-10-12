//! Migration from Chromium-based browsers (Chrome, Edge, Brave).
//!
//! Only runs on explicit user action from the migration wizard. Reads the
//! source browser's files without modifying them: SQLite files are copied
//! (together with their journal) into a private scratch folder and read there.
//!
//! What is read: bookmarks (folder hierarchy kept), history (every visit with its time), the default
//! search engine, saved address profiles, and the open tabs of the last session.
//!
//! What is deliberately **not** read: anything the source browser encrypts for itself. Saved passwords,
//! cookies and the encrypted copy of account bookmarks use app-bound encryption that only the owning
//! browser can unlock, and this importer does not try to get around that (docs/CHROME-MIGRATION.md).
//! Passwords come over through the user's own CSV export instead (`crate::vault`). Free-form autofill
//! history is not copied either: it can hold anything the user ever typed into a form.
//!
//! The work is split in two so the slow part never blocks the browser:
//! [`collect`] only reads files (run it on a blocking thread), [`apply`] only
//! writes to the profile database (run it on the controller).

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::db::bookmarks::{ImportNode, OTHER_ID};
use crate::db::history::{ImportCounts, UrlRow, VisitRow};
use crate::db::Db;
use crate::error::{now_ms, Error, Result};
use crate::navigation::SearchEngine;
use crate::vault::{AddressData, Sealer};

/// Newest history entries read from a source profile.
const HISTORY_LIMIT: u32 = 100_000;
/// Newest individual visits read (each carries its own timestamp).
const VISIT_LIMIT: u32 = 250_000;
/// Open tabs offered for "open them now" in the result screen.
const SESSION_URLS_SHOWN: usize = 50;
/// Largest session file that is read.
const MAX_SESSION_BYTES: u64 = 64 * 1024 * 1024;

/// Chromium keeps local bookmarks in `Bookmarks` and, for a signed-in profile, the
/// account's synced bookmarks in `AccountBookmarks` (same JSON format).
const BOOKMARK_FILES: [&str; 2] = ["Bookmarks", "AccountBookmarks"];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SourceBrowser {
    Chrome,
    Edge,
    Brave,
}

impl SourceBrowser {
    const ALL: [SourceBrowser; 3] = [SourceBrowser::Chrome, SourceBrowser::Edge, SourceBrowser::Brave];

    pub fn label(self) -> &'static str {
        match self {
            SourceBrowser::Chrome => "Google Chrome",
            SourceBrowser::Edge => "Microsoft Edge",
            SourceBrowser::Brave => "Brave",
        }
    }

    fn user_data_dir(self, local_app_data: &Path) -> PathBuf {
        match self {
            SourceBrowser::Chrome => local_app_data.join("Google").join("Chrome").join("User Data"),
            SourceBrowser::Edge => local_app_data.join("Microsoft").join("Edge").join("User Data"),
            SourceBrowser::Brave => local_app_data.join("BraveSoftware").join("Brave-Browser").join("User Data"),
        }
    }
}

/// What the wizard shows for each detected profile. Every data type is listed
/// with whether it can be imported and, if not, why.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedProfile {
    pub browser: SourceBrowser,
    pub browser_label: String,
    pub profile_dir: String,
    pub name: String,
    pub items: Vec<ImportItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportItem {
    pub kind: String,
    pub label: String,
    pub supported: bool,
    pub present: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub bookmarks_imported: usize,
    /// Bookmarks that were already present (or not web addresses) and so were not added again.
    pub bookmarks_skipped: usize,
    pub history_imported: usize,
    pub history_skipped: usize,
    /// Individual visits (with their original timestamps) added to the history page.
    pub history_visits_imported: usize,
    /// Name of the default search engine that was applied, if one was imported.
    pub search_engine: Option<String>,
    /// Open tabs found in the last session.
    pub session_tabs: usize,
    /// Of those, how many were saved as bookmarks (the rest were already there or not web pages).
    pub session_tabs_saved: usize,
    /// The web addresses of the open tabs (at most 50), so the wizard can offer to open them.
    pub session_urls: Vec<String>,
    pub addresses_imported: usize,
    pub addresses_skipped: usize,
    pub errors: Vec<String>,
}

/// A default search engine read from the source browser, already mapped onto what we support.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedSearch {
    pub name: String,
    /// One of the built-in engines, when the source engine is one of them.
    pub engine: Option<SearchEngine>,
    /// Otherwise a custom `https://…%s…` template.
    pub template: Option<String>,
}

/// Microseconds since 1601-01-01 (Chromium/WebKit epoch) to Unix ms.
pub fn chromium_time_to_unix_ms(t: i64) -> i64 {
    if t <= 0 {
        return 0;
    }
    t / 1000 - 11_644_473_600_000
}

#[derive(Deserialize)]
struct LocalState {
    profile: Option<LocalStateProfile>,
}

#[derive(Deserialize)]
struct LocalStateProfile {
    info_cache: Option<serde_json::Map<String, serde_json::Value>>,
}

fn profile_names(user_data: &Path) -> Vec<(String, String)> {
    let parsed: Option<LocalState> = std::fs::read_to_string(user_data.join("Local State")).ok().and_then(|s| serde_json::from_str(&s).ok());
    let mut out: Vec<(String, String)> = parsed
        .and_then(|ls| ls.profile)
        .and_then(|p| p.info_cache)
        .map(|cache| {
            cache
                .into_iter()
                .map(|(dir, v)| {
                    let name = v.get("name").and_then(|n| n.as_str()).unwrap_or(&dir).to_string();
                    (dir, name)
                })
                .collect()
        })
        .unwrap_or_default();
    if out.is_empty() && user_data.join("Default").is_dir() {
        out.push(("Default".into(), "Default".into()));
    }
    out.sort();
    out
}

/// Finds importable profiles under `%LOCALAPPDATA%`.
pub fn detect(local_app_data: &Path) -> Vec<DetectedProfile> {
    let mut found = Vec::new();
    for browser in SourceBrowser::ALL {
        let user_data = browser.user_data_dir(local_app_data);
        if !user_data.is_dir() {
            continue;
        }
        for (dir, name) in profile_names(&user_data) {
            let pdir = user_data.join(&dir);
            if !pdir.is_dir() {
                continue;
            }
            let bookmarks = read_bookmarks(&pdir);
            let bookmark_count = unique_urls(&bookmarks.nodes);
            let history = pdir.join("History").is_file();
            let search = read_default_search(&pdir);
            let logins = login_count(&pdir);
            let addresses = address_count(&pdir);
            let session = latest_session_file(&pdir).is_some();
            let label = browser.label();
            let items = vec![
                ImportItem {
                    kind: "bookmarks".into(),
                    label: "Bookmarks".into(),
                    supported: true,
                    present: bookmark_count > 0,
                    detail: if bookmarks.files_found == 0 {
                        "No bookmarks file".into()
                    } else if !bookmarks.warnings.is_empty() {
                        format!("{bookmark_count} bookmarks found ({})", bookmarks.warnings.join("; "))
                    } else {
                        format!("{bookmark_count} bookmarks found, with their folders")
                    },
                },
                ImportItem {
                    kind: "history".into(),
                    label: "Browsing history".into(),
                    supported: true,
                    present: history,
                    detail: if history {
                        format!("Up to {} most recent pages and {} recent visits, with their dates", format_thousands(HISTORY_LIMIT), format_thousands(VISIT_LIMIT))
                    } else {
                        "No history file".into()
                    },
                },
                ImportItem {
                    kind: "searchEngines".into(),
                    label: "Default search engine".into(),
                    supported: true,
                    present: search.is_some(),
                    detail: match &search {
                        Some(s) => format!("Default: {}", s.name),
                        None => "No custom default search engine set in this profile".into(),
                    },
                },
                ImportItem {
                    kind: "passwords".into(),
                    label: "Saved passwords".into(),
                    supported: true,
                    present: match logins {
                        Some(n) => n > 0,
                        None => pdir.join("Login Data").is_file() || pdir.join("Login Data For Account").is_file(),
                    },
                    detail: match logins {
                        Some(n) => format!(
                            "{n} saved logins found. {label} locks them with encryption that only it can open, and this browser doesn't get around that. \
                             You export them from {label} yourself and choose the file here."
                        ),
                        None => format!(
                            "{label} locks saved passwords with encryption that only it can open. You export them from {label} yourself and choose the file here."
                        ),
                    },
                },
                ImportItem {
                    kind: "autofill".into(),
                    label: "Saved addresses".into(),
                    supported: true,
                    present: addresses.unwrap_or(0) > 0,
                    detail: match addresses {
                        Some(0) => "No saved addresses".into(),
                        Some(n) => format!(
                            "{n} saved address{} found. They are stored encrypted for this Windows account. Form history is not copied.",
                            if n == 1 { "" } else { "es" }
                        ),
                        None => "Saved addresses could not be read".into(),
                    },
                },
                ImportItem {
                    kind: "session".into(),
                    label: "Open tabs".into(),
                    supported: true,
                    present: session,
                    detail: if session {
                        format!("The tabs {label} had open last time. Close {label} completely first.")
                    } else {
                        "No saved session found".into()
                    },
                },
                ImportItem {
                    kind: "cookies".into(),
                    label: "Cookies and site sign-ins".into(),
                    supported: false,
                    present: pdir.join("Network").join("Cookies").is_file() || pdir.join("Cookies").is_file(),
                    detail: "Not imported. Cookies are encrypted so that only the browser that made them can open them, and sites tie sign-ins to that browser. \
                             Copying them would not keep you signed in. Sign in once here; your saved passwords make that quick."
                        .into(),
                },
                ImportItem {
                    kind: "cards".into(),
                    label: "Payment cards".into(),
                    supported: false,
                    present: false,
                    detail: "Never imported. Card numbers are encrypted by the source browser for its own use.".into(),
                },
            ];
            found.push(DetectedProfile { browser, browser_label: label.into(), profile_dir: dir, name, items });
        }
    }
    found
}

/// A `file:` URI that opens a database strictly read-only without taking locks, so counting rows in the
/// source browser's files never disturbs it (and fails quietly when the browser has them locked).
fn readonly_uri(path: &Path) -> Option<String> {
    let text = path.to_str()?.replace('\\', "/").replace('%', "%25").replace(' ', "%20").replace('?', "%3F").replace('#', "%23");
    Some(format!("file:///{}?mode=ro&immutable=1", text.trim_start_matches('/')))
}

fn count_rows(path: &Path, sql: &str) -> Option<i64> {
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI;
    let conn = rusqlite::Connection::open_with_flags(readonly_uri(path)?, flags).ok()?;
    conn.query_row(sql, [], |r| r.get(0)).ok()
}

/// How many logins the profile has saved (never their content), or `None` when the files cannot be read now.
fn login_count(profile_dir: &Path) -> Option<i64> {
    let mut total = None;
    for file in ["Login Data", "Login Data For Account"] {
        let path = profile_dir.join(file);
        if !path.is_file() {
            continue;
        }
        if let Some(n) = count_rows(&path, "SELECT COUNT(*) FROM logins WHERE blacklisted_by_user = 0") {
            total = Some(total.unwrap_or(0) + n);
        }
    }
    total
}

fn address_count(profile_dir: &Path) -> Option<i64> {
    let path = profile_dir.join("Web Data");
    if !path.is_file() {
        return Some(0);
    }
    count_rows(&path, "SELECT COUNT(*) FROM addresses")
}

fn format_thousands(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

// ---- bookmarks ------------------------------------------------------------------------

fn convert_node(v: &serde_json::Value) -> Option<ImportNode> {
    let title = v.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
    match v.get("type").and_then(|t| t.as_str())? {
        "url" => {
            let url = v.get("url")?.as_str()?.to_string();
            let created_at =
                v.get("date_added").and_then(|d| d.as_str()).and_then(|d| d.parse::<i64>().ok()).map(chromium_time_to_unix_ms).unwrap_or(0);
            Some(ImportNode::Url { title, url, created_at })
        }
        "folder" => {
            let children = v.get("children").and_then(|c| c.as_array()).map(|c| c.iter().filter_map(convert_node).collect()).unwrap_or_default();
            Some(ImportNode::Folder { title, children })
        }
        _ => None,
    }
}

/// Parses a Chromium `Bookmarks` JSON document into folders per root.
pub fn parse_bookmarks(json: &str) -> Result<Vec<ImportNode>> {
    let v: serde_json::Value = serde_json::from_str(json)?;
    let roots = v.get("roots").ok_or_else(|| Error::msg("bookmarks file has no roots"))?;
    let mut out = Vec::new();
    for (key, label) in [("bookmark_bar", "Bookmarks bar"), ("other", "Other bookmarks"), ("synced", "Mobile bookmarks")] {
        if let Some(ImportNode::Folder { children, .. }) = roots.get(key).and_then(convert_node) {
            if !children.is_empty() {
                out.push(ImportNode::Folder { title: label.into(), children });
            }
        }
    }
    Ok(out)
}

#[derive(Default)]
struct BookmarkRead {
    nodes: Vec<ImportNode>,
    /// How many of [`BOOKMARK_FILES`] exist.
    files_found: usize,
    /// Files that exist but could not be read; shown to the user, never hidden.
    warnings: Vec<String>,
}

/// Reads every bookmark file the profile has. A broken file does not hide the others.
fn read_bookmarks(profile_dir: &Path) -> BookmarkRead {
    let mut out = BookmarkRead::default();
    for file in BOOKMARK_FILES {
        let path = profile_dir.join(file);
        if !path.is_file() {
            continue;
        }
        out.files_found += 1;
        match std::fs::read_to_string(&path).map_err(Error::from).and_then(|s| parse_bookmarks(&s)) {
            Ok(nodes) => out.nodes.extend(nodes),
            Err(e) => out.warnings.push(format!("{file} could not be read: {e}")),
        }
    }
    out
}

fn collect_urls<'a>(nodes: &'a [ImportNode], seen: &mut HashSet<&'a str>) {
    for n in nodes {
        match n {
            ImportNode::Url { url, .. } => {
                seen.insert(url.as_str());
            }
            ImportNode::Folder { children, .. } => collect_urls(children, seen),
        }
    }
}

/// Number of distinct bookmarked addresses (the same bookmark can be in both files).
fn unique_urls(nodes: &[ImportNode]) -> usize {
    let mut seen = HashSet::new();
    collect_urls(nodes, &mut seen);
    seen.len()
}

// ---- history --------------------------------------------------------------------------

/// A private copy of a SQLite database and its sidecar files. The source browser keeps
/// `History` open and may have a hot journal; copying them together lets SQLite recover a
/// consistent snapshot in our copy without touching the original.
struct SnapshotDb {
    base: PathBuf,
}

impl SnapshotDb {
    fn take(src: &Path, scratch: &Path, tag: &str) -> Result<SnapshotDb> {
        std::fs::create_dir_all(scratch)?;
        let stem = format!("{tag}-{}-{}", std::process::id(), now_ms());
        let base = scratch.join(&stem);
        let snapshot = SnapshotDb { base };
        std::fs::copy(src, &snapshot.base).map_err(|e| {
            Error::msg(format!(
                "Could not read {} ({e}). Close the other browser and try again.",
                src.file_name().unwrap_or_default().to_string_lossy()
            ))
        })?;
        for suffix in ["-journal", "-wal", "-shm"] {
            let mut from = src.as_os_str().to_owned();
            from.push(suffix);
            let from = PathBuf::from(from);
            if from.is_file() {
                // A sidecar that vanishes between the check and the copy just means the browser checkpointed.
                let _ = std::fs::copy(&from, snapshot.sidecar(suffix));
            }
        }
        Ok(snapshot)
    }

    fn sidecar(&self, suffix: &str) -> PathBuf {
        let mut p = self.base.as_os_str().to_owned();
        p.push(suffix);
        PathBuf::from(p)
    }

    fn open(&self) -> Result<rusqlite::Connection> {
        // Read-write on the private copy so SQLite can roll back a hot journal.
        Ok(rusqlite::Connection::open(&self.base)?)
    }
}

impl Drop for SnapshotDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.base);
        for suffix in ["-journal", "-wal", "-shm"] {
            let _ = std::fs::remove_file(self.sidecar(suffix));
        }
    }
}

/// History read from the source profile: one row per page, and the individual visits with their times.
#[derive(Default)]
pub struct HistoryRead {
    pub urls: Vec<UrlRow>,
    pub visits: Vec<VisitRow>,
    /// Why visit times are missing, when only the page list could be read.
    pub visits_warning: Option<String>,
}

fn read_visits(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<VisitRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT u.url, v.visit_time FROM visits v JOIN urls u ON u.id = v.url
          WHERE u.hidden = 0 ORDER BY v.visit_time DESC LIMIT {VISIT_LIMIT}"
    ))?;
    let rows = stmt.query_map([], |r| Ok(VisitRow { url: r.get(0)?, visited_at: chromium_time_to_unix_ms(r.get(1)?) }))?;
    rows.collect()
}

fn read_history(profile_dir: &Path, scratch: &Path) -> Result<HistoryRead> {
    let snapshot = SnapshotDb::take(&profile_dir.join("History"), scratch, "history-import")?;
    let conn = snapshot.open()?;
    let mut stmt = conn.prepare(&format!(
        "SELECT url, title, visit_count, last_visit_time FROM urls
          WHERE hidden = 0 ORDER BY last_visit_time DESC LIMIT {HISTORY_LIMIT}"
    ))?;
    let rows = stmt.query_map([], |r| {
        Ok(UrlRow {
            url: r.get(0)?,
            title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            visit_count: r.get(2)?,
            last_visit: chromium_time_to_unix_ms(r.get(3)?),
        })
    })?;
    let urls = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    drop(stmt);

    // Individual visits give the history page its real timeline. Best effort: the page list above is
    // already enough to import, so a failure here is reported, not fatal.
    Ok(match read_visits(&conn) {
        Ok(visits) => HistoryRead { urls, visits, visits_warning: None },
        Err(e) => HistoryRead {
            urls,
            visits: Vec::new(),
            visits_warning: Some(format!("visit dates could not be read ({e}); pages were imported with their last visit date")),
        },
    })
}

// ---- open tabs (the last session) ------------------------------------------------------------

/// One tab of a saved session: the page it was showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTab {
    pub url: String,
    pub title: String,
    pub pinned: bool,
    pub window: i32,
}

/// Bounds-checked reader over the little-endian values of Chromium's session files.
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Cursor<'a> {
        Cursor { bytes, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let slice = self.bytes.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    /// A pickled string: a length, the bytes, then padding up to a multiple of four.
    fn string(&mut self) -> Option<String> {
        let len = usize::try_from(self.i32()?).ok()?;
        let raw = self.take(len)?;
        self.take((4 - len % 4) % 4)?;
        Some(String::from_utf8_lossy(raw).into_owned())
    }

    /// A pickled UTF-16 string: the length counts characters.
    fn string16(&mut self) -> Option<String> {
        let chars = usize::try_from(self.i32()?).ok()?;
        let len = chars.checked_mul(2)?;
        let raw = self.take(len)?;
        self.take((4 - len % 4) % 4)?;
        let units: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        Some(String::from_utf16_lossy(&units))
    }
}

// Session command ids (chrome/browser/sessions/session_service_commands.cc). Only the ones that decide
// which tabs exist and what they show are read; everything else is skipped by its length.
const CMD_SET_TAB_WINDOW: u8 = 0;
const CMD_SET_TAB_INDEX_IN_WINDOW: u8 = 2;
const CMD_PRUNED_FROM_BACK: u8 = 5;
const CMD_UPDATE_TAB_NAVIGATION: u8 = 6;
const CMD_SET_SELECTED_NAVIGATION_INDEX: u8 = 7;
const CMD_PRUNED_FROM_FRONT: u8 = 11;
const CMD_SET_PINNED_STATE: u8 = 12;
const CMD_TAB_CLOSED: u8 = 16;
const CMD_WINDOW_CLOSED: u8 = 17;

/// Reads the tabs out of a Chromium `Session_*` file (the "SNSS" format): which tabs are open in which
/// window, in order, and the page each is currently showing. Unreadable parts are skipped, never guessed.
pub fn parse_session(data: &[u8]) -> Result<Vec<SessionTab>> {
    if data.len() < 8 || &data[..4] != b"SNSS" {
        return Err(Error::msg("That isn't a saved browser session."));
    }
    let mut commands: Vec<(u8, &[u8])> = Vec::new();
    let mut pos = 8usize;
    while pos + 2 <= data.len() {
        let size = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
        pos += 2;
        if size == 0 || pos + size > data.len() {
            break;
        }
        commands.push((data[pos], &data[pos + 1..pos + size]));
        pos += size;
    }

    let mut tab_window: BTreeMap<i32, i32> = BTreeMap::new();
    let mut tab_index: BTreeMap<i32, i32> = BTreeMap::new();
    let mut navs: BTreeMap<i32, BTreeMap<i32, (String, String)>> = BTreeMap::new();
    let mut selected: BTreeMap<i32, i32> = BTreeMap::new();
    let mut pinned: BTreeMap<i32, bool> = BTreeMap::new();
    let mut closed_tabs: HashSet<i32> = HashSet::new();
    let mut closed_windows: HashSet<i32> = HashSet::new();

    for (id, payload) in commands {
        let mut c = Cursor::new(payload);
        match id {
            CMD_SET_TAB_WINDOW => {
                if let (Some(window), Some(tab)) = (c.i32(), c.i32()) {
                    tab_window.insert(tab, window);
                }
            }
            CMD_SET_TAB_INDEX_IN_WINDOW => {
                if let (Some(tab), Some(index)) = (c.i32(), c.i32()) {
                    tab_index.insert(tab, index);
                }
            }
            CMD_TAB_CLOSED => {
                if let Some(tab) = c.i32() {
                    closed_tabs.insert(tab);
                }
            }
            CMD_WINDOW_CLOSED => {
                if let Some(window) = c.i32() {
                    closed_windows.insert(window);
                }
            }
            CMD_UPDATE_TAB_NAVIGATION => {
                // The payload is a Pickle: its own length first, then tab id, index, url, title, …
                let _pickle_len = c.i32();
                if let (Some(tab), Some(index), Some(url), Some(title)) = (c.i32(), c.i32(), c.string(), c.string16()) {
                    navs.entry(tab).or_default().insert(index, (url, title));
                }
            }
            CMD_SET_SELECTED_NAVIGATION_INDEX => {
                if let (Some(tab), Some(index)) = (c.i32(), c.i32()) {
                    selected.insert(tab, index);
                }
            }
            CMD_SET_PINNED_STATE => {
                if let (Some(tab), Some(flag)) = (c.i32(), c.take(1)) {
                    pinned.insert(tab, flag[0] != 0);
                }
            }
            CMD_PRUNED_FROM_BACK => {
                if let (Some(tab), Some(count)) = (c.i32(), c.i32()) {
                    if let Some(n) = navs.get_mut(&tab) {
                        n.retain(|index, _| *index < count);
                    }
                }
            }
            CMD_PRUNED_FROM_FRONT => {
                if let (Some(tab), Some(count)) = (c.i32(), c.i32()) {
                    if let Some(n) = navs.get_mut(&tab) {
                        *n = std::mem::take(n).into_iter().filter(|(index, _)| *index >= count).map(|(index, v)| (index - count, v)).collect();
                    }
                    if let Some(s) = selected.get_mut(&tab) {
                        *s = (*s - count).max(0);
                    }
                }
            }
            _ => {}
        }
    }

    let mut tabs: Vec<(i32, i32, SessionTab)> = Vec::new();
    for (tab, window) in &tab_window {
        if closed_tabs.contains(tab) || closed_windows.contains(window) {
            continue;
        }
        let Some(history) = navs.get(tab).filter(|n| !n.is_empty()) else { continue };
        let wanted = selected.get(tab).copied().unwrap_or_else(|| *history.keys().next_back().unwrap_or(&0));
        let current = history.get(&wanted).or_else(|| history.range(..=wanted).next_back().map(|(_, v)| v)).or_else(|| history.values().next_back());
        let Some((url, title)) = current else { continue };
        tabs.push((
            *window,
            tab_index.get(tab).copied().unwrap_or(0),
            SessionTab { url: url.clone(), title: title.clone(), pinned: pinned.get(tab).copied().unwrap_or(false), window: *window },
        ));
    }
    tabs.sort_by_key(|(window, index, _)| (*window, *index));
    Ok(tabs.into_iter().map(|(_, _, tab)| tab).collect())
}

/// The newest session file of a profile: `Sessions/Session_<time>` in current versions, `Last Session` /
/// `Current Session` in old ones. (`Tabs_*` files hold recently closed tabs, which are not wanted.)
fn latest_session_file(profile_dir: &Path) -> Option<PathBuf> {
    let newest = std::fs::read_dir(profile_dir.join("Sessions"))
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let stamp = name.strip_prefix("Session_")?.parse::<u64>().ok()?;
            Some((stamp, e.path()))
        })
        .max_by_key(|(stamp, _)| *stamp)
        .map(|(_, path)| path);
    newest.or_else(|| ["Last Session", "Current Session"].iter().map(|n| profile_dir.join(n)).find(|p| p.is_file()))
}

fn read_session(profile_dir: &Path) -> Result<Vec<SessionTab>> {
    let path = latest_session_file(profile_dir).ok_or_else(|| Error::msg("this profile has no saved session"))?;
    let meta = std::fs::metadata(&path)?;
    if meta.len() > MAX_SESSION_BYTES {
        return Err(Error::msg("the saved session is too large to read"));
    }
    let bytes = std::fs::read(&path).map_err(|e| {
        // Chrome holds its current session file open for exclusive use while it runs.
        if e.kind() == std::io::ErrorKind::PermissionDenied || e.raw_os_error() == Some(32) {
            Error::msg("the browser is still running and has its current session open. Close all of its windows (also in the system tray), then import the open tabs")
        } else {
            Error::msg(format!("the saved session could not be read ({e})"))
        }
    })?;
    parse_session(&bytes)
}

/// Pages worth reopening: web addresses and local files, not the source browser's internal pages.
fn is_session_page(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://") || url.starts_with("file:")
}

fn session_nodes(tabs: &[SessionTab]) -> Vec<ImportNode> {
    let mut windows: BTreeMap<i32, Vec<ImportNode>> = BTreeMap::new();
    for t in tabs.iter().filter(|t| is_session_page(&t.url)) {
        let title = if t.title.trim().is_empty() { t.url.clone() } else { t.title.clone() };
        windows.entry(t.window).or_default().push(ImportNode::Url { title, url: t.url.clone(), created_at: 0 });
    }
    if windows.len() <= 1 {
        return windows.into_values().flatten().collect();
    }
    windows.into_values().enumerate().map(|(i, children)| ImportNode::Folder { title: format!("Window {}", i + 1), children }).collect()
}

// ---- saved addresses ------------------------------------------------------------------------

/// The `autocomplete` field a Chromium address type code stands for (`ServerFieldType` in Chromium).
fn address_field(code: i64) -> Option<&'static str> {
    Some(match code {
        3 => "given-name",
        4 => "additional-name",
        5 => "family-name",
        7 => "name",
        9 => "email",
        14 => "tel",
        33 => "address-level2",
        34 => "address-level1",
        35 => "postal-code",
        36 => "country",
        60 => "organization",
        77 => "street-address",
        _ => return None,
    })
}

fn read_address_ids(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT guid FROM addresses")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    rows.collect()
}

fn read_addresses(profile_dir: &Path, scratch: &Path) -> Result<Vec<AddressData>> {
    let snapshot = SnapshotDb::take(&profile_dir.join("Web Data"), scratch, "address-import")?;
    let conn = snapshot.open()?;
    let guids = read_address_ids(&conn).map_err(|_| Error::msg("this version of the browser keeps addresses in a format this one can't read yet"))?;
    let mut out = Vec::new();
    for guid in guids {
        let mut stmt = conn.prepare("SELECT type, value FROM address_type_tokens WHERE guid = ?1")?;
        let rows = stmt.query_map([&guid], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default())))?;
        let mut fields = BTreeMap::new();
        for row in rows {
            let (code, value) = row?;
            if let (Some(field), false) = (address_field(code), value.trim().is_empty()) {
                fields.insert(field.to_string(), value.trim().to_string());
            }
        }
        out.push(AddressData { source_id: guid, fields });
    }
    Ok(out)
}

// ---- default search engine ------------------------------------------------------------

/// Expands Chromium's `{placeholder}` syntax. With `as_template` the search terms become `%s`;
/// otherwise they become a dummy word so the result can be parsed as a URL. Optional tracking
/// parameters (`{google:…}`, `{bing:…}`) are dropped; any other placeholder makes the engine
/// unsupported rather than guessed at.
fn expand_placeholders(url: &str, as_template: bool) -> Option<String> {
    let mut out = String::new();
    let mut rest = url;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let close = rest[open..].find('}')? + open;
        let name = &rest[open + 1..close];
        match name {
            "searchTerms" => out.push_str(if as_template { "%s" } else { "x" }),
            "google:baseURL" => out.push_str("https://www.google.com/"),
            "bing:baseURL" => out.push_str("https://www.bing.com/"),
            "inputEncoding" => out.push_str("UTF-8"),
            n if n.starts_with("google:") || n.starts_with("bing:") => {}
            _ => return None,
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// Maps a Chromium search engine definition onto a built-in engine or a safe custom template.
pub fn map_search_engine(short_name: &str, url: &str) -> Option<ImportedSearch> {
    let probe = url::Url::parse(&expand_placeholders(url, false)?).ok()?;
    if probe.scheme() != "https" {
        return None;
    }
    let host = probe.host_str()?.to_ascii_lowercase();
    let labels: Vec<&str> = host.split('.').collect();
    let has = |name: &str| labels.contains(&name);
    let engine = if has("google") {
        Some(SearchEngine::Google)
    } else if host == "www.bing.com" || host == "bing.com" {
        Some(SearchEngine::Bing)
    } else if host == "duckduckgo.com" || host.ends_with(".duckduckgo.com") {
        Some(SearchEngine::DuckDuckGo)
    } else if host == "search.brave.com" {
        Some(SearchEngine::Brave)
    } else if has("startpage") {
        Some(SearchEngine::Startpage)
    } else if has("ecosia") {
        Some(SearchEngine::Ecosia)
    } else {
        None
    };
    let name = if short_name.trim().is_empty() { host.clone() } else { short_name.trim().to_string() };
    if engine.is_some() {
        return Some(ImportedSearch { name, engine, template: None });
    }
    let template = expand_placeholders(url, true)?;
    if template.matches("%s").count() != 1 || !probe.username().is_empty() || probe.password().is_some() {
        return None;
    }
    Some(ImportedSearch { name, engine: None, template: Some(template) })
}

/// Reads the default search engine the user chose in the source profile. Chromium keeps it in
/// `Secure Preferences` (and mirrors it in `Preferences`); both are plain JSON, and only that one
/// subtree is looked at.
fn read_default_search(profile_dir: &Path) -> Option<ImportedSearch> {
    for file in ["Secure Preferences", "Preferences"] {
        let Ok(text) = std::fs::read_to_string(profile_dir.join(file)) else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
        for key in ["template_url_data", "mirrored_template_url_data"] {
            let Some(t) = v.get("default_search_provider_data").and_then(|d| d.get(key)) else { continue };
            let url = t.get("url").and_then(|u| u.as_str()).unwrap_or("");
            let name = t.get("short_name").and_then(|u| u.as_str()).unwrap_or("");
            if let Some(found) = map_search_engine(name, url) {
                return Some(found);
            }
        }
    }
    None
}

// ---- running an import ----------------------------------------------------------------

pub struct ImportRequest<'a> {
    pub local_app_data: &'a Path,
    pub scratch: &'a Path,
    pub browser: SourceBrowser,
    pub profile_dir: &'a str,
    pub bookmarks: bool,
    pub history: bool,
    pub search_engine: bool,
    pub addresses: bool,
    pub session: bool,
}

/// Everything read from the source profile, ready to be written. Holds no passwords.
pub struct Collected {
    browser_label: &'static str,
    profile_name: String,
    bookmarks: Option<Vec<ImportNode>>,
    history: Option<HistoryRead>,
    search: Option<ImportedSearch>,
    addresses: Option<Vec<AddressData>>,
    session: Option<Vec<SessionTab>>,
    errors: Vec<String>,
}

impl Collected {
    /// Hands the history over so the caller can write it in small pieces (a large history is hundreds of
    /// thousands of rows and must not hold the browser's controller for seconds).
    pub fn take_history(&mut self) -> Option<HistoryRead> {
        self.history.take()
    }
}

/// Reads the requested data from the source profile. Does not touch the browser's own database.
pub fn collect(req: &ImportRequest<'_>) -> Result<Collected> {
    // Only accept profile folder names that were actually detected, never arbitrary paths.
    let detected = detect(req.local_app_data);
    let profile = detected
        .iter()
        .find(|p| p.browser == req.browser && p.profile_dir == req.profile_dir)
        .ok_or_else(|| Error::msg("That browser profile could not be found any more. Close this window and start the import again."))?;
    let pdir = req.browser.user_data_dir(req.local_app_data).join(&profile.profile_dir);
    let mut out = Collected {
        browser_label: req.browser.label(),
        profile_name: profile.name.clone(),
        bookmarks: None,
        history: None,
        search: None,
        addresses: None,
        session: None,
        errors: Vec::new(),
    };

    if req.bookmarks {
        let read = read_bookmarks(&pdir);
        out.errors.extend(read.warnings.iter().map(|w| format!("Bookmarks: {w}")));
        if read.files_found == 0 {
            out.errors.push("Bookmarks: this profile has no bookmarks file".into());
        } else {
            out.bookmarks = Some(read.nodes);
        }
    }
    if req.history {
        match read_history(&pdir, req.scratch) {
            Ok(read) => {
                if let Some(w) = &read.visits_warning {
                    out.errors.push(format!("History: {w}"));
                }
                out.history = Some(read);
            }
            Err(e) => out.errors.push(format!("History: {e}")),
        }
    }
    if req.search_engine {
        match read_default_search(&pdir) {
            Some(s) => out.search = Some(s),
            None => out.errors.push("Search engine: this profile does not use a search engine that can be imported".into()),
        }
    }
    if req.addresses {
        match read_addresses(&pdir, req.scratch) {
            Ok(list) => out.addresses = Some(list),
            Err(e) => out.errors.push(format!("Addresses: {e}")),
        }
    }
    if req.session {
        match read_session(&pdir) {
            Ok(tabs) => out.session = Some(tabs),
            Err(e) => out.errors.push(format!("Open tabs: {e}")),
        }
    }
    Ok(out)
}

/// Result of [`apply`]: what to tell the user, plus a search engine for the caller to adopt.
pub struct ImportOutcome {
    pub report: ImportReport,
    pub search: Option<ImportedSearch>,
}

/// Writes one slice of page rows (history). Idempotent, see `Db::import_urls`.
pub fn apply_history_urls(db: &Db, rows: &[UrlRow]) -> Result<ImportCounts> {
    db.import_urls(rows)
}

/// Writes one slice of visit rows. A visit is only stored for a page that exists and has no visit at that time yet.
pub fn apply_history_visits(db: &Db, rows: &[VisitRow]) -> Result<ImportCounts> {
    db.import_visits(rows)
}

/// Writes collected data to the profile database. Each data type is written in its own
/// transaction and is safe to run again: nothing already imported is added twice. History still held
/// by `collected` is written here in one go (the app takes it out first and writes it in pieces).
pub fn apply(db: &Db, sealer: &dyn Sealer, collected: Collected) -> ImportOutcome {
    let mut report = ImportReport { errors: collected.errors, ..Default::default() };
    let folder = format!("Imported from {} ({})", collected.browser_label, collected.profile_name);

    if let Some(nodes) = collected.bookmarks.filter(|n| !n.is_empty()) {
        match db.bookmarks_import(OTHER_ID, &folder, &nodes, now_ms()) {
            Ok(c) => {
                report.bookmarks_imported = c.added;
                report.bookmarks_skipped = c.skipped;
            }
            Err(e) => report.errors.push(format!("Bookmarks: {e}")),
        }
    }
    if let Some(history) = collected.history {
        match apply_history_urls(db, &history.urls) {
            Ok(c) => {
                report.history_imported = c.added;
                report.history_skipped = c.skipped;
            }
            Err(e) => report.errors.push(format!("History: {e}")),
        }
        match apply_history_visits(db, &history.visits) {
            Ok(c) => report.history_visits_imported = c.added,
            Err(e) => report.errors.push(format!("History visits: {e}")),
        }
    }
    if let Some(tabs) = collected.session {
        report.session_tabs = tabs.len();
        report.session_urls = tabs.iter().filter(|t| is_session_page(&t.url)).map(|t| t.url.clone()).take(SESSION_URLS_SHOWN).collect();
        let nodes = session_nodes(&tabs);
        if !nodes.is_empty() {
            let wrapped = [ImportNode::Folder { title: "Open tabs".into(), children: nodes }];
            match db.bookmarks_import(OTHER_ID, &folder, &wrapped, now_ms()) {
                Ok(c) => report.session_tabs_saved = c.added,
                Err(e) => report.errors.push(format!("Open tabs: {e}")),
            }
        }
    }
    if let Some(profiles) = collected.addresses {
        let total = profiles.len();
        match crate::vault::import_profiles(db, sealer, profiles, now_ms()) {
            Ok(c) => {
                report.addresses_imported = c.imported;
                report.addresses_skipped = total.saturating_sub(c.imported);
            }
            Err(e) => report.errors.push(format!("Addresses: {e}")),
        }
    }
    if let Some(s) = &collected.search {
        report.search_engine = Some(s.name.clone());
    }
    ImportOutcome { report, search: collected.search }
}

/// Collect and apply in one step (used by tests; the app runs the two halves on different threads).
pub fn run_import(db: &Db, sealer: &dyn Sealer, req: ImportRequest<'_>) -> Result<ImportOutcome> {
    Ok(apply(db, sealer, collect(&req)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::TestSealer;

    const SAMPLE: &str = r#"{
      "roots": {
        "bookmark_bar": {"type": "folder", "name": "Bookmarks bar", "children": [
          {"type": "url", "name": "Rust", "url": "https://www.rust-lang.org/", "date_added": "13300000000000000"},
          {"type": "folder", "name": "Games", "children": [
            {"type": "url", "name": "Steam", "url": "https://store.steampowered.com/"}
          ]}
        ]},
        "other": {"type": "folder", "name": "Other", "children": []},
        "synced": {"type": "folder", "name": "Mobile", "children": [
          {"type": "url", "name": "M", "url": "https://m.example/"}
        ]}
      },
      "version": 1
    }"#;

    /// Shape of the `AccountBookmarks` file Chrome writes for a signed-in profile.
    const ACCOUNT_SAMPLE: &str = r#"{
      "checksum": "abc",
      "roots": {
        "bookmark_bar": {"type": "folder", "name": "Bookmarks bar", "children": []},
        "other": {"type": "folder", "name": "Other bookmarks", "children": [
          {"type": "url", "name": "Account only", "url": "https://account.example/"},
          {"type": "url", "name": "Rust again", "url": "https://www.rust-lang.org/"}
        ]},
        "synced": {"type": "folder", "name": "Mobile bookmarks", "children": []}
      },
      "sync_metadata": "ignored",
      "version": 1
    }"#;

    fn temp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("pb-import-{tag}-{}-{}", std::process::id(), now_ms()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    fn fake_chrome(root: &Path) -> PathBuf {
        let ud = SourceBrowser::Chrome.user_data_dir(root);
        let pdir = ud.join("Default");
        std::fs::create_dir_all(&pdir).unwrap();
        std::fs::write(ud.join("Local State"), r#"{"profile":{"info_cache":{"Default":{"name":"Gamer"}}}}"#).unwrap();
        pdir
    }

    /// Chromium time of 2021-01-01T00:00:00Z plus `secs`.
    fn chrome_time(secs: i64) -> i64 {
        13_253_932_800_000_000 + secs * 1_000_000
    }

    fn write_history(pdir: &Path) {
        let conn = rusqlite::Connection::open(pdir.join("History")).unwrap();
        conn.execute_batch(&format!(
            "CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT, title TEXT, visit_count INTEGER, typed_count INTEGER, last_visit_time INTEGER, hidden INTEGER);
             INSERT INTO urls (id, url, title, visit_count, typed_count, last_visit_time, hidden) VALUES
               (1, 'https://a.example/', 'A', 3, 0, {t3}, 0),
               (2, 'https://hidden.example/', 'H', 1, 0, {t1}, 1);
             CREATE TABLE visits (id INTEGER PRIMARY KEY, url INTEGER, visit_time INTEGER, from_visit INTEGER, transition INTEGER);
             INSERT INTO visits (url, visit_time) VALUES (1, {t1}), (1, {t2}), (1, {t3}), (2, {t1});",
            t1 = chrome_time(0),
            t2 = chrome_time(3600),
            t3 = chrome_time(7200),
        ))
        .unwrap();
    }

    fn request<'a>(root: &'a Path, scratch: &'a Path, profile_dir: &'a str) -> ImportRequest<'a> {
        ImportRequest {
            local_app_data: root,
            scratch,
            browser: SourceBrowser::Chrome,
            profile_dir,
            bookmarks: true,
            history: true,
            search_engine: false,
            addresses: false,
            session: false,
        }
    }

    fn import(db: &Db, req: ImportRequest<'_>) -> Result<ImportOutcome> {
        run_import(db, &TestSealer, req)
    }

    #[test]
    fn parses_bookmark_roots() {
        let nodes = parse_bookmarks(SAMPLE).unwrap();
        assert_eq!(nodes.len(), 2, "empty roots are skipped");
        assert_eq!(nodes.iter().map(ImportNode::count_urls).sum::<usize>(), 3);
        match &nodes[0] {
            ImportNode::Folder { title, children } => {
                assert_eq!(title, "Bookmarks bar");
                assert!(matches!(&children[0], ImportNode::Url { created_at, .. } if *created_at > 1_600_000_000_000));
            }
            _ => panic!(),
        }
        assert!(parse_bookmarks("{}").is_err());
    }

    #[test]
    fn chromium_epoch_conversion() {
        // 2021-01-01T00:00:00Z
        assert_eq!(chromium_time_to_unix_ms(13_253_932_800_000_000), 1_609_459_200_000);
        assert_eq!(chromium_time_to_unix_ms(0), 0);
    }

    #[test]
    fn detects_and_imports_from_fake_install() {
        let root = temp_root("full");
        let pdir = fake_chrome(&root);
        std::fs::write(pdir.join("Bookmarks"), SAMPLE).unwrap();
        std::fs::write(pdir.join("Login Data"), b"x").unwrap();
        write_history(&pdir);

        let detected = detect(&root);
        assert_eq!(detected.len(), 1);
        assert_eq!(detected[0].name, "Gamer");
        let pw = detected[0].items.iter().find(|i| i.kind == "passwords").unwrap();
        assert!(pw.present && pw.supported, "passwords are offered through the user's own export, never read from the browser");
        let cookies = detected[0].items.iter().find(|i| i.kind == "cookies").unwrap();
        assert!(!cookies.supported && cookies.detail.contains("Not imported"));

        let db = Db::open_in_memory().unwrap();
        let scratch = root.join("scratch");
        let out = import(&db, request(&root, &scratch, "Default")).unwrap();
        assert_eq!(out.report.bookmarks_imported, 3);
        assert_eq!(out.report.history_imported, 1);
        assert_eq!(out.report.history_visits_imported, 2, "the visit at the last-visit time already exists; the other two are new");
        assert!(out.report.errors.is_empty(), "{:?}", out.report.errors);
        assert!(db.is_bookmarked("https://store.steampowered.com/").unwrap());
        assert!(std::fs::read_dir(&scratch).unwrap().next().is_none(), "scratch copies are removed");

        let bad = import(&db, ImportRequest { profile_dir: "..\\..\\Windows", history: false, ..request(&root, &scratch, "Default") });
        assert!(bad.is_err(), "only detected profile folders are accepted");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn history_keeps_every_visit_with_its_own_time() {
        let root = temp_root("visits");
        let pdir = fake_chrome(&root);
        write_history(&pdir);
        let db = Db::open_in_memory().unwrap();
        import(&db, ImportRequest { bookmarks: false, ..request(&root, &root.join("s"), "Default") }).unwrap();

        let entries = db.history_query("", 10, None).unwrap();
        let times: Vec<i64> = entries.iter().map(|e| e.visited_at).collect();
        assert_eq!(times, vec![1_609_459_200_000 + 7_200_000, 1_609_459_200_000 + 3_600_000, 1_609_459_200_000], "newest first, original timestamps");
        assert!(entries.iter().all(|e| e.url == "https://a.example/"), "hidden pages are left out");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn history_without_a_visits_table_still_imports_pages_and_says_so() {
        let root = temp_root("novisits");
        let pdir = fake_chrome(&root);
        let conn = rusqlite::Connection::open(pdir.join("History")).unwrap();
        conn.execute_batch(
            "CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT, title TEXT, visit_count INTEGER, typed_count INTEGER, last_visit_time INTEGER, hidden INTEGER);
             INSERT INTO urls VALUES (1, 'https://a.example/', 'A', 1, 0, 13253932800000000, 0);",
        )
        .unwrap();
        drop(conn);
        let db = Db::open_in_memory().unwrap();
        let out = import(&db, ImportRequest { bookmarks: false, ..request(&root, &root.join("s"), "Default") }).unwrap();
        assert_eq!((out.report.history_imported, out.report.history_visits_imported), (1, 0));
        assert!(out.report.errors.iter().any(|e| e.contains("visit dates could not be read")), "{:?}", out.report.errors);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn importing_the_same_profile_twice_adds_nothing() {
        let root = temp_root("twice");
        let pdir = fake_chrome(&root);
        std::fs::write(pdir.join("Bookmarks"), SAMPLE).unwrap();
        write_history(&pdir);
        let db = Db::open_in_memory().unwrap();
        let scratch = root.join("scratch");

        let first = import(&db, request(&root, &scratch, "Default")).unwrap().report;
        assert_eq!((first.bookmarks_imported, first.history_imported, first.history_visits_imported), (3, 1, 2));
        let second = import(&db, request(&root, &scratch, "Default")).unwrap().report;
        assert_eq!((second.bookmarks_imported, second.history_imported, second.history_visits_imported), (0, 0, 0));
        assert_eq!((second.bookmarks_skipped, second.history_skipped), (3, 1));
        let bookmark_rows = db.bookmarks_all().unwrap().iter().filter(|b| b.kind == "url").count();
        assert_eq!(bookmark_rows, 3, "no duplicate bookmarks after a retry");
        let visits: i64 = db.conn.query_row("SELECT COUNT(*) FROM visits", [], |r| r.get(0)).unwrap();
        assert_eq!(visits, 3, "no duplicate visits after a retry");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn signed_in_chrome_profile_with_only_account_bookmarks_is_imported() {
        // Real Chrome profiles that sync bookmarks keep them in `AccountBookmarks`, with no `Bookmarks` file.
        let root = temp_root("account");
        let pdir = fake_chrome(&root);
        std::fs::write(pdir.join("AccountBookmarks"), ACCOUNT_SAMPLE).unwrap();
        std::fs::write(pdir.join("EncryptedAccountBookmarks2"), b"v20\x01\x02\x03").unwrap();

        let detected = detect(&root);
        let item = detected[0].items.iter().find(|i| i.kind == "bookmarks").unwrap();
        assert!(item.present && item.detail.starts_with("2 bookmarks"), "{}", item.detail);

        let db = Db::open_in_memory().unwrap();
        let out = import(&db, ImportRequest { history: false, ..request(&root, &root.join("s"), "Default") }).unwrap();
        assert_eq!(out.report.bookmarks_imported, 2);
        assert!(db.is_bookmarked("https://account.example/").unwrap());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn both_bookmark_files_are_merged_without_duplicates() {
        let root = temp_root("merge");
        let pdir = fake_chrome(&root);
        std::fs::write(pdir.join("Bookmarks"), SAMPLE).unwrap();
        std::fs::write(pdir.join("AccountBookmarks"), ACCOUNT_SAMPLE).unwrap();
        let db = Db::open_in_memory().unwrap();
        let out = import(&db, ImportRequest { history: false, ..request(&root, &root.join("s"), "Default") }).unwrap();
        // 3 local + "account.example"; www.rust-lang.org is in both files but lives in different folders.
        assert_eq!(out.report.bookmarks_imported, 5);
        assert!(out.report.errors.is_empty(), "{:?}", out.report.errors);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_corrupt_bookmark_file_is_reported_not_hidden() {
        let root = temp_root("corrupt");
        let pdir = fake_chrome(&root);
        std::fs::write(pdir.join("Bookmarks"), "{ not json").unwrap();
        std::fs::write(pdir.join("AccountBookmarks"), ACCOUNT_SAMPLE).unwrap();
        let db = Db::open_in_memory().unwrap();
        let out = import(&db, ImportRequest { history: false, ..request(&root, &root.join("s"), "Default") }).unwrap();
        assert_eq!(out.report.bookmarks_imported, 2, "the readable file still imports");
        assert!(out.report.errors.iter().any(|e| e.contains("Bookmarks could not be read")), "{:?}", out.report.errors);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_data_is_an_error_not_a_success() {
        let root = temp_root("empty");
        fake_chrome(&root);
        let db = Db::open_in_memory().unwrap();
        let out = import(&db, request(&root, &root.join("s"), "Default")).unwrap();
        assert_eq!((out.report.bookmarks_imported, out.report.history_imported), (0, 0));
        assert!(!out.report.errors.is_empty(), "an import that found nothing must say so");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn history_snapshot_carries_sidecars_and_leaves_the_source_alone() {
        // Chrome can leave `History-journal` beside `History` while it runs; the snapshot copies it along
        // and the originals must stay byte-for-byte unchanged.
        let root = temp_root("journal");
        let pdir = fake_chrome(&root);
        write_history(&pdir);
        std::fs::write(pdir.join("History-journal"), b"").unwrap(); // an empty journal is ignored by SQLite
        let before = std::fs::read(pdir.join("History")).unwrap();
        let read = read_history(&pdir, &root.join("s")).unwrap();
        assert_eq!((read.urls.len(), read.visits.len()), (1, 3));
        assert_eq!(std::fs::read(pdir.join("History")).unwrap(), before, "source history is not modified");
        assert!(pdir.join("History-journal").is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    // ---- login and address counting, addresses ---------------------------------------------------

    fn write_logins(path: &Path, rows: usize, blocked: usize) {
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute_batch("CREATE TABLE logins (origin_url TEXT, username_value TEXT, password_value BLOB, blacklisted_by_user INTEGER);").unwrap();
        for i in 0..rows {
            // The blob stands for Chrome's app-bound ciphertext ("v20" prefix); the importer never reads it.
            conn.execute("INSERT INTO logins VALUES (?1, 'u', X'763230AABB', 0)", [format!("https://site{i}.example/")]).unwrap();
        }
        for _ in 0..blocked {
            conn.execute("INSERT INTO logins VALUES ('https://never.example/', '', X'', 1)", []).unwrap();
        }
    }

    #[test]
    fn saved_logins_are_counted_but_never_read() {
        let root = temp_root("logins");
        let pdir = fake_chrome(&root);
        write_logins(&pdir.join("Login Data"), 2, 1);
        write_logins(&pdir.join("Login Data For Account"), 3, 0);
        assert_eq!(login_count(&pdir), Some(5), "both stores are counted, never-save entries are not");
        let item = detect(&root).remove(0).items.into_iter().find(|i| i.kind == "passwords").unwrap();
        assert!(item.present && item.supported && item.detail.starts_with("5 saved logins found"), "{}", item.detail);
        assert!(item.detail.contains("export"), "the wizard points at the user's own export");
        let json = serde_json::to_string(&detect(&root)).unwrap();
        assert!(!json.contains("site0.example") && !json.contains("763230"), "no login content in what the shell sees");
        let _ = std::fs::remove_dir_all(root);
    }

    fn write_web_data(pdir: &Path) {
        let conn = rusqlite::Connection::open(pdir.join("Web Data")).unwrap();
        conn.execute_batch(
            "CREATE TABLE addresses (guid TEXT PRIMARY KEY, use_count INTEGER);
             CREATE TABLE address_type_tokens (guid TEXT, type INTEGER, value TEXT, verification_status INTEGER, observations BLOB);
             INSERT INTO addresses VALUES ('g-1', 1), ('g-2', 0);
             INSERT INTO address_type_tokens (guid, type, value) VALUES
               ('g-1', 3, 'Ana'), ('g-1', 5, 'Koch'), ('g-1', 9, 'ana@example.com'), ('g-1', 33, 'Graz'), ('g-1', 35, '8010'),
               ('g-1', 36, 'AT'), ('g-1', 77, 'Hauptplatz 1'), ('g-1', 4, ''), ('g-1', 110, 'internal-token'),
               ('g-2', 110, 'only-internal-tokens');
             CREATE TABLE autofill (name TEXT, value TEXT, count INTEGER);
             INSERT INTO autofill VALUES ('ACCESS_TOKEN', 'ghp_secret_typed_into_a_form', 3);",
        )
        .unwrap();
    }

    #[test]
    fn address_profiles_are_imported_sealed_and_form_history_is_not_copied() {
        let root = temp_root("addr");
        let pdir = fake_chrome(&root);
        write_web_data(&pdir);
        assert_eq!(address_count(&pdir), Some(2));
        let item = detect(&root).remove(0).items.into_iter().find(|i| i.kind == "autofill").unwrap();
        assert!(item.present && item.detail.contains("2 saved addresses"), "{}", item.detail);

        let db = Db::open_in_memory().unwrap();
        let scratch = root.join("s");
        let req = ImportRequest { bookmarks: false, history: false, addresses: true, ..request(&root, &scratch, "Default") };
        let out = import(&db, req).unwrap();
        assert_eq!((out.report.addresses_imported, out.report.addresses_skipped), (1, 1), "a profile with no usable field is not stored");
        assert!(out.report.errors.is_empty(), "{:?}", out.report.errors);

        let list = crate::vault::list_profiles(&db, &TestSealer).unwrap();
        assert_eq!(list.len(), 1);
        let fields = crate::vault::open_profile(&db, &TestSealer, list[0].id).unwrap();
        assert_eq!(fields.get("given-name").map(String::as_str), Some("Ana"));
        assert_eq!(fields.get("postal-code").map(String::as_str), Some("8010"));
        assert_eq!(fields.get("street-address").map(String::as_str), Some("Hauptplatz 1"));
        assert!(!fields.contains_key("additional-name"), "empty values are dropped");

        // Neither the free-form autofill history nor any plaintext ends up in the database.
        let everything: Vec<u8> = db.conn.query_row("SELECT sealed FROM vault_profiles", [], |r| r.get(0)).unwrap();
        assert!(!everything.windows(3).any(|w| w == b"Ana"));
        let dump: i64 = db.conn.query_row("SELECT COUNT(*) FROM kv WHERE value LIKE '%ghp_%'", [], |r| r.get(0)).unwrap();
        assert_eq!(dump, 0);

        let again =
            import(&db, ImportRequest { bookmarks: false, history: false, addresses: true, ..request(&root, &root.join("s"), "Default") }).unwrap();
        assert_eq!(again.report.addresses_imported, 0, "importing again adds nothing");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn an_unknown_address_format_is_reported() {
        let root = temp_root("addr-old");
        let pdir = fake_chrome(&root);
        let conn = rusqlite::Connection::open(pdir.join("Web Data")).unwrap();
        conn.execute_batch("CREATE TABLE autofill_profiles (guid TEXT, first_name TEXT);").unwrap();
        drop(conn);
        let db = Db::open_in_memory().unwrap();
        let scratch = root.join("s");
        let req = ImportRequest { bookmarks: false, history: false, addresses: true, ..request(&root, &scratch, "Default") };
        let out = import(&db, req).unwrap();
        assert!(out.report.errors.iter().any(|e| e.starts_with("Addresses:") && e.contains("can't read yet")), "{:?}", out.report.errors);
        let _ = std::fs::remove_dir_all(root);
    }

    // ---- sessions -------------------------------------------------------------------------------------

    fn command(id: u8, payload: &[u8]) -> Vec<u8> {
        let mut out = ((payload.len() + 1) as u16).to_le_bytes().to_vec();
        out.push(id);
        out.extend_from_slice(payload);
        out
    }

    fn ints(values: &[i32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    fn pickled_string(s: &str) -> Vec<u8> {
        let mut out = (s.len() as i32).to_le_bytes().to_vec();
        out.extend_from_slice(s.as_bytes());
        out.resize(out.len() + (4 - s.len() % 4) % 4, 0);
        out
    }

    fn pickled_string16(s: &str) -> Vec<u8> {
        let units: Vec<u16> = s.encode_utf16().collect();
        let mut out = (units.len() as i32).to_le_bytes().to_vec();
        out.extend(units.iter().flat_map(|u| u.to_le_bytes()));
        out.resize(out.len() + (4 - (units.len() * 2) % 4) % 4, 0);
        out
    }

    fn navigation(tab: i32, index: i32, url: &str, title: &str) -> Vec<u8> {
        let mut body = ints(&[tab, index]);
        body.extend(pickled_string(url));
        body.extend(pickled_string16(title));
        body.extend(pickled_string("opaque page state"));
        body.extend(ints(&[0, 0]));
        let mut payload = (body.len() as i32).to_le_bytes().to_vec();
        payload.extend(body);
        command(CMD_UPDATE_TAB_NAVIGATION, &payload)
    }

    fn session_file(commands: Vec<Vec<u8>>) -> Vec<u8> {
        let mut out = b"SNSS".to_vec();
        out.extend(ints(&[3]));
        out.extend(commands.concat());
        out
    }

    /// Two windows; tab 12 was closed; tab 11 has two entries and shows the first; tab 13 is pinned.
    fn sample_session() -> Vec<u8> {
        session_file(vec![
            command(CMD_SET_TAB_WINDOW, &ints(&[1, 10])),
            command(CMD_SET_TAB_INDEX_IN_WINDOW, &ints(&[10, 0])),
            navigation(10, 0, "https://one.example/", "One"),
            command(CMD_SET_TAB_WINDOW, &ints(&[1, 11])),
            command(CMD_SET_TAB_INDEX_IN_WINDOW, &ints(&[11, 1])),
            navigation(11, 0, "https://two.example/a", "Two A"),
            navigation(11, 1, "https://two.example/b", "Two B (ümlaut)"),
            command(CMD_SET_SELECTED_NAVIGATION_INDEX, &ints(&[11, 0])),
            command(CMD_SET_TAB_WINDOW, &ints(&[1, 12])),
            navigation(12, 0, "https://closed.example/", "Closed"),
            command(CMD_TAB_CLOSED, &[ints(&[12]), vec![0; 12]].concat()),
            command(CMD_SET_TAB_WINDOW, &ints(&[2, 13])),
            navigation(13, 0, "https://pinned.example/", "Pinned"),
            command(CMD_SET_PINNED_STATE, &[ints(&[13]), vec![1, 0, 0, 0]].concat()),
            command(CMD_SET_TAB_WINDOW, &ints(&[2, 14])),
            navigation(14, 0, "chrome://settings/", "Settings"),
            command(99, b"unknown commands are skipped"),
        ])
    }

    #[test]
    fn reads_tabs_from_a_session_file() {
        let tabs = parse_session(&sample_session()).unwrap();
        let summary: Vec<(i32, &str, &str, bool)> = tabs.iter().map(|t| (t.window, t.url.as_str(), t.title.as_str(), t.pinned)).collect();
        assert_eq!(
            summary,
            vec![
                (1, "https://one.example/", "One", false),
                (1, "https://two.example/a", "Two A", false), // the selected entry, not the last one
                (2, "https://pinned.example/", "Pinned", true),
                (2, "chrome://settings/", "Settings", false),
            ],
            "closed tabs are gone, windows and order are kept"
        );
    }

    #[test]
    fn session_navigation_pruning_and_closed_windows() {
        let data = session_file(vec![
            command(CMD_SET_TAB_WINDOW, &ints(&[1, 1])),
            navigation(1, 0, "https://a.example/", "A"),
            navigation(1, 1, "https://b.example/", "B"),
            navigation(1, 2, "https://c.example/", "C"),
            command(CMD_PRUNED_FROM_BACK, &ints(&[1, 2])), // forget entries from index 2
            command(CMD_SET_TAB_WINDOW, &ints(&[2, 2])),
            navigation(2, 0, "https://gone.example/", "Gone"),
            command(CMD_WINDOW_CLOSED, &[ints(&[2]), vec![0; 12]].concat()),
        ]);
        let tabs = parse_session(&data).unwrap();
        assert_eq!(tabs.iter().map(|t| t.url.as_str()).collect::<Vec<_>>(), vec!["https://b.example/"]);

        let front = session_file(vec![
            command(CMD_SET_TAB_WINDOW, &ints(&[1, 1])),
            navigation(1, 0, "https://a.example/", "A"),
            navigation(1, 1, "https://b.example/", "B"),
            command(CMD_SET_SELECTED_NAVIGATION_INDEX, &ints(&[1, 1])),
            command(CMD_PRUNED_FROM_FRONT, &ints(&[1, 1])),
        ]);
        assert_eq!(parse_session(&front).unwrap()[0].url, "https://b.example/");
    }

    #[test]
    fn damaged_session_files_are_handled_without_guessing() {
        assert!(parse_session(b"").is_err());
        assert!(parse_session(b"NOPE\x03\x00\x00\x00").is_err(), "wrong signature");
        let good = sample_session();
        for cut in [9, 20, good.len() / 2, good.len() - 3] {
            assert!(parse_session(&good[..cut]).is_ok(), "a truncated file keeps the tabs that were complete (cut at {cut})");
        }
        let mut garbage = b"SNSS".to_vec();
        garbage.extend(ints(&[3]));
        garbage.extend(command(CMD_UPDATE_TAB_NAVIGATION, &[0xFF; 9]));
        garbage.extend(command(CMD_SET_TAB_WINDOW, &ints(&[1, 1])));
        assert_eq!(parse_session(&garbage).unwrap(), vec![], "a navigation that can't be read means no tab, not a made-up one");
    }

    #[test]
    fn session_tabs_are_saved_as_bookmarks_and_offered_for_opening() {
        let root = temp_root("session");
        let pdir = fake_chrome(&root);
        let sessions = pdir.join("Sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::write(
            sessions.join("Session_13436101230581611"),
            session_file(vec![command(CMD_SET_TAB_WINDOW, &ints(&[1, 1])), navigation(1, 0, "https://old.example/", "Old")]),
        )
        .unwrap();
        std::fs::write(sessions.join("Session_13436105503560033"), sample_session()).unwrap();
        std::fs::write(sessions.join("Tabs_13436105503560099"), b"recently closed tabs, not wanted").unwrap();

        assert!(detect(&root)[0].items.iter().any(|i| i.kind == "session" && i.present));
        let db = Db::open_in_memory().unwrap();
        let scratch = root.join("s");
        let req = ImportRequest { bookmarks: false, history: false, session: true, ..request(&root, &scratch, "Default") };
        let out = import(&db, req).unwrap();
        let r = &out.report;
        assert_eq!((r.session_tabs, r.session_tabs_saved), (4, 3), "chrome:// pages are not saved");
        assert_eq!(r.session_urls, vec!["https://one.example/", "https://two.example/a", "https://pinned.example/"]);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(db.is_bookmarked("https://pinned.example/").unwrap());
        assert!(!db.is_bookmarked("https://old.example/").unwrap(), "only the newest session counts");
        let folders: Vec<String> = db.bookmarks_all().unwrap().into_iter().filter(|b| b.kind == "folder").map(|b| b.title).collect();
        assert!(folders.iter().any(|t| t == "Window 1") && folders.iter().any(|t| t == "Window 2"), "{folders:?}");

        let again =
            import(&db, ImportRequest { bookmarks: false, history: false, session: true, ..request(&root, &root.join("s"), "Default") }).unwrap();
        assert_eq!(again.report.session_tabs_saved, 0, "importing again adds nothing");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn single_window_sessions_are_not_split_into_folders() {
        let tabs = vec![
            SessionTab { url: "https://a.example/".into(), title: "".into(), pinned: false, window: 7 },
            SessionTab { url: "https://b.example/".into(), title: "B".into(), pinned: false, window: 7 },
        ];
        let nodes = session_nodes(&tabs);
        assert_eq!(nodes.len(), 2);
        assert!(matches!(&nodes[0], ImportNode::Url { title, .. } if title == "https://a.example/"), "untitled tabs fall back to their address");
    }

    #[test]
    fn old_chrome_session_file_names_are_found() {
        let root = temp_root("oldsession");
        let pdir = fake_chrome(&root);
        assert!(latest_session_file(&pdir).is_none());
        std::fs::write(pdir.join("Last Session"), sample_session()).unwrap();
        assert_eq!(latest_session_file(&pdir), Some(pdir.join("Last Session")));
        assert_eq!(read_session(&pdir).unwrap().len(), 4);
        let _ = std::fs::remove_dir_all(root);
    }

    /// Chrome holds its live session file open exclusively; the wizard must say what to do, not show a raw OS error.
    #[cfg(windows)]
    #[test]
    fn a_session_file_held_open_by_the_browser_gives_an_actionable_message() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = temp_root("locked");
        let pdir = fake_chrome(&root);
        let sessions = pdir.join("Sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        let path = sessions.join("Session_13436105539410228");
        std::fs::write(&path, sample_session()).unwrap();
        let _held = std::fs::OpenOptions::new().read(true).write(true).share_mode(0).open(&path).unwrap();
        let err = read_session(&pdir).unwrap_err().to_string();
        assert!(err.contains("still running") && err.contains("Close all of its windows"), "{err}");
        drop(_held);
        let _ = std::fs::remove_dir_all(root);
    }

    // Strings below are the ones Chrome and Edge actually write (checked against local profiles).
    const GOOGLE_URL: &str = "{google:baseURL}search?q={searchTerms}&{google:RLZ}{google:originalQueryForSuggestion}{google:assistedQueryStats}{google:searchFieldtrialParameter}{google:language}{google:prefetchSource}{google:searchClient}{google:sourceId}{google:searchSource}{google:contextualSearchVersion}ie={inputEncoding}";

    #[test]
    fn maps_real_chromium_search_engine_definitions() {
        let google = map_search_engine("Google", GOOGLE_URL).unwrap();
        assert_eq!(google.engine, Some(SearchEngine::Google));
        let ddg = map_search_engine("DuckDuckGo", "https://duckduckgo.com/?q={searchTerms}").unwrap();
        assert_eq!(ddg.engine, Some(SearchEngine::DuckDuckGo));
        let brave = map_search_engine("Brave", "https://search.brave.com/search?q={searchTerms}").unwrap();
        assert_eq!(brave.engine, Some(SearchEngine::Brave));
        let bing = map_search_engine("Bing", "{bing:baseURL}search?q={searchTerms}&{bing:cvid}{bing:msb}{google:assistedQueryStats}").unwrap();
        assert_eq!(bing.engine, Some(SearchEngine::Bing));
        let yahoo =
            map_search_engine("Yahoo Search", "https://at.search.yahoo.com/search{google:pathWildcard}?ei={inputEncoding}&p={searchTerms}").unwrap();
        assert_eq!(yahoo.engine, None);
        assert_eq!(yahoo.template.as_deref(), Some("https://at.search.yahoo.com/search?ei=UTF-8&p=%s"));
    }

    #[test]
    fn rejects_unsafe_or_unknown_search_engine_definitions() {
        assert_eq!(map_search_engine("x", "http://insecure.example/?q={searchTerms}"), None, "https only");
        assert_eq!(map_search_engine("x", "javascript:alert({searchTerms})"), None);
        assert_eq!(map_search_engine("x", "https://e.example/?q={searchTerms}&z={mystery}"), None, "unknown placeholders are not guessed");
        assert_eq!(map_search_engine("x", "https://e.example/?q=fixed"), None, "needs the search terms");
        assert_eq!(map_search_engine("x", "https://e.example/?a={searchTerms}&b={searchTerms}"), None);
        assert_eq!(map_search_engine("x", "https://u:p@e.example/?q={searchTerms}"), None, "no credentials in a template");
        assert_eq!(map_search_engine("x", "https://e.example/?q={searchTerms"), None);
    }

    #[test]
    fn default_search_engine_is_read_from_secure_preferences_then_preferences() {
        let root = temp_root("search");
        let pdir = fake_chrome(&root);
        std::fs::write(
            pdir.join("Preferences"),
            r#"{"default_search_provider_data":{"mirrored_template_url_data":{"short_name":"DuckDuckGo","url":"https://duckduckgo.com/?q={searchTerms}"}}}"#,
        )
        .unwrap();
        assert_eq!(read_default_search(&pdir).unwrap().engine, Some(SearchEngine::DuckDuckGo));
        std::fs::write(
            pdir.join("Secure Preferences"),
            r#"{"default_search_provider_data":{"template_url_data":{"short_name":"Brave","url":"https://search.brave.com/search?q={searchTerms}"}}}"#,
        )
        .unwrap();
        assert_eq!(read_default_search(&pdir).unwrap().engine, Some(SearchEngine::Brave), "Secure Preferences wins");

        let db = Db::open_in_memory().unwrap();
        let out = import(&db, ImportRequest { bookmarks: false, history: false, search_engine: true, ..request(&root, &root.join("s"), "Default") })
            .unwrap();
        assert_eq!(out.report.search_engine.as_deref(), Some("Brave"));
        assert_eq!(out.search.unwrap().engine, Some(SearchEngine::Brave));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn source_files_are_never_modified_by_any_import() {
        let root = temp_root("readonly");
        let pdir = fake_chrome(&root);
        std::fs::write(pdir.join("Bookmarks"), SAMPLE).unwrap();
        write_history(&pdir);
        write_web_data(&pdir);
        write_logins(&pdir.join("Login Data"), 1, 0);
        let sessions = pdir.join("Sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::write(sessions.join("Session_1"), sample_session()).unwrap();
        let snapshot = |dir: &Path| -> Vec<(String, Vec<u8>)> {
            let mut files = Vec::new();
            let mut stack = vec![dir.to_path_buf()];
            while let Some(d) = stack.pop() {
                for e in std::fs::read_dir(&d).unwrap().flatten() {
                    if e.path().is_dir() {
                        stack.push(e.path());
                    } else {
                        files.push((e.path().display().to_string(), std::fs::read(e.path()).unwrap()));
                    }
                }
            }
            files.sort();
            files
        };
        let before = snapshot(&pdir);
        let db = Db::open_in_memory().unwrap();
        let scratch = root.join("scratch");
        let req = ImportRequest { search_engine: true, addresses: true, session: true, ..request(&root, &scratch, "Default") };
        import(&db, req).unwrap();
        detect(&root);
        assert_eq!(snapshot(&pdir), before, "the source profile is left exactly as it was");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn read_only_uris_survive_spaces_and_odd_characters() {
        let uri = readonly_uri(Path::new("C:\\Users\\Jane Doe\\App Data\\50%#x\\Login Data")).unwrap();
        assert_eq!(uri, "file:///C:/Users/Jane%20Doe/App%20Data/50%25%23x/Login%20Data?mode=ro&immutable=1");
    }

    #[test]
    fn thousands_separator() {
        assert_eq!(format_thousands(100_000), "100,000");
        assert_eq!(format_thousands(999), "999");
        assert_eq!(format_thousands(1_000), "1,000");
    }
}
