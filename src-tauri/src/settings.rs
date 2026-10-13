//! User preferences. Stored as one JSON document per profile; every field has a
//! default so older profiles keep loading as fields are added.

use serde::{Deserialize, Serialize};

use crate::navigation::SearchEngine;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum StartupBehavior {
    #[default]
    RestoreSession,
    NewTab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum BackgroundTabAction {
    /// WebView2 `TrySuspend`: page state is kept, script timers stop, some memory is released.
    #[default]
    Sleep,
    /// Destroy the webview: frees the renderer entirely; the page reloads when reopened.
    Unload,
}

/// Dark pages for sites that are bright: off, ask sites for their own dark theme, or darken bright pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum DarkPages {
    #[default]
    Off,
    /// Sites are told the user prefers dark (`prefers-color-scheme: dark`); those with a dark theme use it.
    Auto,
    /// Auto, and pages that are still bright are darkened by the browser (pictures and video keep their colours).
    Force,
}

/// A user-defined address-bar shortcut: `!key words` searches `url` with `%s` replaced by the words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CustomBang {
    pub key: String,
    pub name: String,
    pub url: String,
}

/// Where a page's request for a new window (a `target="_blank"` link, `window.open`) opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum NewWindowRequests {
    #[default]
    Tab,
    /// A browser window of its own. Pop-ups that ask for a size (sign-in windows) stay small pop-up windows either way.
    Window,
}

/// The shape language of the interface: angular (bevelled corners, the default), rounded, or soft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum CornerStyle {
    #[default]
    Angular,
    Rounded,
    Soft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Density {
    Compact,
    #[default]
    Comfortable,
    Spacious,
}

/// How much the interface animates. `reduce_motion` (or Gaming Mode's reduce-animations) turns animation off entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum MotionLevel {
    Subtle,
    #[default]
    Standard,
    Expressive,
}

/// How tabs are drawn: shaped (the active tab merges with the toolbar), pills, or minimal text with an underline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TabStyle {
    #[default]
    Shaped,
    Pill,
    Minimal,
}

/// The sidebar: an icon rail, a rail with labels, or hidden (its items stay in the main menu).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SidebarMode {
    #[default]
    Rail,
    Expanded,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum NtpLayout {
    #[default]
    Full,
    Minimal,
}

/// A site the user pinned (to the sidebar or the New Tab page).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinnedSite {
    pub title: String,
    pub url: String,
}

/// Sidebar entries, in their default order. The shell knows what each one opens; unknown ids are dropped.
pub const SIDEBAR_ITEMS: &[&str] = &[
    "bookmarks",
    "history",
    "downloads",
    "ai",
    "memory",
    "research",
    "gaming",
    "shield",
    "passwords",
    "privacy",
    "performance",
    "notes",
    "media",
    "focus",
];
pub const DEFAULT_SIDEBAR_ITEMS: &[&str] = &["bookmarks", "history", "downloads", "ai", "gaming", "shield", "passwords"];
/// Optional toolbar buttons (back, forward, reload, the address bar and the menu are always there).
pub const TOOLBAR_ITEMS: &[&str] = &[
    "gaming",
    "shield",
    "ai",
    "downloads",
    "bookmarks",
    "history",
    "memory",
    "performance",
    "privateWindow",
    "split",
    "reader",
    "screenshot",
    "commands",
    "dark",
];
pub const DEFAULT_TOOLBAR_ITEMS: &[&str] = &["gaming", "shield", "ai", "downloads"];
pub const NTP_WIDGETS: &[&str] = &["shortcuts", "continue", "closed", "bookmarks", "downloads", "system", "gaming", "ai"];
pub const DEFAULT_NTP_WIDGETS: &[&str] = &["shortcuts", "continue", "closed", "bookmarks", "downloads", "system", "gaming", "ai"];
pub const THEMES: &[&str] = &["neon", "oled", "violet", "tactical", "crimson", "minimal", "performance", "studio", "daylight", "custom"];
pub const NTP_BACKGROUNDS: &[&str] = &["grid", "aurora", "circuit", "horizon", "plain"];

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// Keeps the known ids of `list`, in the user's order, without duplicates.
fn known_ids(list: Vec<String>, known: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in list {
        if known.contains(&id.as_str()) && !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

fn clean_sites(sites: Vec<PinnedSite>, max: usize) -> Vec<PinnedSite> {
    let mut out: Vec<PinnedSite> = Vec::new();
    for s in sites {
        let url = s.url.trim().to_string();
        let ok = url::Url::parse(&url).map(|u| matches!(u.scheme(), "http" | "https") && u.host_str().is_some()).unwrap_or(false);
        if ok && !out.iter().any(|o| o.url == url) {
            out.push(PinnedSite { title: s.title.trim().chars().take(80).collect(), url });
        }
    }
    out.truncate(max);
    out
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub search_engine: SearchEngine,
    /// Optional custom search template containing `%s`; overrides `search_engine` when set.
    pub custom_search_template: Option<String>,
    pub startup: StartupBehavior,
    pub show_bookmarks_bar: bool,
    /// Absolute folder for downloads; `None` means the Windows Downloads folder.
    pub download_dir: Option<String>,
    /// Show the Save dialog for every download instead of saving into `download_dir` straight away.
    pub download_ask: bool,

    pub accent: String,
    pub new_tab_background: String,
    pub reduce_motion: bool,
    pub show_ram_on_new_tab: bool,

    pub sleeping_tabs_enabled: bool,
    pub sleep_after_minutes: u32,
    /// Hosts (e.g. `music.youtube.com`) that are never put to sleep.
    pub never_sleep_hosts: Vec<String>,

    pub gaming_mode: bool,
    pub gaming_background_action: BackgroundTabAction,
    pub gaming_pause_downloads: bool,
    pub gaming_reduce_animations: bool,
    pub gaming_low_memory_target: bool,

    /// WebView2's built-in password saving (stored DPAPI-encrypted in the profile).
    pub save_passwords: bool,
    /// WebView2's built-in form/address autofill.
    pub autofill: bool,

    pub heavy_tab_memory_mb: u32,

    /// Pinned tabs are never put to sleep or unloaded automatically.
    pub protect_pinned_tabs: bool,
    /// When the browser's total memory exceeds this many MB, the least recently used background tabs are put to
    /// sleep (then unloaded) until it is below the limit. 0 turns the limit off.
    pub memory_limit_mb: u32,

    /// The most page text (in characters, about four per token) one AI request may carry. Every request is
    /// checked against it in the core, whatever the shell asks for.
    pub ai_max_context_chars: u32,

    /// Look for a new version once after startup (it is never installed without asking).
    pub check_updates: bool,

    /// Sites that may open pop-ups without being asked to (a pop-up nobody asked for is blocked everywhere else).
    pub popup_allowed_hosts: Vec<String>,
    pub new_window_requests: NewWindowRequests,

    /// Theme preset (`THEMES`); `accent` overrides its accent colour unless it is "auto".
    pub theme: String,
    /// Hue (0-359) of the accent when `accent` is "custom".
    pub custom_hue: u32,
    pub corner_style: CornerStyle,
    pub density: Density,
    pub motion: MotionLevel,
    /// Short, quiet interface sounds (off by default).
    pub ui_sounds: bool,
    /// Volume of the interface sounds, 0-100.
    pub ui_sound_volume: u32,
    pub tab_style: TabStyle,
    pub vertical_tabs: bool,
    pub compact_title_bar: bool,
    pub sidebar: SidebarMode,
    pub sidebar_items: Vec<String>,
    /// Sites pinned to the sidebar; a click opens the site (or switches to its tab).
    pub sidebar_sites: Vec<PinnedSite>,
    pub toolbar_items: Vec<String>,
    pub ntp_layout: NtpLayout,
    pub ntp_widgets: Vec<String>,
    /// Shortcuts the user added to the New Tab page, shown before the most visited sites.
    pub ntp_shortcuts: Vec<PinnedSite>,
    /// Most-visited sites the user removed from the New Tab page (by address).
    pub ntp_hidden_sites: Vec<String>,

    /// Link Cleaner: remove click-tracking parameters (`fbclid`, `utm_*`, …) from addresses before pages load.
    pub clean_links: bool,
    /// Sites (and their subdomains) where Link Cleaner leaves addresses alone.
    pub clean_links_exceptions: Vec<String>,
    /// Dark pages (Off / Auto / Force); per-site choices are kept by the shell.
    pub dark_pages: DarkPages,
    /// The user's own `!` shortcuts, checked before the built-in ones.
    pub custom_bangs: Vec<CustomBang>,
    /// SponsorSkip: skip sponsor segments in YouTube videos (only a hash prefix of the video id is sent; see sponsor.rs).
    pub sponsor_skip: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            search_engine: SearchEngine::Google,
            custom_search_template: None,
            startup: StartupBehavior::RestoreSession,
            show_bookmarks_bar: true,
            download_dir: None,
            download_ask: false,
            accent: "auto".into(),
            new_tab_background: "grid".into(),
            reduce_motion: false,
            show_ram_on_new_tab: true,
            sleeping_tabs_enabled: true,
            sleep_after_minutes: 30,
            never_sleep_hosts: Vec::new(),
            gaming_mode: false,
            gaming_background_action: BackgroundTabAction::Sleep,
            gaming_pause_downloads: true,
            gaming_reduce_animations: true,
            gaming_low_memory_target: true,
            save_passwords: true,
            autofill: true,
            heavy_tab_memory_mb: 500,
            protect_pinned_tabs: true,
            memory_limit_mb: 0,
            ai_max_context_chars: 24000,
            check_updates: true,
            popup_allowed_hosts: Vec::new(),
            new_window_requests: NewWindowRequests::Tab,
            theme: "neon".into(),
            custom_hue: 190,
            corner_style: CornerStyle::Angular,
            density: Density::Comfortable,
            motion: MotionLevel::Standard,
            ui_sounds: false,
            ui_sound_volume: 60,
            tab_style: TabStyle::Shaped,
            vertical_tabs: false,
            compact_title_bar: false,
            sidebar: SidebarMode::Rail,
            sidebar_items: strings(DEFAULT_SIDEBAR_ITEMS),
            sidebar_sites: Vec::new(),
            toolbar_items: strings(DEFAULT_TOOLBAR_ITEMS),
            ntp_layout: NtpLayout::Full,
            ntp_widgets: strings(DEFAULT_NTP_WIDGETS),
            ntp_shortcuts: Vec::new(),
            ntp_hidden_sites: Vec::new(),
            clean_links: true,
            clean_links_exceptions: Vec::new(),
            dark_pages: DarkPages::Off,
            custom_bangs: Vec::new(),
            sponsor_skip: true,
        }
    }
}

impl Settings {
    pub fn search_template(&self) -> String {
        match &self.custom_search_template {
            Some(t) if t.contains("%s") && (t.starts_with("https://") || t.starts_with("http://")) => t.clone(),
            _ => self.search_engine.template().to_string(),
        }
    }

    /// Clamps values coming from the UI into sane ranges.
    pub fn sanitized(mut self) -> Settings {
        self.popup_allowed_hosts = self.popup_allowed_hosts.iter().filter_map(|h| crate::shield::normalize_site(h)).collect();
        self.popup_allowed_hosts.sort();
        self.popup_allowed_hosts.dedup();
        self.popup_allowed_hosts.truncate(200);
        self.sleep_after_minutes = self.sleep_after_minutes.clamp(1, 24 * 60);
        self.heavy_tab_memory_mb = self.heavy_tab_memory_mb.clamp(100, 16_384);
        // 0 = off; otherwise keep the limit in a range that cannot starve the browser itself.
        if self.memory_limit_mb != 0 {
            self.memory_limit_mb = self.memory_limit_mb.clamp(512, 65_536);
        }
        self.ai_max_context_chars = self.ai_max_context_chars.clamp(4_000, 120_000);
        self.never_sleep_hosts = self
            .never_sleep_hosts
            .into_iter()
            .map(|h| h.trim().trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/').to_ascii_lowercase())
            .filter(|h| !h.is_empty())
            .collect();
        self.never_sleep_hosts.sort();
        self.never_sleep_hosts.dedup();
        if let Some(t) = &self.custom_search_template {
            if t.trim().is_empty() {
                self.custom_search_template = None;
            }
        }
        if !THEMES.contains(&self.theme.as_str()) {
            self.theme = "neon".into();
        }
        // "ion" was the default accent before themes existed: it now means "the theme's own accent".
        if self.accent == "ion" || self.accent.trim().is_empty() {
            self.accent = "auto".into();
        }
        self.custom_hue %= 360;
        self.ui_sound_volume = self.ui_sound_volume.min(100);
        self.clean_links_exceptions = self.clean_links_exceptions.iter().filter_map(|h| crate::shield::normalize_site(h)).collect();
        self.clean_links_exceptions.sort();
        self.clean_links_exceptions.dedup();
        self.clean_links_exceptions.truncate(200);
        self.custom_bangs = clean_bangs(self.custom_bangs);
        if !NTP_BACKGROUNDS.contains(&self.new_tab_background.as_str()) {
            self.new_tab_background = "grid".into();
        }
        self.sidebar_items = known_ids(self.sidebar_items, SIDEBAR_ITEMS);
        self.toolbar_items = known_ids(self.toolbar_items, TOOLBAR_ITEMS);
        self.ntp_widgets = known_ids(self.ntp_widgets, NTP_WIDGETS);
        self.sidebar_sites = clean_sites(self.sidebar_sites, 12);
        self.ntp_shortcuts = clean_sites(self.ntp_shortcuts, 24);
        self.ntp_hidden_sites.retain(|u| !u.trim().is_empty());
        self.ntp_hidden_sites.dedup();
        self.ntp_hidden_sites.truncate(200);
        self
    }

    /// These settings with the fields of a JSON object (camelCase names) replaced. Used for the
    /// `PROJECT_BROWSER_TEST_SETTINGS` test switch, so CI probes can start the real app in a known configuration.
    pub fn with_overrides(self, json: &str) -> std::result::Result<Settings, String> {
        let patch: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let serde_json::Value::Object(patch) = patch else { return Err("expected a JSON object".into()) };
        let mut base = serde_json::to_value(&self).map_err(|e| e.to_string())?;
        if let Some(obj) = base.as_object_mut() {
            for (k, v) in patch {
                obj.insert(k, v);
            }
        }
        serde_json::from_value::<Settings>(base).map(Settings::sanitized).map_err(|e| e.to_string())
    }

    /// True when `host` matches a never-sleep entry exactly or as a parent domain.
    pub fn is_never_sleep(&self, host: &str) -> bool {
        self.never_sleep_hosts.iter().any(|h| host == h || host.ends_with(&format!(".{h}")))
    }
}

/// Keeps shortcuts with a short lowercase key (letters and digits) and an http(s) address containing `%s`.
fn clean_bangs(bangs: Vec<CustomBang>) -> Vec<CustomBang> {
    let mut out: Vec<CustomBang> = Vec::new();
    for b in bangs {
        let key = b.key.trim().trim_start_matches('!').to_ascii_lowercase();
        let url = b.url.trim().to_string();
        let ok_key = !key.is_empty() && key.len() <= 12 && key.chars().all(|c| c.is_ascii_alphanumeric());
        let ok_url = url.contains("%s") && url::Url::parse(&url.replace("%s", "x")).map(|u| matches!(u.scheme(), "http" | "https")).unwrap_or(false);
        if ok_key && ok_url && !out.iter().any(|o| o.key == key) {
            out.push(CustomBang { key, name: b.name.trim().chars().take(40).collect(), url });
        }
    }
    out.truncate(50);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_bangs_and_link_cleaner_exceptions_are_cleaned() {
        let s = Settings {
            custom_bangs: vec![
                CustomBang { key: "!Gh".into(), name: "GitHub".into(), url: "https://github.com/search?q=%s".into() },
                CustomBang { key: "bad key".into(), name: String::new(), url: "https://x/?q=%s".into() },
                CustomBang { key: "js".into(), name: String::new(), url: "javascript:%s".into() },
                CustomBang { key: "nos".into(), name: String::new(), url: "https://x/".into() },
                CustomBang { key: "gh".into(), name: "dup".into(), url: "https://x/?q=%s".into() },
            ],
            clean_links_exceptions: vec!["https://Shop.Example.com/".into(), "shop.example.com".into()],
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(s.custom_bangs.len(), 1);
        assert_eq!(s.custom_bangs[0].key, "gh");
        assert_eq!(s.clean_links_exceptions, vec!["shop.example.com".to_string()]);
    }

    #[test]
    fn test_overrides_replace_only_the_given_fields() {
        let s = Settings::default().with_overrides(r#"{"gamingMode":true,"downloadDir":"C:\\dl"}"#).unwrap();
        assert!(s.gaming_mode);
        assert_eq!(s.download_dir.as_deref(), Some("C:\\dl"));
        assert_eq!(s.sleep_after_minutes, 30);
        assert!(Settings::default().with_overrides("[1]").is_err());
        assert!(Settings::default().with_overrides("{").is_err());
    }

    #[test]
    fn unknown_and_missing_fields_fall_back_to_defaults() {
        let s: Settings = serde_json::from_str(r#"{"showBookmarksBar":false,"futureField":1}"#).unwrap();
        assert!(!s.show_bookmarks_bar);
        assert_eq!(s.sleep_after_minutes, 30);
    }

    #[test]
    fn custom_template_requires_placeholder_and_http() {
        let mut s = Settings { custom_search_template: Some("https://s.example/?q=%s".into()), ..Default::default() };
        assert_eq!(s.search_template(), "https://s.example/?q=%s");
        s.custom_search_template = Some("javascript:%s".into());
        assert_eq!(s.search_template(), SearchEngine::Google.template());
    }

    #[test]
    fn never_sleep_matching_and_sanitizing() {
        let s = Settings {
            never_sleep_hosts: vec![" https://YouTube.com/ ".into(), "".into(), "youtube.com".into()],
            sleep_after_minutes: 0,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(s.never_sleep_hosts, vec!["youtube.com".to_string()]);
        assert_eq!(s.sleep_after_minutes, 1);
        assert!(s.is_never_sleep("music.youtube.com"));
        assert!(s.is_never_sleep("youtube.com"));
        assert!(!s.is_never_sleep("notyoutube.com"));
    }

    #[test]
    fn interface_preferences_keep_only_known_values() {
        let s = Settings {
            theme: "rainbow".into(),
            accent: "ion".into(),
            custom_hue: 725,
            new_tab_background: "video".into(),
            sidebar_items: vec!["history".into(), "nope".into(), "history".into(), "ai".into()],
            toolbar_items: vec!["shield".into(), "made-up".into()],
            ntp_widgets: vec![],
            sidebar_sites: vec![
                PinnedSite { title: " Discord ".into(), url: "https://discord.com/app".into() },
                PinnedSite { title: "bad".into(), url: "javascript:alert(1)".into() },
                PinnedSite { title: "dup".into(), url: "https://discord.com/app".into() },
            ],
            ..Default::default()
        }
        .sanitized();
        assert_eq!(s.theme, "neon");
        assert_eq!(s.accent, "auto", "the old default accent follows the theme");
        assert_eq!(s.custom_hue, 5);
        assert_eq!(s.new_tab_background, "grid");
        assert_eq!(s.sidebar_items, vec!["history".to_string(), "ai".to_string()]);
        assert_eq!(s.toolbar_items, vec!["shield".to_string()]);
        assert!(s.ntp_widgets.is_empty(), "an empty widget list is a valid choice (minimal page)");
        assert_eq!(s.sidebar_sites, vec![PinnedSite { title: "Discord".into(), url: "https://discord.com/app".into() }]);
    }

    #[test]
    fn profiles_saved_before_themes_load_with_the_new_defaults() {
        let s: Settings = serde_json::from_str(r#"{"accent":"violet","reduceMotion":true}"#).unwrap();
        assert_eq!((s.theme.as_str(), s.accent.as_str()), ("neon", "violet"));
        assert_eq!(s.sidebar_items, strings(DEFAULT_SIDEBAR_ITEMS));
        assert_eq!(s.corner_style, CornerStyle::Angular);
        assert!(s.reduce_motion);
        let json = serde_json::to_string(&Settings::default()).unwrap();
        assert!(json.contains("\"cornerStyle\":\"angular\"") && json.contains("\"sidebar\":\"rail\""), "{json}");
    }

    #[test]
    fn the_ai_context_limit_is_kept_in_a_sane_range() {
        let low = Settings { ai_max_context_chars: 0, ..Default::default() }.sanitized();
        let high = Settings { ai_max_context_chars: u32::MAX, ..Default::default() }.sanitized();
        assert_eq!((low.ai_max_context_chars, high.ai_max_context_chars), (4_000, 120_000));
        assert_eq!(Settings::default().ai_max_context_chars, 24_000);
    }
}
