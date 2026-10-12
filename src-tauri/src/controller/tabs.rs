//! Tab lifecycle, navigation, layout and keyboard commands.

use tauri::{LogicalPosition, LogicalSize, Manager, Rect};

use super::{Controller, UiEvent, View};
use crate::error::now_ms;
use crate::native::{self, NavAction};
use crate::navigation::{self, internal_page, is_internal, resolve_input, Resolved, NEW_TAB_URL};
use crate::shortcuts::{zoom_step, Command};
use crate::tabs::{Lifecycle, NewTab, Security, TabId};

/// What to write in the log about a tab's page: scheme and host only, and nothing for private tabs.
fn describe_target(url: &str, private: bool) -> String {
    if private {
        return "private tab".into();
    }
    match url::Url::parse(url) {
        Ok(u) => format!("{}://{}", u.scheme(), u.host_str().unwrap_or("")),
        Err(_) => "unparseable address".into(),
    }
}

pub fn internal_title(url: &str) -> &'static str {
    match internal_page(url) {
        Some("newtab") => "New Tab",
        Some("settings") => "Settings",
        Some("history") => "History",
        Some("bookmarks") => "Bookmarks",
        Some("downloads") => "Downloads",
        Some("performance") => "Performance",
        Some("gaming") => "Gaming",
        _ => "",
    }
}

impl Controller {
    /// Refreshes derived per-tab fields after the URL changed.
    pub(crate) fn refresh_tab_meta(&mut self, id: TabId) {
        let Some(url) = self.tabs.get(id).map(|t| t.url.clone()) else { return };
        let bookmarked = !is_internal(&url) && self.db.is_bookmarked(&url).unwrap_or(false);
        if let Some(t) = self.tabs.get_mut(id) {
            t.bookmarked = bookmarked;
            if t.security != Security::CertError || is_internal(&url) {
                t.security = Security::for_url(&url);
            }
            if is_internal(&url) {
                t.title = internal_title(&url).to_string();
                t.favicon = None;
            }
        }
    }

    pub fn new_tab(&mut self, private: bool, focus_omnibox: bool) -> TabId {
        let id = self.tabs.create(NewTab { url: NEW_TAB_URL.into(), private, activate: true, ..Default::default() }, now_ms());
        self.refresh_tab_meta(id);
        self.after_activation();
        if focus_omnibox {
            self.focus_omnibox();
        }
        self.mark_session();
        id
    }

    /// Opens a URL (or omnibox text) in a new tab.
    pub fn open_url(&mut self, input: String, activate: bool) -> Option<TabId> {
        let resolved = resolve_input(&input, &self.settings.search_template())?;
        let id = self.tabs.create(NewTab { url: resolved.into_url(), activate, ..Default::default() }, now_ms());
        self.refresh_tab_meta(id);
        if activate {
            self.after_activation();
        } else {
            self.create_view(id, false);
        }
        self.mark_session();
        Some(id)
    }

    /// Link opened by a page (target=_blank, window.open, Ctrl+click).
    pub fn open_from_page(&mut self, opener: TabId, url: String, background: bool) {
        if url::Url::parse(&url).map(|u| navigation::is_blocked_tab_navigation(&u)).unwrap_or(true) {
            return;
        }
        let private = self.tabs.get(opener).map(|t| t.private).unwrap_or(false);
        let id = self.tabs.create(NewTab { url, private, activate: !background, opener: Some(opener), ..Default::default() }, now_ms());
        self.refresh_tab_meta(id);
        if background {
            self.create_view(id, false);
        } else {
            self.after_activation();
        }
        self.mark_session();
    }

    /// Switches to an existing tab showing `url`, or opens one.
    pub fn open_internal(&mut self, page: &str) {
        let url = format!("browser://{page}");
        if let Some(id) = self.tabs.tabs().iter().find(|t| t.url == url && !t.private).map(|t| t.id) {
            self.activate(id);
            return;
        }
        // Reuse the current tab if it's an empty new-tab page.
        if let Some(active) = self.tabs.active().filter(|t| t.url == NEW_TAB_URL && !t.private).map(|t| t.id) {
            self.navigate(active, url);
            return;
        }
        let id = self.tabs.create(NewTab { url, activate: true, ..Default::default() }, now_ms());
        self.refresh_tab_meta(id);
        self.after_activation();
        self.mark_session();
    }

    pub fn activate(&mut self, id: TabId) {
        if self.tabs.active_id() == Some(id) {
            return;
        }
        let prev = self.tabs.active_id();
        if self.tabs.activate(id, now_ms()) {
            if let Some(p) = prev {
                self.on_backgrounded(p);
            }
            self.after_activation();
            self.mark_session();
        }
    }

    /// Common work after the active tab changed.
    pub(crate) fn after_activation(&mut self) {
        self.close_find();
        let Some(tab) = self.tabs.active().cloned() else { return };
        // Any suspend still in flight for this tab is now out of date.
        *self.epochs.entry(tab.id).or_insert(0) += 1;
        self.dirty_until.remove(&tab.id);
        if tab.lifecycle == Lifecycle::Sleeping {
            // WebView2 resumes a suspended webview automatically when it becomes visible.
            if let Some(t) = self.tabs.get_mut(tab.id) {
                t.lifecycle = Lifecycle::Live;
            }
        }
        if self.settings.gaming_mode && self.settings.gaming_low_memory_target && self.views.contains_key(&tab.id) {
            let id = tab.id;
            let _ = self.app.run_on_main_thread(move || native::set_memory_target(id, false));
        }
        self.relayout();
        if is_internal(&tab.url) {
            if tab.url == NEW_TAB_URL {
                self.focus_omnibox();
            } else {
                self.focus_shell();
            }
        } else if let Some(v) = self.views.get(&tab.id) {
            let _ = v.webview.set_focus();
        }
        self.mark_prompts();
        self.mark_state();
    }

    pub(crate) fn focus_shell(&self) {
        if let Some(w) = self.app.get_webview("main") {
            let _ = w.set_focus();
        }
    }

    pub(crate) fn focus_omnibox(&self) {
        self.focus_shell();
        self.ui(UiEvent::FocusOmnibox);
    }

    pub fn close_tab(&mut self, id: TabId) {
        if self.tabs.len() == 1 && self.tabs.get(id).is_some() {
            // Keep the window open: replace the last tab with a new-tab page.
            self.new_tab(false, true);
        }
        let was_active = self.tabs.active_id() == Some(id);
        if self.tabs.close(id, now_ms()).is_none() {
            return;
        }
        self.destroy_view(id);
        self.deny_prompts_for(id);
        if self.layout.element_fullscreen == Some(id) {
            self.on_fullscreen_element(id, false);
        }
        if was_active {
            self.after_activation();
        }
        self.mark_session();
    }

    pub fn close_other_tabs(&mut self, keep: TabId) {
        let others: Vec<TabId> = self.tabs.tabs().iter().filter(|t| t.id != keep && !t.pinned).map(|t| t.id).collect();
        self.activate(keep);
        for id in others {
            self.close_tab(id);
        }
    }

    pub fn reopen_closed(&mut self) {
        if let Some(closed) = self.tabs.pop_closed() {
            let prev = self.tabs.active_id();
            let id = self.tabs.restore_closed(closed, now_ms());
            self.refresh_tab_meta(id);
            if let Some(p) = prev {
                self.on_backgrounded(p);
            }
            self.after_activation();
            self.mark_session();
        }
    }

    pub fn duplicate_tab(&mut self, id: TabId) {
        let Some(t) = self.tabs.get(id).cloned() else { return };
        let url = t.error.map(|e| e.url).unwrap_or(t.url);
        let new_id = self.tabs.create(
            NewTab { url, private: t.private, activate: true, opener: Some(id), title: Some(t.title), favicon: t.favicon, ..Default::default() },
            now_ms(),
        );
        self.refresh_tab_meta(new_id);
        self.on_backgrounded(id);
        self.after_activation();
        self.mark_session();
    }

    pub fn set_pinned(&mut self, id: TabId, pinned: bool) {
        if self.tabs.set_pinned(id, pinned) {
            self.mark_session();
        }
    }

    pub fn move_tab(&mut self, id: TabId, index: usize) {
        if self.tabs.move_to(id, index) {
            self.mark_session();
        }
    }

    pub fn set_muted(&mut self, id: TabId, muted: bool) {
        if self.views.contains_key(&id) {
            let _ = self.app.run_on_main_thread(move || native::set_muted(id, muted));
        }
    }

    /// Omnibox / programmatic navigation of an existing tab.
    pub fn navigate(&mut self, id: TabId, input: String) {
        let Some(resolved) = resolve_input(&input, &self.settings.search_template()) else { return };
        if self.tabs.get(id).is_none() {
            return;
        }
        if id == self.tabs.active_id().unwrap_or_default() {
            self.close_find();
        }
        match resolved {
            Resolved::Internal(url) => {
                // Internal pages are rendered by the shell; the tab no longer needs a webview.
                self.destroy_view(id);
                if let Some(t) = self.tabs.get_mut(id) {
                    t.url = url;
                    t.loading = false;
                    t.error = None;
                    t.can_go_back = false;
                    t.can_go_forward = false;
                    t.lifecycle = Lifecycle::Unloaded;
                    t.audible = false;
                    t.heavy = false;
                }
                self.refresh_tab_meta(id);
                self.focus_shell();
            }
            Resolved::Web(url) => {
                if let Some(t) = self.tabs.get_mut(id) {
                    t.url = url.clone();
                    t.error = None;
                    t.loading = true;
                    if is_internal(&t.title) || t.title == internal_title(NEW_TAB_URL) {
                        t.title.clear();
                    }
                }
                self.refresh_tab_meta(id);
                if self.views.contains_key(&id) {
                    let _ = self.app.run_on_main_thread(move || native::navigate(id, &url));
                    if let Some(v) = self.views.get(&id) {
                        let _ = v.webview.set_focus();
                    }
                } else if self.tabs.active_id() == Some(id) {
                    self.relayout();
                    if let Some(v) = self.views.get(&id) {
                        let _ = v.webview.set_focus();
                    }
                } else {
                    self.create_view(id, false);
                }
            }
        }
        self.mark_session();
    }

    pub fn nav_action(&mut self, id: TabId, action: NavAction) {
        let Some(tab) = self.tabs.get(id).cloned() else { return };
        if !self.views.contains_key(&id) {
            // Unloaded or internal tab: "reload" means load it.
            if matches!(action, NavAction::Reload | NavAction::HardReload) && !is_internal(&tab.url) {
                if self.tabs.active_id() == Some(id) {
                    self.relayout();
                } else {
                    self.create_view(id, false);
                }
            }
            return;
        }
        if matches!(action, NavAction::Reload | NavAction::HardReload) {
            if let Some(err) = tab.error {
                // Reloading an error page retries the failed URL.
                if let Some(t) = self.tabs.get_mut(id) {
                    t.error = None;
                    t.url = err.url.clone();
                    t.loading = true;
                }
                let _ = self.app.run_on_main_thread(move || native::navigate(id, &err.url));
                self.mark_state();
                return;
            }
        }
        let _ = self.app.run_on_main_thread(move || native::nav_action(id, action));
    }

    pub fn zoom(&mut self, id: TabId, mode: ZoomMode) {
        let Some(tab) = self.tabs.get(id) else { return };
        let factor = match mode {
            ZoomMode::In => zoom_step(tab.zoom, true),
            ZoomMode::Out => zoom_step(tab.zoom, false),
            ZoomMode::Reset => 1.0,
            ZoomMode::Set(f) => f.clamp(0.25, 5.0),
        };
        if self.views.contains_key(&id) {
            let _ = self.app.run_on_main_thread(move || native::set_zoom(id, factor));
        }
    }

    // ---- webviews -----------------------------------------------------------------

    /// Creates the tab's webview. Background tabs are created hidden.
    pub(crate) fn create_view(&mut self, id: TabId, visible: bool) {
        if self.views.contains_key(&id) {
            return;
        }
        let Some(tab) = self.tabs.get(id).cloned() else { return };
        if is_internal(&tab.url) {
            return;
        }
        let Some(window) = self.app.get_window("main") else { return };
        let rect = self.layout.content.unwrap_or_default();
        let builder = crate::window::tab_webview_builder(&self.app, id, tab.private, &self.profile, self.handle.clone());
        let created = window.add_child(builder, LogicalPosition::new(rect.x, rect.y), LogicalSize::new(rect.width.max(1.0), rect.height.max(1.0)));
        match created {
            Ok(webview) => {
                if !visible {
                    let _ = webview.hide();
                }
                let init = native::TabInit {
                    url: tab.url.clone(),
                    save_passwords: self.settings.save_passwords,
                    autofill: self.settings.autofill,
                    private: tab.private,
                };
                let ctl = self.handle.clone();
                if let Err(e) = webview.with_webview(move |pw| native::install_tab(id, pw, ctl, init)) {
                    log::error!("with_webview failed for tab {id}: {e}");
                }
                if !visible && self.settings.gaming_mode && self.settings.gaming_low_memory_target {
                    let _ = self.app.run_on_main_thread(move || native::set_memory_target(id, true));
                }
                log::info!(
                    "tab {id}: webview created ({}), visible={visible}, bounds={}x{} at {},{}",
                    describe_target(&tab.url, tab.private),
                    rect.width,
                    rect.height,
                    rect.x,
                    rect.y
                );
                self.views.insert(id, View { webview, visible, bounds: visible.then_some(rect) });
                if let Some(t) = self.tabs.get_mut(id) {
                    t.lifecycle = Lifecycle::Live;
                    t.loading = true;
                }
                self.mark_state();
            }
            Err(e) => {
                log::error!("tab {id}: failed to create webview: {e}");
                if let Some(t) = self.tabs.get_mut(id) {
                    t.error = Some(crate::tabs::NavError {
                        url: t.url.clone(),
                        code: "WEBVIEW_FAILED".into(),
                        description: format!("The page could not be created: {e}"),
                    });
                }
                self.mark_state();
            }
        }
    }

    pub(crate) fn destroy_view(&mut self, id: TabId) {
        self.epochs.remove(&id);
        self.suspend_cooldown.remove(&id);
        self.dirty_until.remove(&id);
        if let Some(view) = self.views.remove(&id) {
            log::info!("tab {id}: webview destroyed");
            let _ = self.app.run_on_main_thread(move || native::remove_tab(id));
            if let Err(e) = view.webview.close() {
                log::warn!("closing webview for tab {id} failed: {e}");
            }
        }
        if let Some(t) = self.tabs.get_mut(id) {
            t.lifecycle = Lifecycle::Unloaded;
            t.loading = false;
            t.audible = false;
            t.heavy = false;
        }
    }

    /// Safety net: compares the tab model with the native views after every controller step and repairs
    /// any disagreement (see `consistency.rs`). Each repair is logged, so it points at the bug behind it.
    pub(crate) fn reconcile_views(&mut self) {
        use super::consistency::{find_drift, Drift};
        let views: Vec<(TabId, bool)> = self.views.iter().map(|(id, v)| (*id, v.visible)).collect();
        let drift = find_drift(self.tabs.tabs(), &views, self.tabs.active_id(), self.layout.overlay);
        if drift.is_empty() {
            return;
        }
        let mut relayout = false;
        for d in drift {
            log::warn!("tab/webview drift repaired: {d:?}");
            match d {
                Drift::OrphanView(id) | Drift::InternalWithView(id) => self.destroy_view(id),
                Drift::MissingView(id) => {
                    if let Some(t) = self.tabs.get_mut(id) {
                        t.lifecycle = Lifecycle::Unloaded;
                        t.loading = false;
                    }
                    relayout = true;
                }
                Drift::ExtraVisible(_) => relayout = true,
            }
        }
        if relayout {
            self.relayout();
        }
        self.mark_state();
    }

    /// Positions/shows the active tab's webview and hides all others.
    pub(crate) fn relayout(&mut self) {
        let Some(rect) = self.layout.content else { return };
        let active = self.tabs.active_id();
        if let Some(a) = active {
            let needs_view = self.tabs.get(a).is_some_and(|t| !is_internal(&t.url));
            if needs_view && !self.views.contains_key(&a) && rect.width > 0.0 && rect.height > 0.0 {
                self.create_view(a, true);
            }
        }
        let overlay = self.layout.overlay;
        for (id, view) in self.views.iter_mut() {
            let show = Some(*id) == active && !overlay && rect.width > 0.0 && rect.height > 0.0;
            if show {
                if view.bounds != Some(rect) {
                    let _ = view.webview.set_bounds(Rect {
                        position: LogicalPosition::new(rect.x, rect.y).into(),
                        size: LogicalSize::new(rect.width, rect.height).into(),
                    });
                    view.bounds = Some(rect);
                }
                if !view.visible {
                    let _ = view.webview.show();
                    view.visible = true;
                }
            } else if view.visible {
                let _ = view.webview.hide();
                view.visible = false;
            }
        }
    }

    /// Hides (`open`) or restores the page under shell popups. When restoring,
    /// `focus_page` gives keyboard focus back to the page; the shell passes false
    /// while one of its own text fields still has focus.
    pub fn set_overlay(&mut self, open: bool, focus_page: bool) {
        if self.layout.overlay != open {
            self.layout.overlay = open;
            self.relayout();
            if !open && focus_page {
                if let Some(v) = self.tabs.active_id().and_then(|a| self.views.get(&a)) {
                    if v.visible {
                        let _ = v.webview.set_focus();
                    }
                }
            }
        }
    }

    /// Active tab id if it currently has a live, visible webview.
    pub fn active_live_tab(&self) -> Option<TabId> {
        let id = self.tabs.active_id()?;
        self.views.get(&id).filter(|v| v.visible).map(|_| id)
    }

    /// Active tab id if it has a webview at all, shown or hidden. A shell popup hides the page while it is open,
    /// and filling a saved login from that popup must still reach the page underneath.
    pub fn active_tab_with_view(&self) -> Option<TabId> {
        let id = self.tabs.active_id()?;
        self.views.contains_key(&id).then_some(id)
    }

    pub fn toggle_browser_fullscreen(&mut self) {
        self.layout.browser_fullscreen = !self.layout.browser_fullscreen;
        self.apply_window_fullscreen();
        self.mark_state();
    }

    pub(crate) fn apply_window_fullscreen(&self) {
        let want = self.layout.browser_fullscreen || self.layout.element_fullscreen.is_some();
        if let Some(w) = self.app.get_window("main") {
            let _ = w.set_fullscreen(want);
        }
    }

    // ---- commands -----------------------------------------------------------------

    pub fn run_command(&mut self, cmd: Command) {
        let active = self.tabs.active_id();
        match cmd {
            Command::NewTab => {
                self.new_tab(false, true);
            }
            Command::NewPrivateTab => {
                self.new_tab(true, true);
            }
            Command::CloseTab => {
                if let Some(a) = active {
                    self.close_tab(a);
                }
            }
            Command::ReopenTab => self.reopen_closed(),
            Command::NextTab | Command::PrevTab => {
                let prev = active;
                if let Some(id) = self.tabs.cycle(cmd == Command::NextTab, now_ms()) {
                    if prev != Some(id) {
                        if let Some(p) = prev {
                            self.on_backgrounded(p);
                        }
                        self.after_activation();
                        self.mark_session();
                    }
                }
            }
            Command::SelectTab(n) => {
                let target = if n >= 9 { self.tabs.tabs().last().map(|t| t.id) } else { self.tabs.tabs().get(n as usize - 1).map(|t| t.id) };
                if let Some(id) = target {
                    self.activate(id);
                }
            }
            Command::FocusOmnibox => self.focus_omnibox(),
            Command::Reload => {
                if let Some(a) = active {
                    self.nav_action(a, NavAction::Reload);
                }
            }
            Command::HardReload => {
                if let Some(a) = active {
                    self.nav_action(a, NavAction::HardReload);
                }
            }
            Command::Back => {
                if let Some(a) = active {
                    self.nav_action(a, NavAction::Back);
                }
            }
            Command::Forward => {
                if let Some(a) = active {
                    self.nav_action(a, NavAction::Forward);
                }
            }
            Command::Stop => {
                if let Some(a) = active {
                    self.nav_action(a, NavAction::Stop);
                }
            }
            Command::Find => {
                if self.active_live_tab().is_some() {
                    self.focus_shell();
                    self.ui(UiEvent::OpenFind);
                }
            }
            Command::FindNext => self.find_step(true),
            Command::FindPrev => self.find_step(false),
            Command::BookmarkPage => {
                if let Some(a) = active {
                    self.toggle_bookmark(a);
                }
            }
            Command::ToggleBookmarksBar => {
                let mut s = self.settings.clone();
                s.show_bookmarks_bar = !s.show_bookmarks_bar;
                let _ = self.update_settings(s);
            }
            Command::OpenHistory => self.open_internal("history"),
            Command::OpenDownloads => self.open_internal("downloads"),
            Command::OpenBookmarks => self.open_internal("bookmarks"),
            Command::OpenSettings => self.open_internal("settings"),
            Command::OpenPerformance => self.open_internal("performance"),
            Command::OpenGaming => self.open_internal("gaming"),
            Command::ZoomIn => {
                if let Some(a) = active {
                    self.zoom(a, ZoomMode::In);
                }
            }
            Command::ZoomOut => {
                if let Some(a) = active {
                    self.zoom(a, ZoomMode::Out);
                }
            }
            Command::ZoomReset => {
                if let Some(a) = active {
                    self.zoom(a, ZoomMode::Reset);
                }
            }
            Command::Fullscreen => self.toggle_browser_fullscreen(),
            Command::ToggleGamingMode => {
                let mut s = self.settings.clone();
                s.gaming_mode = !s.gaming_mode;
                let on = s.gaming_mode;
                if self.update_settings(s).is_ok() {
                    self.toast(if on { "Gaming Mode on" } else { "Gaming Mode off" });
                }
            }
        }
    }

    pub fn toggle_bookmark(&mut self, id: TabId) {
        let Some(tab) = self.tabs.get(id).cloned() else { return };
        if is_internal(&tab.url) || tab.error.is_some() {
            return;
        }
        let result = if tab.bookmarked {
            self.db.bookmark_delete_url(&tab.url).map(|_| "Bookmark removed")
        } else {
            let title = if tab.title.is_empty() { tab.url.clone() } else { tab.title.clone() };
            self.db.bookmark_add(crate::db::bookmarks::BAR_ID, &title, Some(&tab.url), now_ms()).map(|_| "Added to bookmarks bar")
        };
        match result {
            Ok(msg) => {
                self.toast(msg);
                self.on_bookmarks_changed();
            }
            Err(e) => self.toast(format!("Bookmark failed: {e}")),
        }
    }

    /// Bookmarks changed: refresh star state on every tab and tell the shell.
    pub(crate) fn on_bookmarks_changed(&mut self) {
        let ids: Vec<TabId> = self.tabs.tabs().iter().map(|t| t.id).collect();
        for id in ids {
            self.refresh_tab_meta(id);
        }
        self.mark_bookmarks();
        self.mark_state();
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ZoomMode {
    In,
    Out,
    Reset,
    Set(f64),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_description_never_contains_paths_queries_or_private_pages() {
        assert_eq!(describe_target("https://example.com/secret/path?token=abc#frag", false), "https://example.com");
        assert_eq!(describe_target("https://example.com/secret", true), "private tab");
        assert_eq!(describe_target("not a url", false), "unparseable address");
    }
}
