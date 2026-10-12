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

/// Where a page's request for a new window (a `target="_blank"` link, `window.open`) opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum NewWindowRequests {
    #[default]
    Tab,
    /// A browser window of its own. Pop-ups that ask for a size (sign-in windows) stay small pop-up windows either way.
    Window,
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
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            search_engine: SearchEngine::Google,
            custom_search_template: None,
            startup: StartupBehavior::RestoreSession,
            show_bookmarks_bar: true,
            download_dir: None,
            accent: "ion".into(),
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
        self
    }

    /// True when `host` matches a never-sleep entry exactly or as a parent domain.
    pub fn is_never_sleep(&self, host: &str) -> bool {
        self.never_sleep_hosts.iter().any(|h| host == h || host.ends_with(&format!(".{h}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn the_ai_context_limit_is_kept_in_a_sane_range() {
        let low = Settings { ai_max_context_chars: 0, ..Default::default() }.sanitized();
        let high = Settings { ai_max_context_chars: u32::MAX, ..Default::default() }.sanitized();
        assert_eq!((low.ai_max_context_chars, high.ai_max_context_chars), (4_000, 120_000));
        assert_eq!(Settings::default().ai_max_context_chars, 24_000);
    }
}
