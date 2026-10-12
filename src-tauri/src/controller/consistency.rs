//! Detects disagreement between the tab model (`TabStrip`) and the native webviews.
//!
//! The controller owns both, and every operation keeps them in step, but the two live in different
//! worlds (a pure model and real windows), so a bug in one code path could leave a webview behind
//! for a closed tab, a "live" tab without a webview (a blank page area), or two visible webviews.
//! `find_drift` is pure so the rules are unit-tested; `Controller::reconcile_views` repairs what it
//! finds and logs it, so such a bug is corrected within one controller step and is visible in the log.

use crate::navigation::is_internal;
use crate::tabs::{Lifecycle, Tab, TabId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Drift {
    /// A webview exists for a tab that is no longer in the strip.
    OrphanView(TabId),
    /// A browser:// page is drawn by the shell and must not have a webview.
    InternalWithView(TabId),
    /// The model says the page is loaded (live or sleeping) but there is no webview.
    MissingView(TabId),
    /// A webview is visible although it is not on screen (the active tab or its split partner), or a shell popup hides the page.
    ExtraVisible(TabId),
}

/// `views` is `(tab id, currently visible)` for every existing webview; `shown` are the tabs that belong on screen.
pub(crate) fn find_drift(tabs: &[Tab], views: &[(TabId, bool)], shown: &[TabId], overlay: bool) -> Vec<Drift> {
    let mut out = Vec::new();
    for &(id, visible) in views {
        match tabs.iter().find(|t| t.id == id) {
            None => out.push(Drift::OrphanView(id)),
            Some(t) if is_internal(&t.url) => out.push(Drift::InternalWithView(id)),
            Some(_) if visible && (!shown.contains(&id) || overlay) => out.push(Drift::ExtraVisible(id)),
            Some(_) => {}
        }
    }
    for t in tabs {
        let has_view = views.iter().any(|(id, _)| *id == t.id);
        if !has_view && !is_internal(&t.url) && t.lifecycle != Lifecycle::Unloaded {
            out.push(Drift::MissingView(t.id));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabs::{NewTab, TabStrip};

    fn strip(urls: &[&str]) -> TabStrip {
        let mut s = TabStrip::new();
        for u in urls {
            s.create(NewTab { url: (*u).into(), ..Default::default() }, 1);
        }
        s
    }

    fn set_lifecycle(s: &mut TabStrip, id: TabId, l: Lifecycle) {
        s.get_mut(id).unwrap().lifecycle = l;
    }

    #[test]
    fn a_consistent_state_has_no_drift() {
        let mut s = strip(&["https://a/", "https://b/", "browser://newtab"]);
        set_lifecycle(&mut s, 1, Lifecycle::Live);
        set_lifecycle(&mut s, 2, Lifecycle::Sleeping);
        // 3 is a shell page: no webview. Tab 1 is active and visible, tab 2 hidden and suspended.
        assert!(find_drift(s.tabs(), &[(1, true), (2, false)], &[1], false).is_empty());
        // Restored (unloaded) web tabs have no webview either.
        let u = strip(&["https://a/"]);
        assert!(find_drift(u.tabs(), &[], &[1], false).is_empty());
    }

    #[test]
    fn finds_a_webview_left_behind_for_a_closed_tab() {
        let s = strip(&["https://a/"]);
        assert_eq!(find_drift(s.tabs(), &[(1, true), (9, false)], &[1], false), vec![Drift::OrphanView(9)]);
    }

    #[test]
    fn finds_a_webview_on_a_shell_page() {
        let s = strip(&["browser://settings"]);
        assert_eq!(find_drift(s.tabs(), &[(1, false)], &[1], false), vec![Drift::InternalWithView(1)]);
    }

    #[test]
    fn finds_a_live_tab_without_a_webview_which_would_be_a_blank_page() {
        let mut s = strip(&["https://a/", "https://b/"]);
        set_lifecycle(&mut s, 1, Lifecycle::Live);
        set_lifecycle(&mut s, 2, Lifecycle::Sleeping);
        assert_eq!(find_drift(s.tabs(), &[], &[1], false), vec![Drift::MissingView(1), Drift::MissingView(2)]);
    }

    #[test]
    fn finds_visible_background_webviews_and_pages_shown_under_a_popup() {
        let mut s = strip(&["https://a/", "https://b/"]);
        set_lifecycle(&mut s, 1, Lifecycle::Live);
        set_lifecycle(&mut s, 2, Lifecycle::Live);
        assert_eq!(find_drift(s.tabs(), &[(1, true), (2, true)], &[1], false), vec![Drift::ExtraVisible(2)]);
        // While a shell menu is open the page must be hidden, active tab included.
        assert_eq!(find_drift(s.tabs(), &[(1, true), (2, false)], &[1], true), vec![Drift::ExtraVisible(1)]);
        // In split view the active tab's partner is on screen too.
        assert!(find_drift(s.tabs(), &[(1, true), (2, true)], &[1, 2], false).is_empty());
    }
}
