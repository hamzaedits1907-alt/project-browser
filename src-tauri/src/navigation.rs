//! Omnibox input resolution and URL classification.
//!
//! Everything here is pure so it can be unit-tested without a webview.

use serde::{Deserialize, Serialize};
use url::Url;

/// Scheme used for browser-internal pages rendered by the shell (never by a tab webview).
pub const INTERNAL_SCHEME: &str = "browser";

/// Internal pages the shell knows how to render. Anything else under `browser://` is rejected.
pub const INTERNAL_PAGES: &[&str] = &["newtab", "settings", "history", "bookmarks", "downloads", "performance", "gaming", "memory", "research"];

pub const NEW_TAB_URL: &str = "browser://newtab";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SearchEngine {
    #[default]
    Google,
    Bing,
    DuckDuckGo,
    Brave,
    Startpage,
    Ecosia,
}

impl SearchEngine {
    pub fn template(self) -> &'static str {
        match self {
            SearchEngine::Google => "https://www.google.com/search?q=%s",
            SearchEngine::Bing => "https://www.bing.com/search?q=%s",
            SearchEngine::DuckDuckGo => "https://duckduckgo.com/?q=%s",
            SearchEngine::Brave => "https://search.brave.com/search?q=%s",
            SearchEngine::Startpage => "https://www.startpage.com/do/search?query=%s",
            SearchEngine::Ecosia => "https://www.ecosia.org/search?q=%s",
        }
    }
}

/// Builds a search URL from a template containing `%s`.
pub fn search_url(template: &str, query: &str) -> String {
    let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
    template.replace("%s", &encoded)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// A page loaded by a tab webview.
    Web(String),
    /// A shell-rendered internal page, e.g. `browser://settings`.
    Internal(String),
}

impl Resolved {
    pub fn into_url(self) -> String {
        match self {
            Resolved::Web(u) | Resolved::Internal(u) => u,
        }
    }
}

/// Returns the internal page name if `url` is a known `browser://` page.
pub fn internal_page(url: &str) -> Option<&'static str> {
    let rest = url.strip_prefix("browser://")?;
    let name = rest.split(['/', '?', '#']).next().unwrap_or("");
    INTERNAL_PAGES.iter().copied().find(|p| p.eq_ignore_ascii_case(name))
}

pub fn is_internal(url: &str) -> bool {
    url.starts_with("browser://")
}

/// Resolves raw omnibox text to something navigable. Returns `None` for empty input.
pub fn resolve_input(input: &str, search_template: &str) -> Option<Resolved> {
    let text = input.trim();
    if text.is_empty() {
        return None;
    }

    // A leading '?' forces a search, like Chrome.
    if let Some(q) = text.strip_prefix('?') {
        let q = q.trim();
        if q.is_empty() {
            return None;
        }
        return Some(Resolved::Web(search_url(search_template, q)));
    }

    if text.get(..10).is_some_and(|p| p.eq_ignore_ascii_case("browser://")) {
        return Some(match internal_page(&text.to_ascii_lowercase()) {
            Some(page) => Resolved::Internal(format!("browser://{page}")),
            None => Resolved::Web(search_url(search_template, text)),
        });
    }

    let search = || Some(Resolved::Web(search_url(search_template, text)));

    if text.chars().any(char::is_whitespace) {
        return search();
    }

    if let Some((scheme, _)) = text.split_once(':') {
        let scheme = scheme.to_ascii_lowercase();
        match scheme.as_str() {
            "http" | "https" | "file" | "ftp" => return Url::parse(text).ok().map(|u| Resolved::Web(u.to_string())).or_else(search),
            "about" => return if text.eq_ignore_ascii_case("about:blank") { Some(Resolved::Web("about:blank".into())) } else { search() },
            // Script/data URLs typed into the omnibox are a classic self-XSS vector.
            "javascript" | "data" | "vbscript" => return search(),
            _ => {}
        }
    }

    if looks_like_host(text) {
        let scheme = if is_local_host(text) { "http" } else { "https" };
        if let Ok(u) = Url::parse(&format!("{scheme}://{text}")) {
            return Some(Resolved::Web(u.to_string()));
        }
    }

    search()
}

fn host_part(text: &str) -> &str {
    let end = text.find(['/', '?', '#']).unwrap_or(text.len());
    let authority = &text[..end];
    // Strip userinfo and port.
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    match authority.rsplit_once(':') {
        Some((h, port)) if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => h,
        _ => authority,
    }
}

fn is_local_host(text: &str) -> bool {
    let host = host_part(text).to_ascii_lowercase();
    host == "localhost" || host.ends_with(".localhost") || is_ipv4(&host)
}

fn is_ipv4(host: &str) -> bool {
    host.parse::<std::net::Ipv4Addr>().is_ok()
}

fn looks_like_host(text: &str) -> bool {
    let host = host_part(text);
    if host.is_empty() {
        return false;
    }
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" || lower.ends_with(".localhost") || is_ipv4(&lower) {
        return true;
    }
    if !host.contains('.') || host.starts_with('.') || host.ends_with('.') {
        return false;
    }
    let valid_chars = host.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '.');
    let tld = host.rsplit('.').next().unwrap_or("");
    valid_chars && tld.len() >= 2 && tld.chars().all(|c| c.is_alphabetic()) && !is_file_extension(tld)
}

/// Common file extensions that are not top-level domains: `notes.txt` is a search, not a website.
/// Extensions that are real TLDs (`.rs`, `.md`, `.sh`, `.py`...) are deliberately not listed.
fn is_file_extension(tld: &str) -> bool {
    const EXTENSIONS: &[&str] = &[
        "txt", "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "rtf", "csv", "json", "xml", "yaml", "yml", "toml", "ini", "cfg", "log", "png",
        "jpg", "jpeg", "gif", "bmp", "svg", "webp", "ico", "zip", "rar", "tar", "gz", "exe", "msi", "dll", "bat", "cmd", "iso", "mp3", "mp4", "mkv",
        "avi", "wav", "html", "htm", "css", "js", "mjs", "jsx", "tsx", "cpp", "java",
    ];
    EXTENSIONS.iter().any(|e| e.eq_ignore_ascii_case(tld))
}

/// Lowercase host of a web URL (used for per-site exceptions and permissions).
pub fn host_of(url: &str) -> Option<String> {
    Url::parse(url).ok()?.host_str().map(|h| h.to_ascii_lowercase())
}

/// Origin string (`scheme://host[:port]`) for permission bookkeeping.
pub fn origin_of(url: &str) -> Option<String> {
    let u = Url::parse(url).ok()?;
    let origin = u.origin();
    origin.is_tuple().then(|| origin.ascii_serialization())
}

/// Navigations a tab webview must never perform: internal pages and the shell's own origin.
pub fn is_blocked_tab_navigation(url: &Url) -> bool {
    match url.scheme() {
        "browser" | "tauri" | "ipc" | "asset" => true,
        "http" | "https" => matches!(url.host_str(), Some("tauri.localhost") | Some("ipc.localhost") | Some("asset.localhost")),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: &str = "https://www.google.com/search?q=%s";

    fn web(input: &str) -> String {
        match resolve_input(input, G) {
            Some(Resolved::Web(u)) => u,
            other => panic!("expected web url for {input:?}, got {other:?}"),
        }
    }

    #[test]
    fn non_ascii_text_near_the_scheme_length_does_not_panic() {
        // Regression: slicing the first 10 bytes split a multi-byte character and crashed the app.
        for input in ["schönheit und mehr", "çalışma saati", "ärger ärger ärger", "日本語のテキスト入力"] {
            assert!(matches!(resolve_input(input, G), Some(Resolved::Web(_))), "{input}");
        }
    }

    #[test]
    fn empty_input_is_none() {
        assert_eq!(resolve_input("   ", G), None);
        assert_eq!(resolve_input("?", G), None);
    }

    #[test]
    fn full_urls_pass_through() {
        assert_eq!(web("https://example.com/a?b=c"), "https://example.com/a?b=c");
        assert_eq!(web("HTTP://Example.com"), "http://example.com/");
    }

    #[test]
    fn bare_domains_get_https() {
        assert_eq!(web("example.com"), "https://example.com/");
        assert_eq!(web("news.ycombinator.com/item?id=1"), "https://news.ycombinator.com/item?id=1");
        assert_eq!(web("sub.domain.co.uk:8443/x"), "https://sub.domain.co.uk:8443/x");
    }

    #[test]
    fn local_hosts_get_http() {
        assert_eq!(web("localhost:3000"), "http://localhost:3000/");
        assert_eq!(web("192.168.1.1"), "http://192.168.1.1/");
        assert_eq!(web("127.0.0.1:8080/api"), "http://127.0.0.1:8080/api");
    }

    #[test]
    fn words_become_searches() {
        assert_eq!(web("rust lang"), "https://www.google.com/search?q=rust+lang");
        assert_eq!(web("hello"), "https://www.google.com/search?q=hello");
        assert_eq!(web("file.txt"), "https://www.google.com/search?q=file.txt");
        assert_eq!(web("report.PDF"), "https://www.google.com/search?q=report.PDF");
        assert_eq!(web("docs.rs"), "https://docs.rs/", "real TLDs that look like extensions stay websites");
        assert_eq!(web("?example.com"), "https://www.google.com/search?q=example.com");
        assert_eq!(web("c++ & rust"), "https://www.google.com/search?q=c%2B%2B+%26+rust");
    }

    #[test]
    fn dangerous_schemes_are_searched_not_executed() {
        assert!(web("javascript:alert(1)").starts_with("https://www.google.com/search?q="));
        assert!(web("data:text/html,hi").starts_with("https://www.google.com/search?q="));
    }

    #[test]
    fn internal_pages() {
        assert_eq!(resolve_input("browser://settings", G), Some(Resolved::Internal("browser://settings".into())));
        assert_eq!(resolve_input("BROWSER://History", G), Some(Resolved::Internal("browser://history".into())));
        assert!(matches!(resolve_input("browser://nope", G), Some(Resolved::Web(_))));
        assert_eq!(internal_page("browser://newtab"), Some("newtab"));
        assert_eq!(internal_page("https://newtab"), None);
    }

    #[test]
    fn about_blank_allowed() {
        assert_eq!(web("about:blank"), "about:blank");
    }

    #[test]
    fn origin_and_host() {
        assert_eq!(origin_of("https://a.example.com:8443/x"), Some("https://a.example.com:8443".into()));
        assert_eq!(host_of("https://WWW.Example.com/x"), Some("www.example.com".into()));
        assert_eq!(origin_of("about:blank"), None);
    }

    #[test]
    fn blocks_shell_origins_in_tabs() {
        let blocked = ["browser://settings", "tauri://localhost", "http://tauri.localhost/index.html", "ipc://localhost/x"];
        for u in blocked {
            assert!(is_blocked_tab_navigation(&Url::parse(u).unwrap()), "{u}");
        }
        assert!(!is_blocked_tab_navigation(&Url::parse("https://example.com").unwrap()));
        assert!(!is_blocked_tab_navigation(&Url::parse("mailto:a@b.c").unwrap()));
    }
}
