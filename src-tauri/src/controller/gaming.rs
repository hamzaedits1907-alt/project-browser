//! Gaming Mode and the background tab policy (sleeping/unloading).

use std::time::Duration;

use super::Controller;
use crate::error::now_ms;
use crate::native::{self, DownloadAction};
use crate::navigation::host_of;
use crate::settings::BackgroundTabAction;
use crate::tabs::{Lifecycle, TabId};

/// In Gaming Mode a tab that leaves the foreground is put to sleep this quickly.
const GAMING_BACKGROUND_GRACE_MS: i64 = 10_000;

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

    /// True when a background tab is allowed to sleep right now.
    fn can_sleep(&self, id: TabId) -> bool {
        let Some(t) = self.tabs.get(id) else { return false };
        if Some(id) == self.tabs.active_id() || t.lifecycle != Lifecycle::Live || t.audible || t.loading {
            return false;
        }
        if !self.views.contains_key(&id) || self.prompts.iter().any(|p| p.tab_id == id) {
            return false;
        }
        !host_of(&t.url).is_some_and(|h| self.settings.is_never_sleep(&h))
    }

    /// Periodic check that puts idle background tabs to sleep.
    pub(crate) fn sleep_tick(&mut self) {
        let now = now_ms();
        let timeout_ms = if self.settings.gaming_mode { GAMING_BACKGROUND_GRACE_MS } else { self.settings.sleep_after_minutes as i64 * 60_000 };
        let action = if self.settings.gaming_mode { self.settings.gaming_background_action } else { BackgroundTabAction::Sleep };
        let candidates: Vec<TabId> =
            self.tabs.tabs().iter().filter(|t| now - t.last_active >= timeout_ms).map(|t| t.id).filter(|id| self.can_sleep(*id)).collect();
        for id in candidates {
            self.put_to_sleep(id, action);
        }
    }

    /// Manually or automatically put a tab to sleep.
    pub fn put_to_sleep(&mut self, id: TabId, action: BackgroundTabAction) {
        if !self.can_sleep(id) {
            return;
        }
        match action {
            BackgroundTabAction::Sleep => {
                let ctl = self.handle.clone();
                let _ = self.app.run_on_main_thread(move || native::try_suspend(id, ctl));
            }
            BackgroundTabAction::Unload => {
                self.destroy_view(id);
                self.mark_state();
            }
        }
    }

    pub fn on_suspended(&mut self, id: TabId, ok: bool) {
        // The tab may have been activated while suspension was in flight.
        if ok && Some(id) != self.tabs.active_id() {
            if let Some(t) = self.tabs.get_mut(id) {
                if t.lifecycle == Lifecycle::Live {
                    t.lifecycle = Lifecycle::Sleeping;
                    self.mark_state();
                }
            }
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
                self.put_to_sleep(id, action);
            }
            if self.settings.gaming_pause_downloads {
                for d in self.downloads.iter_mut().filter(|d| d.state == crate::db::downloads::DownloadState::InProgress) {
                    d.paused_by_gaming = true;
                    let id = d.id;
                    let _ = self.app.run_on_main_thread(move || native::download_action(id, DownloadAction::Pause));
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
                let _ = self.app.run_on_main_thread(move || native::download_action(id, DownloadAction::Resume));
            }
            self.mark_downloads();
        }
        self.perf_reschedule();
        self.mark_state();
    }
}
