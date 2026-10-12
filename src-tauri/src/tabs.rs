//! Tab strip model: ordering, pinning, activation and the reopen stack.
//!
//! This module holds no webview handles; the controller maps tab ids to
//! webviews. Keeping it pure lets the trickiest browser-state rules be tested.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub type TabId = u64;

/// How "alive" a tab's page currently is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Lifecycle {
    /// No webview exists (internal page, restored session tab, or unloaded to free memory).
    #[default]
    Unloaded,
    /// Webview exists and is running.
    Live,
    /// Webview exists but was suspended via WebView2 `TrySuspend`.
    Sleeping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Security {
    #[default]
    Internal,
    Secure,
    Insecure,
    CertError,
    Local,
}

impl Security {
    pub fn for_url(url: &str) -> Security {
        if url.starts_with("browser://") || url == "about:blank" {
            Security::Internal
        } else if url.starts_with("https://") {
            Security::Secure
        } else if url.starts_with("file:") {
            Security::Local
        } else {
            Security::Insecure
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NavError {
    pub url: String,
    pub code: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Tab {
    pub id: TabId,
    pub url: String,
    pub title: String,
    pub favicon: Option<String>,
    pub pinned: bool,
    pub private: bool,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub zoom: f64,
    pub audible: bool,
    pub muted: bool,
    pub lifecycle: Lifecycle,
    pub security: Security,
    pub error: Option<NavError>,
    pub heavy: bool,
    pub bookmarked: bool,
    /// Unix ms when the tab was last the active tab (drives tab sleeping).
    pub last_active: i64,
    /// The address of the last pop-up this page opened without being asked to. It was blocked; the shell offers it.
    pub blocked_popup: Option<String>,
    #[serde(skip)]
    pub opener: Option<TabId>,
}

impl Tab {
    fn new(id: TabId, url: String, pinned: bool, private: bool) -> Tab {
        Tab {
            id,
            security: Security::for_url(&url),
            url,
            title: String::new(),
            favicon: None,
            pinned,
            private,
            loading: false,
            can_go_back: false,
            can_go_forward: false,
            zoom: 1.0,
            audible: false,
            muted: false,
            lifecycle: Lifecycle::Unloaded,
            error: None,
            heavy: false,
            bookmarked: false,
            last_active: 0,
            blocked_popup: None,
            opener: None,
        }
    }
}

/// Enough information to bring a closed tab back.
#[derive(Debug, Clone, PartialEq)]
pub struct ClosedTab {
    pub url: String,
    pub title: String,
    pub favicon: Option<String>,
    pub pinned: bool,
    pub index: usize,
}

#[derive(Debug, Clone, Default)]
pub struct NewTab {
    pub url: String,
    pub pinned: bool,
    pub private: bool,
    pub activate: bool,
    pub opener: Option<TabId>,
    pub title: Option<String>,
    pub favicon: Option<String>,
}

const MAX_CLOSED: usize = 25;

#[derive(Debug, Default)]
pub struct TabStrip {
    tabs: Vec<Tab>,
    active: Option<TabId>,
    closed: Vec<ClosedTab>,
    next_id: TabId,
    /// Shared by all windows of the browser so a tab id names one tab everywhere.
    shared_ids: Option<Arc<AtomicU64>>,
}

impl TabStrip {
    pub fn new() -> Self {
        TabStrip { next_id: 1, ..Default::default() }
    }

    /// A strip whose ids come from a counter shared with the other windows.
    pub fn with_shared_ids(ids: Arc<AtomicU64>) -> Self {
        TabStrip { next_id: 1, shared_ids: Some(ids), ..Default::default() }
    }

    fn alloc_id(&mut self) -> TabId {
        match &self.shared_ids {
            Some(ids) => ids.fetch_add(1, Ordering::SeqCst),
            None => {
                let id = self.next_id;
                self.next_id += 1;
                id
            }
        }
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn active_id(&self) -> Option<TabId> {
        self.active
    }

    pub fn active(&self) -> Option<&Tab> {
        self.active.and_then(|id| self.get(id))
    }

    pub fn get(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    pub fn get_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    pub fn index_of(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    pub fn closed(&self) -> &[ClosedTab] {
        &self.closed
    }

    fn pinned_count(&self) -> usize {
        self.tabs.iter().take_while(|t| t.pinned).count()
    }

    /// Inserts a tab. Pinned tabs go to the end of the pinned group; tabs opened
    /// from another tab go right after the opener (after its earlier children),
    /// everything else goes to the end.
    pub fn create(&mut self, spec: NewTab, now: i64) -> TabId {
        let id = self.alloc_id();
        let mut tab = Tab::new(id, spec.url, spec.pinned, spec.private);
        tab.opener = spec.opener;
        tab.title = spec.title.unwrap_or_default();
        tab.favicon = spec.favicon;

        let pinned_count = self.pinned_count();
        let index = if spec.pinned {
            pinned_count
        } else if let Some(opener_idx) = spec.opener.and_then(|o| self.index_of(o)) {
            let mut i = opener_idx + 1;
            while i < self.tabs.len() && self.tabs[i].opener == spec.opener && !self.tabs[i].pinned {
                i += 1;
            }
            i.max(pinned_count)
        } else {
            self.tabs.len()
        };
        self.tabs.insert(index, tab);
        if spec.activate || self.active.is_none() {
            self.activate(id, now);
        }
        id
    }

    pub fn activate(&mut self, id: TabId, now: i64) -> bool {
        if self.get(id).is_none() {
            return false;
        }
        if let Some(prev) = self.active.and_then(|p| self.get_mut(p)) {
            prev.last_active = now;
        }
        self.active = Some(id);
        if let Some(t) = self.get_mut(id) {
            t.last_active = now;
        }
        true
    }

    /// Removes a tab and picks the next active tab: the opener if this was a
    /// freshly opened child, else the right neighbour, else the left one.
    pub fn close(&mut self, id: TabId, now: i64) -> Option<Tab> {
        self.remove(id, now, true)
    }

    /// Takes a tab out of the strip to put it into another window. Unlike `close`, it is not remembered for reopening.
    pub fn detach(&mut self, id: TabId, now: i64) -> Option<Tab> {
        self.remove(id, now, false)
    }

    /// Puts a tab that came from another window into this strip (pinned tabs join the pinned group, others go last).
    pub fn attach(&mut self, mut tab: Tab, activate: bool, now: i64) {
        tab.opener = None;
        let id = tab.id;
        let index = if tab.pinned { self.pinned_count() } else { self.tabs.len() };
        self.tabs.insert(index, tab);
        if activate || self.active.is_none() {
            self.activate(id, now);
        }
    }

    fn remove(&mut self, id: TabId, now: i64, remember: bool) -> Option<Tab> {
        let index = self.index_of(id)?;
        let tab = self.tabs.remove(index);
        if remember && !tab.private && !(tab.url == crate::navigation::NEW_TAB_URL && tab.title.is_empty()) {
            self.closed.push(ClosedTab { url: tab.url.clone(), title: tab.title.clone(), favicon: tab.favicon.clone(), pinned: tab.pinned, index });
            if self.closed.len() > MAX_CLOSED {
                self.closed.remove(0);
            }
        }
        // Children of the closed tab lose their opener link.
        for t in &mut self.tabs {
            if t.opener == Some(id) {
                t.opener = None;
            }
        }
        if self.active == Some(id) {
            self.active = None;
            let next = tab
                .opener
                .filter(|o| self.get(*o).is_some())
                .or_else(|| self.tabs.get(index).map(|t| t.id))
                .or_else(|| index.checked_sub(1).and_then(|i| self.tabs.get(i)).map(|t| t.id));
            if let Some(next) = next {
                self.activate(next, now);
            }
        }
        Some(tab)
    }

    pub fn pop_closed(&mut self) -> Option<ClosedTab> {
        self.closed.pop()
    }

    /// Reopens a closed tab at (or as near as possible to) its old position.
    pub fn restore_closed(&mut self, closed: ClosedTab, now: i64) -> TabId {
        let id = self.create(
            NewTab {
                url: closed.url,
                pinned: closed.pinned,
                activate: true,
                title: Some(closed.title),
                favicon: closed.favicon,
                ..Default::default()
            },
            now,
        );
        let pinned_count = self.pinned_count();
        let target =
            if closed.pinned { closed.index.min(pinned_count.saturating_sub(1)) } else { closed.index.clamp(pinned_count, self.tabs.len() - 1) };
        self.move_to(id, target);
        id
    }

    /// Moves a tab, keeping it inside its pinned/unpinned group.
    pub fn move_to(&mut self, id: TabId, to: usize) -> bool {
        let Some(from) = self.index_of(id) else { return false };
        let tab = self.tabs.remove(from);
        let pinned_count = self.pinned_count();
        let to = if tab.pinned { to.min(pinned_count) } else { to.clamp(pinned_count, self.tabs.len()) };
        self.tabs.insert(to, tab);
        true
    }

    pub fn set_pinned(&mut self, id: TabId, pinned: bool) -> bool {
        let Some(from) = self.index_of(id) else { return false };
        if self.tabs[from].pinned == pinned {
            return true;
        }
        let mut tab = self.tabs.remove(from);
        tab.pinned = pinned;
        let boundary = self.pinned_count();
        self.tabs.insert(boundary, tab);
        true
    }

    /// Ctrl+Tab / Ctrl+Shift+Tab with wrap-around.
    pub fn cycle(&mut self, forward: bool, now: i64) -> Option<TabId> {
        let len = self.tabs.len();
        if len == 0 {
            return None;
        }
        let cur = self.active.and_then(|a| self.index_of(a)).unwrap_or(0);
        let next = if forward { (cur + 1) % len } else { (cur + len - 1) % len };
        let id = self.tabs[next].id;
        self.activate(id, now);
        Some(id)
    }

    /// Ctrl+1..8 select by position; Ctrl+9 selects the last tab.
    pub fn activate_number(&mut self, n: usize, now: i64) -> Option<TabId> {
        let id = if n >= 9 { self.tabs.last()?.id } else { self.tabs.get(n.checked_sub(1)?)?.id };
        self.activate(id, now);
        Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(url: &str) -> NewTab {
        NewTab { url: url.into(), activate: true, ..Default::default() }
    }

    fn urls(s: &TabStrip) -> Vec<String> {
        s.tabs().iter().map(|t| t.url.clone()).collect()
    }

    #[test]
    fn strips_sharing_a_counter_never_hand_out_the_same_id() {
        let ids = Arc::new(AtomicU64::new(1));
        let mut a = TabStrip::with_shared_ids(ids.clone());
        let mut b = TabStrip::with_shared_ids(ids);
        let seen = [a.create(spec("a1"), 0), b.create(spec("b1"), 0), a.create(spec("a2"), 0), b.create(spec("b2"), 0)];
        let mut sorted = seen.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 4, "{seen:?}");
        // Strips created with `new` keep counting from 1, as before.
        assert_eq!(TabStrip::new().create(spec("x"), 0), 1);
    }

    #[test]
    fn a_tab_can_move_between_strips_with_its_state_and_without_being_remembered_as_closed() {
        let ids = Arc::new(AtomicU64::new(1));
        let mut from = TabStrip::with_shared_ids(ids.clone());
        let mut to = TabStrip::with_shared_ids(ids);
        let a = from.create(spec("a"), 0);
        let b = from.create(spec("b"), 0);
        let c = from.create(spec("c"), 0);
        from.get_mut(b).unwrap().title = "Bee".into();
        from.get_mut(b).unwrap().zoom = 1.5;
        from.activate(b, 1);

        let moved = from.detach(b, 2).unwrap();
        assert_eq!((moved.id, moved.title.as_str(), moved.zoom), (b, "Bee", 1.5));
        assert!(from.closed().is_empty(), "moving is not closing");
        assert_eq!(from.active_id(), Some(c), "the neighbour takes over in the window the tab left");
        assert_eq!(urls(&from), ["a", "c"]);

        to.create(spec("x"), 0);
        to.attach(moved, true, 3);
        assert_eq!(urls(&to), ["x", "b"]);
        assert_eq!(to.active_id(), Some(b));
        assert_eq!(to.get(b).unwrap().title, "Bee");
        assert!(from.get(a).is_some() && from.get(b).is_none());
    }

    #[test]
    fn attached_pinned_tabs_join_the_pinned_group_and_openers_are_dropped() {
        let mut s = TabStrip::new();
        let p = s.create(NewTab { url: "pinned".into(), pinned: true, activate: true, ..Default::default() }, 0);
        let plain = s.create(spec("plain"), 0);
        let mut foreign = Tab::new(99, "pin2".into(), true, false);
        foreign.opener = Some(p);
        s.attach(foreign, false, 1);
        assert_eq!(urls(&s), ["pinned", "pin2", "plain"]);
        assert_eq!(s.get(99).unwrap().opener, None);
        assert_eq!(s.active_id(), Some(plain), "attaching without activate keeps the current tab");
        let mut empty = TabStrip::new();
        empty.attach(Tab::new(7, "only".into(), false, false), false, 1);
        assert_eq!(empty.active_id(), Some(7), "an empty strip always has an active tab");
    }

    #[test]
    fn first_tab_becomes_active_even_in_background() {
        let mut s = TabStrip::new();
        let a = s.create(NewTab { url: "a".into(), ..Default::default() }, 0);
        assert_eq!(s.active_id(), Some(a));
    }

    #[test]
    fn children_open_after_opener_in_order() {
        let mut s = TabStrip::new();
        let a = s.create(spec("a"), 0);
        s.create(spec("b"), 0);
        s.activate(a, 0);
        s.create(NewTab { url: "a1".into(), opener: Some(a), ..Default::default() }, 0);
        s.create(NewTab { url: "a2".into(), opener: Some(a), ..Default::default() }, 0);
        assert_eq!(urls(&s), ["a", "a1", "a2", "b"]);
    }

    #[test]
    fn closing_active_prefers_opener_then_right_then_left() {
        let mut s = TabStrip::new();
        let a = s.create(spec("a"), 0);
        let b = s.create(spec("b"), 0);
        let c = s.create(spec("c"), 0);
        let child = s.create(NewTab { url: "a1".into(), opener: Some(a), activate: true, ..Default::default() }, 0);
        s.close(child, 1);
        assert_eq!(s.active_id(), Some(a));
        s.activate(b, 2);
        s.close(b, 3);
        assert_eq!(s.active_id(), Some(c), "right neighbour");
        s.close(c, 4);
        assert_eq!(s.active_id(), Some(a), "left neighbour when closing last");
        s.close(a, 5);
        assert_eq!(s.active_id(), None);
        assert!(s.is_empty());
    }

    #[test]
    fn closing_background_tab_keeps_active() {
        let mut s = TabStrip::new();
        let a = s.create(spec("a"), 0);
        let b = s.create(spec("b"), 0);
        s.activate(a, 0);
        s.close(b, 0);
        assert_eq!(s.active_id(), Some(a));
    }

    #[test]
    fn reopen_restores_position_and_metadata() {
        let mut s = TabStrip::new();
        s.create(spec("a"), 0);
        let b = s.create(spec("b"), 0);
        s.create(spec("c"), 0);
        s.get_mut(b).unwrap().title = "Bee".into();
        s.close(b, 0);
        let closed = s.pop_closed().unwrap();
        let id = s.restore_closed(closed, 0);
        assert_eq!(urls(&s), ["a", "b", "c"]);
        assert_eq!(s.get(id).unwrap().title, "Bee");
        assert_eq!(s.active_id(), Some(id));
    }

    #[test]
    fn private_and_blank_new_tabs_are_not_reopenable() {
        let mut s = TabStrip::new();
        let p = s.create(NewTab { url: "https://x".into(), private: true, activate: true, ..Default::default() }, 0);
        let n = s.create(spec(crate::navigation::NEW_TAB_URL), 0);
        s.close(p, 0);
        s.close(n, 0);
        assert!(s.closed().is_empty());
    }

    #[test]
    fn closed_stack_is_bounded() {
        let mut s = TabStrip::new();
        for i in 0..40 {
            let id = s.create(spec(&format!("u{i}")), 0);
            s.close(id, 0);
        }
        assert_eq!(s.closed().len(), MAX_CLOSED);
        assert_eq!(s.closed().last().unwrap().url, "u39");
    }

    #[test]
    fn pinning_moves_to_group_boundary_and_moves_stay_in_group() {
        let mut s = TabStrip::new();
        let a = s.create(spec("a"), 0);
        let b = s.create(spec("b"), 0);
        let c = s.create(spec("c"), 0);
        s.set_pinned(c, true);
        assert_eq!(urls(&s), ["c", "a", "b"]);
        s.set_pinned(b, true);
        assert_eq!(urls(&s), ["c", "b", "a"]);
        // Unpinned tab cannot move into the pinned group.
        s.move_to(a, 0);
        assert_eq!(urls(&s), ["c", "b", "a"]);
        // Pinned tab cannot move past the pinned group.
        s.move_to(c, 10);
        assert_eq!(urls(&s), ["b", "c", "a"]);
        s.set_pinned(b, false);
        assert_eq!(urls(&s), ["c", "b", "a"]);
        // New pinned tabs join the end of the pinned group.
        s.create(NewTab { url: "p".into(), pinned: true, ..Default::default() }, 0);
        assert_eq!(urls(&s), ["c", "p", "b", "a"]);
    }

    #[test]
    fn cycling_and_numbers() {
        let mut s = TabStrip::new();
        let a = s.create(spec("a"), 0);
        let b = s.create(spec("b"), 0);
        let c = s.create(spec("c"), 0);
        assert_eq!(s.cycle(true, 0), Some(a), "wraps forward");
        assert_eq!(s.cycle(false, 0), Some(c), "wraps backward");
        assert_eq!(s.activate_number(2, 0), Some(b));
        assert_eq!(s.activate_number(9, 0), Some(c));
        assert_eq!(s.activate_number(7, 0), None);
    }

    #[test]
    fn activation_tracks_last_active_time() {
        let mut s = TabStrip::new();
        let a = s.create(spec("a"), 10);
        let b = s.create(spec("b"), 20);
        assert_eq!(s.get(a).unwrap().last_active, 20, "stamped when it lost focus");
        s.activate(a, 30);
        assert_eq!(s.get(b).unwrap().last_active, 30);
    }

    #[test]
    fn security_classification() {
        assert_eq!(Security::for_url("https://a"), Security::Secure);
        assert_eq!(Security::for_url("http://a"), Security::Insecure);
        assert_eq!(Security::for_url("browser://newtab"), Security::Internal);
        assert_eq!(Security::for_url("file:///C:/x"), Security::Local);
    }
}
