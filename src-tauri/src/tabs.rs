//! Tab strip model: ordering, pinning, activation and the reopen stack.
//!
//! This module holds no webview handles; the controller maps tab ids to
//! webviews. Keeping it pure lets the trickiest browser-state rules be tested.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub type TabId = u64;
pub type GroupId = u32;

/// Colours a tab group can have (the shell maps each to a hue that suits the theme).
pub const GROUP_COLORS: &[&str] = &["grey", "blue", "red", "yellow", "green", "pink", "purple", "cyan", "orange"];

/// A named, coloured run of neighbouring tabs. Its tabs are always next to each other; pinned tabs are never grouped.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TabGroup {
    pub id: GroupId,
    pub title: String,
    pub color: String,
    pub collapsed: bool,
}

fn clean_group_title(title: &str) -> String {
    title.trim().chars().take(40).collect()
}

fn clean_group_color(color: &str) -> String {
    if GROUP_COLORS.contains(&color) {
        color.to_string()
    } else {
        "blue".to_string()
    }
}

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
    /// The tab group this tab belongs to.
    pub group: Option<GroupId>,
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
            group: None,
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
    groups: Vec<TabGroup>,
    next_group: GroupId,
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

    pub fn groups(&self) -> &[TabGroup] {
        &self.groups
    }

    pub fn group(&self, id: GroupId) -> Option<&TabGroup> {
        self.groups.iter().find(|g| g.id == id)
    }

    fn alloc_group(&mut self) -> GroupId {
        let used = self.groups.iter().map(|g| g.id).max().unwrap_or(0);
        self.next_group = self.next_group.max(used) + 1;
        self.next_group
    }

    /// The tabs of a group, in strip order.
    pub fn group_tabs(&self, group: GroupId) -> Vec<TabId> {
        self.tabs.iter().filter(|t| t.group == Some(group)).map(|t| t.id).collect()
    }

    /// Puts the given (unpinned) tabs into a new group, next to each other where the first of them is.
    pub fn group_create(&mut self, ids: &[TabId], title: &str, color: &str) -> Option<GroupId> {
        let mut members: Vec<(usize, TabId)> =
            ids.iter().filter_map(|id| self.index_of(*id).filter(|i| !self.tabs[*i].pinned).map(|i| (i, *id))).collect();
        members.sort_unstable();
        members.dedup();
        let (first, _) = *members.first()?;
        let group = self.alloc_group();
        self.groups.push(TabGroup { id: group, title: clean_group_title(title), color: clean_group_color(color), collapsed: false });
        let mut taken = Vec::new();
        for (_, id) in members.iter().rev() {
            if let Some(i) = self.index_of(*id) {
                taken.push(self.tabs.remove(i));
            }
        }
        taken.reverse();
        let at = first.min(self.tabs.len());
        for (k, mut tab) in taken.into_iter().enumerate() {
            tab.group = Some(group);
            self.tabs.insert(at + k, tab);
        }
        self.normalize_groups();
        Some(group)
    }

    /// Adds a tab to the end of an existing group.
    pub fn group_add(&mut self, id: TabId, group: GroupId) -> bool {
        if self.group(group).is_none() || self.get(id).is_none_or(|t| t.pinned) {
            return false;
        }
        let Some(from) = self.index_of(id) else { return false };
        let mut tab = self.tabs.remove(from);
        tab.group = Some(group);
        let at = self.tabs.iter().rposition(|t| t.group == Some(group)).map(|i| i + 1).unwrap_or(self.tabs.len());
        self.tabs.insert(at, tab);
        self.normalize_groups();
        true
    }

    /// Takes a tab out of its group; it goes right after the group.
    pub fn group_remove_tab(&mut self, id: TabId) -> bool {
        let Some(group) = self.get(id).and_then(|t| t.group) else { return false };
        let Some(from) = self.index_of(id) else { return false };
        let mut tab = self.tabs.remove(from);
        tab.group = None;
        let at = self.tabs.iter().rposition(|t| t.group == Some(group)).map(|i| i + 1).unwrap_or(from.min(self.tabs.len()));
        self.tabs.insert(at, tab);
        self.normalize_groups();
        true
    }

    /// Renames, recolours or collapses a group. Collapsing is refused while the active tab is in the group (the caller
    /// first switches to a tab outside it).
    pub fn group_update(&mut self, group: GroupId, title: Option<&str>, color: Option<&str>, collapsed: Option<bool>) -> bool {
        let active_inside = self.active().is_some_and(|t| t.group == Some(group));
        let Some(g) = self.groups.iter_mut().find(|g| g.id == group) else { return false };
        if let Some(t) = title {
            g.title = clean_group_title(t);
        }
        if let Some(c) = color {
            g.color = clean_group_color(c);
        }
        if let Some(c) = collapsed {
            if c && active_inside {
                return false;
            }
            g.collapsed = c;
        }
        true
    }

    /// Dissolves a group; its tabs stay where they are.
    pub fn group_dissolve(&mut self, group: GroupId) -> bool {
        let before = self.groups.len();
        self.groups.retain(|g| g.id != group);
        for t in &mut self.tabs {
            if t.group == Some(group) {
                t.group = None;
            }
        }
        before != self.groups.len()
    }

    /// Recreates saved groups (session restore). `assignments` maps restored tabs to their saved group ids.
    pub fn restore_groups(&mut self, groups: Vec<TabGroup>, assignments: &[(TabId, GroupId)]) {
        for g in groups {
            if assignments.iter().any(|(_, gid)| *gid == g.id) && self.group(g.id).is_none() {
                self.groups.push(TabGroup { title: clean_group_title(&g.title), color: clean_group_color(&g.color), ..g });
            }
        }
        for (tab, group) in assignments {
            if self.group(*group).is_some() {
                if let Some(t) = self.get_mut(*tab) {
                    t.group = Some(*group);
                }
            }
        }
        self.normalize_groups();
    }

    /// Keeps the group rules after any change: pinned tabs are ungrouped; each group is one run of neighbouring tabs
    /// (tabs cut off from their group's longest run leave it); empty groups disappear; a collapsed group that holds the
    /// active tab opens.
    fn normalize_groups(&mut self) {
        for t in &mut self.tabs {
            if t.pinned {
                t.group = None;
            }
        }
        let ids: Vec<GroupId> = self.groups.iter().map(|g| g.id).collect();
        for g in ids {
            // Runs of this group: (start, length).
            let mut runs: Vec<(usize, usize)> = Vec::new();
            for (i, t) in self.tabs.iter().enumerate() {
                if t.group == Some(g) {
                    match runs.last_mut() {
                        Some((start, len)) if *start + *len == i => *len += 1,
                        _ => runs.push((i, 1)),
                    }
                }
            }
            if runs.len() > 1 {
                let keep = runs.iter().copied().max_by_key(|(_, len)| *len).map(|(start, _)| start);
                for (start, len) in runs {
                    if Some(start) != keep {
                        for t in &mut self.tabs[start..start + len] {
                            t.group = None;
                        }
                    }
                }
            }
        }
        let tabs = &self.tabs;
        self.groups.retain(|g| tabs.iter().any(|t| t.group == Some(g.id)));
        if let Some(active_group) = self.active().and_then(|t| t.group) {
            if let Some(g) = self.groups.iter_mut().find(|g| g.id == active_group) {
                g.collapsed = false;
            }
        }
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
        // A tab opened from a grouped tab joins that group, as in Chrome.
        if !spec.pinned {
            tab.group = spec.opener.and_then(|o| self.get(o)).and_then(|o| o.group);
        }
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
        self.normalize_groups();
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
        // Switching to a tab inside a collapsed group opens the group.
        if let Some(g) = self.get(id).and_then(|t| t.group) {
            if let Some(group) = self.groups.iter_mut().find(|x| x.id == g) {
                group.collapsed = false;
            }
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
        // Group ids belong to a window; a tab joining another window arrives ungrouped.
        tab.group = None;
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
        self.normalize_groups();
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
        // Where it was dropped decides its group: between two tabs of a group it joins that group; away from its own
        // group it leaves it; at the edge of its own group it stays.
        if !self.tabs[to].pinned {
            let left = to.checked_sub(1).and_then(|i| self.tabs.get(i)).and_then(|t| t.group);
            let right = self.tabs.get(to + 1).and_then(|t| t.group);
            match self.tabs[to].group {
                _ if left.is_some() && left == right => self.tabs[to].group = left,
                Some(own) if left != Some(own) && right != Some(own) => self.tabs[to].group = None,
                _ => {}
            }
        }
        self.normalize_groups();
        true
    }

    pub fn set_pinned(&mut self, id: TabId, pinned: bool) -> bool {
        let Some(from) = self.index_of(id) else { return false };
        if self.tabs[from].pinned == pinned {
            return true;
        }
        let mut tab = self.tabs.remove(from);
        tab.pinned = pinned;
        if pinned {
            tab.group = None;
        }
        let boundary = self.pinned_count();
        self.tabs.insert(boundary, tab);
        self.normalize_groups();
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

    fn groups_of(s: &TabStrip) -> Vec<Option<GroupId>> {
        s.tabs().iter().map(|t| t.group).collect()
    }

    #[test]
    fn creating_a_group_gathers_its_tabs_where_the_first_one_was() {
        let mut s = TabStrip::new();
        let ids: Vec<TabId> = ["a", "b", "c", "d", "e"].iter().map(|u| s.create(spec(u), 0)).collect();
        let g = s.group_create(&[ids[3], ids[1]], "  Work  ", "purple").unwrap();
        assert_eq!(urls(&s), ["a", "b", "d", "c", "e"]);
        assert_eq!(groups_of(&s), [None, Some(g), Some(g), None, None]);
        assert_eq!(s.group(g).unwrap().title, "Work");
        assert_eq!(s.group(g).unwrap().color, "purple");
        let g2 = s.group_create(&[ids[4]], "x", "not-a-colour").unwrap();
        assert_ne!(g, g2);
        assert_eq!(s.group(g2).unwrap().color, "blue", "unknown colours fall back");
        assert!(s.group_create(&[999], "none", "red").is_none(), "no existing tab, no group");
    }

    #[test]
    fn pinned_tabs_are_never_grouped() {
        let mut s = TabStrip::new();
        let p = s.create(NewTab { url: "p".into(), pinned: true, activate: true, ..Default::default() }, 0);
        let a = s.create(spec("a"), 0);
        assert!(s.group_create(&[p], "x", "red").is_none());
        let g = s.group_create(&[a], "x", "red").unwrap();
        assert!(!s.group_add(p, g));
        s.set_pinned(a, true);
        assert_eq!(s.get(a).unwrap().group, None, "pinning leaves the group");
        assert!(s.groups().is_empty(), "the empty group is gone");
    }

    #[test]
    fn moves_keep_groups_contiguous() {
        let mut s = TabStrip::new();
        let ids: Vec<TabId> = ["a", "b", "c", "d", "e"].iter().map(|u| s.create(spec(u), 0)).collect();
        let g = s.group_create(&[ids[1], ids[2], ids[3]], "G", "green").unwrap();
        // Dragging a grouped tab far away takes it out of the group.
        s.move_to(ids[2], 4);
        assert_eq!(urls(&s), ["a", "b", "d", "e", "c"]);
        assert_eq!(groups_of(&s), [None, Some(g), Some(g), None, None]);
        // Dropping an ungrouped tab between two tabs of a group puts it in the group.
        s.move_to(ids[0], 1);
        assert_eq!(urls(&s), ["b", "a", "d", "e", "c"]);
        assert_eq!(groups_of(&s), [Some(g), Some(g), Some(g), None, None]);
        // Next to (not inside) a group stays ungrouped.
        s.move_to(ids[4], 3);
        assert_eq!(s.get(ids[4]).unwrap().group, None);
    }

    #[test]
    fn group_membership_follows_openers_add_remove_and_dissolve() {
        let mut s = TabStrip::new();
        let a = s.create(spec("a"), 0);
        let b = s.create(spec("b"), 0);
        let g = s.group_create(&[a], "A", "red").unwrap();
        let child = s.create(NewTab { url: "a1".into(), opener: Some(a), ..Default::default() }, 0);
        assert_eq!(s.get(child).unwrap().group, Some(g), "links opened from a grouped tab join its group");
        assert!(s.group_add(b, g));
        assert_eq!(s.group_tabs(g), vec![a, child, b]);
        assert!(s.group_remove_tab(child));
        assert_eq!(urls(&s), ["a", "b", "a1"]);
        assert_eq!(s.get(child).unwrap().group, None);
        assert!(s.group_dissolve(g));
        assert!(s.tabs().iter().all(|t| t.group.is_none()) && s.groups().is_empty());
    }

    #[test]
    fn a_group_with_the_active_tab_cannot_be_collapsed_and_opens_when_a_tab_in_it_is_selected() {
        let mut s = TabStrip::new();
        let a = s.create(spec("a"), 0);
        let b = s.create(spec("b"), 0);
        let g = s.group_create(&[a], "A", "red").unwrap();
        s.activate(a, 1);
        assert!(!s.group_update(g, None, None, Some(true)));
        s.activate(b, 2);
        assert!(s.group_update(g, Some("Renamed"), Some("cyan"), Some(true)));
        assert!(s.group(g).unwrap().collapsed);
        s.activate(a, 3);
        assert!(!s.group(g).unwrap().collapsed);
        assert_eq!(s.group(g).unwrap().title, "Renamed");
    }

    #[test]
    fn closing_the_last_tab_of_a_group_removes_it_and_moved_tabs_arrive_ungrouped() {
        let mut s = TabStrip::new();
        let a = s.create(spec("a"), 0);
        s.create(spec("b"), 0);
        let g = s.group_create(&[a], "A", "red").unwrap();
        let moved = s.detach(a, 1).unwrap();
        assert!(s.group(g).is_none());
        let mut other = TabStrip::new();
        other.attach(moved, true, 2);
        assert_eq!(other.tabs()[0].group, None);
    }

    #[test]
    fn saved_groups_come_back_with_their_tabs() {
        let mut s = TabStrip::new();
        let a = s.create(spec("a"), 0);
        let b = s.create(spec("b"), 0);
        let saved = TabGroup { id: 7, title: "Saved".into(), color: "orange".into(), collapsed: false };
        s.restore_groups(vec![saved.clone(), TabGroup { id: 9, ..saved.clone() }], &[(a, 7), (b, 7)]);
        assert_eq!(s.groups(), &[saved][..], "groups without tabs are not restored");
        assert_eq!(groups_of(&s), [Some(7), Some(7)]);
        let next = s.group_create(&[a], "new", "red").unwrap();
        assert!(next > 7, "new group ids never collide with restored ones");
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
