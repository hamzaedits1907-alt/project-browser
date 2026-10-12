//! Browser windows.
//!
//! A window is a shell webview plus its own tab strip, the native webviews of its tabs, its page-area layout,
//! find bar and pending UI updates. The controller keeps the state of the *current* window in its own fields
//! (`tabs`, `views`, `layout`, `find`, `shell_ready`, `dirty`, `private_window`) and the state of every other window
//! in `windows`. Every entry point that belongs to a window first selects it ([`Controller::select_window`] or
//! [`Controller::select_tab_window`]), which swaps that window's state into those fields. All other controller
//! code therefore keeps working on exactly one window, and a command can never touch a tab of another window by
//! accident: tab ids come from one shared counter, so an id names one tab in the whole browser.
//!
//! * Windows share the profile (cookies and site storage), the database, downloads and settings.
//! * A private window holds private tabs only and is never saved.
//! * Moving a tab to another window re-parents its native webview, so the page keeps running (no reload).

use std::collections::HashMap;

use serde::Serialize;
use tauri::Manager;

use super::{Controller, Dirty, FindState, Layout, RestorePlan, Session, SessionTab, View, WindowGeometry};
use crate::error::{now_ms, Error, Result};
use crate::tabs::{Tab, TabId, TabStrip};

/// How many closed windows are remembered for "reopen closed window".
const MAX_CLOSED_WINDOWS: usize = 5;

/// The state of a window that is not the current one.
pub(crate) struct WindowState {
    pub(super) tabs: TabStrip,
    pub(super) views: HashMap<TabId, View>,
    pub(super) layout: Layout,
    pub(super) find: Option<FindState>,
    pub(super) shell_ready: bool,
    pub(super) dirty: Dirty,
    pub(super) private: bool,
}

/// A closed window, kept so it can be brought back.
pub(crate) struct ClosedWindow {
    pub(super) session: Session,
}

/// What to put in a new window.
pub(super) struct WindowSpec {
    pub tabs: Vec<SessionTab>,
    pub active: usize,
    pub geometry: Option<WindowGeometry>,
    pub private: bool,
    /// Create no tab at all (the caller is about to move one in).
    pub empty: bool,
    pub groups: Vec<crate::tabs::TabGroup>,
}

/// One window as the "move to window" menu lists it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowBrief {
    pub label: String,
    pub title: String,
    pub private: bool,
    pub tab_count: usize,
    /// The window that asked.
    pub current: bool,
}

/// A saved-tab record for a page that has just been asked for.
pub(super) fn session_tab(url: String) -> SessionTab {
    SessionTab { url, title: String::new(), pinned: false, favicon: None, group: None }
}

fn tab_title(tabs: &TabStrip) -> String {
    tabs.active().map(|t| if t.title.trim().is_empty() { t.url.clone() } else { t.title.clone() }).unwrap_or_default()
}

impl Controller {
    // ---- selecting a window --------------------------------------------------------------

    fn swap_state(&mut self, other: &mut WindowState) {
        std::mem::swap(&mut self.tabs, &mut other.tabs);
        std::mem::swap(&mut self.views, &mut other.views);
        std::mem::swap(&mut self.layout, &mut other.layout);
        std::mem::swap(&mut self.find, &mut other.find);
        std::mem::swap(&mut self.shell_ready, &mut other.shell_ready);
        std::mem::swap(&mut self.dirty, &mut other.dirty);
        std::mem::swap(&mut self.private_window, &mut other.private);
    }

    /// Makes `label` the current window. Returns false when there is no such window.
    pub(crate) fn select_window(&mut self, label: &str) -> bool {
        if self.current == label {
            return true;
        }
        let Some(mut incoming) = self.windows.remove(label) else { return false };
        self.swap_state(&mut incoming);
        let previous = std::mem::replace(&mut self.current, label.to_string());
        self.windows.insert(previous, incoming);
        true
    }

    /// Makes the window that owns tab `id` the current one. Returns false when no window has that tab.
    pub(crate) fn select_tab_window(&mut self, id: TabId) -> bool {
        if self.tabs.get(id).is_some() {
            return true;
        }
        let owner = self.windows.iter().find(|(_, w)| w.tabs.get(id).is_some()).map(|(label, _)| label.clone());
        match owner {
            Some(label) => self.select_window(&label),
            None => false,
        }
    }

    /// Runs `f` once for every window, then goes back to the window that was current.
    pub(crate) fn each_window(&mut self, mut f: impl FnMut(&mut Controller)) {
        if self.windows.is_empty() {
            f(self);
            return;
        }
        let keep = self.current.clone();
        for label in self.window_order.clone() {
            if self.select_window(&label) {
                f(self);
            }
        }
        self.select_window(&keep);
    }

    /// Every tab of every window.
    pub(crate) fn all_tabs(&self) -> Vec<Tab> {
        let mut all = self.tabs.tabs().to_vec();
        for w in self.windows.values() {
            all.extend(w.tabs.tabs().iter().cloned());
        }
        all
    }

    /// Windows in creation order, for the "move to window" menu.
    pub fn windows_brief(&self, asking: &str) -> Vec<WindowBrief> {
        self.window_order
            .iter()
            .map(|label| {
                let (title, private, tab_count) = if *label == self.current {
                    (tab_title(&self.tabs), self.private_window, self.tabs.len())
                } else {
                    match self.windows.get(label) {
                        Some(w) => (tab_title(&w.tabs), w.private, w.tabs.len()),
                        None => (String::new(), false, 0),
                    }
                };
                WindowBrief { label: label.clone(), title, private, tab_count, current: label == asking }
            })
            .collect()
    }

    /// The private flag a new tab gets in the current window.
    pub(crate) fn tab_is_private(&self, requested: bool) -> bool {
        requested || self.private_window
    }

    /// Links opened from outside (another program, a second launch) go to the window the user last used.
    pub fn open_url_in_focused_window(&mut self, url: String) {
        let target = self.last_focused.clone();
        if !self.select_window(&target) {
            let first = self.window_order.first().cloned().unwrap_or_default();
            self.select_window(&first);
        }
        self.open_url(url, true);
    }

    // ---- creating windows ----------------------------------------------------------------

    /// Where a new window goes when nobody said: a little down and right of the current one.
    fn cascade_geometry(&self) -> Option<WindowGeometry> {
        let v = self.layout.viewport;
        if v.maximized || v.width < 640.0 || v.height < 420.0 {
            return None;
        }
        Some(WindowGeometry { x: Some(v.screen_x + 32.0), y: Some(v.screen_y + 32.0), width: v.width, height: v.height, maximized: false })
    }

    /// Creates the native window and an empty state for it. The current window does not change.
    fn create_window(&mut self, private: bool, geometry: Option<WindowGeometry>) -> Option<String> {
        let label = format!("win-{}", self.next_window);
        self.next_window += 1;
        let geometry = geometry.or_else(|| self.cascade_geometry());
        let shell = match crate::window::build_shell_window(&self.app, &self.profile, &label, geometry, private) {
            Ok(w) => w,
            Err(e) => {
                log::error!("could not create window {label}: {e}");
                self.toast("Could not open a new window.");
                return None;
            }
        };
        let ctl = self.handle.clone();
        let shell_label = label.clone();
        let devtools = cfg!(debug_assertions);
        if let Err(e) = shell.with_webview(move |pw| crate::native::install_shell(pw, ctl, devtools, shell_label)) {
            log::error!("could not configure window {label}: {e}");
        }
        self.windows.insert(
            label.clone(),
            WindowState {
                tabs: TabStrip::with_shared_ids(self.tab_ids.clone()),
                views: HashMap::new(),
                layout: Layout::default(),
                find: None,
                shell_ready: false,
                dirty: Dirty::default(),
                private,
            },
        );
        self.window_order.push(label.clone());
        log::info!("window {label} created{}", if private { " (private)" } else { "" });
        self.mark_all_windows_state();
        Some(label)
    }

    /// Opens a window with the given tabs. The current window does not change; the new one shows itself (and takes
    /// focus) once its shell has loaded.
    pub(super) fn open_window(&mut self, spec: WindowSpec) -> Option<String> {
        let label = self.create_window(spec.private, spec.geometry)?;
        let keep = self.current.clone();
        if !self.select_window(&label) {
            return None;
        }
        if !spec.empty {
            if spec.tabs.is_empty() {
                self.new_tab(false, false);
            } else {
                let plan = RestorePlan { tabs: spec.tabs, active: Some(spec.active), groups: spec.groups };
                self.restore_window_tabs(plan, now_ms());
                if self.tabs.is_empty() {
                    self.new_tab(false, false);
                }
            }
        }
        self.select_window(&keep);
        self.mark_session();
        Some(label)
    }

    pub fn new_window(&mut self, private: bool) -> Option<String> {
        self.open_window(WindowSpec { tabs: Vec::new(), active: 0, geometry: None, private, empty: false, groups: Vec::new() })
    }

    pub fn reopen_window(&mut self) -> bool {
        let Some(closed) = self.closed_windows.pop() else { return false };
        let s = closed.session;
        self.last_close_was_window = false;
        self.open_window(WindowSpec { tabs: s.tabs, active: s.active, geometry: s.geometry, private: false, empty: false, groups: s.groups })
            .is_some()
    }

    fn mark_all_windows_state(&mut self) {
        self.dirty.state = true;
        for w in self.windows.values_mut() {
            w.dirty.state = true;
        }
    }

    // ---- closing windows -----------------------------------------------------------------

    /// The user closed a window (or the last tab was moved out of it). Saves it for "reopen", keeps the webviews of
    /// running downloads alive, removes its state and finally destroys the native window. When it was the last
    /// window the whole browser exits instead.
    pub fn close_window(&mut self, label: &str) {
        if !self.window_order.iter().any(|l| l == label) {
            return;
        }
        if self.window_order.len() <= 1 {
            self.prepare_exit();
            self.app.exit(0);
            return;
        }
        let Some(survivor) = self.window_order.iter().find(|l| l.as_str() != label).cloned() else { return };
        self.select_window(label);
        if let Some(session) = self.window_session().filter(|s| !s.tabs.is_empty()) {
            self.closed_windows.push(ClosedWindow { session });
            if self.closed_windows.len() > MAX_CLOSED_WINDOWS {
                self.closed_windows.remove(0);
            }
            self.last_close_was_window = true;
        }
        // Each tab's webview goes with the window, except those a running download still needs.
        let keep_alive_in = self.app.get_window(&survivor);
        let ids: Vec<TabId> = self.tabs.tabs().iter().map(|t| t.id).collect();
        for id in ids {
            self.deny_prompts_for(id);
            let running = self.tab_has_active_download(id);
            match (running, self.views.remove(&id), keep_alive_in.as_ref()) {
                (true, Some(view), Some(target)) => {
                    let _ = view.webview.hide();
                    match view.webview.reparent(target) {
                        Ok(()) => {
                            log::info!("tab {id}: its window closed, its webview stays hidden until its download finishes");
                            self.lingering.insert(id, View { visible: false, bounds: None, ..view });
                        }
                        Err(e) => {
                            log::warn!("tab {id}: could not keep its webview for a running download: {e}");
                            let _ = view.webview.close();
                        }
                    }
                }
                (_, Some(view), _) => {
                    let _ = view.webview.close();
                }
                _ => {}
            }
            self.epochs.remove(&id);
            self.dirty_until.remove(&id);
            self.suspend_cooldown.remove(&id);
            let _ = self.app.run_on_main_thread(move || crate::native::remove_tab(id));
        }
        // The window's state is dropped; another window becomes the current one.
        self.select_window(&survivor);
        self.windows.remove(label);
        self.window_order.retain(|l| l != label);
        if self.last_focused == label {
            self.last_focused = survivor.clone();
        }
        crate::window::unregister_shell(label);
        let shell_label = label.to_string();
        let _ = self.app.run_on_main_thread(move || crate::native::remove_shell(&shell_label));
        if let Some(w) = self.app.get_window(label) {
            if let Err(e) = w.destroy() {
                log::warn!("closing window {label} failed: {e}");
            }
        }
        log::info!("window {label} closed, {} left", self.window_order.len());
        self.mark_all_windows_state();
        self.mark_session();
    }

    // ---- moving tabs between windows -----------------------------------------------------

    /// Moves a tab to another window, or to a new one when `target` is `None`. The page keeps running: its native
    /// webview is re-parented, not recreated. Returns the label of the window the tab is now in.
    pub fn move_tab_to_window(&mut self, tab_id: TabId, target: Option<String>) -> Result<String> {
        self.move_tab_to_window_at(tab_id, target, None, None)
    }

    /// Like `move_tab_to_window`, placing the tab at `index` of the target strip (a drop position) and, for a new
    /// window, putting that window at `geometry` (a tab dragged out into empty space).
    pub fn move_tab_to_window_at(
        &mut self,
        tab_id: TabId,
        target: Option<String>,
        index: Option<usize>,
        geometry: Option<WindowGeometry>,
    ) -> Result<String> {
        if !self.select_tab_window(tab_id) {
            return Err(Error::msg("That tab is no longer open."));
        }
        let source = self.current.clone();
        let tab = self.tabs.get(tab_id).cloned().ok_or_else(|| Error::msg("That tab is no longer open."))?;
        let target_label = match target {
            Some(t) if t == source => return Ok(source),
            Some(t) => {
                let (exists, private) = if t == self.current {
                    (true, self.private_window)
                } else {
                    match self.windows.get(&t) {
                        Some(w) => (true, w.private),
                        None => (false, false),
                    }
                };
                if !exists {
                    return Err(Error::msg("That window is closed."));
                }
                if private && !tab.private {
                    return Err(Error::msg("Private windows only hold private tabs."));
                }
                t
            }
            None => {
                if self.tabs.len() <= 1 {
                    return Err(Error::msg("This is the only tab in its window."));
                }
                let geometry = geometry.or_else(|| self.cascade_geometry());
                self.create_window(tab.private, geometry).ok_or_else(|| Error::msg("Could not open a new window."))?
            }
        };

        // Leave the source window.
        if self.find.as_ref().is_some_and(|f| f.tab_id == tab_id) {
            self.find = None;
            self.mark_find();
        }
        if self.layout.element_fullscreen == Some(tab_id) {
            self.on_fullscreen_element(tab_id, false);
        }
        let was_active = self.tabs.active_id() == Some(tab_id);
        let view = self.views.remove(&tab_id);
        let Some(moved) = self.tabs.detach(tab_id, now_ms()) else { return Err(Error::msg("That tab is no longer open.")) };
        if let Some(v) = &view {
            let _ = v.webview.hide();
        }
        let source_empty = self.tabs.is_empty();
        if was_active && !source_empty {
            self.after_activation();
        }
        self.mark_prompts();
        self.mark_state();

        // Join the target window.
        if !self.select_window(&target_label) {
            return Err(Error::msg("That window is closed."));
        }
        self.tabs.attach(moved, true, now_ms());
        if let Some(i) = index {
            self.tabs.move_to(tab_id, i);
        }
        if let Some(v) = view {
            let target_window = self.app.get_window(&target_label);
            match target_window.as_ref().map(|w| v.webview.reparent(w)) {
                Some(Ok(())) => {
                    self.views.insert(tab_id, View { visible: false, bounds: None, ..v });
                }
                other => {
                    log::warn!("tab {tab_id}: its webview could not move to window {target_label} ({other:?}); the page will reload there");
                    let _ = v.webview.close();
                    let _ = self.app.run_on_main_thread(move || crate::native::remove_tab(tab_id));
                    if let Some(t) = self.tabs.get_mut(tab_id) {
                        t.lifecycle = crate::tabs::Lifecycle::Unloaded;
                        t.loading = false;
                    }
                }
            }
        }
        self.after_activation();
        self.mark_prompts();
        self.mark_state();
        self.mark_session();
        log::info!("tab {tab_id} moved from {source} to {target_label}");

        if source_empty {
            // A window left without tabs closes, as in other browsers.
            self.close_window(&source);
            self.select_window(&target_label);
        }
        self.mark_all_windows_state();
        Ok(target_label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_title_of_a_window_is_its_active_tab() {
        let mut strip = TabStrip::new();
        assert_eq!(tab_title(&strip), "");
        let a = strip.create(crate::tabs::NewTab { url: "https://a.example/".into(), activate: true, ..Default::default() }, 0);
        assert_eq!(tab_title(&strip), "https://a.example/", "no title yet: the address");
        strip.get_mut(a).unwrap().title = "A page".into();
        assert_eq!(tab_title(&strip), "A page");
    }
}
