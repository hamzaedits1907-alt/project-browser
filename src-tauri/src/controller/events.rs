//! Reactions to WebView2 events, plus permission prompts and find-in-page.

use super::{Controller, FindState, PermissionPrompt};
use crate::db::history::is_recordable;
use crate::error::now_ms;
use crate::native;
use crate::navigation::origin_of;
use crate::tabs::{NavError, Security, TabId};

/// URLs WebView2 reports for our own NavigateToString error pages.
fn is_error_document(uri: &str) -> bool {
    uri.is_empty() || uri == "about:blank" || uri.starts_with("data:")
}

/// Every tab webview is created on `about:blank` and then navigated to the real URL once the event
/// hooks are installed. Events for that first blank document can still arrive after the real
/// navigation was requested; they must not overwrite the tab's address or end its loading state.
fn is_stale_blank(current_url: &str, reported: &str) -> bool {
    reported == "about:blank" && current_url != "about:blank"
}

impl Controller {
    pub fn on_nav_starting(&mut self, id: TabId, uri: String) {
        let Some(tab) = self.tabs.get_mut(id) else { return };
        if tab.error.is_some() && is_error_document(&uri) {
            return;
        }
        tab.loading = true;
        tab.error = None;
        tab.blocked_popup = None;
        if !uri.is_empty() && uri != "about:blank" {
            tab.url = uri;
            tab.security = Security::for_url(&tab.url);
        }
        self.refresh_tab_meta(id);
        self.mark_state();
    }

    pub fn on_source_changed(&mut self, id: TabId, uri: String, new_document: bool) {
        let Some(tab) = self.tabs.get_mut(id) else { return };
        if (tab.error.is_some() && is_error_document(&uri)) || uri.is_empty() || is_stale_blank(&tab.url, &uri) {
            return;
        }
        let changed = tab.url != uri;
        tab.url = uri.clone();
        let (private, title) = (tab.private, tab.title.clone());
        self.refresh_tab_meta(id);
        // Same-document navigations (pushState in SPAs) never fire NavigationCompleted.
        if changed && !new_document && !private && is_recordable(&uri) {
            if let Err(e) = self.db.record_visit(&uri, &title, now_ms()) {
                log::warn!("history write failed: {e}");
            }
        }
        if id == self.tabs.active_id().unwrap_or_default() {
            self.close_find();
        }
        self.mark_session();
    }

    pub fn on_history_changed(&mut self, id: TabId, back: bool, forward: bool) {
        if let Some(t) = self.tabs.get_mut(id) {
            t.can_go_back = back;
            t.can_go_forward = forward;
            self.mark_state();
        }
    }

    pub fn on_nav_completed(&mut self, id: TabId, success: bool, error: Option<(String, String)>, source: String, title: String) {
        let Some(tab) = self.tabs.get_mut(id) else { return };
        if success && tab.error.is_none() && is_stale_blank(&tab.url, &source) {
            return;
        }
        tab.loading = false;
        if success {
            if tab.error.is_some() && is_error_document(&source) {
                // Our error page finished rendering; keep showing the failed URL.
                self.mark_state();
                return;
            }
            tab.error = None;
            if !source.is_empty() {
                tab.url = source.clone();
            }
            if !title.is_empty() {
                tab.title = title.clone();
            }
            let private = tab.private;
            if self.startup.first_page_ms.is_none() && is_recordable(&source) {
                if let Some(start) = self.startup.process_start {
                    self.startup.first_page_ms = Some(now_ms() - start);
                }
            }
            if !private && is_recordable(&source) {
                if let Err(e) = self.db.record_visit(&source, &title, now_ms()) {
                    log::warn!("history write failed: {e}");
                }
            }
            self.refresh_tab_meta(id);
            self.mark_session();
            return;
        }
        let Some((code, description)) = error else {
            self.mark_state();
            return;
        };
        // Cancelled/aborted navigations (Stop, superseded loads, link that became a
        // download) keep the current page instead of showing an error.
        if code == "CANCELED" || code == "CONNECTION_ABORTED" {
            self.mark_state();
            return;
        }
        log::warn!("tab {id}: navigation failed ({code})");
        let failed_url = tab.url.clone();
        tab.error = Some(NavError { url: failed_url.clone(), code: code.clone(), description: description.clone() });
        let html = crate::error_page::render(&failed_url, &code, &description);
        let _ = self.app.run_on_main_thread(move || native::show_error_page(id, html));
        self.mark_state();
    }

    pub fn on_title(&mut self, id: TabId, title: String) {
        let Some(tab) = self.tabs.get_mut(id) else { return };
        if tab.error.is_some() {
            return;
        }
        tab.title = title.clone();
        let (url, private) = (tab.url.clone(), tab.private);
        if !private && is_recordable(&url) {
            let _ = self.db.update_title(&url, &title);
        }
        self.mark_session();
    }

    pub fn on_favicon(&mut self, id: TabId, uri: String) {
        if let Some(t) = self.tabs.get_mut(id) {
            let ok = uri.starts_with("https://") || uri.starts_with("http://") || uri.starts_with("data:image/");
            t.favicon = ok.then_some(uri);
            self.mark_session();
        }
    }

    pub fn on_fullscreen_element(&mut self, id: TabId, fullscreen: bool) {
        if fullscreen {
            if self.tabs.active_id() != Some(id) {
                return;
            }
            self.layout.element_fullscreen = Some(id);
        } else if self.layout.element_fullscreen == Some(id) {
            self.layout.element_fullscreen = None;
        } else {
            return;
        }
        self.apply_window_fullscreen();
        self.mark_state();
    }

    pub fn on_zoom(&mut self, id: TabId, zoom: f64) {
        if let Some(t) = self.tabs.get_mut(id) {
            t.zoom = (zoom * 100.0).round() / 100.0;
            self.mark_state();
        }
    }

    pub fn on_audio(&mut self, id: TabId, playing: bool, muted: bool) {
        if let Some(t) = self.tabs.get_mut(id) {
            t.audible = playing;
            t.muted = muted;
            self.mark_state();
        }
    }

    pub fn on_cert_error(&mut self, id: TabId) {
        if let Some(t) = self.tabs.get_mut(id) {
            t.security = Security::CertError;
            self.mark_state();
        }
    }

    pub fn on_crashed(&mut self, id: TabId) {
        if let Some(t) = self.tabs.get_mut(id) {
            t.loading = false;
            t.audible = false;
            t.error = Some(NavError {
                url: t.url.clone(),
                code: "CRASHED".into(),
                description: "This page's process stopped (it may have crashed or run out of memory). Reload to try again.".into(),
            });
            self.mark_state();
        }
    }

    pub fn on_native_error(&mut self, id: TabId, message: String) {
        if let Some(t) = self.tabs.get_mut(id) {
            t.loading = false;
            t.error = Some(NavError { url: t.url.clone(), code: "WEBVIEW_ERROR".into(), description: message });
            self.mark_state();
        }
    }

    // ---- permissions --------------------------------------------------------------

    pub fn on_permission_request(&mut self, req: u64, id: TabId, uri: String, kind: &'static str) {
        let Some(tab) = self.tabs.get(id) else {
            let _ = self.app.run_on_main_thread(move || native::respond_permission(req, None));
            return;
        };
        let Some(origin) = origin_of(&uri) else {
            let _ = self.app.run_on_main_thread(move || native::respond_permission(req, None));
            return;
        };
        // Private tabs never use or create remembered decisions.
        if !tab.private {
            if let Ok(Some(allow)) = self.db.permission_get(&origin, kind) {
                let _ = self.app.run_on_main_thread(move || native::respond_permission(req, Some(allow)));
                return;
            }
        }
        self.prompts.push(PermissionPrompt { id: req, tab_id: id, origin, kind: kind.to_string() });
        self.mark_prompts();
    }

    pub fn respond_prompt(&mut self, req: u64, allow: bool, remember: bool) {
        let Some(pos) = self.prompts.iter().position(|p| p.id == req) else { return };
        let prompt = self.prompts.remove(pos);
        self.select_tab_window(prompt.tab_id);
        let private = self.tabs.get(prompt.tab_id).map(|t| t.private).unwrap_or(true);
        if remember && !private {
            if let Err(e) = self.db.permission_set(&prompt.origin, &prompt.kind, allow, now_ms()) {
                log::warn!("could not store permission: {e}");
            }
        }
        let _ = self.app.run_on_main_thread(move || native::respond_permission(req, Some(allow)));
        self.mark_prompts();
    }

    pub(crate) fn deny_prompts_for(&mut self, id: TabId) {
        let (deny, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut self.prompts).into_iter().partition(|p| p.tab_id == id);
        self.prompts = keep;
        for p in deny {
            let req = p.id;
            let _ = self.app.run_on_main_thread(move || native::respond_permission(req, None));
        }
        self.mark_prompts();
    }

    // ---- find in page -------------------------------------------------------------

    pub fn find_query(&mut self, query: String, case_sensitive: bool) {
        let Some(id) = self.active_live_tab() else { return };
        if query.is_empty() {
            let _ = self.app.run_on_main_thread(move || native::find_stop(id));
            self.find = Some(FindState { tab_id: id, ..Default::default() });
            self.mark_find();
            return;
        }
        self.find = Some(FindState { tab_id: id, query: query.clone(), ..Default::default() });
        let ctl = self.handle.clone();
        let _ = self.app.run_on_main_thread(move || native::find_start(id, query, case_sensitive, ctl));
        self.mark_find();
    }

    pub fn find_step(&mut self, forward: bool) {
        if let Some(f) = &self.find {
            if !f.query.is_empty() {
                let id = f.tab_id;
                let _ = self.app.run_on_main_thread(move || native::find_step(id, forward));
            }
        }
    }

    pub fn close_find(&mut self) {
        if let Some(f) = self.find.take() {
            let id = f.tab_id;
            let _ = self.app.run_on_main_thread(move || native::find_stop(id));
            self.mark_find();
        }
    }

    pub fn on_find_update(&mut self, id: TabId, matches: i32, active: i32) {
        if let Some(f) = self.find.as_mut().filter(|f| f.tab_id == id) {
            f.matches = matches.max(0);
            f.active = active;
            self.mark_find();
        }
    }

    pub fn on_find_unavailable(&mut self, id: TabId) {
        if let Some(f) = self.find.as_mut().filter(|f| f.tab_id == id) {
            f.unavailable = true;
            self.mark_find();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_blank_document_never_replaces_the_requested_url() {
        assert!(is_stale_blank("https://example.com/", "about:blank"));
        assert!(!is_stale_blank("about:blank", "about:blank"), "navigating to about:blank on purpose is fine");
        assert!(!is_stale_blank("https://example.com/", "https://example.com/"));
    }

    #[test]
    fn error_documents_are_recognised() {
        assert!(is_error_document(""));
        assert!(is_error_document("about:blank"));
        assert!(is_error_document("data:text/html;charset=utf-8,x"));
        assert!(!is_error_document("https://example.com/"));
    }
}
