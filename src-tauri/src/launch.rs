//! Pages the command line asks to open: from Windows when Project Browser is the handler for a link or an HTML file
//! (`"…exe" --single-argument <target>`, registered by the installer, see windows/hooks.nsh), from a second launch that
//! the single-instance plugin forwards, and from scripts (`project-browser.exe https://a https://b`).
//!
//! Only web addresses, local files and plain address-bar text are accepted. Other schemes (`javascript:`, `data:`,
//! `vbscript:`, …) given on the command line are dropped: a link that another program hands over must never run script.

use url::Url;

/// The longest target accepted (Windows limits a command line to 32767 characters).
const MAX_TARGET: usize = 32_767;

/// Every page the arguments ask for, in order. `args[0]` is the executable.
pub fn urls_from_args(args: &[String]) -> Vec<String> {
    let rest = args.iter().skip(1);
    if let Some(pos) = args.iter().skip(1).position(|a| a == "--single-argument") {
        // Everything after the switch is one target, even if it was split at spaces.
        let target = args[pos + 2..].join(" ");
        return normalize(&target).into_iter().collect();
    }
    rest.filter(|a| !a.starts_with('-')).filter_map(|a| normalize(a)).collect()
}

/// One command-line target as something the address bar can open, or `None` if it must not be opened.
pub fn normalize(raw: &str) -> Option<String> {
    let t = raw.trim().trim_matches('"').trim();
    if t.is_empty() || t.len() > MAX_TARGET || t.chars().any(|c| c.is_control()) {
        return None;
    }
    if is_windows_path(t) {
        return file_url(t);
    }
    let lower = t.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("file:") {
        return Url::parse(t).ok().map(|u| u.to_string());
    }
    if let Some((scheme, _)) = lower.split_once(':') {
        let looks_like_scheme = !scheme.is_empty() && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c));
        // "example.com:8080/x" and "localhost:3000" are addresses with a port, not a scheme.
        let has_port = t[scheme.len() + 1..].chars().next().is_some_and(|c| c.is_ascii_digit());
        if looks_like_scheme && !has_port {
            return None;
        }
    }
    // Plain text ("example.com", "news"): the address bar decides between an address and a search.
    Some(t.to_string())
}

fn is_windows_path(t: &str) -> bool {
    let b = t.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')) || t.starts_with(r"\\")
}

#[cfg(windows)]
fn file_url(path: &str) -> Option<String> {
    Url::from_file_path(path).ok().map(|u| u.to_string())
}

#[cfg(not(windows))]
fn file_url(path: &str) -> Option<String> {
    Url::parse(&format!("file:///{}", path.replace('\\', "/"))).ok().map(|u| u.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        std::iter::once("project-browser.exe").chain(list.iter().copied()).map(String::from).collect()
    }

    #[test]
    fn a_link_from_windows_is_one_target_with_its_query_kept() {
        assert_eq!(urls_from_args(&args(&["--single-argument", "https://example.com/a?b=1&c=2#d"])), vec!["https://example.com/a?b=1&c=2#d"]);
    }

    #[test]
    fn a_target_split_at_spaces_is_joined_again() {
        assert_eq!(urls_from_args(&args(&["--single-argument", "https://example.com/a", "b"])), vec!["https://example.com/a%20b"]);
    }

    #[test]
    fn quotes_and_unicode_are_handled() {
        assert_eq!(normalize("\"https://bücher.example/straße\""), Some("https://xn--bcher-kva.example/stra%C3%9Fe".into()));
    }

    #[test]
    fn script_and_data_schemes_are_refused() {
        assert_eq!(normalize("javascript:alert(1)"), None);
        assert_eq!(normalize("JavaScript:alert(1)"), None);
        assert_eq!(normalize("data:text/html,<script>x</script>"), None);
        assert_eq!(normalize("vbscript:msgbox"), None);
        assert_eq!(urls_from_args(&args(&["--single-argument", "javascript:alert(1)"])), Vec::<String>::new());
    }

    #[test]
    fn malformed_and_empty_input_is_refused() {
        assert_eq!(normalize(""), None);
        assert_eq!(normalize("\"\""), None);
        assert_eq!(normalize("https://"), None);
        assert_eq!(normalize("https://exa\u{0}mple.com"), None);
        assert_eq!(urls_from_args(&args(&["--single-argument"])), Vec::<String>::new());
    }

    #[test]
    fn plain_text_and_host_with_port_go_to_the_address_bar() {
        assert_eq!(normalize("example.com"), Some("example.com".into()));
        assert_eq!(normalize("localhost:3000"), Some("localhost:3000".into()));
    }

    #[test]
    fn local_html_files_become_file_urls() {
        let u = normalize(r"C:\Users\me\My Page.html").unwrap();
        assert!(u.starts_with("file:///C:/Users/me/My%20Page.html"), "{u}");
    }

    #[test]
    fn several_plain_arguments_and_switches() {
        assert_eq!(
            urls_from_args(&args(&["--gaming", "https://a.example/", "https://b.example/"])),
            vec!["https://a.example/", "https://b.example/"]
        );
    }
}
