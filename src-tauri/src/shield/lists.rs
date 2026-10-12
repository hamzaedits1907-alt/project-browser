//! Filter lists: which public lists Project Shield uses, how they are downloaded, checked and stored.
//!
//! * The lists are fixed here (nothing is fetched from an address a web page or a setting can choose).
//! * A download only replaces the stored list after it has been checked: it must be text, look like a filter list
//!   and contain a believable number of rules. Anything else is reported and the previous list stays in use.
//! * The stored list is replaced atomically (write a temporary file, then rename over the old one), so a crash
//!   in the middle never leaves half a list.
//! * An update asks the server whether the list changed (`ETag` / `Last-Modified`) and downloads it only if so.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::rules::EngineBuilder;

/// A public list the Shield can use.
pub struct ListSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub url: &'static str,
    pub homepage: &'static str,
    pub license: &'static str,
    pub description: &'static str,
    pub default_enabled: bool,
}

pub const LISTS: &[ListSpec] = &[
    ListSpec {
        id: "easylist",
        name: "EasyList",
        url: "https://easylist.to/easylist/easylist.txt",
        homepage: "https://easylist.to/",
        license: "Dual-licensed CC BY-SA 3.0 and GPL v3, by the EasyList authors",
        description: "Advertisements.",
        default_enabled: true,
    },
    ListSpec {
        id: "easyprivacy",
        name: "EasyPrivacy",
        url: "https://easylist.to/easylist/easyprivacy.txt",
        homepage: "https://easylist.to/",
        license: "Dual-licensed CC BY-SA 3.0 and GPL v3, by the EasyList authors",
        description: "Trackers and analytics.",
        default_enabled: true,
    },
];

/// Engine list numbers: 0 is the built-in starter set, 1 the user's own rules, then the public lists in order.
pub const LIST_STARTER: u8 = 0;
pub const LIST_USER: u8 = 1;
pub const LIST_FIRST_PUBLIC: u8 = 2;

pub fn list_number(index: usize) -> u8 {
    LIST_FIRST_PUBLIC + index as u8
}

pub fn list_name(number: u8) -> String {
    match number {
        LIST_STARTER => "Starter rules".into(),
        LIST_USER => "Your rules".into(),
        n => LISTS.get(usize::from(n - LIST_FIRST_PUBLIC)).map(|l| l.name.to_string()).unwrap_or_else(|| "A filter list".into()),
    }
}

/// A list this short is a broken download (an error page, a captive portal), not a filter list.
pub const MIN_LIST_RULES: usize = 1000;
pub const MAX_LIST_BYTES: usize = 12 * 1024 * 1024;
/// How long a stored list is trusted before the next start looks for a new version.
pub const REFRESH_AFTER_MS: i64 = 3 * 24 * 60 * 60 * 1000;

/// What is remembered about the stored copy of a list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ListMeta {
    pub fetched_at: i64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub title: Option<String>,
    pub version: Option<String>,
    pub bytes: usize,
}

pub fn list_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.txt"))
}

fn meta_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.meta.json"))
}

pub fn read_meta(dir: &Path, id: &str) -> Option<ListMeta> {
    serde_json::from_str(&std::fs::read_to_string(meta_path(dir, id)).ok()?).ok()
}

pub fn read_list(dir: &Path, id: &str) -> Option<String> {
    std::fs::read_to_string(list_path(dir, id)).ok()
}

/// Reads `! Title:` and `! Version:` from a list's header.
pub fn header_field(text: &str, field: &str) -> Option<String> {
    let prefix = format!("! {field}:");
    text.lines().take(40).find_map(|l| l.strip_prefix(prefix.as_str()).map(|v| v.trim().to_string())).filter(|v| !v.is_empty())
}

/// Decides whether downloaded bytes are a filter list worth installing. Returns the number of network rules.
pub fn validate(bytes: &[u8]) -> Result<(String, usize), String> {
    if bytes.len() > MAX_LIST_BYTES {
        return Err("The download is larger than any filter list should be, so it was ignored.".into());
    }
    let text = String::from_utf8(bytes.to_vec()).map_err(|_| "The download isn't text, so it can't be a filter list.".to_string())?;
    let head: String = text.chars().take(2048).collect();
    if !head.contains("[Adblock") && !head.contains("! Title:") {
        return Err("The download doesn't look like a filter list (no list header), so it was ignored.".into());
    }
    let mut builder = EngineBuilder::new();
    let rules = builder.add_list(&text, 0);
    if rules < MIN_LIST_RULES {
        return Err(format!("The download holds only {rules} usable rules, far fewer than a real list, so it was ignored."));
    }
    Ok((text, rules))
}

/// Writes `contents` next to `path` and renames it into place, so readers see the old file or the new one.
pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Stores a validated list and its metadata.
pub fn install(dir: &Path, id: &str, text: &str, mut meta: ListMeta) -> std::io::Result<ListMeta> {
    meta.title = header_field(text, "Title");
    meta.version = header_field(text, "Version");
    meta.bytes = text.len();
    write_atomic(&list_path(dir, id), text.as_bytes())?;
    write_atomic(&meta_path(dir, id), serde_json::to_string(&meta).unwrap_or_default().as_bytes())?;
    Ok(meta)
}

/// Remembers that the stored copy was confirmed up to date just now.
pub fn touch(dir: &Path, id: &str, mut meta: ListMeta, now: i64) -> ListMeta {
    meta.fetched_at = now;
    let _ = write_atomic(&meta_path(dir, id), serde_json::to_string(&meta).unwrap_or_default().as_bytes());
    meta
}

#[derive(Debug)]
pub enum Fetched {
    /// The server says the stored copy is still current.
    NotModified,
    Fresh {
        text: String,
        meta: ListMeta,
    },
}

/// Downloads a list. `previous` makes the request conditional. Nothing is stored here.
pub async fn fetch(url: &str, previous: Option<&ListMeta>, now: i64) -> Result<Fetched, String> {
    if !url.starts_with("https://") && !cfg!(test) {
        return Err("Filter lists are only downloaded over HTTPS.".into());
    }
    crate::ai::ensure_crypto_provider();
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(90))
        .redirect(reqwest::redirect::Policy::limited(3))
        .user_agent(concat!("ProjectBrowser/", env!("CARGO_PKG_VERSION"), " filter-list-updater"))
        .build()
        .map_err(|e| format!("Could not start the network client: {e}"))?;
    let mut request = client.get(url);
    if let Some(prev) = previous {
        if let Some(etag) = &prev.etag {
            request = request.header("If-None-Match", etag.as_str());
        }
        if let Some(modified) = &prev.last_modified {
            request = request.header("If-Modified-Since", modified.as_str());
        }
    }
    let mut resp = request.send().await.map_err(|e| {
        if e.is_timeout() {
            "The server didn't answer in time.".to_string()
        } else if e.is_connect() {
            "Couldn't reach the server. Check your internet connection.".to_string()
        } else {
            format!("The download failed: {e}")
        }
    })?;
    let status = resp.status();
    if status == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(Fetched::NotModified);
    }
    if !status.is_success() {
        return Err(format!("The server answered with HTTP {}.", status.as_u16()));
    }
    if resp.content_length().is_some_and(|n| n as usize > MAX_LIST_BYTES) {
        return Err("The list is larger than any filter list should be, so it was not downloaded.".into());
    }
    let etag = resp.headers().get("etag").and_then(|v| v.to_str().ok()).map(str::to_string);
    let last_modified = resp.headers().get("last-modified").and_then(|v| v.to_str().ok()).map(str::to_string);
    let mut bytes: Vec<u8> = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| format!("The download was interrupted: {e}"))? {
        if bytes.len() + chunk.len() > MAX_LIST_BYTES {
            return Err("The list is larger than any filter list should be, so the download was stopped.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let (text, _) = validate(&bytes)?;
    Ok(Fetched::Fresh { text, meta: ListMeta { fetched_at: now, etag, last_modified, ..ListMeta::default() } })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pb-shield-{tag}-{}-{}", std::process::id(), crate::error::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_list(rules: usize) -> String {
        let mut s = String::from("[Adblock Plus 2.0]\n! Title: Test list\n! Version: 202601010000\n");
        for i in 0..rules {
            s.push_str(&format!("||ads{i}.example^\n"));
        }
        s
    }

    #[test]
    fn a_real_looking_list_is_accepted_and_its_header_read() {
        let text = sample_list(1500);
        let (accepted, rules) = validate(text.as_bytes()).unwrap();
        assert_eq!(rules, 1500);
        assert_eq!(header_field(&accepted, "Title").as_deref(), Some("Test list"));
        assert_eq!(header_field(&accepted, "Version").as_deref(), Some("202601010000"));
    }

    #[test]
    fn broken_downloads_are_refused_with_a_reason() {
        let html = "<html><head><title>Sign in to the network</title></head><body>Please log in</body></html>";
        assert!(validate(html.as_bytes()).unwrap_err().contains("doesn't look like a filter list"));
        assert!(validate(&[0xff, 0xfe, 0x00, 0x01]).unwrap_err().contains("isn't text"));
        assert!(validate(sample_list(10).as_bytes()).unwrap_err().contains("only 10 usable rules"));
        assert!(validate(&vec![b'a'; MAX_LIST_BYTES + 1]).unwrap_err().contains("larger"));
        assert!(validate(b"").is_err());
    }

    #[test]
    fn installing_replaces_the_stored_list_whole_and_leaves_no_temporary_files() {
        let dir = temp_dir("install");
        let first = sample_list(1200);
        let meta = install(&dir, "easylist", &first, ListMeta { fetched_at: 5, etag: Some("\"a\"".into()), ..ListMeta::default() }).unwrap();
        assert_eq!(meta.title.as_deref(), Some("Test list"));
        assert_eq!(read_list(&dir, "easylist").unwrap(), first);
        assert_eq!(read_meta(&dir, "easylist").unwrap().etag.as_deref(), Some("\"a\""));

        let second = sample_list(2000);
        install(&dir, "easylist", &second, ListMeta { fetched_at: 9, ..ListMeta::default() }).unwrap();
        assert_eq!(read_list(&dir, "easylist").unwrap(), second);
        assert_eq!(read_meta(&dir, "easylist").unwrap().fetched_at, 9);
        let leftovers: Vec<_> =
            std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().ends_with(".tmp")).collect();
        assert!(leftovers.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_or_corrupt_meta_file_is_not_an_error() {
        let dir = temp_dir("meta");
        assert!(read_meta(&dir, "easylist").is_none());
        std::fs::write(meta_path(&dir, "easylist"), "{ not json").unwrap();
        assert!(read_meta(&dir, "easylist").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_numbers_and_names() {
        assert_eq!(list_name(LIST_STARTER), "Starter rules");
        assert_eq!(list_name(LIST_USER), "Your rules");
        assert_eq!(list_name(list_number(0)), "EasyList");
        assert_eq!(list_name(list_number(1)), "EasyPrivacy");
        assert_eq!(list_name(200), "A filter list");
        assert!(LISTS.iter().all(|l| l.url.starts_with("https://")));
    }

    // ---- downloads against a small local server -------------------------------------------------

    struct Canned {
        status: u16,
        body: Vec<u8>,
        etag: Option<&'static str>,
    }

    /// Serves `answers` in order (the last one repeats) and counts requests.
    fn serve(answers: Vec<Canned>) -> (String, Arc<AtomicUsize>, Arc<std::sync::Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://127.0.0.1:{}/list.txt", listener.local_addr().unwrap().port());
        let count = Arc::new(AtomicUsize::new(0));
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (c, s) = (count.clone(), seen.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                s.lock().unwrap().push(String::from_utf8_lossy(&buf[..n]).to_string());
                let i = c.fetch_add(1, Ordering::SeqCst).min(answers.len() - 1);
                let a = &answers[i];
                let etag = a.etag.map(|e| format!("ETag: {e}\r\n")).unwrap_or_default();
                let head = format!(
                    "HTTP/1.1 {} X\r\nContent-Type: text/plain\r\n{etag}Content-Length: {}\r\nConnection: close\r\n\r\n",
                    a.status,
                    a.body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&a.body);
            }
        });
        (base, count, seen)
    }

    fn run<F: std::future::Future>(f: F) -> F::Output {
        tauri::async_runtime::block_on(f)
    }

    #[test]
    fn a_good_download_comes_back_with_its_validators() {
        let (url, _, _) = serve(vec![Canned { status: 200, body: sample_list(1300).into_bytes(), etag: Some("\"v1\"") }]);
        match run(fetch(&url, None, 42)).unwrap() {
            Fetched::Fresh { text, meta } => {
                assert!(text.starts_with("[Adblock"));
                assert_eq!((meta.fetched_at, meta.etag.as_deref()), (42, Some("\"v1\"")));
            }
            other => panic!("expected a fresh list, got {other:?}"),
        }
    }

    #[test]
    fn an_update_check_is_conditional_and_not_modified_changes_nothing() {
        let (url, _, seen) = serve(vec![Canned { status: 304, body: Vec::new(), etag: None }]);
        let prev = ListMeta { etag: Some("\"v1\"".into()), last_modified: Some("Mon, 01 Jan 2026 00:00:00 GMT".into()), ..ListMeta::default() };
        assert!(matches!(run(fetch(&url, Some(&prev), 1)).unwrap(), Fetched::NotModified));
        let request = seen.lock().unwrap()[0].to_ascii_lowercase();
        assert!(request.contains("if-none-match: \"v1\""), "{request}");
        assert!(request.contains("if-modified-since: mon, 01 jan 2026"), "{request}");
    }

    #[test]
    fn server_errors_html_pages_and_tiny_lists_are_all_reported_and_install_nothing() {
        let cases: Vec<(Canned, &str)> = vec![
            (Canned { status: 500, body: b"oops".to_vec(), etag: None }, "HTTP 500"),
            (Canned { status: 404, body: b"nope".to_vec(), etag: None }, "HTTP 404"),
            (Canned { status: 200, body: b"<html>captive portal</html>".to_vec(), etag: None }, "doesn't look like a filter list"),
            (Canned { status: 200, body: sample_list(5).into_bytes(), etag: None }, "usable rules"),
        ];
        for (canned, expected) in cases {
            let (url, _, _) = serve(vec![canned]);
            let err = run(fetch(&url, None, 1)).unwrap_err();
            assert!(err.contains(expected), "{err}");
        }
    }

    #[test]
    fn an_unreachable_server_is_reported_as_such() {
        let dead = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}/x", dead.local_addr().unwrap().port());
        drop(dead);
        let err = run(fetch(&url, None, 1)).unwrap_err();
        assert!(err.contains("reach") || err.contains("failed"), "{err}");
    }
}
