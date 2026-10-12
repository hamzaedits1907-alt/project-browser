//! Cosmetic filtering: hiding page elements (`example.com##.ad-box`), the part of the filter lists that network
//! blocking cannot do (empty ad frames, "sponsored" boxes).
//!
//! What is applied: **site-specific** element-hiding rules (`site##selector`) and their exceptions
//! (`site#@#selector`), for the site in the tab and its parent domains. They become one stylesheet per page, added
//! when the document has loaded. Selectors are wrapped in `:is(...)`, whose forgiving parsing ignores a selector the
//! browser does not understand instead of dropping its neighbours, so one bad rule cannot break a page.
//!
//! What is not applied, and counted as such: **generic** rules (`##.ad`, which would mean a stylesheet of tens of
//! thousands of selectors on every page), procedural rules (`#?#`, `:has-text()`, `:xpath()`, …), scriptlets
//! (`#$#`, `##+js(...)`) and HTML filters (`$$`). Nothing is guessed at.

use std::collections::{HashMap, HashSet};

/// A parsed element-hiding rule.
#[derive(Debug, Clone, PartialEq)]
pub enum Cosmetic {
    /// `sites##selector`: hide on these sites (and their subdomains), not on the `~` ones.
    Hide { include: Vec<String>, exclude: Vec<String>, selector: String },
    /// `sites#@#selector`: never hide this selector on these sites.
    Allow { sites: Vec<String>, selector: String },
    /// `##selector` with no site: generic (counted, not applied).
    Generic,
    /// Procedural, scriptlet or HTML rules (counted, not applied).
    Unsupported,
}

const MAX_SELECTOR: usize = 400;
const PROCEDURAL: &[&str] = &[
    ":-abp-",
    ":has-text(",
    ":xpath(",
    ":upward(",
    ":matches-css",
    ":matches-path(",
    ":matches-attr(",
    ":min-text-length(",
    ":style(",
    ":remove(",
    ":watch-attr(",
    ":others(",
    ":if(",
    ":if-not(",
    ":nth-ancestor(",
    ":contains(",
    ":matches-media(",
];

fn clean_sites(list: &str) -> Option<(Vec<String>, Vec<String>)> {
    let mut include = Vec::new();
    let mut exclude = Vec::new();
    for raw in list.split(',') {
        let d = raw.trim().to_ascii_lowercase();
        if d.is_empty() {
            continue;
        }
        let (neg, name) = match d.strip_prefix('~') {
            Some(n) => (true, n.to_string()),
            None => (false, d),
        };
        // Entity rules (`google.*`) and paths are not site names this engine understands.
        if name.contains('*') || name.contains('/') || !name.contains('.') {
            return None;
        }
        if neg {
            exclude.push(name);
        } else {
            include.push(name);
        }
    }
    Some((include, exclude))
}

fn usable_selector(sel: &str) -> bool {
    !sel.is_empty() && sel.len() <= MAX_SELECTOR && !sel.contains(['{', '}']) && !sel.contains("+js(") && !PROCEDURAL.iter().any(|p| sel.contains(p))
}

/// Parses a cosmetic line; `None` when the line is not cosmetic at all.
pub fn parse(line: &str) -> Option<Cosmetic> {
    let line = line.trim();
    for marker in ["#?#", "#$#", "#%#", "#@$#", "#@%#", "#@?#", "$$", "$@$"] {
        if line.contains(marker) {
            return Some(Cosmetic::Unsupported);
        }
    }
    let (sites, selector, allow) = if let Some(i) = line.find("#@#") {
        (&line[..i], &line[i + 3..], true)
    } else if let Some(i) = line.find("##") {
        (&line[..i], &line[i + 2..], false)
    } else {
        return None;
    };
    let selector = selector.trim();
    if !usable_selector(selector) {
        return Some(Cosmetic::Unsupported);
    }
    if sites.trim().is_empty() {
        return Some(if allow { Cosmetic::Unsupported } else { Cosmetic::Generic });
    }
    let Some((include, exclude)) = clean_sites(sites) else { return Some(Cosmetic::Unsupported) };
    if allow {
        if include.is_empty() {
            return Some(Cosmetic::Unsupported);
        }
        return Some(Cosmetic::Allow { sites: include, selector: selector.to_string() });
    }
    if include.is_empty() {
        // `~site##sel` only: a generic rule with exceptions.
        return Some(Cosmetic::Generic);
    }
    Some(Cosmetic::Hide { include, exclude, selector: selector.to_string() })
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CosmeticStats {
    /// Site-specific hiding rules in use.
    pub site_rules: usize,
    pub site_exceptions: usize,
    /// Generic rules that are not applied.
    pub generic_skipped: usize,
    /// Procedural, scriptlet and HTML rules that are not applied.
    pub unsupported_skipped: usize,
}

/// Site-specific hiding rules indexed by site.
#[derive(Default)]
pub struct CosmeticIndex {
    /// site -> (selector, sites where it must not apply)
    hide: HashMap<String, Vec<(String, Vec<String>)>>,
    allow: HashMap<String, HashSet<String>>,
    stats: CosmeticStats,
}

/// `host` and every parent domain of it (`a.b.example.com` → itself, `b.example.com`, `example.com`).
fn host_and_parents(host: &str) -> Vec<&str> {
    let mut out = vec![host];
    let mut rest = host;
    while let Some(i) = rest.find('.') {
        rest = &rest[i + 1..];
        if rest.contains('.') {
            out.push(rest);
        } else {
            break;
        }
    }
    out
}

fn is_or_under(host: &str, site: &str) -> bool {
    host == site || host.ends_with(&format!(".{site}"))
}

impl CosmeticIndex {
    /// Adds one line if it is a cosmetic rule. Returns whether it was cosmetic (applied or counted).
    pub fn add_line(&mut self, line: &str) -> bool {
        match parse(line) {
            None => false,
            Some(Cosmetic::Hide { include, exclude, selector }) => {
                for site in include {
                    self.hide.entry(site).or_default().push((selector.clone(), exclude.clone()));
                }
                self.stats.site_rules += 1;
                true
            }
            Some(Cosmetic::Allow { sites, selector }) => {
                for site in sites {
                    self.allow.entry(site).or_default().insert(selector.clone());
                }
                self.stats.site_exceptions += 1;
                true
            }
            Some(Cosmetic::Generic) => {
                self.stats.generic_skipped += 1;
                true
            }
            Some(Cosmetic::Unsupported) => {
                self.stats.unsupported_skipped += 1;
                true
            }
        }
    }

    pub fn stats(&self) -> CosmeticStats {
        self.stats
    }

    /// The selectors to hide on `host`, without duplicates, minus the site's exceptions.
    pub fn selectors_for(&self, host: &str) -> Vec<&str> {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        if host.is_empty() {
            return Vec::new();
        }
        let levels = host_and_parents(&host);
        let allowed: HashSet<&str> = levels.iter().filter_map(|l| self.allow.get(*l)).flat_map(|s| s.iter().map(String::as_str)).collect();
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for level in &levels {
            for (sel, exclude) in self.hide.get(*level).map(Vec::as_slice).unwrap_or_default() {
                if allowed.contains(sel.as_str()) || exclude.iter().any(|x| is_or_under(&host, x)) {
                    continue;
                }
                if seen.insert(sel.as_str()) {
                    out.push(sel.as_str());
                }
            }
        }
        out
    }

    /// A stylesheet hiding `selectors`, or `None` when there is nothing to hide. Selectors go into `:is()` groups so
    /// one the browser cannot parse is skipped without dropping the others.
    pub fn stylesheet(selectors: &[&str]) -> Option<String> {
        if selectors.is_empty() {
            return None;
        }
        let mut css = String::new();
        for chunk in selectors.chunks(120) {
            css.push_str(":is(");
            css.push_str(&chunk.join(","));
            css.push_str("){display:none!important}\n");
        }
        Some(css)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_site_rules_exceptions_and_counts_the_rest() {
        assert_eq!(
            parse("example.com,~shop.example.com##.ad-box"),
            Some(Cosmetic::Hide { include: vec!["example.com".into()], exclude: vec!["shop.example.com".into()], selector: ".ad-box".into() })
        );
        assert_eq!(parse("example.com#@#.ad-box"), Some(Cosmetic::Allow { sites: vec!["example.com".into()], selector: ".ad-box".into() }));
        assert_eq!(parse("##.banner"), Some(Cosmetic::Generic));
        assert_eq!(parse("~example.com##.banner"), Some(Cosmetic::Generic));
        assert_eq!(parse("example.com#?#div:-abp-has(.ad)"), Some(Cosmetic::Unsupported));
        assert_eq!(parse("example.com##+js(nobab)"), Some(Cosmetic::Unsupported));
        assert_eq!(parse("example.com##div:has-text(Sponsored)"), Some(Cosmetic::Unsupported));
        assert_eq!(parse("example.com##div { color: red }"), Some(Cosmetic::Unsupported));
        assert_eq!(parse("google.*##.ad"), Some(Cosmetic::Unsupported), "entity rules are not guessed at");
        assert_eq!(parse("example.com$$script"), Some(Cosmetic::Unsupported));
        assert_eq!(parse("||ads.example^"), None, "network rules are not cosmetic");
    }

    #[test]
    fn selectors_apply_to_the_site_and_its_subdomains_minus_exceptions() {
        let mut idx = CosmeticIndex::default();
        for line in [
            "example.com##.ad-box",
            "example.com##.sponsored",
            "news.example.com##.promo",
            "example.com,~shop.example.com##.hero-ad",
            "news.example.com#@#.sponsored",
            "##.everywhere",
            "other.org##.x",
        ] {
            assert!(idx.add_line(line));
        }
        assert!(!idx.add_line("||net.example^"));
        let mut news = idx.selectors_for("news.example.com");
        news.sort_unstable();
        assert_eq!(news, vec![".ad-box", ".hero-ad", ".promo"]);
        let mut shop = idx.selectors_for("shop.example.com");
        shop.sort_unstable();
        assert_eq!(shop, vec![".ad-box", ".sponsored"]);
        assert!(idx.selectors_for("example.org").is_empty());
        assert!(idx.selectors_for("").is_empty());
        assert_eq!(idx.stats(), CosmeticStats { site_rules: 5, site_exceptions: 1, generic_skipped: 1, unsupported_skipped: 0 });
    }

    #[test]
    fn the_stylesheet_uses_forgiving_groups() {
        assert_eq!(CosmeticIndex::stylesheet(&[]), None);
        let css = CosmeticIndex::stylesheet(&[".a", "#b"]).unwrap();
        assert_eq!(css, ":is(.a,#b){display:none!important}\n");
        let many: Vec<String> = (0..250).map(|i| format!(".c{i}")).collect();
        let refs: Vec<&str> = many.iter().map(String::as_str).collect();
        assert_eq!(CosmeticIndex::stylesheet(&refs).unwrap().matches(":is(").count(), 3);
    }
}
