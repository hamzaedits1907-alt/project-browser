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
        self.sleep_after_minutes = self.sleep_after_minutes.clamp(1, 24 * 60);
        self.heavy_tab_memory_mb = self.heavy_tab_memory_mb.clamp(100, 16_384);
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
}
