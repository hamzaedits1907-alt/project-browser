//! Project Shield: request-level ad and tracker blocking.
//!
//! How it works, honestly:
//!
//! * Every web page's sub-resource request (scripts, images, XHR, fonts, ...) is offered to WebView2's
//!   `WebResourceRequested` event, and every sub-frame navigation to `FrameNavigationStarting` (see
//!   `native::hook_shield`). The request is checked against an indexed rule set ([`rules`]) and, if a block rule
//!   matches, answered with an empty `403` before it leaves the machine.
//! * Top-level page loads are never blocked. WebView2 cannot tell a main-frame document request from a
//!   sub-frame one in `WebResourceRequested`, so sub-frames are handled through `FrameNavigationStarting`.
//! * Rules come from public lists ([`lists`]), a small built-in starter set and the user's own rules. There is
//!   no cosmetic (element-hiding) filtering in this version; those rules are counted and skipped.
//! * The user can switch the Shield off everywhere or for one site. Nothing is recorded per URL: only counts.

pub mod lists;
pub mod rules;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use serde::{Deserialize, Serialize};

use crate::error::now_ms;
use crate::tabs::TabId;
use lists::{ListMeta, LISTS, LIST_STARTER, LIST_USER};
use rules::{host_is_or_under, host_range, Engine, EngineBuilder, EngineStats, Request, Verdict};

pub const KV_KEY: &str = "shield";
const STARTER: &str = include_str!("starter.txt");
const MAX_USER_RULES_BYTES: usize = 64 * 1024;
const MAX_SITES: usize = 500;
const MAX_TRACKED_HOSTS: usize = 200;
const MAX_TRACKED_SITES: usize = 500;

// ---- configuration ------------------------------------------------------------------------------

/// What the user chose. Stored per profile; the lists themselves are shared by all profiles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ShieldConfig {
    pub enabled: bool,
    /// Sites (and their subdomains) where blocking is off.
    pub disabled_sites: Vec<String>,
    /// Public lists the user switched off (all others in [`LISTS`] are on).
    pub disabled_lists: Vec<String>,
    /// The user's own rules, one per line, in the same syntax as the public lists (`@@` makes an exception).
    pub user_rules: String,
    /// Requests blocked on non-private tabs since the Shield was first used.
    pub lifetime_blocked: u64,
}

impl Default for ShieldConfig {
    fn default() -> Self {
        ShieldConfig { enabled: true, disabled_sites: Vec::new(), disabled_lists: Vec::new(), user_rules: String::new(), lifetime_blocked: 0 }
    }
}

/// A site name as typed by the user or taken from an address: lowercase host, no scheme, port or path.
pub fn normalize_site(input: &str) -> Option<String> {
    let t = input.trim().to_ascii_lowercase();
    let t = t.strip_prefix("https://").or_else(|| t.strip_prefix("http://")).unwrap_or(&t);
    let host = t.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or("");
    let host = host.split(':').next().unwrap_or("").trim_start_matches("www.").trim_matches('.');
    let valid = !host.is_empty()
        && host.len() <= 253
        && host.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        && host.split('.').all(|l| !l.is_empty() && l.len() <= 63);
    valid.then(|| host.to_string())
}

impl ShieldConfig {
    /// Clamps and cleans a configuration that came from the UI or from an older file.
    pub fn sanitized(mut self) -> ShieldConfig {
        let mut sites: Vec<String> = self.disabled_sites.iter().filter_map(|s| normalize_site(s)).collect();
        sites.sort();
        sites.dedup();
        sites.truncate(MAX_SITES);
        self.disabled_sites = sites;
        self.disabled_lists.retain(|id| LISTS.iter().any(|l| l.id == id));
        self.disabled_lists.sort();
        self.disabled_lists.dedup();
        if self.user_rules.len() > MAX_USER_RULES_BYTES {
            let mut cut = MAX_USER_RULES_BYTES;
            while !self.user_rules.is_char_boundary(cut) {
                cut -= 1;
            }
            self.user_rules.truncate(cut);
        }
        self
    }

    pub fn site_disabled(&self, host: &str) -> bool {
        !host.is_empty() && self.disabled_sites.iter().any(|s| host_is_or_under(host, s))
    }

    pub fn list_enabled(&self, id: &str) -> bool {
        !self.disabled_lists.iter().any(|d| d == id)
    }
}

// ---- state --------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ListState {
    /// No copy has been downloaded yet.
    Missing,
    Ready,
    Updating,
    /// The last update failed; the earlier copy (if any) is still in use.
    Failed,
}

#[derive(Debug, Clone)]
struct ListStatus {
    meta: Option<ListMeta>,
    rules: usize,
    state: ListState,
    error: Option<String>,
}

#[derive(Default)]
struct TabStats {
    blocked: u32,
    hosts: HashMap<String, u32>,
}

#[derive(Default)]
struct Stats {
    tabs: HashMap<TabId, TabStats>,
    sites: HashMap<String, u32>,
    session_total: u64,
}

pub struct Shield {
    dir: PathBuf,
    active: AtomicBool,
    engine: RwLock<Arc<Engine>>,
    cfg: RwLock<ShieldConfig>,
    lists: Mutex<HashMap<String, ListStatus>>,
    stats: Mutex<Stats>,
    /// A notification for changed counts has been requested and not yet delivered.
    pending: AtomicBool,
    lifetime_unsaved: AtomicBool,
    updating: AtomicBool,
    /// Counts of the last engine build, for display.
    built: Mutex<(EngineStats, usize, usize, usize)>,
    generation: AtomicU64,
}

static GLOBAL: OnceLock<Arc<Shield>> = OnceLock::new();

/// The running Shield, once [`init`] has been called.
pub fn global() -> Option<&'static Arc<Shield>> {
    GLOBAL.get()
}

/// Creates the Shield (starter rules only, instantly) and loads stored lists in the background.
pub fn init(dir: PathBuf, cfg: ShieldConfig) -> Arc<Shield> {
    let cfg = cfg.sanitized();
    let shield = Arc::new(Shield::new(dir, cfg));
    shield.rebuild_now_starter_only();
    let _ = GLOBAL.set(shield.clone());
    shield
}

/// Lowercase host of an address, or an empty string.
pub fn host_of_url(url: &str) -> String {
    let lower = url.to_ascii_lowercase();
    host_range(lower.as_bytes()).map(|(s, e)| lower[s..e].to_string()).unwrap_or_default()
}

/// Hosts that are the browser's own or local development: never filtered.
fn protected_host(host: &str) -> bool {
    host == "localhost" || host.ends_with(".localhost") || host == "127.0.0.1" || host == "[::1]"
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub verdict: Verdict,
    /// Why the request was not even checked, for the "test a request" tool.
    pub note: Option<&'static str>,
}

impl Shield {
    fn new(dir: PathBuf, cfg: ShieldConfig) -> Shield {
        Shield {
            dir,
            active: AtomicBool::new(cfg.enabled),
            engine: RwLock::new(Arc::new(Engine::empty())),
            cfg: RwLock::new(cfg),
            lists: Mutex::new(HashMap::new()),
            stats: Mutex::new(Stats::default()),
            pending: AtomicBool::new(false),
            lifetime_unsaved: AtomicBool::new(false),
            updating: AtomicBool::new(false),
            built: Mutex::new((EngineStats::default(), 0, 0, 0)),
            generation: AtomicU64::new(0),
        }
    }

    /// Fast check for the request hot path.
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    pub fn config(&self) -> ShieldConfig {
        self.cfg.read().map(|c| c.clone()).unwrap_or_default()
    }

    /// Replaces the configuration, returning it as stored (cleaned). Lists are rebuilt if rules changed.
    pub fn set_config(self: &Arc<Self>, next: ShieldConfig) -> ShieldConfig {
        let (next, rules_changed) = self.apply_config(next);
        if rules_changed {
            self.rebuild_in_background();
        }
        next
    }

    /// Stores a configuration. The second value says whether the rules need to be compiled again.
    fn apply_config(&self, next: ShieldConfig) -> (ShieldConfig, bool) {
        let next = next.sanitized();
        let rules_changed = {
            let Ok(mut cfg) = self.cfg.write() else { return (next, false) };
            let changed = cfg.disabled_lists != next.disabled_lists || cfg.user_rules != next.user_rules;
            // The lifetime counter belongs to the Shield, not to whoever edits the settings.
            let lifetime = cfg.lifetime_blocked;
            *cfg = ShieldConfig { lifetime_blocked: lifetime.max(next.lifetime_blocked), ..next.clone() };
            changed
        };
        self.active.store(next.enabled, Ordering::Relaxed);
        (next, rules_changed)
    }

    pub fn site_disabled(&self, host: &str) -> bool {
        self.cfg.read().map(|c| c.site_disabled(host)).unwrap_or(false)
    }

    /// Is blocking switched on for a page on `host`?
    pub fn on_for(&self, host: &str) -> bool {
        self.is_active() && !protected_host(host) && !self.site_disabled(host)
    }

    /// Decides one request. Pure with respect to counters: call [`Shield::record_block`] when acting on a block.
    pub fn check(&self, url: &str, top_host: &str, kind: u16) -> Decision {
        if !self.is_active() {
            return Decision { verdict: Verdict::Allow, note: Some("The Shield is switched off.") };
        }
        if protected_host(top_host) {
            return Decision { verdict: Verdict::Allow, note: Some("Pages on this computer are never filtered.") };
        }
        if self.site_disabled(top_host) {
            return Decision { verdict: Verdict::Allow, note: Some("The Shield is off for this site.") };
        }
        if protected_host(&host_of_url(url)) {
            return Decision { verdict: Verdict::Allow, note: Some("Requests to this computer are never filtered.") };
        }
        let engine = match self.engine.read() {
            Ok(e) => e.clone(),
            Err(_) => return Decision { verdict: Verdict::Allow, note: None },
        };
        Decision { verdict: engine.check(&Request { url, top_host, kind }), note: None }
    }

    /// Counts one blocked request. Returns true when nobody has asked for a notification yet, so the caller
    /// schedules exactly one (coalesced) notification for a burst of blocks.
    pub fn record_block(&self, tab: TabId, private: bool, top_host: &str, url: &str) -> bool {
        let blocked_host = host_of_url(url);
        if let Ok(mut guard) = self.stats.lock() {
            let stats = &mut *guard;
            stats.session_total += 1;
            let tab_stats = stats.tabs.entry(tab).or_default();
            tab_stats.blocked = tab_stats.blocked.saturating_add(1);
            if tab_stats.hosts.len() < MAX_TRACKED_HOSTS || tab_stats.hosts.contains_key(&blocked_host) {
                *tab_stats.hosts.entry(blocked_host).or_insert(0) += 1;
            }
            if !private && !top_host.is_empty() && (stats.sites.len() < MAX_TRACKED_SITES || stats.sites.contains_key(top_host)) {
                *stats.sites.entry(top_host.to_string()).or_insert(0) += 1;
            }
        }
        if !private {
            if let Ok(mut c) = self.cfg.write() {
                c.lifetime_blocked = c.lifetime_blocked.saturating_add(1);
            }
            self.lifetime_unsaved.store(true, Ordering::Relaxed);
        }
        !self.pending.swap(true, Ordering::Relaxed)
    }

    /// A new page is loading in the tab: its count starts again. Returns true when a notification must be scheduled.
    pub fn reset_tab(&self, tab: TabId) -> bool {
        let had = self.stats.lock().map(|mut s| s.tabs.remove(&tab).is_some()).unwrap_or(false);
        had && !self.pending.swap(true, Ordering::Relaxed)
    }

    pub fn forget_tab(&self, tab: TabId) {
        if let Ok(mut s) = self.stats.lock() {
            s.tabs.remove(&tab);
        }
    }

    /// The requested notification is being delivered now: the next change asks for a new one.
    pub fn clear_pending(&self) {
        self.pending.store(false, Ordering::Relaxed);
    }

    /// Blocked-request counts for tabs that have any.
    pub fn tab_counts(&self) -> HashMap<TabId, u32> {
        self.stats.lock().map(|s| s.tabs.iter().filter(|(_, t)| t.blocked > 0).map(|(id, t)| (*id, t.blocked)).collect()).unwrap_or_default()
    }

    /// The configuration to save if the lifetime counter moved since it was last taken.
    pub fn take_unsaved_config(&self) -> Option<ShieldConfig> {
        self.lifetime_unsaved.swap(false, Ordering::Relaxed).then(|| self.config())
    }

    pub fn tab_summary(&self, tab: TabId) -> (u32, Vec<(String, u32)>) {
        let Ok(s) = self.stats.lock() else { return (0, Vec::new()) };
        match s.tabs.get(&tab) {
            Some(t) => {
                let mut hosts: Vec<(String, u32)> = t.hosts.iter().map(|(h, n)| (h.clone(), *n)).collect();
                hosts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                hosts.truncate(10);
                (t.blocked, hosts)
            }
            None => (0, Vec::new()),
        }
    }

    pub fn session_total(&self) -> u64 {
        self.stats.lock().map(|s| s.session_total).unwrap_or(0)
    }

    pub fn site_blocked(&self, host: &str) -> u32 {
        self.stats.lock().map(|s| s.sites.get(host).copied().unwrap_or(0)).unwrap_or(0)
    }

    // ---- building the rule set -----------------------------------------------------------------

    fn rebuild_now_starter_only(&self) {
        let mut b = EngineBuilder::new();
        b.add_list(STARTER, LIST_STARTER);
        let engine = b.build();
        let count = engine.rule_count();
        if let Ok(mut e) = self.engine.write() {
            *e = Arc::new(engine);
        }
        if let Ok(mut built) = self.built.lock() {
            *built = (EngineStats::default(), count, 0, 0);
        }
    }

    /// Builds the engine from the starter rules, the user's rules and the stored copy of every enabled list.
    /// Reads files and parses ~100,000 lines, so it runs off the UI and controller threads.
    fn compile(&self) -> (Engine, HashMap<String, usize>, usize) {
        let cfg = self.config();
        let mut builder = EngineBuilder::new();
        let mut per_list = HashMap::new();
        builder.add_list(STARTER, LIST_STARTER);
        let user = builder.add_list(&cfg.user_rules, LIST_USER);
        for (i, spec) in LISTS.iter().enumerate() {
            if !cfg.list_enabled(spec.id) {
                continue;
            }
            if let Some(text) = lists::read_list(&self.dir, spec.id) {
                per_list.insert(spec.id.to_string(), builder.add_list(&text, lists::list_number(i)));
            }
        }
        (builder.build(), per_list, user)
    }

    /// Rebuilds the engine and swaps it in whole; requests keep using the old one until then.
    pub fn rebuild_blocking(&self) {
        let ticket = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let (engine, per_list, user_rules) = self.compile();
        if self.generation.load(Ordering::SeqCst) != ticket {
            // A newer rebuild was requested while this one ran; let that one publish.
            return;
        }
        let stats = engine.stats();
        let total = engine.rule_count();
        let fallback = engine.fallback_count();
        if let Ok(mut lists) = self.lists.lock() {
            for (id, rules) in &per_list {
                if let Some(status) = lists.get_mut(id) {
                    status.rules = *rules;
                }
            }
        }
        if let Ok(mut e) = self.engine.write() {
            *e = Arc::new(engine);
        }
        if let Ok(mut built) = self.built.lock() {
            *built = (stats, total, fallback, user_rules);
        }
        log::info!(
            "shield: rules ready ({total} rules, {fallback} unindexed, {} cosmetic and {} unsupported skipped)",
            stats.cosmetic_skipped,
            stats.unsupported_skipped
        );
    }

    pub fn rebuild_in_background(self: &Arc<Self>) {
        let me = self.clone();
        drop(tauri::async_runtime::spawn_blocking(move || me.rebuild_blocking()));
    }

    /// Reads the stored copies and metadata of the lists. Call once at startup, off the UI thread.
    fn load_stored(&self) {
        if let Ok(mut lists) = self.lists.lock() {
            for spec in LISTS {
                let meta = lists::read_meta(&self.dir, spec.id);
                let present = lists::list_path(&self.dir, spec.id).is_file();
                lists.insert(
                    spec.id.to_string(),
                    ListStatus {
                        meta: meta.filter(|_| present),
                        rules: 0,
                        state: if present { ListState::Ready } else { ListState::Missing },
                        error: None,
                    },
                );
            }
        }
    }

    /// Startup work: read what is stored, compile it, and look for newer versions if the copies are old.
    pub fn start(self: &Arc<Self>) {
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            let loader = me.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || {
                loader.load_stored();
                loader.rebuild_blocking();
            })
            .await;
            // Do not compete with the first page load for the network.
            tokio::time::sleep(std::time::Duration::from_secs(20)).await;
            if me.is_active() {
                me.update_lists(false).await;
            }
        });
    }

    /// Downloads every enabled list that is missing or old (or all of them when `force`), then rebuilds.
    pub async fn update_lists(self: &Arc<Self>, force: bool) -> UpdateSummary {
        let mut summary = UpdateSummary::default();
        if self.updating.swap(true, Ordering::SeqCst) {
            summary.already_running = true;
            return summary;
        }
        let cfg = self.config();
        for spec in LISTS.iter().filter(|s| cfg.list_enabled(s.id)) {
            let now = now_ms();
            let previous = self.lists.lock().ok().and_then(|l| l.get(spec.id).and_then(|s| s.meta.clone()));
            let stale = previous.as_ref().is_none_or(|m| now - m.fetched_at > lists::REFRESH_AFTER_MS);
            if !force && !stale {
                continue;
            }
            self.set_state(spec.id, ListState::Updating, None);
            match lists::fetch(spec.url, previous.as_ref(), now).await {
                Ok(lists::Fetched::NotModified) => {
                    let meta = lists::touch(&self.dir, spec.id, previous.unwrap_or_default(), now);
                    self.finish_list(spec.id, Some(meta), None);
                    summary.unchanged += 1;
                }
                Ok(lists::Fetched::Fresh { text, meta }) => match lists::install(&self.dir, spec.id, &text, meta) {
                    Ok(meta) => {
                        self.finish_list(spec.id, Some(meta), None);
                        summary.updated += 1;
                        log::info!("shield: list {} updated ({} bytes)", spec.id, text.len());
                    }
                    Err(e) => {
                        self.finish_list(spec.id, None, Some(format!("The list could not be saved: {e}")));
                        summary.failed += 1;
                    }
                },
                Err(message) => {
                    log::warn!("shield: updating {} failed: {message}", spec.id);
                    self.finish_list(spec.id, None, Some(message));
                    summary.failed += 1;
                }
            }
        }
        if summary.updated > 0 {
            let me = self.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || me.rebuild_blocking()).await;
        }
        self.updating.store(false, Ordering::SeqCst);
        summary
    }

    fn set_state(&self, id: &str, state: ListState, error: Option<String>) {
        if let Ok(mut lists) = self.lists.lock() {
            if let Some(s) = lists.get_mut(id) {
                s.state = state;
                s.error = error;
            }
        }
    }

    /// Records the outcome of an update. A failure keeps whatever copy was there before.
    fn finish_list(&self, id: &str, meta: Option<ListMeta>, error: Option<String>) {
        if let Ok(mut lists) = self.lists.lock() {
            if let Some(s) = lists.get_mut(id) {
                let have_copy = lists::list_path(&self.dir, id).is_file();
                if let Some(m) = meta {
                    s.meta = Some(m);
                }
                s.error = error.clone();
                s.state = match (&error, have_copy) {
                    (Some(_), _) => ListState::Failed,
                    (None, true) => ListState::Ready,
                    (None, false) => ListState::Missing,
                };
            }
        }
    }

    // ---- what the UI is shown ------------------------------------------------------------------

    pub fn view(&self, site: Option<&str>) -> ShieldView {
        let cfg = self.config();
        let lists = self.lists.lock().map(|l| l.clone()).unwrap_or_default();
        let (stats, total, fallback, user_rules_accepted) = self.built.lock().map(|b| *b).unwrap_or_default();
        let list_views = LISTS
            .iter()
            .map(|spec| {
                let status = lists.get(spec.id);
                ListView {
                    id: spec.id.to_string(),
                    name: spec.name.to_string(),
                    description: spec.description.to_string(),
                    homepage: spec.homepage.to_string(),
                    license: spec.license.to_string(),
                    enabled: cfg.list_enabled(spec.id),
                    state: status.map(|s| s.state).unwrap_or(ListState::Missing),
                    error: status.and_then(|s| s.error.clone()),
                    rules: status.map(|s| s.rules).unwrap_or(0),
                    fetched_at: status.and_then(|s| s.meta.as_ref()).map(|m| m.fetched_at),
                    version: status.and_then(|s| s.meta.as_ref()).and_then(|m| m.version.clone()),
                }
            })
            .collect();
        let site_view =
            site.and_then(normalize_site).map(|host| SiteView { enabled: !cfg.site_disabled(&host), blocked: self.site_blocked(&host), host });
        ShieldView {
            enabled: cfg.enabled,
            lists: list_views,
            user_rules: cfg.user_rules.clone(),
            user_rules_accepted,
            disabled_sites: cfg.disabled_sites.clone(),
            engine: EngineView { rules: total, unindexed: fallback, stats },
            blocked_session: self.session_total(),
            blocked_lifetime: cfg.lifetime_blocked,
            updating: self.updating.load(Ordering::SeqCst),
            site: site_view,
            now: now_ms(),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSummary {
    pub updated: usize,
    pub unchanged: usize,
    pub failed: usize,
    pub already_running: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub homepage: String,
    pub license: String,
    pub enabled: bool,
    pub state: ListState,
    pub error: Option<String>,
    pub rules: usize,
    pub fetched_at: Option<i64>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteView {
    pub host: String,
    pub enabled: bool,
    /// Blocked on this site since the browser started (not counting private tabs).
    pub blocked: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineView {
    pub rules: usize,
    pub unindexed: usize,
    pub stats: EngineStats,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShieldView {
    pub enabled: bool,
    pub lists: Vec<ListView>,
    pub user_rules: String,
    /// How many of the user's rules were understood.
    pub user_rules_accepted: usize,
    pub disabled_sites: Vec<String>,
    pub engine: EngineView,
    pub blocked_session: u64,
    pub blocked_lifetime: u64,
    pub updating: bool,
    pub site: Option<SiteView>,
    /// When this view was made (ms since 1970), so the page can say how old a list is without reading the clock.
    pub now: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabShieldView {
    pub blocked: u32,
    pub hosts: Vec<HostCount>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostCount {
    pub host: String,
    pub count: u32,
}

impl Shield {
    pub fn tab_view(&self, tab: TabId) -> TabShieldView {
        let (blocked, hosts) = self.tab_summary(tab);
        TabShieldView { blocked, hosts: hosts.into_iter().map(|(host, count)| HostCount { host, count }).collect() }
    }

    /// Answers "would this request be blocked?" for the settings page's test box, naming the rule's list.
    pub fn explain(&self, url: &str, page_host: &str, kind: u16) -> String {
        let decision = self.check(url, &page_host.to_ascii_lowercase(), kind);
        if let Some(note) = decision.note {
            return format!("Allowed. {note}");
        }
        match decision.verdict {
            Verdict::Allow => "Allowed: no rule matches.".into(),
            Verdict::Exception => "Allowed: an exception rule matches.".into(),
            Verdict::Block { list } => format!("Blocked by {}.", lists::list_name(list)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rules::{T_IMAGE, T_SCRIPT};

    fn shield() -> Arc<Shield> {
        let dir = std::env::temp_dir().join(format!("pb-shield-mod-{}-{}", std::process::id(), now_ms()));
        let s = Arc::new(Shield::new(dir, ShieldConfig::default()));
        s.rebuild_now_starter_only();
        s
    }

    fn blocked(s: &Shield, url: &str, top: &str) -> bool {
        matches!(s.check(url, top, T_SCRIPT).verdict, Verdict::Block { .. })
    }

    #[test]
    fn the_starter_rules_block_known_trackers_on_other_sites_only() {
        let s = shield();
        assert!(blocked(&s, "https://securepubads.g.doubleclick.net/tag/js/gpt.js", "news.example"));
        assert!(blocked(&s, "https://www.google-analytics.com/analytics.js", "shop.example"));
        assert!(!blocked(&s, "https://www.google-analytics.com/analytics.js", "google-analytics.com"), "first-party use is left alone");
        assert!(!blocked(&s, "https://news.example/app.js", "news.example"));
        assert!(!blocked(&s, "https://cdn.jsdelivr.net/npm/x.js", "news.example"), "ordinary third-party resources are not blocked");
    }

    #[test]
    fn switching_off_globally_or_per_site_lets_everything_through() {
        let s = shield();
        let url = "https://securepubads.g.doubleclick.net/x.js";
        assert!(blocked(&s, url, "news.example"));

        let mut cfg = s.config();
        cfg.disabled_sites = vec!["news.example".into()];
        s.apply_config(cfg);
        assert!(!blocked(&s, url, "news.example"));
        assert!(!blocked(&s, url, "m.news.example"), "subdomains follow the site");
        assert_eq!(s.check(url, "news.example", T_SCRIPT).note, Some("The Shield is off for this site."));
        assert!(blocked(&s, url, "other.example"), "other sites keep their protection");

        let mut cfg = s.config();
        cfg.disabled_sites.clear();
        cfg.enabled = false;
        s.apply_config(cfg);
        assert!(!s.is_active());
        assert!(!blocked(&s, url, "other.example"));
    }

    #[test]
    fn local_pages_and_requests_are_never_filtered() {
        let s = shield();
        assert!(!blocked(&s, "https://securepubads.g.doubleclick.net/x.js", "localhost"));
        assert!(!blocked(&s, "https://tauri.localhost/doubleclick.net/x.js", "news.example"));
        assert!(!blocked(&s, "http://localhost:3000/ads.js", "news.example"));
    }

    #[test]
    fn user_rules_can_block_and_make_exceptions() {
        let s = shield();
        let mut cfg = s.config();
        cfg.user_rules = "||annoying.example^\n@@||securepubads.g.doubleclick.net^$domain=allowed.example\n".into();
        s.apply_config(cfg);
        s.rebuild_blocking();
        assert!(blocked(&s, "https://annoying.example/x.js", "any.example"));
        assert!(blocked(&s, "https://securepubads.g.doubleclick.net/x.js", "news.example"));
        assert!(!blocked(&s, "https://securepubads.g.doubleclick.net/x.js", "allowed.example"), "the exception applies on that page");
        assert!(s.explain("https://annoying.example/x.js", "any.example", T_SCRIPT).starts_with("Blocked by Your rules"));
        assert!(s.explain("https://nothing.example/x.js", "any.example", T_IMAGE).contains("no rule matches"));
    }

    #[test]
    fn counts_are_per_tab_reset_on_navigation_and_coalesce_notifications() {
        let s = shield();
        assert!(s.record_block(7, false, "news.example", "https://ads.doubleclick.net/a"), "first change asks for a notification");
        assert!(!s.record_block(7, false, "news.example", "https://ads.doubleclick.net/b"), "later ones are folded into it");
        s.record_block(7, false, "news.example", "https://x.adnxs.com/c");
        s.record_block(8, true, "private.example", "https://x.adnxs.com/c");
        s.clear_pending();
        assert!(s.record_block(7, false, "news.example", "https://x.adnxs.com/d"), "after delivery the next change asks again");

        let (n, hosts) = s.tab_summary(7);
        assert_eq!(n, 4);
        assert_eq!(hosts[0], ("ads.doubleclick.net".to_string(), 2));
        assert_eq!(hosts[1], ("x.adnxs.com".to_string(), 2), "ties are listed alphabetically");
        assert_eq!(s.tab_counts().get(&8), Some(&1));
        assert_eq!(s.site_blocked("news.example"), 4);
        assert_eq!(s.site_blocked("private.example"), 0, "private tabs are not remembered per site");
        assert_eq!(s.config().lifetime_blocked, 4, "private tabs do not count towards the lifetime total");
        assert_eq!(s.session_total(), 5);

        s.clear_pending();
        assert!(s.reset_tab(7), "a reset that removes a count asks for a notification");
        assert!(!s.reset_tab(7), "nothing left to reset");
        assert_eq!(s.tab_summary(7).0, 0);
        assert!(s.take_unsaved_config().is_some());
        assert!(s.take_unsaved_config().is_none());
    }

    #[test]
    fn configuration_from_the_ui_is_cleaned() {
        let cfg = ShieldConfig {
            enabled: true,
            disabled_sites: vec![
                "HTTPS://WWW.Example.com/path?x=1".into(),
                "example.com".into(),
                "not a host".into(),
                "".into(),
                "user@evil.example:8080".into(),
            ],
            disabled_lists: vec!["easylist".into(), "easylist".into(), "made-up".into()],
            user_rules: "x".repeat(MAX_USER_RULES_BYTES + 100),
            lifetime_blocked: 0,
        }
        .sanitized();
        assert_eq!(cfg.disabled_sites, vec!["evil.example".to_string(), "example.com".to_string()]);
        assert_eq!(cfg.disabled_lists, vec!["easylist".to_string()]);
        assert_eq!(cfg.user_rules.len(), MAX_USER_RULES_BYTES);
        assert_eq!(normalize_site("  Sub.Example.com:443/x "), Some("sub.example.com".into()));
        assert_eq!(normalize_site("a..b"), None);
    }

    #[test]
    fn an_old_config_file_without_the_new_fields_still_loads() {
        let cfg: ShieldConfig = serde_json::from_str(r#"{"enabled":false}"#).unwrap();
        assert!(!cfg.enabled);
        assert!(cfg.disabled_sites.is_empty() && cfg.user_rules.is_empty());
    }

    #[test]
    fn stored_lists_are_compiled_and_a_broken_one_falls_back_to_what_was_there() {
        let s = shield();
        // A stored EasyList copy with one extra rule.
        let mut text = String::from("[Adblock Plus 2.0]\n! Title: EasyList\n! Version: 1\n||listed.example^\n");
        for i in 0..1100 {
            text.push_str(&format!("||filler{i}.example^\n"));
        }
        lists::install(&s.dir, "easylist", &text, ListMeta { fetched_at: 1, ..ListMeta::default() }).unwrap();
        s.load_stored();
        s.rebuild_blocking();
        assert!(blocked(&s, "https://listed.example/x.js", "page.example"));
        assert_eq!(s.view(None).lists[0].state, ListState::Ready);
        assert_eq!(s.view(None).lists[0].rules, 1101);

        // A list switched off stops applying.
        let mut cfg = s.config();
        cfg.disabled_lists = vec!["easylist".into()];
        s.apply_config(cfg);
        s.rebuild_blocking();
        assert!(!blocked(&s, "https://listed.example/x.js", "page.example"));
        let _ = std::fs::remove_dir_all(&s.dir);
    }

    #[test]
    fn a_failed_update_keeps_the_old_copy_in_use_and_says_why() {
        let s = shield();
        let mut text = String::from("[Adblock Plus 2.0]\n! Title: EasyList\n||kept.example^\n");
        for i in 0..1100 {
            text.push_str(&format!("||filler{i}.example^\n"));
        }
        lists::install(&s.dir, "easylist", &text, ListMeta { fetched_at: 1, ..ListMeta::default() }).unwrap();
        s.load_stored();
        s.rebuild_blocking();
        s.finish_list("easylist", None, Some("The server answered with HTTP 500.".into()));
        let view = s.view(None);
        assert_eq!(view.lists[0].state, ListState::Failed);
        assert_eq!(view.lists[0].error.as_deref(), Some("The server answered with HTTP 500."));
        assert!(blocked(&s, "https://kept.example/x.js", "page.example"), "the old list still works");
        let _ = std::fs::remove_dir_all(&s.dir);
    }

    #[test]
    fn the_view_reports_the_current_site() {
        let s = shield();
        let mut cfg = s.config();
        cfg.disabled_sites = vec!["off.example".into()];
        s.apply_config(cfg);
        let v = s.view(Some("https://www.off.example/page"));
        let site = v.site.unwrap();
        assert_eq!((site.host.as_str(), site.enabled), ("off.example", false));
        assert!(s.view(Some("https://on.example")).site.unwrap().enabled);
        assert!(s.view(None).site.is_none());
    }
}
