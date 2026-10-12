//! Gaming Mode and the background tab policy (sleeping/unloading). The rules themselves are pure and live in
//! `sleep_policy.rs`; this file gathers the facts, applies the decisions and talks to the native views.

use std::time::Duration;

use super::sleep_policy::{self, PressureAction, Reason, TabFacts};
use super::Controller;
use crate::error::now_ms;
use crate::native::{self, DownloadAction};
use crate::navigation::host_of;
use crate::settings::BackgroundTabAction;
use crate::tabs::{Lifecycle, TabId};

/// In Gaming Mode a tab that leaves the foreground is put to sleep this quickly.
const GAMING_BACKGROUND_GRACE_MS: i64 = 10_000;
/// A page that had typed-in data is not asked again for this long.
const DIRTY_RECHECK_MS: i64 = 5 * 60_000;
/// A tab WebView2 refused to suspend is not asked again for this long.
const SUSPEND_RETRY_MS: i64 = 60_000;
/// How long to wait for the page to answer the unsaved-input question. No answer counts as "has input".
const DIRTY_CHECK_TIMEOUT: Duration = Duration::from_millis(1500);

impl Controller {
    pub(crate) fn sleep_policy_active(&self) -> bool {
        self.settings.sleeping_tabs_enabled || self.settings.gaming_mode
    }

    pub(crate) fn sleep_check_interval(&self) -> Duration {
        if self.settings.gaming_mode {
            Duration::from_secs(5)
        } else {
            Duration::from_secs(30)
        }
    }

    /// What the sleep policy needs to know about every tab right now.
    pub(crate) fn tab_facts(&self) -> Vec<TabFacts> {
        let active = self.tabs.active_id();
        self.tabs
            .tabs()
            .iter()
            .map(|t| TabFacts {
                id: t.id,
                active: Some(t.id) == active,
                lifecycle: t.lifecycle,
                audible: t.audible,
                loading: t.loading,
                pinned: t.pinned,
                has_view: self.views.contains_key(&t.id),
                has_prompt: self.prompts.iter().any(|p| p.tab_id == t.id),
                has_download: self
                    .download_tabs
                    .iter()
                    .any(|(did, tid)| *tid == t.id && self.downloads.iter().any(|d| d.id == *did && d.state.is_active())),
                never_sleep: host_of(&t.url).is_some_and(|h| self.settings.is_never_sleep(&h)),
                last_active: t.last_active,
            })
            .collect()
    }

    fn facts_for(&self, id: TabId) -> Option<TabFacts> {
        self.tab_facts().into_iter().find(|f| f.id == id)
    }

    /// Periodic check that puts idle background tabs to sleep.
    pub(crate) fn sleep_tick(&mut self) {
        let now = now_ms();
        let timeout_ms = if self.settings.gaming_mode { GAMING_BACKGROUND_GRACE_MS } else { self.settings.sleep_after_minutes as i64 * 60_000 };
        let action = if self.settings.gaming_mode { self.settings.gaming_background_action } else { BackgroundTabAction::Sleep };
        let ids = sleep_policy::idle_sleepers(&self.tab_facts(), now, timeout_ms, self.settings.protect_pinned_tabs);
        for id in ids {
            self.put_to_sleep(id, action, Reason::Auto);
        }
    }

    /// Puts a tab to sleep or unloads it, if the policy allows it for this `reason`.
    pub fn put_to_sleep(&mut self, id: TabId, action: BackgroundTabAction, reason: Reason) {
        let Some(facts) = self.facts_for(id) else { return };
        let protect = self.settings.protect_pinned_tabs;
        match action {
            BackgroundTabAction::Sleep => match sleep_policy::sleep_blocker(&facts, reason, protect) {
                None => self.suspend(id),
                Some(why) if reason == Reason::Manual => self.toast(format!("This tab can't be put to sleep: {why}.")),
                Some(_) => {}
            },
            BackgroundTabAction::Unload => match sleep_policy::unload_blocker(&facts, reason, protect) {
                None => self.request_unload(id, reason),
                Some(why) if reason == Reason::Manual => self.toast(format!("This tab can't be unloaded: {why}.")),
                Some(_) => {}
            },
        }
    }

    /// Asks WebView2 to suspend the (hidden) tab. The answer is matched against the tab's activation epoch.
    fn suspend(&mut self, id: TabId) {
        if self.suspend_cooldown.get(&id).is_some_and(|until| *until > now_ms()) {
            return;
        }
        let epoch = self.epochs.get(&id).copied().unwrap_or(0);
        let ctl = self.handle.clone();
        let _ = self.app.run_on_main_thread(move || native::try_suspend(id, ctl, epoch));
    }

    /// Unloading destroys the page, so look for typed-in data first. A page that has some (or that does not
    /// answer in time) is put to sleep instead: nothing the user typed is thrown away.
    fn request_unload(&mut self, id: TabId, reason: Reason) {
        if self.dirty_until.get(&id).is_some_and(|until| *until > now_ms()) {
            // Already known to hold typed-in data: no new question and no new log line.
            self.sleep_instead_of_unload(id, reason, false);
            return;
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        if self.app.run_on_main_thread(move || native::page_has_unsaved_input(id, tx)).is_err() {
            self.sleep_instead_of_unload(id, reason, true);
            return;
        }
        let ctl = self.handle.clone();
        tauri::async_runtime::spawn(async move {
            let dirty = tokio::time::timeout(DIRTY_CHECK_TIMEOUT, rx).await.ok().and_then(|r| r.ok()).flatten().unwrap_or(true);
            ctl.post(move |c| c.finish_unload(id, dirty, reason));
        });
    }

    fn sleep_instead_of_unload(&mut self, id: TabId, reason: Reason, announce: bool) {
        if announce {
            log::info!("tab {id}: not unloaded (it may hold typed-in data); sleeping it instead");
        }
        if announce && reason == Reason::Manual {
            self.toast("This page has text you typed, so it was put to sleep instead of being unloaded.");
        }
        let Some(facts) = self.facts_for(id) else { return };
        if sleep_policy::sleep_blocker(&facts, reason, self.settings.protect_pinned_tabs).is_none() {
            self.suspend(id);
        }
    }

    /// The page answered the unsaved-input question: unload it, or fall back to sleeping.
    pub(crate) fn finish_unload(&mut self, id: TabId, dirty: bool, reason: Reason) {
        if dirty {
            self.dirty_until.insert(id, now_ms() + DIRTY_RECHECK_MS);
            self.sleep_instead_of_unload(id, reason, true);
            return;
        }
        // Things may have changed while the page was being asked (a download started, the tab was selected...).
        let Some(facts) = self.facts_for(id) else { return };
        if sleep_policy::unload_blocker(&facts, reason, self.settings.protect_pinned_tabs).is_some() {
            return;
        }
        log::info!("tab {id}: unloaded to free memory");
        self.destroy_view(id);
        self.mark_state();
    }

    /// WebView2 answered a suspend request. Ignored when the tab was activated since it was made.
    pub fn on_suspended(&mut self, id: TabId, ok: bool, epoch: u64) {
        let current = self.epochs.get(&id).copied().unwrap_or(0);
        let is_active = self.tabs.active_id() == Some(id);
        if !ok && sleep_policy::suspend_still_valid(epoch, current, is_active) {
            log::info!("tab {id}: WebView2 declined to suspend it; trying again in {} s", SUSPEND_RETRY_MS / 1000);
            self.suspend_cooldown.insert(id, now_ms() + SUSPEND_RETRY_MS);
        }
        if ok && sleep_policy::suspend_still_valid(epoch, current, is_active) {
            self.suspend_cooldown.remove(&id);
            if let Some(t) = self.tabs.get_mut(id) {
                if t.lifecycle == Lifecycle::Live {
                    t.lifecycle = Lifecycle::Sleeping;
                    log::info!("tab {id}: sleeping (WebView2 suspended)");
                    self.mark_state();
                }
            }
        }
    }

    /// One step towards the configured memory limit, taken after each process sample.
    pub(crate) fn apply_memory_pressure(&mut self, total_bytes: u64) {
        let limit = self.settings.memory_limit_mb;
        if limit == 0 {
            return;
        }
        let facts = self.tab_facts();
        match sleep_policy::pressure_action(&facts, total_bytes, limit, self.settings.protect_pinned_tabs) {
            Some(PressureAction::Sleep(id)) => {
                log::info!("memory limit: {} MB in use, limit {limit} MB: sleeping tab {id}", total_bytes / (1024 * 1024));
                self.suspend(id);
            }
            Some(PressureAction::Unload(id)) => {
                log::info!("memory limit: {} MB in use, limit {limit} MB: unloading sleeping tab {id}", total_bytes / (1024 * 1024));
                self.request_unload(id, Reason::Auto);
            }
            None => {}
        }
    }

    /// Called when `id` stops being the active tab.
    pub(crate) fn on_backgrounded(&mut self, id: TabId) {
        if self.settings.gaming_mode && self.settings.gaming_low_memory_target && self.views.contains_key(&id) {
            let _ = self.app.run_on_main_thread(move || native::set_memory_target(id, true));
        }
    }

    /// Applies or reverts Gaming Mode side effects.
    pub(crate) fn apply_gaming(&mut self, on: bool) {
        let background: Vec<TabId> = self.views.keys().copied().filter(|id| Some(*id) != self.tabs.active_id()).collect();
        if on {
            if self.settings.gaming_low_memory_target {
                for id in &background {
                    let id = *id;
                    let _ = self.app.run_on_main_thread(move || native::set_memory_target(id, true));
                }
            }
            let action = self.settings.gaming_background_action;
            for id in background {
                self.put_to_sleep(id, action, Reason::Auto);
            }
            if self.settings.gaming_pause_downloads {
                for d in self.downloads.iter_mut().filter(|d| d.state == crate::db::downloads::DownloadState::InProgress) {
                    d.paused_by_gaming = true;
                    let id = d.id;
                    let h = self.handle.clone();
                    let _ = self.app.run_on_main_thread(move || native::download_action(id, DownloadAction::Pause, h));
                }
                self.mark_downloads();
            }
            self.sleep_check_at = std::time::Instant::now() + self.sleep_check_interval();
        } else {
            for id in background {
                let _ = self.app.run_on_main_thread(move || native::set_memory_target(id, false));
            }
            for d in self.downloads.iter_mut().filter(|d| d.paused_by_gaming) {
                d.paused_by_gaming = false;
                let id = d.id;
                let h = self.handle.clone();
                let _ = self.app.run_on_main_thread(move || native::download_action(id, DownloadAction::Resume, h));
            }
            self.mark_downloads();
        }
        self.perf_reschedule();
        self.mark_state();
    }
}
