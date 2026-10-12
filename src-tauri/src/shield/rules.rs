//! The network-filter engine behind Project Shield.
//!
//! It reads the network subset of the Adblock Plus / uBlock Origin filter syntax (the format of EasyList and
//! EasyPrivacy) and answers one question quickly: should this request be blocked?
//!
//! * Rules are parsed once into a compact form (the pattern is a single byte string; `*` and `^` are stored as
//!   two control bytes that can never appear in a URL).
//! * Every rule is filed under its rarest whole word (for example `doubleclick`), so a request only looks at the
//!   few rules that share a word with its URL instead of the whole list.
//! * Rules this engine cannot honour faithfully (regular expressions, `redirect=`, `csp=`, `removeparam=`, ...)
//!   and cosmetic rules are counted and skipped, never guessed at.
//!
//! The module is pure: no I/O, no WebView2, so all of it is unit-tested.

use std::collections::HashMap;

// ---- request types ------------------------------------------------------------------------------

pub const T_SCRIPT: u16 = 1;
pub const T_IMAGE: u16 = 1 << 1;
pub const T_STYLESHEET: u16 = 1 << 2;
pub const T_XHR: u16 = 1 << 3;
pub const T_SUBDOCUMENT: u16 = 1 << 4;
pub const T_MEDIA: u16 = 1 << 5;
pub const T_FONT: u16 = 1 << 6;
pub const T_PING: u16 = 1 << 7;
pub const T_WEBSOCKET: u16 = 1 << 8;
pub const T_OTHER: u16 = 1 << 9;
/// A top-level page load. The engine never blocks these; the bit exists so `$document` rules parse.
pub const T_DOCUMENT: u16 = 1 << 10;
/// Every kind of sub-resource: the default for a rule without type options.
pub const T_SUBRESOURCES: u16 = (1 << 10) - 1;

const STAR: u8 = 1;
const SEP: u8 = 2;
const MAX_URL: usize = 2048;
const MIN_PATTERN: usize = 3;

/// What a request is, from the engine's point of view.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub url: &'a str,
    /// Host of the page that made the request, lowercase. Empty when unknown.
    pub top_host: &'a str,
    /// Exactly one of the `T_*` bits.
    pub kind: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    /// An exception rule (`@@...`) matched, so the request is let through even though a block rule may match.
    Exception,
    /// Blocked by a rule of list number `list`.
    Block {
        list: u8,
    },
}

// ---- parsing ------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// Element-hiding, scriptlet and HTML filters (`##`, `#@#`, ...).
    Cosmetic,
    /// A valid rule this engine does not implement (regular expressions, `redirect=`, `csp=`, ...).
    Unsupported,
    /// Too short, too long, empty or otherwise unusable.
    Malformed,
}

#[derive(Debug)]
pub enum Parsed {
    /// A comment, header or blank line.
    Nothing,
    Skipped(Skip),
    Rule(Rule),
}

#[derive(Debug, Clone)]
struct DomainOption {
    include: Vec<String>,
    exclude: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pattern: Box<[u8]>,
    host_anchor: bool,
    start_anchor: bool,
    end_anchor: bool,
    pub exception: bool,
    important: bool,
    types: u16,
    /// `Some(true)`: only third-party requests; `Some(false)`: only first-party ones.
    third_party: Option<bool>,
    domains: Option<Box<DomainOption>>,
    list: u8,
}

fn known_type(name: &str) -> Option<u16> {
    Some(match name {
        "script" => T_SCRIPT,
        "image" => T_IMAGE,
        "stylesheet" | "css" => T_STYLESHEET,
        "xmlhttprequest" | "xhr" => T_XHR,
        "subdocument" | "frame" => T_SUBDOCUMENT,
        "media" => T_MEDIA,
        "font" => T_FONT,
        "ping" => T_PING,
        "websocket" => T_WEBSOCKET,
        "other" | "object" | "object-subrequest" => T_OTHER,
        "document" | "doc" => T_DOCUMENT,
        _ => return None,
    })
}

/// True when `s` looks like the option list after a final `$`: names made of letters, digits, `-` and `_`.
fn looks_like_options(s: &str) -> bool {
    !s.is_empty()
        && s.split(',').all(|opt| {
            let name = opt.split('=').next().unwrap_or("");
            let name = name.strip_prefix('~').unwrap_or(name);
            !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
}

fn parse_domains(value: &str) -> Option<DomainOption> {
    let mut out = DomainOption { include: Vec::new(), exclude: Vec::new() };
    for d in value.split('|') {
        let d = d.trim().to_ascii_lowercase();
        if d.is_empty() {
            continue;
        }
        let (negated, name) = match d.strip_prefix('~') {
            Some(n) => (true, n.to_string()),
            None => (false, d),
        };
        // Entity (`google.*`) and regular-expression domains are not supported.
        if name.is_empty() || name.contains('*') || name.contains('/') {
            return None;
        }
        if negated {
            out.exclude.push(name);
        } else {
            out.include.push(name);
        }
    }
    (!out.include.is_empty() || !out.exclude.is_empty()).then_some(out)
}

/// Parses one line of a filter list. `list` is the number recorded in the rule (which list it came from).
pub fn parse_line(line: &str, list: u8) -> Parsed {
    let line = line.trim();
    if line.is_empty() || line.starts_with('!') || (line.starts_with('[') && line.ends_with(']')) {
        return Parsed::Nothing;
    }
    if line.len() > MAX_URL {
        return Parsed::Skipped(Skip::Malformed);
    }
    for marker in ["##", "#@#", "#?#", "#$#", "#%#", "#@$#", "#@%#", "$$"] {
        if line.contains(marker) {
            return Parsed::Skipped(Skip::Cosmetic);
        }
    }
    let (exception, body) = match line.strip_prefix("@@") {
        Some(rest) => (true, rest),
        None => (false, line),
    };

    // Options come after the last `$`, but only if what follows really looks like options.
    let (pattern_text, options) = match body.rfind('$') {
        Some(i) if looks_like_options(&body[i + 1..]) => (&body[..i], Some(&body[i + 1..])),
        _ => (body, None),
    };

    // `/regex/` rules are not supported.
    if pattern_text.len() > 2 && pattern_text.starts_with('/') && pattern_text.ends_with('/') {
        return Parsed::Skipped(Skip::Unsupported);
    }

    let mut rule = Rule {
        pattern: Box::default(),
        host_anchor: false,
        start_anchor: false,
        end_anchor: false,
        exception,
        important: false,
        types: T_SUBRESOURCES,
        third_party: None,
        domains: None,
        list,
    };

    if let Some(options) = options {
        let (mut yes, mut no) = (0u16, 0u16);
        for opt in options.split(',') {
            let (name, value) = match opt.split_once('=') {
                Some((n, v)) => (n, Some(v)),
                None => (opt, None),
            };
            let (negated, name) = match name.strip_prefix('~') {
                Some(n) => (true, n),
                None => (false, name),
            };
            if let Some(bit) = known_type(name) {
                if value.is_some() {
                    return Parsed::Skipped(Skip::Unsupported);
                }
                if negated {
                    no |= bit;
                } else {
                    yes |= bit;
                }
                continue;
            }
            match (name, value) {
                ("third-party" | "3p", None) => rule.third_party = Some(!negated),
                ("first-party" | "1p", None) => rule.third_party = Some(negated),
                ("important", None) if !negated => rule.important = true,
                ("match-case", None) if !negated => {}
                ("all", None) if !negated => yes |= T_SUBRESOURCES | T_DOCUMENT,
                ("domain" | "from", Some(v)) if !negated => match parse_domains(v) {
                    Some(d) => rule.domains = Some(Box::new(d)),
                    None => return Parsed::Skipped(Skip::Unsupported),
                },
                _ => return Parsed::Skipped(Skip::Unsupported),
            }
        }
        let base = if yes == 0 { T_SUBRESOURCES } else { yes };
        rule.types = base & !no;
        if rule.types == 0 {
            return Parsed::Skipped(Skip::Malformed);
        }
    }

    // The pattern itself.
    let mut p = pattern_text;
    if let Some(rest) = p.strip_prefix("||") {
        rule.host_anchor = true;
        p = rest;
    } else if let Some(rest) = p.strip_prefix('|') {
        rule.start_anchor = true;
        p = rest;
    }
    if let Some(rest) = p.strip_suffix('|') {
        rule.end_anchor = true;
        p = rest;
    }
    if !rule.host_anchor && !rule.start_anchor {
        p = p.trim_start_matches('*');
    }
    if !rule.end_anchor {
        p = p.trim_end_matches('*');
    }
    let mut encoded: Vec<u8> = Vec::with_capacity(p.len());
    for b in p.to_ascii_lowercase().bytes() {
        match b {
            b'*' => {
                if encoded.last() != Some(&STAR) {
                    encoded.push(STAR);
                }
            }
            b'^' => encoded.push(SEP),
            // Control bytes would collide with the markers; no real URL contains them.
            0..=2 => return Parsed::Skipped(Skip::Malformed),
            other => encoded.push(other),
        }
    }
    let meaningful = encoded.iter().filter(|b| **b != STAR && **b != SEP).count();
    if meaningful < MIN_PATTERN {
        return Parsed::Skipped(Skip::Malformed);
    }
    rule.pattern = encoded.into_boxed_slice();
    Parsed::Rule(rule)
}

// ---- matching -----------------------------------------------------------------------------------

fn is_separator(b: u8) -> bool {
    !(b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'%') || b >= 0x80)
}

fn find_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Does `pat` match `url` starting exactly at `pos`?
fn match_at(pat: &[u8], url: &[u8], pos: usize, end_anchor: bool) -> bool {
    let (mut p, mut u) = (0usize, pos);
    loop {
        if p == pat.len() {
            return !end_anchor || u == url.len();
        }
        match pat[p] {
            STAR => {
                let rest = &pat[p + 1..];
                if rest.is_empty() {
                    return true;
                }
                return (u..=url.len()).any(|s| match_at(rest, url, s, end_anchor));
            }
            SEP => {
                if u == url.len() {
                    // `^` also matches the end of the address.
                    p += 1;
                } else if is_separator(url[u]) {
                    u += 1;
                    p += 1;
                } else {
                    return false;
                }
            }
            b => {
                if u < url.len() && url[u] == b {
                    u += 1;
                    p += 1;
                } else {
                    return false;
                }
            }
        }
    }
}

/// Byte range of the host inside `url` (`scheme://user@host:port/...`), or `None` when it has no authority.
pub fn host_range(url: &[u8]) -> Option<(usize, usize)> {
    let scheme_end = find_bytes(url, b"://")?;
    let start = scheme_end + 3;
    let rest = &url[start..];
    let auth_len = rest.iter().position(|b| matches!(b, b'/' | b'?' | b'#')).unwrap_or(rest.len());
    let authority = &rest[..auth_len];
    let host_start = authority.iter().rposition(|b| *b == b'@').map(|i| i + 1).unwrap_or(0);
    let host_part = &authority[host_start..];
    let host_len = if host_part.first() == Some(&b'[') {
        host_part.iter().position(|b| *b == b']').map(|i| i + 1).unwrap_or(host_part.len())
    } else {
        host_part.iter().position(|b| *b == b':').unwrap_or(host_part.len())
    };
    if host_len == 0 {
        return None;
    }
    Some((start + host_start, start + host_start + host_len))
}

const TWO_PART_SUFFIXES: &[&str] = &["co", "com", "org", "net", "gov", "edu", "ac", "or", "ne", "go", "ltd", "plc", "nom", "gob", "mil"];

/// The part of a host that identifies who owns it (`a.b.example.co.uk` -> `example.co.uk`). A small built-in
/// rule, not the full public-suffix list; it only decides first-party versus third-party.
pub fn registrable_domain(host: &str) -> &str {
    if host.is_empty() || host.starts_with('[') || host.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return host;
    }
    let labels: Vec<&str> = host.split('.').collect();
    let n = labels.len();
    if n <= 2 {
        return host;
    }
    let keep = if labels[n - 1].len() == 2 && TWO_PART_SUFFIXES.contains(&labels[n - 2]) { 3 } else { 2 };
    let skip: usize = labels[..n - keep].iter().map(|l| l.len() + 1).sum();
    &host[skip..]
}

pub fn host_is_or_under(host: &str, domain: &str) -> bool {
    host == domain || (host.len() > domain.len() && host.ends_with(domain) && host.as_bytes()[host.len() - domain.len() - 1] == b'.')
}

impl Rule {
    fn options_allow(&self, req: &Request<'_>, request_host: &str) -> bool {
        if self.types & req.kind == 0 {
            return false;
        }
        if let Some(want_third) = self.third_party {
            let is_third = !req.top_host.is_empty() && registrable_domain(request_host) != registrable_domain(req.top_host);
            if is_third != want_third {
                return false;
            }
        }
        if let Some(d) = &self.domains {
            if !d.include.is_empty() && !d.include.iter().any(|x| host_is_or_under(req.top_host, x)) {
                return false;
            }
            if d.exclude.iter().any(|x| host_is_or_under(req.top_host, x)) {
                return false;
            }
        }
        true
    }

    fn pattern_matches(&self, url: &[u8], host: Option<(usize, usize)>) -> bool {
        let pat = &self.pattern[..];
        if self.host_anchor {
            let Some((hs, he)) = host else { return false };
            let mut pos = hs;
            loop {
                if match_at(pat, url, pos, self.end_anchor) {
                    return true;
                }
                match url[pos..he].iter().position(|b| *b == b'.') {
                    Some(i) => pos += i + 1,
                    None => return false,
                }
            }
        }
        if self.start_anchor {
            return match_at(pat, url, 0, self.end_anchor);
        }
        // Unanchored: try every place the first literal run occurs.
        let first_len = pat.iter().position(|b| *b == STAR || *b == SEP).unwrap_or(pat.len());
        if first_len == 0 {
            return (0..=url.len()).any(|p| match_at(pat, url, p, self.end_anchor));
        }
        let first = &pat[..first_len];
        let mut from = 0usize;
        while from <= url.len() {
            match find_bytes(&url[from..], first) {
                Some(i) => {
                    if match_at(pat, url, from + i, self.end_anchor) {
                        return true;
                    }
                    from += i + 1;
                }
                None => return false,
            }
        }
        false
    }

    /// Whole words of the pattern that a matching URL must also contain as whole words.
    fn tokens(&self) -> Vec<&[u8]> {
        let pat = &self.pattern[..];
        let mut out = Vec::new();
        let mut i = 0;
        while i < pat.len() {
            if !pat[i].is_ascii_alphanumeric() {
                i += 1;
                continue;
            }
            let start = i;
            while i < pat.len() && pat[i].is_ascii_alphanumeric() {
                i += 1;
            }
            let end = i;
            if end - start < 3 {
                continue;
            }
            let left_ok = if start == 0 { self.host_anchor || self.start_anchor } else { pat[start - 1] != STAR };
            let right_ok = if end == pat.len() { self.end_anchor } else { pat[end] != STAR };
            if left_ok && right_ok {
                out.push(&pat[start..end]);
            }
        }
        out
    }
}

// ---- the engine ---------------------------------------------------------------------------------

/// What was read from the lists, for display.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStats {
    pub block_rules: usize,
    pub exception_rules: usize,
    pub cosmetic_skipped: usize,
    pub unsupported_skipped: usize,
    pub malformed_skipped: usize,
}

#[derive(Default)]
pub struct EngineBuilder {
    rules: Vec<Rule>,
    stats: EngineStats,
    cosmetic: super::cosmetic::CosmeticIndex,
}

impl EngineBuilder {
    pub fn new() -> EngineBuilder {
        EngineBuilder::default()
    }

    /// Adds every rule of a list. Returns how many network rules it contributed.
    pub fn add_list(&mut self, text: &str, list: u8) -> usize {
        let before = self.rules.len();
        for line in text.lines() {
            match parse_line(line, list) {
                Parsed::Nothing => {}
                Parsed::Skipped(Skip::Cosmetic) => {
                    self.stats.cosmetic_skipped += 1;
                    self.cosmetic.add_line(line);
                }
                Parsed::Skipped(Skip::Unsupported) => self.stats.unsupported_skipped += 1,
                Parsed::Skipped(Skip::Malformed) => self.stats.malformed_skipped += 1,
                Parsed::Rule(r) => {
                    if r.exception {
                        self.stats.exception_rules += 1;
                    } else {
                        self.stats.block_rules += 1;
                    }
                    self.rules.push(r);
                }
            }
        }
        self.rules.len() - before
    }

    pub fn build(self) -> Engine {
        let EngineBuilder { rules, stats, cosmetic } = self;
        // How many rules could be filed under each word: the rarest word wins, so buckets stay small. Words are
        // kept as 64-bit hashes; a collision only means one extra rule is checked, never a wrong answer.
        let mut freq: HashMap<u64, u32> = HashMap::new();
        for r in &rules {
            for t in r.tokens() {
                *freq.entry(word_hash(t)).or_insert(0) += 1;
            }
        }
        let mut index: HashMap<u64, Bucket> = HashMap::new();
        let mut fallback = Vec::new();
        for (i, r) in rules.iter().enumerate() {
            let best = r
                .tokens()
                .into_iter()
                .map(|t| (word_hash(t), t.len()))
                .min_by_key(|(h, len)| (freq.get(h).copied().unwrap_or(u32::MAX), std::cmp::Reverse(*len)));
            match best {
                Some((h, _)) => match index.get_mut(&h) {
                    Some(bucket) => bucket.push(i as u32),
                    None => {
                        index.insert(h, Bucket::One(i as u32));
                    }
                },
                None => fallback.push(i as u32),
            }
        }
        index.shrink_to_fit();
        Engine { rules, index, fallback, stats, cosmetic }
    }
}

fn word_hash(word: &[u8]) -> u64 {
    // FNV-1a
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in word {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// The rules filed under one word. Most words have exactly one rule, which then needs no allocation.
enum Bucket {
    One(u32),
    Many(Vec<u32>),
}

impl Bucket {
    fn push(&mut self, i: u32) {
        match self {
            Bucket::One(first) => *self = Bucket::Many(vec![*first, i]),
            Bucket::Many(v) => v.push(i),
        }
    }

    fn as_slice(&self) -> &[u32] {
        match self {
            Bucket::One(x) => std::slice::from_ref(x),
            Bucket::Many(v) => v,
        }
    }
}

pub struct Engine {
    rules: Vec<Rule>,
    index: HashMap<u64, Bucket>,
    /// Rules with no usable word: tried for every request, so there must be few.
    fallback: Vec<u32>,
    stats: EngineStats,
    cosmetic: super::cosmetic::CosmeticIndex,
}

impl Engine {
    pub fn empty() -> Engine {
        EngineBuilder::new().build()
    }

    pub fn stats(&self) -> EngineStats {
        self.stats
    }

    /// The element-hiding stylesheet for a site (site-specific rules only; see `cosmetic.rs`).
    pub fn cosmetic_css(&self, host: &str) -> Option<String> {
        super::cosmetic::CosmeticIndex::stylesheet(&self.cosmetic.selectors_for(host))
    }

    pub fn cosmetic_stats(&self) -> super::cosmetic::CosmeticStats {
        self.cosmetic.stats()
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    pub fn fallback_count(&self) -> usize {
        self.fallback.len()
    }

    pub fn check(&self, req: &Request<'_>) -> Verdict {
        if self.rules.is_empty() || req.kind == T_DOCUMENT {
            return Verdict::Allow;
        }
        let lower = req.url.to_ascii_lowercase();
        let bytes = &lower.as_bytes()[..lower.len().min(MAX_URL)];
        let scheme_ok =
            bytes.starts_with(b"http://") || bytes.starts_with(b"https://") || bytes.starts_with(b"ws://") || bytes.starts_with(b"wss://");
        if !scheme_ok {
            return Verdict::Allow;
        }
        let host = host_range(bytes);
        let request_host = host.and_then(|(s, e)| std::str::from_utf8(&bytes[s..e]).ok()).unwrap_or("");

        let mut blocked: Option<u8> = None;
        let mut important = false;
        let mut excepted = false;
        let mut consider = |idx: u32| {
            let r = &self.rules[idx as usize];
            if !r.options_allow(req, request_host) || !r.pattern_matches(bytes, host) {
                return;
            }
            if r.exception {
                excepted = true;
            } else {
                blocked = Some(r.list);
                important |= r.important;
            }
        };

        let mut seen: Vec<u64> = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if !bytes[i].is_ascii_alphanumeric() {
                i += 1;
                continue;
            }
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
                i += 1;
            }
            let word = &bytes[start..i];
            if word.len() < 3 {
                continue;
            }
            let h = word_hash(word);
            if seen.contains(&h) {
                continue;
            }
            seen.push(h);
            if let Some(bucket) = self.index.get(&h) {
                for idx in bucket.as_slice() {
                    consider(*idx);
                }
            }
        }
        for idx in &self.fallback {
            consider(*idx);
        }

        match blocked {
            Some(list) if important || !excepted => Verdict::Block { list },
            Some(_) => Verdict::Exception,
            None if excepted => Verdict::Exception,
            None => Verdict::Allow,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(lines: &str) -> Engine {
        let mut b = EngineBuilder::new();
        b.add_list(lines, 0);
        b.build()
    }

    fn check(e: &Engine, url: &str, top: &str, kind: u16) -> Verdict {
        e.check(&Request { url, top_host: top, kind })
    }

    fn blocked(e: &Engine, url: &str, top: &str, kind: u16) -> bool {
        matches!(check(e, url, top, kind), Verdict::Block { .. })
    }

    #[test]
    fn domain_anchor_matches_the_host_and_its_subdomains_only() {
        let e = engine("||ads.example.com^");
        assert!(blocked(&e, "https://ads.example.com/banner.js", "site.org", T_SCRIPT));
        assert!(blocked(&e, "https://cdn.ads.example.com/x", "site.org", T_IMAGE));
        assert!(blocked(&e, "https://ads.example.com", "site.org", T_IMAGE), "^ matches the end of the address");
        assert!(!blocked(&e, "https://ads.example.com.evil.org/x", "site.org", T_IMAGE), "a longer host is another host");
        assert!(!blocked(&e, "https://notads.example.com/x", "site.org", T_IMAGE));
        assert!(!blocked(&e, "https://example.com/ads.example.com/x", "site.org", T_IMAGE), "the anchor is on the host, not the path");
    }

    #[test]
    fn plain_patterns_wildcards_and_anchors() {
        let e = engine("/banner/ad_*.gif\n|https://track.example/pixel|\n.swf|\n");
        assert!(blocked(&e, "https://x.org/banner/ad_top.gif", "x.org", T_IMAGE));
        assert!(!blocked(&e, "https://x.org/banner/top.gif", "x.org", T_IMAGE));
        assert!(blocked(&e, "https://track.example/pixel", "x.org", T_IMAGE));
        assert!(!blocked(&e, "https://track.example/pixel?x=1", "x.org", T_IMAGE), "end anchor");
        assert!(!blocked(&e, "http://track.example/pixel", "x.org", T_IMAGE), "start anchor includes the scheme");
        assert!(blocked(&e, "https://x.org/movie.swf", "x.org", T_OTHER));
        assert!(!blocked(&e, "https://x.org/movie.swf?v=2", "x.org", T_OTHER));
    }

    #[test]
    fn separator_matches_punctuation_but_not_word_characters() {
        let e = engine("/adserver^");
        assert!(blocked(&e, "https://x.org/adserver/foo", "x.org", T_SCRIPT));
        assert!(blocked(&e, "https://x.org/adserver?x=1", "x.org", T_SCRIPT));
        assert!(blocked(&e, "https://x.org/adserver", "x.org", T_SCRIPT));
        assert!(!blocked(&e, "https://x.org/adservers/foo", "x.org", T_SCRIPT));
        assert!(!blocked(&e, "https://x.org/adserver.js", "x.org", T_SCRIPT), "a dot is part of a name, not a separator");
    }

    #[test]
    fn exception_rules_win_over_block_rules_unless_important() {
        let e = engine("||tracker.example^\n@@||tracker.example/allowed.js\n||strict.example^$important\n@@||strict.example^\n");
        assert!(blocked(&e, "https://tracker.example/x.js", "x.org", T_SCRIPT));
        assert_eq!(check(&e, "https://tracker.example/allowed.js", "x.org", T_SCRIPT), Verdict::Exception);
        assert!(blocked(&e, "https://strict.example/x.js", "x.org", T_SCRIPT), "$important beats an exception");
    }

    #[test]
    fn type_options_restrict_and_negate() {
        let e = engine("||a.example^$script\n||b.example^$~image\n||c.example^$image,font\n");
        assert!(blocked(&e, "https://a.example/x", "s.org", T_SCRIPT));
        assert!(!blocked(&e, "https://a.example/x", "s.org", T_IMAGE));
        assert!(!blocked(&e, "https://b.example/x", "s.org", T_IMAGE));
        assert!(blocked(&e, "https://b.example/x", "s.org", T_SCRIPT));
        assert!(blocked(&e, "https://c.example/x", "s.org", T_FONT));
        assert!(!blocked(&e, "https://c.example/x", "s.org", T_SCRIPT));
    }

    #[test]
    fn third_party_and_first_party_options() {
        let e = engine("||cdn.example^$third-party\n||own.example^$~third-party\n");
        assert!(blocked(&e, "https://cdn.example/x.js", "other.org", T_SCRIPT));
        assert!(!blocked(&e, "https://cdn.example/x.js", "cdn.example", T_SCRIPT));
        assert!(!blocked(&e, "https://cdn.example/x.js", "www.cdn.example", T_SCRIPT), "same registrable domain");
        assert!(blocked(&e, "https://own.example/x.js", "own.example", T_SCRIPT));
        assert!(!blocked(&e, "https://own.example/x.js", "other.org", T_SCRIPT));
        assert!(!blocked(&e, "https://cdn.example/x.js", "", T_SCRIPT), "an unknown page is never treated as third-party");
    }

    #[test]
    fn domain_option_includes_and_excludes_pages() {
        let e = engine("/promo.js$domain=news.example|~free.news.example\n");
        assert!(blocked(&e, "https://cdn.org/promo.js", "news.example", T_SCRIPT));
        assert!(blocked(&e, "https://cdn.org/promo.js", "m.news.example", T_SCRIPT));
        assert!(!blocked(&e, "https://cdn.org/promo.js", "free.news.example", T_SCRIPT));
        assert!(!blocked(&e, "https://cdn.org/promo.js", "other.org", T_SCRIPT));
        assert!(!blocked(&e, "https://cdn.org/promo.js", "notnews.example", T_SCRIPT), "a suffix of the text is not a subdomain");
    }

    #[test]
    fn main_frame_documents_are_never_blocked() {
        let e = engine("||example.com^\n");
        assert_eq!(check(&e, "https://example.com/", "example.com", T_DOCUMENT), Verdict::Allow);
        assert!(blocked(&e, "https://example.com/frame.html", "other.org", T_SUBDOCUMENT));
    }

    #[test]
    fn non_web_addresses_are_never_blocked() {
        let e = engine("||example.com^\n/ads/\n");
        for url in [
            "data:text/html,/ads/",
            "blob:https://example.com/abc",
            "file:///c:/ads/x.png",
            "about:blank",
            "tauri://localhost/ads/x",
            "browser://newtab",
        ] {
            assert_eq!(check(&e, url, "x.org", T_IMAGE), Verdict::Allow, "{url}");
        }
    }

    #[test]
    fn unsupported_cosmetic_and_malformed_rules_are_counted_not_applied() {
        let text = "! comment\n[Adblock Plus 2.0]\n##.banner\nexample.com#@#.ad\n/regex[0-9]+/\n||x.example^$redirect=noopjs\n||y.example^$csp=script-src\n||z.example^$removeparam=utm\n*\nab\n||ok.example^\n";
        let mut b = EngineBuilder::new();
        let added = b.add_list(text, 0);
        let e = b.build();
        assert_eq!(added, 1);
        assert_eq!(e.stats().cosmetic_skipped, 2);
        assert_eq!(e.stats().unsupported_skipped, 4);
        assert_eq!(e.stats().malformed_skipped, 2);
        assert!(blocked(&e, "https://ok.example/x", "s.org", T_IMAGE));
        assert!(!blocked(&e, "https://x.example/x", "s.org", T_IMAGE), "a rule with redirect= is skipped, not turned into a plain block");
        assert!(!blocked(&e, "https://z.example/x?utm=1", "s.org", T_IMAGE));
    }

    #[test]
    fn rules_that_would_match_everything_are_refused() {
        for line in ["*", "||", "^", "|", "$third-party", "$script", "**", "a", "ab", "||a^"] {
            assert!(matches!(parse_line(line, 0), Parsed::Skipped(_) | Parsed::Nothing), "{line}");
        }
    }

    #[test]
    fn garbage_lines_never_panic() {
        let garbage = [
            "@@",
            "@@||",
            "||$",
            "$",
            ",,,$,,",
            "||x$domain=",
            "||x$domain=|",
            "|||||",
            "\u{0}\u{1}\u{2}abc",
            "||é.example^",
            "/a/b/c$$",
            "||example.com^$unknownoption",
            "||example.com^$",
        ];
        for g in garbage {
            let _ = parse_line(g, 0);
        }
        let e = engine(&garbage.join("\n"));
        let _ = check(&e, "https://example.com/é?x=%00", "é.org", T_IMAGE);
    }

    #[test]
    fn host_and_registrable_domain_helpers() {
        let url = b"https://user:pw@www.example.co.uk:8443/a/b?c=d";
        let (s, e) = host_range(url).unwrap();
        assert_eq!(&url[s..e], b"www.example.co.uk");
        assert_eq!(host_range(b"https://example.com"), Some((8, 19)));
        assert_eq!(host_range(b"data:text/plain,hi"), None);
        assert_eq!(registrable_domain("a.b.example.com"), "example.com");
        assert_eq!(registrable_domain("a.example.co.uk"), "example.co.uk");
        assert_eq!(registrable_domain("example.com"), "example.com");
        assert_eq!(registrable_domain("localhost"), "localhost");
        assert_eq!(registrable_domain("192.168.0.1"), "192.168.0.1");
        assert!(host_is_or_under("a.example.com", "example.com"));
        assert!(!host_is_or_under("badexample.com", "example.com"));
    }

    #[test]
    fn only_a_few_rules_are_tried_for_a_request() {
        // 20,000 rules about other sites must not slow down or change the answer for an unrelated request.
        let mut text = String::new();
        for i in 0..20_000 {
            text.push_str(&format!("||tracker{i}.example^\n/path{i}/banner_{i}.gif\n"));
        }
        text.push_str("||needle.example^\n");
        let e = engine(&text);
        assert_eq!(e.rule_count(), 40_001);
        assert!(e.fallback_count() < 10);
        assert!(blocked(&e, "https://needle.example/x", "s.org", T_IMAGE));
        assert!(blocked(&e, "https://tracker1234.example/x", "s.org", T_IMAGE));
        assert!(blocked(&e, "https://s.org/path77/banner_77.gif", "s.org", T_IMAGE));
        assert!(!blocked(&e, "https://s.org/path77/banner_78.gif", "s.org", T_IMAGE));
        assert!(!blocked(&e, "https://harmless.example/index.html", "s.org", T_IMAGE));
    }

    #[test]
    fn matching_is_case_insensitive() {
        let e = engine("||Ads.Example.COM^\n/BANNER/ad.gif\n");
        assert!(blocked(&e, "HTTPS://ADS.EXAMPLE.COM/X", "s.org", T_IMAGE));
        assert!(blocked(&e, "https://s.org/Banner/AD.gif", "s.org", T_IMAGE));
    }

    #[test]
    fn a_rule_enclosed_in_slashes_is_a_regular_expression_and_is_skipped() {
        // That is what the format defines: a path rule is written /banner/*, never /banner/.
        assert!(matches!(parse_line("/banner/", 0), Parsed::Skipped(Skip::Unsupported)));
        assert!(matches!(parse_line("/ad[0-9]+/", 0), Parsed::Skipped(Skip::Unsupported)));
        assert!(matches!(parse_line("/banner/*", 0), Parsed::Rule(_)));
    }
}
