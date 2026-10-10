//! Migration from Chromium-based browsers (Chrome, Edge, Brave).
//!
//! Only runs on explicit user action from the migration wizard. Reads the
//! source browser's files without modifying them: SQLite files are copied
//! (together with their journal) into a private scratch folder and read there.
//! Sensitive data is never decrypted: saved passwords and the encrypted copy of
//! account bookmarks use app-bound encryption that only the owning browser can
//! unlock, and this importer does not try to get around that.
//!
//! The work is split in two so the slow part never blocks the browser:
//! [`collect`] only reads files (run it on a blocking thread), [`apply`] only
//! writes to the profile database (run it on the controller).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::db::bookmarks::{ImportNode, OTHER_ID};
use crate::db::history::UrlRow;
use crate::db::Db;
use crate::error::{now_ms, Error, Result};
use crate::navigation::SearchEngine;

/// Newest history entries read from a source profile.
const HISTORY_LIMIT: u32 = 100_000;

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
    /// Name of the default search engine that was applied, if one was imported.
    pub search_engine: Option<String>,
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
                        format!("{bookmark_count} bookmarks found")
                    },
                },
                ImportItem {
                    kind: "history".into(),
                    label: "Browsing history".into(),
                    supported: true,
                    present: history,
                    detail: if history { format!("Up to {} most recent pages", format_thousands(HISTORY_LIMIT)) } else { "No history file".into() },
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
                    supported: false,
                    present: pdir.join("Login Data").is_file() || pdir.join("Login Data For Account").is_file(),
                    detail: format!(
                        "Not imported. {} protects saved passwords with encryption that only it can unlock. \
                         We don't bypass that protection. Export them from {} (Settings → Passwords) and keep \
                         using a password manager, or sign in again here.",
                        browser.label(),
                        browser.label()
                    ),
                },
                ImportItem {
                    kind: "autofill".into(),
                    label: "Autofill (addresses, cards)".into(),
                    supported: false,
                    present: pdir.join("Web Data").is_file(),
                    detail: "Not imported. Addresses and payment cards are protected by the source browser and are not read.".into(),
                },
                ImportItem {
                    kind: "cookies".into(),
                    label: "Cookies and site sign-ins".into(),
                    supported: false,
                    present: pdir.join("Network").join("Cookies").is_file() || pdir.join("Cookies").is_file(),
                    detail: "Not imported. Cookies are encrypted per browser; you will sign in to sites again.".into(),
                },
            ];
            found.push(DetectedProfile { browser, browser_label: browser.label().into(), profile_dir: dir, name, items });
        }
    }
    found
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

fn read_history(profile_dir: &Path, scratch: &Path) -> Result<Vec<UrlRow>> {
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
    let rows = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
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
}

/// Everything read from the source profile, ready to be written. Holds no secrets.
pub struct Collected {
    browser_label: &'static str,
    profile_name: String,
    bookmarks: Option<Vec<ImportNode>>,
    history: Option<Vec<UrlRow>>,
    search: Option<ImportedSearch>,
    errors: Vec<String>,
}

/// Reads the requested data from the source profile. Does not touch the browser's own database.
pub fn collect(req: &ImportRequest<'_>) -> Result<Collected> {
    // Only accept profile folder names that were actually detected, never arbitrary paths.
    let detected = detect(req.local_app_data);
    let profile = detected
        .iter()
        .find(|p| p.browser == req.browser && p.profile_dir == req.profile_dir)
        .ok_or_else(|| Error::msg("source profile not found"))?;
    let pdir = req.browser.user_data_dir(req.local_app_data).join(&profile.profile_dir);
    let mut out = Collected {
        browser_label: req.browser.label(),
        profile_name: profile.name.clone(),
        bookmarks: None,
        history: None,
        search: None,
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
            Ok(rows) => out.history = Some(rows),
            Err(e) => out.errors.push(format!("History: {e}")),
        }
    }
    if req.search_engine {
        match read_default_search(&pdir) {
            Some(s) => out.search = Some(s),
            None => out.errors.push("Search engine: this profile does not use a search engine that can be imported".into()),
        }
    }
    Ok(out)
}

/// Result of [`apply`]: what to tell the user, plus a search engine for the caller to adopt.
pub struct ImportOutcome {
    pub report: ImportReport,
    pub search: Option<ImportedSearch>,
}

/// Writes collected data to the profile database. Each data type is written in its own
/// transaction and is safe to run again: nothing already imported is added twice.
pub fn apply(db: &Db, collected: Collected) -> ImportOutcome {
    let mut report = ImportReport { errors: collected.errors, ..Default::default() };

    if let Some(nodes) = collected.bookmarks.filter(|n| !n.is_empty()) {
        let folder = format!("Imported from {} ({})", collected.browser_label, collected.profile_name);
        match db.bookmarks_import(OTHER_ID, &folder, &nodes, now_ms()) {
            Ok(c) => {
                report.bookmarks_imported = c.added;
                report.bookmarks_skipped = c.skipped;
            }
            Err(e) => report.errors.push(format!("Bookmarks: {e}")),
        }
    }
    if let Some(rows) = collected.history {
        match db.import_urls(&rows) {
            Ok(c) => {
                report.history_imported = c.added;
                report.history_skipped = c.skipped;
            }
            Err(e) => report.errors.push(format!("History: {e}")),
        }
    }
    if let Some(s) = &collected.search {
        report.search_engine = Some(s.name.clone());
    }
    ImportOutcome { report, search: collected.search }
}

/// Collect and apply in one step (used by tests; the app runs the two halves on different threads).
pub fn run_import(db: &Db, req: ImportRequest<'_>) -> Result<ImportOutcome> {
    Ok(apply(db, collect(&req)?))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn write_history(pdir: &Path) {
        let conn = rusqlite::Connection::open(pdir.join("History")).unwrap();
        conn.execute_batch(
            "CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT, title TEXT, visit_count INTEGER, typed_count INTEGER, last_visit_time INTEGER, hidden INTEGER);
             INSERT INTO urls (url, title, visit_count, typed_count, last_visit_time, hidden) VALUES
               ('https://a.example/', 'A', 3, 0, 13253932800000000, 0),
               ('https://hidden.example/', 'H', 1, 0, 13253932800000000, 1);",
        )
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
        }
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
        assert!(pw.present && !pw.supported, "passwords are reported, never imported");

        let db = Db::open_in_memory().unwrap();
        let scratch = root.join("scratch");
        let out = run_import(&db, request(&root, &scratch, "Default")).unwrap();
        assert_eq!(out.report.bookmarks_imported, 3);
        assert_eq!(out.report.history_imported, 1);
        assert!(out.report.errors.is_empty(), "{:?}", out.report.errors);
        assert!(db.is_bookmarked("https://store.steampowered.com/").unwrap());
        assert!(std::fs::read_dir(&scratch).unwrap().next().is_none(), "scratch copies are removed");

        let bad = run_import(&db, ImportRequest { profile_dir: "..\\..\\Windows", history: false, ..request(&root, &scratch, "Default") });
        assert!(bad.is_err(), "only detected profile folders are accepted");
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

        let first = run_import(&db, request(&root, &scratch, "Default")).unwrap().report;
        assert_eq!((first.bookmarks_imported, first.history_imported), (3, 1));
        let second = run_import(&db, request(&root, &scratch, "Default")).unwrap().report;
        assert_eq!((second.bookmarks_imported, second.history_imported), (0, 0));
        assert_eq!((second.bookmarks_skipped, second.history_skipped), (3, 1));
        let bookmark_rows = db.bookmarks_all().unwrap().iter().filter(|b| b.kind == "url").count();
        assert_eq!(bookmark_rows, 3, "no duplicate bookmarks after a retry");
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
        let out = run_import(&db, ImportRequest { history: false, ..request(&root, &root.join("s"), "Default") }).unwrap();
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
        let out = run_import(&db, ImportRequest { history: false, ..request(&root, &root.join("s"), "Default") }).unwrap();
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
        let out = run_import(&db, ImportRequest { history: false, ..request(&root, &root.join("s"), "Default") }).unwrap();
        assert_eq!(out.report.bookmarks_imported, 2, "the readable file still imports");
        assert!(out.report.errors.iter().any(|e| e.contains("Bookmarks could not be read")), "{:?}", out.report.errors);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_data_is_an_error_not_a_success() {
        let root = temp_root("empty");
        fake_chrome(&root);
        let db = Db::open_in_memory().unwrap();
        let out = run_import(&db, request(&root, &root.join("s"), "Default")).unwrap();
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
        let rows = read_history(&pdir, &root.join("s")).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(std::fs::read(pdir.join("History")).unwrap(), before, "source history is not modified");
        assert!(pdir.join("History-journal").is_file());
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
        let out =
            run_import(&db, ImportRequest { bookmarks: false, history: false, search_engine: true, ..request(&root, &root.join("s"), "Default") })
                .unwrap();
        assert_eq!(out.report.search_engine.as_deref(), Some("Brave"));
        assert_eq!(out.search.unwrap().engine, Some(SearchEngine::Brave));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn thousands_separator() {
        assert_eq!(format_thousands(100_000), "100,000");
        assert_eq!(format_thousands(999), "999");
        assert_eq!(format_thousands(1_000), "1,000");
    }
}
