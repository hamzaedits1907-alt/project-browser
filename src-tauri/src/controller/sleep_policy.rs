//! The rules behind sleeping and unloading background tabs, as pure functions.
//!
//! * **Sleep** = WebView2 `TrySuspend`: the page keeps all its state (scroll, typed text, JS memory) and wakes
//!   instantly, but the renderer stops using CPU and gives memory back.
//! * **Unload** = the webview is destroyed: the most memory is returned, but the page reloads from its address
//!   when it is opened again and the site may lose transient state (unsent text, video position, game progress
//!   that is not saved). Because of that, unloading checks for typed-in data first (see `gaming.rs`).
//!
//! The controller gathers [`TabFacts`] and calls these; they decide, the controller acts.

use crate::tabs::{Lifecycle, TabId};

/// Everything the policy needs to know about one tab.
#[derive(Debug, Clone, PartialEq)]
pub struct TabFacts {
    pub id: TabId,
    pub active: bool,
    pub lifecycle: Lifecycle,
    pub audible: bool,
    pub loading: bool,
    pub pinned: bool,
    /// A native webview currently exists for the tab.
    pub has_view: bool,
    /// A permission prompt for the tab is waiting for an answer.
    pub has_prompt: bool,
    /// A download that started in this tab is still running or paused.
    pub has_download: bool,
    /// The tab's site is on the "never sleep" list.
    pub never_sleep: bool,
    pub last_active: i64,
}

/// Who asked: the automatic policy (idle timer, Gaming Mode, memory limit) or the user for one tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Auto,
    Manual,
}

/// Why this tab must not be put to sleep right now (`None` = it may).
pub fn sleep_blocker(t: &TabFacts, reason: Reason, protect_pinned: bool) -> Option<&'static str> {
    if t.active {
        return Some("it is the tab you are using");
    }
    if !t.has_view || t.lifecycle != Lifecycle::Live {
        return Some("it is not running");
    }
    if reason == Reason::Manual {
        return None; // the user asked for this one tab
    }
    auto_protection(t, protect_pinned)
}

/// Why this tab's webview must not be destroyed right now (`None` = it may be).
/// Works for running and sleeping tabs. A running download is tied to the webview that started it.
pub fn unload_blocker(t: &TabFacts, reason: Reason, protect_pinned: bool) -> Option<&'static str> {
    if t.active {
        return Some("it is the tab you are using");
    }
    if !t.has_view || t.lifecycle == Lifecycle::Unloaded {
        return Some("it is already unloaded");
    }
    if t.has_download {
        return Some("a download from it is in progress");
    }
    if reason == Reason::Manual {
        return None;
    }
    auto_protection(t, protect_pinned)
}

fn auto_protection(t: &TabFacts, protect_pinned: bool) -> Option<&'static str> {
    if t.audible {
        return Some("it is playing audio");
    }
    if t.loading {
        return Some("it is still loading");
    }
    if t.has_prompt {
        return Some("it is waiting for a permission answer");
    }
    if t.has_download {
        return Some("a download from it is in progress");
    }
    if t.never_sleep {
        return Some("its site is on the never-sleep list");
    }
    if protect_pinned && t.pinned {
        return Some("it is pinned");
    }
    None
}

/// Tabs the idle timer should put to sleep now: unused for at least `idle_ms` and not protected.
/// Least recently used first.
pub fn idle_sleepers(tabs: &[TabFacts], now: i64, idle_ms: i64, protect_pinned: bool) -> Vec<TabId> {
    let mut out: Vec<&TabFacts> =
        tabs.iter().filter(|t| now - t.last_active >= idle_ms && sleep_blocker(t, Reason::Auto, protect_pinned).is_none()).collect();
    out.sort_by_key(|t| (t.last_active, t.id));
    out.into_iter().map(|t| t.id).collect()
}

/// One step towards getting under the memory limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressureAction {
    Sleep(TabId),
    Unload(TabId),
}

/// While the browser uses more than `limit_mb`, free memory one tab per step, least recently used first:
/// first put running background tabs to sleep, and only when none is left unload sleeping ones.
/// `limit_mb == 0` means no limit.
pub fn pressure_action(tabs: &[TabFacts], total_bytes: u64, limit_mb: u32, protect_pinned: bool) -> Option<PressureAction> {
    if limit_mb == 0 || total_bytes <= limit_mb as u64 * 1024 * 1024 {
        return None;
    }
    let lru = |mut v: Vec<&TabFacts>| {
        v.sort_by_key(|t| (t.last_active, t.id));
        v.first().map(|t| t.id)
    };
    if let Some(id) = lru(tabs.iter().filter(|t| sleep_blocker(t, Reason::Auto, protect_pinned).is_none()).collect()) {
        return Some(PressureAction::Sleep(id));
    }
    lru(tabs.iter().filter(|t| t.lifecycle == Lifecycle::Sleeping && unload_blocker(t, Reason::Auto, protect_pinned).is_none()).collect())
        .map(PressureAction::Unload)
}

/// True if a suspend request made at `requested_epoch` still applies. Every time a tab becomes the active
/// tab its epoch changes, so a suspend that was in flight while the user switched back (and away again) is
/// ignored instead of marking a running page as sleeping.
pub fn suspend_still_valid(requested_epoch: u64, current_epoch: u64, is_active: bool) -> bool {
    requested_epoch == current_epoch && !is_active
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(id: TabId) -> TabFacts {
        TabFacts {
            id,
            active: false,
            lifecycle: Lifecycle::Live,
            audible: false,
            loading: false,
            pinned: false,
            has_view: true,
            has_prompt: false,
            has_download: false,
            never_sleep: false,
            last_active: 0,
        }
    }

    #[test]
    fn an_ordinary_background_tab_may_sleep() {
        assert_eq!(sleep_blocker(&tab(1), Reason::Auto, true), None);
    }

    #[test]
    fn protected_tabs_are_never_slept_automatically() {
        let cases: [(&str, Box<dyn Fn(&mut TabFacts)>); 8] = [
            ("active", Box::new(|t| t.active = true)),
            ("audio", Box::new(|t| t.audible = true)),
            ("loading", Box::new(|t| t.loading = true)),
            ("prompt", Box::new(|t| t.has_prompt = true)),
            ("download", Box::new(|t| t.has_download = true)),
            ("never-sleep site", Box::new(|t| t.never_sleep = true)),
            ("pinned", Box::new(|t| t.pinned = true)),
            ("no webview", Box::new(|t| t.has_view = false)),
        ];
        for (name, edit) in cases {
            let mut t = tab(1);
            edit(&mut t);
            assert!(sleep_blocker(&t, Reason::Auto, true).is_some(), "{name} must block automatic sleep");
        }
        let mut sleeping = tab(2);
        sleeping.lifecycle = Lifecycle::Sleeping;
        assert!(sleep_blocker(&sleeping, Reason::Auto, true).is_some(), "an already sleeping tab is not slept twice");
    }

    #[test]
    fn pinned_protection_is_a_setting() {
        let mut t = tab(1);
        t.pinned = true;
        assert!(sleep_blocker(&t, Reason::Auto, true).is_some());
        assert_eq!(sleep_blocker(&t, Reason::Auto, false), None);
    }

    #[test]
    fn the_user_can_put_one_tab_to_sleep_even_if_it_plays_audio_or_is_pinned() {
        let mut t = tab(1);
        t.audible = true;
        t.pinned = true;
        t.never_sleep = true;
        assert_eq!(sleep_blocker(&t, Reason::Manual, true), None);
        t.active = true;
        assert!(sleep_blocker(&t, Reason::Manual, true).is_some(), "but never the tab being used");
    }

    #[test]
    fn a_running_download_blocks_unloading_even_when_the_user_asks() {
        let mut t = tab(1);
        t.has_download = true;
        assert!(unload_blocker(&t, Reason::Manual, true).is_some());
        assert!(unload_blocker(&t, Reason::Auto, true).is_some());
        t.has_download = false;
        assert_eq!(unload_blocker(&t, Reason::Manual, true), None);
    }

    #[test]
    fn sleeping_tabs_can_be_unloaded_but_unloaded_ones_cannot() {
        let mut t = tab(1);
        t.lifecycle = Lifecycle::Sleeping;
        assert_eq!(unload_blocker(&t, Reason::Auto, true), None);
        t.lifecycle = Lifecycle::Unloaded;
        t.has_view = false;
        assert!(unload_blocker(&t, Reason::Auto, true).is_some());
    }

    #[test]
    fn idle_tabs_sleep_least_recently_used_first_and_only_after_the_timeout() {
        let mut a = tab(1);
        a.last_active = 1_000;
        let mut b = tab(2);
        b.last_active = 500;
        let mut fresh = tab(3);
        fresh.last_active = 9_500;
        let mut pinned = tab(4);
        pinned.last_active = 0;
        pinned.pinned = true;
        let tabs = [a, b, fresh, pinned];
        assert_eq!(idle_sleepers(&tabs, 10_000, 5_000, true), vec![2, 1], "tab 3 is too recent, tab 4 is pinned");
        assert_eq!(idle_sleepers(&tabs, 10_000, 5_000, false), vec![4, 2, 1]);
        assert!(idle_sleepers(&tabs, 10_000, 60_000, true).is_empty());
    }

    #[test]
    fn nothing_happens_below_the_memory_limit_or_when_it_is_off() {
        let tabs = [tab(1), tab(2)];
        let mb = 1024 * 1024;
        assert_eq!(pressure_action(&tabs, 900 * mb, 1000, true), None);
        assert_eq!(pressure_action(&tabs, 1000 * mb, 1000, true), None, "exactly at the limit is fine");
        assert_eq!(pressure_action(&tabs, 99_999 * mb, 0, true), None, "0 = no limit");
    }

    #[test]
    fn over_the_limit_the_least_recently_used_running_tab_sleeps_first() {
        let mut a = tab(1);
        a.last_active = 300;
        let mut b = tab(2);
        b.last_active = 100;
        let mut active = tab(3);
        active.active = true;
        active.last_active = 0;
        let mb = 1024 * 1024;
        assert_eq!(pressure_action(&[a, b, active], 1500 * mb, 1000, true), Some(PressureAction::Sleep(2)));
    }

    #[test]
    fn when_nothing_is_left_to_sleep_sleeping_tabs_are_unloaded_oldest_first() {
        let mut a = tab(1);
        a.lifecycle = Lifecycle::Sleeping;
        a.last_active = 200;
        let mut b = tab(2);
        b.lifecycle = Lifecycle::Sleeping;
        b.last_active = 50;
        let mut active = tab(3);
        active.active = true;
        let mb = 1024 * 1024;
        assert_eq!(pressure_action(&[a, b, active], 2000 * mb, 1000, true), Some(PressureAction::Unload(2)));
    }

    #[test]
    fn pressure_never_touches_protected_tabs() {
        let mut playing = tab(1);
        playing.audible = true;
        let mut pinned = tab(2);
        pinned.pinned = true;
        let mut downloading = tab(3);
        downloading.has_download = true;
        downloading.lifecycle = Lifecycle::Sleeping;
        let mut active = tab(4);
        active.active = true;
        let mb = 1024 * 1024;
        assert_eq!(pressure_action(&[playing, pinned, downloading, active], 5000 * mb, 1000, true), None);
    }

    #[test]
    fn a_stale_suspend_result_is_ignored() {
        assert!(suspend_still_valid(3, 3, false));
        assert!(!suspend_still_valid(3, 4, false), "the tab was activated in between");
        assert!(!suspend_still_valid(3, 3, true), "the tab is the active one now");
    }

    #[test]
    fn rapid_switching_never_leaves_two_states_for_one_tab() {
        // Simulates: suspend requested (epoch 1) -> user switches to the tab (epoch 2) and away again ->
        // the old suspend completes. It must be ignored, and the tab stays "running".
        let mut epoch = 1u64;
        let requested = epoch;
        epoch += 1; // activated
        let still_active = false; // switched away again
        assert!(!suspend_still_valid(requested, epoch, still_active));
        // A suspend requested after the second backgrounding is valid.
        assert!(suspend_still_valid(epoch, epoch, still_active));
    }
}
