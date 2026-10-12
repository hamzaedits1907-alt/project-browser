//! The browser controller.
//!
//! One dedicated thread owns all browser state (tab strip, database, settings,
//! downloads, permission prompts, performance sampling). Everything else talks
//! to it by posting closures:
//!
//! * shell commands `call(...).await` from Tauri's async runtime,
//! * WebView2 event handlers on the UI thread `post(...)` and never block,
//! * the controller drives webviews through Tauri (which marshals to the UI
//!   thread) and `run_on_main_thread` for raw WebView2 calls.
//!
//! The UI thread never waits on the controller (except a bounded wait at
//! shutdown), so the two cannot deadlock.

mod consistency;
mod context_menu;
mod downloads;
mod drag;
mod events;
mod extras;
mod gaming;
mod groups;
mod perf;
mod sleep_policy;
mod split;
mod tabs;
mod window_set;

use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::db::downloads::Download;
use crate::db::Db;
use crate::error::{now_ms, Error, Result};
use crate::profile::{Profile, ProfileStore};
use crate::settings::Settings;
use crate::tabs::{GroupId, Tab, TabGroup, TabId, TabStrip};
pub use split::{SplitOp, SplitView};

pub use downloads::DownloadCommand;
pub use drag::DragOutcome;
pub use groups::GroupOp;
pub use perf::PerfSample;
pub use sleep_policy::Reason;
pub use tabs::ZoomMode;
pub use window_set::WindowBrief;

type Job = Box<dyn FnOnce(&mut Controller) + Send>;

/// Cheap, cloneable handle used to send work to the controller thread.
#[derive(Clone)]
pub struct ControllerHandle {
    tx: Sender<Job>,
}

impl ControllerHandle {
    /// Fire-and-forget. Safe to call from any thread, including the UI thread.
    pub fn post(&self, f: impl FnOnce(&mut Controller) + Send + 'static) {
        let _ = self.tx.send(Box::new(f));
    }

    /// Runs `f` on the controller and awaits its result (for async commands).
    pub async fn call<T: Send + 'static>(&self, f: impl FnOnce(&mut Controller) -> T + Send + 'static) -> Result<T> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.post(move |c| {
            let _ = tx.send(f(c));
        });
        rx.await.map_err(|_| Error::msg("browser controller is not running"))
    }

    /// Fire-and-forget, run with the window that owns tab `tab` selected (see `windows.rs`).
    pub fn post_tab(&self, tab: TabId, f: impl FnOnce(&mut Controller) + Send + 'static) {
        self.post(move |c| {
            c.select_tab_window(tab);
            f(c)
        });
    }

    /// Fire-and-forget, run with window `label` selected.
    pub fn post_window(&self, label: String, f: impl FnOnce(&mut Controller) + Send + 'static) {
        self.post(move |c| {
            c.select_window(&label);
            f(c)
        });
    }

    /// Like `call`, with window `label` selected. Commands from a shell window use this so they act on that window.
    pub async fn call_in<T: Send + 'static>(&self, label: String, f: impl FnOnce(&mut Controller) -> T + Send + 'static) -> Result<T> {
        self.call(move |c| {
            c.select_window(&label);
            f(c)
        })
        .await
    }

    /// Like `call`, with the window that owns tab `tab` selected.
    pub async fn call_tab<T: Send + 'static>(&self, tab: TabId, f: impl FnOnce(&mut Controller) -> T + Send + 'static) -> Result<T> {
        self.call(move |c| {
            c.select_tab_window(tab);
            f(c)
        })
        .await
    }

    /// Blocking variant with a timeout; only used during shutdown.
    pub fn call_blocking<T: Send + 'static>(&self, timeout: Duration, f: impl FnOnce(&mut Controller) -> T + Send + 'static) -> Option<T> {
        let (tx, rx) = mpsc::channel();
        self.post(move |c| {
            let _ = tx.send(f(c));
        });
        rx.recv_timeout(timeout).ok()
    }
}

/// Logical (CSS px) rectangle reported by the shell for the page area.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Default)]
pub struct ContentRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Viewport {
    pub width: f64,
    pub height: f64,
    pub scale: f64,
    pub maximized: bool,
    pub screen_x: f64,
    pub screen_y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WindowGeometry {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub width: f64,
    pub height: f64,
    pub maximized: bool,
}

#[derive(Default)]
pub(crate) struct Layout {
    pub content: Option<ContentRect>,
    pub viewport: Viewport,
    pub overlay: bool,
    pub element_fullscreen: Option<TabId>,
    pub browser_fullscreen: bool,
    /// The right-click menu this window's shell is showing (its WebView2 request id).
    pub context_menu: Option<u64>,
    /// Two tabs shown side by side (see `split.rs`).
    pub split: Option<SplitView>,
}

/// Blocked-request counts per tab, sent to the shell a few times per second at most.
#[derive(Debug, Clone, Serialize)]
pub struct ShieldEvent {
    pub tabs: HashMap<TabId, u32>,
    pub total: u64,
}

pub(crate) struct View {
    pub webview: tauri::Webview,
    pub visible: bool,
    pub bounds: Option<ContentRect>,
    /// The webview moved to another window and has not been re-hosted there yet (see `native::rehost`).
    pub rehost: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionPrompt {
    pub id: u64,
    pub tab_id: TabId,
    pub origin: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FindState {
    pub tab_id: TabId,
    pub query: String,
    pub matches: i32,
    pub active: i32,
    pub unavailable: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StartupMetrics {
    pub process_start: Option<i64>,
    /// Process creation → shell UI rendered (ms).
    pub shell_ready_ms: Option<i64>,
    /// Process creation → first web page finished loading (ms).
    pub first_page_ms: Option<i64>,
}

/// Snapshot the shell renders from.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserState {
    pub tabs: Vec<Tab>,
    pub groups: Vec<TabGroup>,
    pub active_id: Option<TabId>,
    pub closed_count: usize,
    pub fullscreen: bool,
    pub browser_fullscreen: bool,
    pub gaming: bool,
    /// This window holds private tabs only.
    pub private_window: bool,
    pub window_count: usize,
    /// The window's split-view pair, if it has one.
    pub split: Option<SplitView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionTab {
    url: String,
    title: String,
    pinned: bool,
    favicon: Option<String>,
    /// Sessions written before tab groups have no group.
    #[serde(default)]
    group: Option<GroupId>,
}

/// One window's saved tabs. Sessions written before multi-window support have no `geometry`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Session {
    tabs: Vec<SessionTab>,
    active: usize,
    #[serde(default)]
    geometry: Option<WindowGeometry>,
    #[serde(default)]
    groups: Vec<TabGroup>,
}

/// Which saved tabs to recreate on startup, and which of them is selected.
#[derive(Debug, PartialEq)]
struct RestorePlan {
    tabs: Vec<SessionTab>,
    /// Index into `tabs`; `None` keeps the first tab selected.
    active: Option<usize>,
    groups: Vec<TabGroup>,
}

/// Pure decision logic for session restoration, so it can be tested without a webview.
///
/// * `RestoreSession`: every saved tab except pages that no longer exist, selecting the saved active tab.
/// * `NewTab`: only pinned tabs survive.
///
/// Back/forward history is not part of the session: WebView2 has no API to rebuild a navigation
/// history, so a restored tab starts with just its last URL.
fn plan_restore(session: Session, startup: crate::settings::StartupBehavior) -> RestorePlan {
    use crate::settings::StartupBehavior;
    let saved_active = session.active;
    let groups = session.groups;
    let mut tabs = Vec::new();
    let mut active = None;
    for (i, st) in session.tabs.into_iter().enumerate() {
        let obsolete_internal = crate::navigation::is_internal(&st.url) && crate::navigation::internal_page(&st.url).is_none();
        let wanted = match startup {
            StartupBehavior::RestoreSession => !obsolete_internal,
            StartupBehavior::NewTab => st.pinned && !obsolete_internal,
        };
        if !wanted {
            continue;
        }
        if startup == StartupBehavior::RestoreSession && i == saved_active {
            active = Some(tabs.len());
        }
        tabs.push(st);
    }
    // Only groups that still have a tab come back.
    let groups = groups.into_iter().filter(|g| tabs.iter().any(|t| t.group == Some(g.id))).collect();
    RestorePlan { tabs, active, groups }
}

/// Where the main window was when the browser last closed: its entry in the saved window list, else the size saved
/// by earlier versions.
pub fn saved_main_geometry(db: &Db) -> Option<WindowGeometry> {
    let sessions: Vec<Session> = db.kv_get("session_windows").ok().flatten().unwrap_or_default();
    sessions.into_iter().next().and_then(|s| s.geometry).or_else(|| db.kv_get("window").ok().flatten())
}

fn plan_restore_with_geometry(session: Session, startup: crate::settings::StartupBehavior) -> (RestorePlan, Option<WindowGeometry>) {
    let geometry = session.geometry.clone();
    (plan_restore(session, startup), geometry)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum UiEvent {
    FocusOmnibox,
    OpenFind,
    OpenPalette,
    Screenshot,
    Reader,
    Shortcuts,
    Toast { message: String },
}

#[derive(Default)]
struct Dirty {
    state: bool,
    downloads: bool,
    prompts: bool,
    find: bool,
    settings: bool,
    bookmarks: bool,
}

impl Dirty {
    fn any(&self) -> bool {
        self.state || self.downloads || self.prompts || self.find || self.settings || self.bookmarks
    }
}

pub struct Controller {
    pub(crate) app: AppHandle,
    pub(crate) handle: ControllerHandle,
    pub(crate) profile: Profile,
    pub(crate) profiles: ProfileStore,
    pub(crate) db: Db,
    pub(crate) settings: Settings,
    pub(crate) tabs: TabStrip,
    pub(crate) views: HashMap<TabId, View>,
    /// Webviews of closed tabs that are kept (hidden) until the downloads they started are over.
    pub(crate) lingering: HashMap<TabId, View>,
    pub(crate) layout: Layout,
    pub(crate) downloads: Vec<Download>,
    pub(crate) prompts: Vec<PermissionPrompt>,
    pub(crate) find: Option<FindState>,
    pub(crate) perf: perf::PerfState,
    pub(crate) startup: StartupMetrics,
    pub(crate) first_run: bool,
    pub(crate) shell_ready: bool,
    /// Label of the window whose state is in `tabs`, `views`, `layout`, `find`, `shell_ready` and `dirty` right now.
    /// Every entry point that belongs to a window selects it first (see `windows.rs`).
    pub(crate) current: String,
    /// The state of every other window.
    pub(crate) windows: HashMap<String, window_set::WindowState>,
    /// All window labels in the order they were created (the current one included).
    pub(crate) window_order: Vec<String>,
    /// The current window holds private tabs only and is never saved.
    pub(crate) private_window: bool,
    /// Tab ids are handed out from one counter for all windows.
    pub(crate) tab_ids: Arc<AtomicU64>,
    pub(crate) next_window: u32,
    pub(crate) closed_windows: Vec<window_set::ClosedWindow>,
    /// The window that last had keyboard focus; toasts and links opened from outside go there.
    pub(crate) last_focused: String,
    /// What the last "close" was, so that Ctrl+Shift+T reopens the right thing.
    pub(crate) last_close_was_window: bool,
    /// The window a tab dragged out of another window is over (it shows the drop marker).
    pub(crate) drag_hover: Option<String>,
    /// Download id -> the tab that started it (a running download blocks sleeping/unloading that tab).
    pub(crate) download_tabs: HashMap<i64, TabId>,
    /// Bumped each time a tab becomes active; a suspend answer from an older epoch is ignored.
    pub(crate) epochs: HashMap<TabId, u64>,
    /// Tabs found to hold typed-in data, and until when they are not asked again.
    pub(crate) dirty_until: HashMap<TabId, i64>,
    /// Tabs WebView2 declined to suspend, and until when they are not asked again.
    pub(crate) suspend_cooldown: HashMap<TabId, i64>,
    /// The YouTube video whose sponsor segments were last handed to each tab's current document.
    pub(crate) sponsor_seen: HashMap<TabId, String>,
    /// Addresses Link Cleaner rewrote this session.
    pub(crate) links_cleaned: u64,
    pending_urls: Vec<String>,
    load_all: bool,
    dirty: Dirty,
    /// Changes that concern every window (downloads, settings, bookmarks).
    gdirty: Dirty,
    session_save_at: Option<Instant>,
    sleep_check_at: Instant,
    /// When the next `pb:shield` event (blocked counts) is due.
    shield_due: Option<Instant>,
    shield_saved_at: Instant,
}

pub struct ControllerInit {
    pub app: AppHandle,
    pub profile: Profile,
    pub profiles: ProfileStore,
    pub db: Db,
    pub settings: Settings,
    pub first_run: bool,
    pub process_start: Option<i64>,
    pub startup_urls: Vec<String>,
    /// Load every startup URL immediately instead of leaving all but the last unloaded (benchmarking).
    pub load_all: bool,
}

const SESSION_SAVE_DELAY: Duration = Duration::from_millis(800);
const DOWNLOAD_HISTORY_LIMIT: u32 = 200;

impl Controller {
    /// Starts the controller thread and returns its handle.
    pub fn spawn(init: ControllerInit) -> ControllerHandle {
        let (tx, rx) = mpsc::channel::<Job>();
        let handle = ControllerHandle { tx };
        let h2 = handle.clone();
        std::thread::Builder::new()
            .name("browser-controller".into())
            .spawn(move || {
                let mut c = Controller::new(init, h2);
                c.restore_session();
                c.run(rx);
            })
            .expect("spawn controller thread");
        handle
    }

    fn new(init: ControllerInit, handle: ControllerHandle) -> Controller {
        let now = now_ms();
        if let Err(e) = init.db.downloads_mark_stale_interrupted(now) {
            log::warn!("could not update stale downloads: {e}");
        }
        let downloads = init.db.downloads_list(DOWNLOAD_HISTORY_LIMIT).unwrap_or_default();
        crate::native::set_download_dir(init.settings.download_dir.as_ref().map(Into::into));
        crate::native::set_clean_links(init.settings.clean_links);
        crate::native::set_popup_policy(
            init.settings.popup_allowed_hosts.clone(),
            init.settings.new_window_requests == crate::settings::NewWindowRequests::Window,
        );
        let tab_ids = Arc::new(AtomicU64::new(1));
        let main = crate::window::SHELL_LABEL.to_string();
        Controller {
            app: init.app,
            handle,
            profile: init.profile,
            profiles: init.profiles,
            db: init.db,
            settings: init.settings,
            tabs: TabStrip::with_shared_ids(tab_ids.clone()),
            views: HashMap::new(),
            lingering: HashMap::new(),
            layout: Layout::default(),
            downloads,
            prompts: Vec::new(),
            find: None,
            perf: perf::PerfState::new(),
            startup: StartupMetrics { process_start: init.process_start, ..Default::default() },
            first_run: init.first_run,
            shell_ready: false,
            current: main.clone(),
            windows: HashMap::new(),
            window_order: vec![main.clone()],
            private_window: false,
            tab_ids,
            next_window: 2,
            closed_windows: Vec::new(),
            last_focused: main,
            last_close_was_window: false,
            drag_hover: None,
            download_tabs: HashMap::new(),
            epochs: HashMap::new(),
            dirty_until: HashMap::new(),
            suspend_cooldown: HashMap::new(),
            sponsor_seen: HashMap::new(),
            links_cleaned: 0,
            pending_urls: init.startup_urls,
            load_all: init.load_all,
            dirty: Dirty::default(),
            gdirty: Dirty::default(),
            session_save_at: None,
            sleep_check_at: Instant::now() + Duration::from_secs(30),
            shield_due: None,
            shield_saved_at: Instant::now(),
        }
    }

    fn run(mut self, rx: Receiver<Job>) {
        loop {
            let wait = self.next_timer_in();
            match rx.recv_timeout(wait) {
                Ok(job) => {
                    job(&mut self);
                    // Coalesce bursts (e.g. many WebView2 events) into one UI update.
                    for _ in 0..64 {
                        match rx.try_recv() {
                            Ok(job) => job(&mut self),
                            Err(_) => break,
                        }
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            self.run_timers();
            self.each_window(|c| c.reconcile_views());
            self.flush();
        }
        self.save_session();
    }

    fn next_timer_in(&self) -> Duration {
        let now = Instant::now();
        let mut next = now + Duration::from_secs(60);
        if let Some(t) = self.session_save_at {
            next = next.min(t);
        }
        if self.sleep_policy_active() {
            next = next.min(self.sleep_check_at);
        }
        if let Some(t) = self.perf.next_at {
            next = next.min(t);
        }
        if let Some(t) = self.shield_due {
            next = next.min(t);
        }
        next.saturating_duration_since(now)
    }

    fn run_timers(&mut self) {
        let now = Instant::now();
        if self.session_save_at.is_some_and(|t| t <= now) {
            self.session_save_at = None;
            self.save_session();
        }
        if self.sleep_policy_active() && self.sleep_check_at <= now {
            self.sleep_check_at = now + self.sleep_check_interval();
            self.sleep_tick();
        }
        if self.perf.next_at.is_some_and(|t| t <= now) {
            self.perf_tick();
        }
        if self.shield_due.is_some_and(|t| t <= now) {
            self.flush_shield();
        }
    }

    /// The Shield blocked something: tell the UI soon, but at most a few times per second however busy a page is.
    pub(crate) fn mark_shield(&mut self) {
        if self.shield_due.is_none() {
            self.shield_due = Some(Instant::now() + Duration::from_millis(400));
        }
    }

    fn flush_shield(&mut self) {
        self.shield_due = None;
        let Some(shield) = crate::shield::global() else { return };
        shield.clear_pending();
        if self.shell_ready {
            self.emit_all("pb:shield", ShieldEvent { tabs: shield.tab_counts(), total: shield.session_total() });
        }
        if self.shield_saved_at.elapsed() >= Duration::from_secs(30) {
            self.shield_saved_at = Instant::now();
            self.save_shield_counter();
        }
    }

    /// Writes the Shield's lifetime counter if it moved. The rest of its configuration is saved when it is changed.
    fn save_shield_counter(&self) {
        if let Some(cfg) = crate::shield::global().and_then(|s| s.take_unsaved_config()) {
            if let Err(e) = self.db.kv_set(crate::shield::KV_KEY, &cfg) {
                log::warn!("could not save the Shield counter: {e}");
            }
        }
    }

    // ---- change tracking & events -------------------------------------------------

    pub(crate) fn mark_state(&mut self) {
        self.dirty.state = true;
    }

    pub(crate) fn mark_session(&mut self) {
        self.dirty.state = true;
        if self.session_save_at.is_none() {
            self.session_save_at = Some(Instant::now() + SESSION_SAVE_DELAY);
        }
    }

    pub(crate) fn mark_downloads(&mut self) {
        self.gdirty.downloads = true;
    }

    pub(crate) fn mark_prompts(&mut self) {
        self.dirty.prompts = true;
    }

    pub(crate) fn mark_find(&mut self) {
        self.dirty.find = true;
    }

    pub(crate) fn mark_settings(&mut self) {
        self.gdirty.settings = true;
    }

    pub(crate) fn mark_bookmarks(&mut self) {
        self.gdirty.bookmarks = true;
    }

    /// Sends an event to the current window's shell webview. Events only ever go to shell webviews, never broadcast:
    /// tab webviews host untrusted content.
    pub(crate) fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) {
        if let Err(e) = self.app.emit_to(self.current.as_str(), event, payload) {
            log::warn!("emit {event} failed: {e}");
        }
    }

    /// Sends an event to every window's shell webview.
    pub(crate) fn emit_all<S: Serialize + Clone>(&self, event: &str, payload: S) {
        for label in &self.window_order {
            if let Err(e) = self.app.emit_to(label.as_str(), event, payload.clone()) {
                log::warn!("emit {event} to {label} failed: {e}");
            }
        }
    }

    pub(crate) fn ui(&self, event: UiEvent) {
        self.emit("pb:ui", event);
    }

    /// Shows a message in the window the user last used.
    pub(crate) fn toast(&self, message: impl Into<String>) {
        let target = if self.window_order.contains(&self.last_focused) { &self.last_focused } else { &self.current };
        if let Err(e) = self.app.emit_to(target.as_str(), "pb:ui", UiEvent::Toast { message: message.into() }) {
            log::warn!("toast failed: {e}");
        }
    }

    /// Sends every pending change to the window it concerns. Changes that concern all windows reach all of them.
    fn flush(&mut self) {
        let global = std::mem::take(&mut self.gdirty);
        if self.windows.is_empty() {
            self.flush_current(&global);
            return;
        }
        let everywhere = global.any();
        let keep = self.current.clone();
        for label in self.window_order.clone() {
            let pending = if label == self.current { self.dirty.any() } else { self.windows.get(&label).is_some_and(|w| w.dirty.any()) };
            if (everywhere || pending) && self.select_window(&label) {
                self.flush_current(&global);
            }
        }
        self.select_window(&keep);
    }

    fn flush_current(&mut self, global: &Dirty) {
        if !self.shell_ready {
            return;
        }
        let d = std::mem::take(&mut self.dirty);
        if d.state {
            self.emit("pb:state", self.snapshot());
        }
        if d.downloads || global.downloads {
            self.emit("pb:downloads", &self.downloads);
        }
        if d.prompts {
            self.emit("pb:prompts", self.window_prompts());
        }
        if d.find {
            self.emit("pb:find", &self.find);
        }
        if d.settings || global.settings {
            self.emit("pb:settings", &self.settings);
        }
        if d.bookmarks || global.bookmarks {
            self.emit("pb:bookmarks", ());
        }
    }

    /// The permission prompts that belong to the current window's tabs.
    pub(crate) fn window_prompts(&self) -> Vec<PermissionPrompt> {
        self.prompts.iter().filter(|p| self.tabs.get(p.tab_id).is_some()).cloned().collect()
    }

    pub fn snapshot(&self) -> BrowserState {
        BrowserState {
            tabs: self.tabs.tabs().to_vec(),
            groups: self.tabs.groups().to_vec(),
            active_id: self.tabs.active_id(),
            closed_count: self.tabs.closed().len(),
            fullscreen: self.layout.browser_fullscreen || self.layout.element_fullscreen.is_some(),
            browser_fullscreen: self.layout.browser_fullscreen,
            gaming: self.settings.gaming_mode,
            private_window: self.private_window,
            window_count: self.window_order.len(),
            split: self.layout.split,
        }
    }

    // ---- session ------------------------------------------------------------------

    /// The saved form of the current window, or `None` for a private window or one with nothing worth saving.
    fn window_session(&self) -> Option<Session> {
        if self.private_window {
            return None;
        }
        let mut session = Session::default();
        for t in self.tabs.tabs().iter().filter(|t| !t.private) {
            if Some(t.id) == self.tabs.active_id() {
                session.active = session.tabs.len();
            }
            let url = t.error.as_ref().map(|e| e.url.clone()).unwrap_or_else(|| t.url.clone());
            session.tabs.push(SessionTab { url, title: t.title.clone(), pinned: t.pinned, favicon: t.favicon.clone(), group: t.group });
        }
        session.groups = self.tabs.groups().to_vec();
        let v = self.layout.viewport;
        if v.width > 0.0 && v.height > 0.0 {
            session.geometry = Some(self.window_geometry());
        }
        Some(session)
    }

    pub(crate) fn save_session(&mut self) {
        let keep = self.current.clone();
        let mut sessions = Vec::new();
        for label in self.window_order.clone() {
            if self.select_window(&label) {
                if let Some(s) = self.window_session() {
                    sessions.push(s);
                }
            }
        }
        self.select_window(&keep);
        // Older versions read the first window from "session"; keep it current so a downgrade still finds its tabs.
        if let Some(first) = sessions.first() {
            if let Err(e) = self.db.kv_set("session", first) {
                log::error!("failed to save session: {e}");
            }
        }
        if let Err(e) = self.db.kv_set("session_windows", &sessions) {
            log::error!("failed to save the window list: {e}");
        }
    }

    /// Creates the tabs of one saved window in the current window. Returns how many were restored.
    fn restore_window_tabs(&mut self, plan: RestorePlan, now: i64) -> usize {
        let mut restored = 0;
        let mut active = None;
        let mut assignments = Vec::new();
        for (i, st) in plan.tabs.into_iter().enumerate() {
            let group = st.group;
            // Restored tabs start unloaded; their webview is created when first shown.
            let id = self.tabs.create(
                crate::tabs::NewTab {
                    url: st.url,
                    pinned: st.pinned,
                    title: Some(st.title),
                    favicon: st.favicon,
                    private: self.private_window,
                    ..Default::default()
                },
                now,
            );
            self.refresh_tab_meta(id);
            if let Some(g) = group {
                assignments.push((id, g));
            }
            if plan.active == Some(i) {
                active = Some(id);
            }
            restored += 1;
        }
        if !plan.groups.is_empty() {
            self.tabs.restore_groups(plan.groups, &assignments);
        }
        if let Some(a) = active {
            self.tabs.activate(a, now);
        }
        restored
    }

    fn restore_session(&mut self) {
        let now = now_ms();
        let mut sessions: Vec<Session> = self.db.kv_get("session_windows").ok().flatten().unwrap_or_default();
        if sessions.is_empty() {
            if let Some(legacy) = self.db.kv_get::<Session>("session").ok().flatten() {
                sessions.push(legacy);
            }
        }
        let mut plans: Vec<(RestorePlan, Option<WindowGeometry>)> =
            sessions.into_iter().map(|s| plan_restore_with_geometry(s, self.settings.startup)).collect();
        let first = if plans.is_empty() { None } else { Some(plans.remove(0)) };
        let mut restored = match first {
            Some((plan, _)) => self.restore_window_tabs(plan, now),
            None => 0,
        };
        // The other saved windows come back as windows of their own.
        for (plan, geometry) in plans {
            if plan.tabs.is_empty() {
                continue;
            }
            let tabs = plan.tabs;
            let active = plan.active.unwrap_or(0);
            let groups = plan.groups;
            if self.open_window(window_set::WindowSpec { tabs, active, geometry, private: false, empty: false, groups }).is_some() {
                restored += 1;
            }
        }
        let mut opened_from_args = 0;
        let load_all = self.load_all;
        for url in std::mem::take(&mut self.pending_urls) {
            // Normally each URL becomes the active tab in turn (so only the last one is loaded). With --load-all the
            // first stays active and the rest load hidden in the background.
            self.open_url(url, !load_all || opened_from_args == 0);
            opened_from_args += 1;
        }
        let start_on_new_tab = self.settings.startup == crate::settings::StartupBehavior::NewTab && opened_from_args == 0;
        if self.tabs.is_empty() || restored + opened_from_args == 0 || start_on_new_tab {
            self.new_tab(false, false);
        }
        log::info!("session restored with {} windows", self.window_order.len());
    }

    // ---- shell lifecycle ----------------------------------------------------------

    pub fn on_shell_ready(&mut self) -> Bootstrap {
        if !self.shell_ready {
            self.shell_ready = true;
            if let (Some(start), None) = (self.startup.process_start, self.startup.shell_ready_ms) {
                self.startup.shell_ready_ms = Some(now_ms() - start);
                log::info!("startup: shell UI ready {} ms after process start", now_ms() - start);
            }
            if let Some(w) = self.app.get_webview_window(&self.current) {
                let _ = w.show();
                let _ = w.set_focus();
            }
            // Tabs moved into this window before it was shown are re-hosted now that it is visible.
            for id in self.views.keys().copied().collect::<Vec<_>>() {
                let _ = self.app.run_on_main_thread(move || crate::native::rehost(id));
            }
        }
        self.dirty = Dirty { state: true, downloads: true, prompts: true, find: true, settings: true, bookmarks: false };
        Bootstrap {
            state: self.snapshot(),
            settings: self.settings.clone(),
            profile: self.profile.meta.clone(),
            first_run: self.first_run,
            downloads: self.downloads.clone(),
            prompts: self.window_prompts(),
            version: self.app.package_info().version.to_string(),
        }
    }

    pub fn set_layout(&mut self, content: ContentRect, viewport: Viewport) {
        if self.layout.content.is_none() {
            log::info!("startup: first page-area layout received ({}x{})", content.width, content.height);
        }
        self.layout.content = Some(content);
        self.layout.viewport = viewport;
        self.relayout();
    }

    pub fn window_geometry(&self) -> WindowGeometry {
        let v = self.layout.viewport;
        WindowGeometry { x: Some(v.screen_x), y: Some(v.screen_y), width: v.width, height: v.height, maximized: v.maximized }
    }

    /// Persists everything that must survive an exit. Called on window close.
    pub fn prepare_exit(&mut self) {
        self.save_session();
        self.save_shield_counter();
        // Older versions (and the very first window of this one) read the main window's size from "window".
        let keep = self.current.clone();
        if self.select_window(crate::window::SHELL_LABEL) {
            let v = self.layout.viewport;
            if v.width > 0.0 && v.height > 0.0 && !self.layout.browser_fullscreen {
                let geom = self.window_geometry();
                let _ = self.db.kv_set("window", &geom);
            }
        }
        self.select_window(&keep);
    }

    pub fn complete_first_run(&mut self, profile_name: Option<String>, search: Option<crate::navigation::SearchEngine>) -> Result<()> {
        if let Some(name) = profile_name.filter(|n| !n.trim().is_empty()) {
            self.profiles.rename(&self.profile.meta.id, &name)?;
            self.profile.meta.name = name.trim().to_string();
        }
        if let Some(engine) = search {
            self.settings.search_engine = engine;
            self.db.kv_set("settings", &self.settings)?;
            self.mark_settings();
        }
        self.db.kv_set("onboarded", &true)?;
        self.first_run = false;
        Ok(())
    }

    /// Adopts the default search engine read from another browser (explicitly requested in the import wizard).
    pub fn adopt_imported_search(&mut self, search: &crate::import::ImportedSearch) -> Result<()> {
        let mut next = self.settings.clone();
        match (&search.engine, &search.template) {
            (Some(engine), _) => {
                next.search_engine = *engine;
                next.custom_search_template = None;
            }
            (None, Some(template)) => next.custom_search_template = Some(template.clone()),
            (None, None) => return Err(Error::msg("no usable search engine")),
        }
        self.update_settings(next)?;
        Ok(())
    }

    pub fn update_settings(&mut self, next: Settings) -> Result<Settings> {
        let next = next.sanitized();
        if let Some(dir) = &next.download_dir {
            let p = std::path::Path::new(dir);
            if !p.is_absolute() || !p.is_dir() {
                return Err(Error::msg("Download folder must be an existing absolute path"));
            }
        }
        let prev = std::mem::replace(&mut self.settings, next.clone());
        self.db.kv_set("settings", &self.settings)?;
        if prev.download_dir != next.download_dir {
            crate::native::set_download_dir(next.download_dir.as_ref().map(Into::into));
        }
        if prev.clean_links != next.clean_links {
            crate::native::set_clean_links(next.clean_links);
        }
        if prev.popup_allowed_hosts != next.popup_allowed_hosts || prev.new_window_requests != next.new_window_requests {
            crate::native::set_popup_policy(next.popup_allowed_hosts.clone(), next.new_window_requests == crate::settings::NewWindowRequests::Window);
        }
        if prev.save_passwords != next.save_passwords || prev.autofill != next.autofill {
            let private: Vec<TabId> = self.all_tabs().iter().filter(|t| t.private).map(|t| t.id).collect();
            let (sp, af) = (next.save_passwords, next.autofill);
            let _ = self.app.run_on_main_thread(move || crate::native::apply_tab_settings(sp, af, private));
        }
        if prev.gaming_mode != next.gaming_mode {
            self.apply_gaming(next.gaming_mode);
        }
        if prev.memory_limit_mb != next.memory_limit_mb {
            // Turning the limit on starts the (slow) sampler; turning it off stops it.
            self.perf_reschedule();
        }
        if prev.search_engine != next.search_engine || prev.custom_search_template != next.custom_search_template {
            // nothing to do eagerly; used on next navigation
        }
        self.mark_settings();
        self.mark_state();
        Ok(self.settings.clone())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub state: BrowserState,
    pub settings: Settings,
    pub profile: crate::profile::ProfileMeta,
    pub first_run: bool,
    pub downloads: Vec<Download>,
    pub prompts: Vec<PermissionPrompt>,
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::StartupBehavior;
    use crate::tabs::{Lifecycle, NewTab};

    fn st(url: &str, pinned: bool) -> SessionTab {
        SessionTab { url: url.into(), title: format!("title of {url}"), pinned, favicon: None, group: None }
    }

    fn session() -> Session {
        Session {
            tabs: vec![
                st("https://pinned.example/", true),
                st("browser://removed-page", false),
                st("https://b.example/", false),
                st("browser://settings", false),
            ],
            active: 2,
            geometry: None,
            groups: Vec::new(),
        }
    }

    #[test]
    fn restore_session_keeps_valid_tabs_and_the_selected_one() {
        let plan = plan_restore(session(), StartupBehavior::RestoreSession);
        let urls: Vec<&str> = plan.tabs.iter().map(|t| t.url.as_str()).collect();
        assert_eq!(urls, ["https://pinned.example/", "https://b.example/", "browser://settings"]);
        // The obsolete internal page was dropped, so the saved index 2 now points at position 1.
        assert_eq!(plan.active, Some(1));
        assert_eq!(plan.tabs[plan.active.unwrap()].url, "https://b.example/");
    }

    #[test]
    fn a_selected_tab_that_no_longer_exists_falls_back_to_the_first() {
        let mut s = session();
        s.active = 1; // the removed internal page
        let plan = plan_restore(s, StartupBehavior::RestoreSession);
        assert_eq!(plan.active, None);
        assert_eq!(plan.tabs.len(), 3);
    }

    #[test]
    fn new_tab_startup_only_keeps_pinned_tabs() {
        let plan = plan_restore(session(), StartupBehavior::NewTab);
        assert_eq!(plan.tabs, vec![st("https://pinned.example/", true)]);
        assert_eq!(plan.active, None);
    }

    #[test]
    fn groups_survive_a_restore_only_while_they_have_tabs() {
        let mut s = session();
        s.tabs[2].group = Some(4);
        s.tabs[1].group = Some(5); // the removed internal page
        s.groups = vec![
            TabGroup { id: 4, title: "Kept".into(), color: "green".into(), collapsed: false },
            TabGroup { id: 5, title: "Gone".into(), color: "red".into(), collapsed: false },
        ];
        let plan = plan_restore(s, StartupBehavior::RestoreSession);
        assert_eq!(plan.groups.iter().map(|g| g.id).collect::<Vec<_>>(), vec![4]);
        assert_eq!(plan.tabs.iter().filter_map(|t| t.group).collect::<Vec<_>>(), vec![4]);
        // Sessions saved before groups existed still load.
        let old: Session = serde_json::from_str(r#"{"tabs":[{"url":"https://a/","title":"A","pinned":false,"favicon":null}],"active":0}"#).unwrap();
        assert!(old.groups.is_empty() && old.tabs[0].group.is_none());
    }

    #[test]
    fn empty_or_missing_session_restores_nothing() {
        for startup in [StartupBehavior::RestoreSession, StartupBehavior::NewTab] {
            let plan = plan_restore(Session::default(), startup);
            assert!(plan.tabs.is_empty());
            assert_eq!(plan.active, None);
        }
    }

    #[test]
    fn session_survives_a_database_round_trip_with_stable_field_names() {
        let json = serde_json::to_string(&session()).unwrap();
        assert!(json.contains("\"pinned\"") && json.contains("\"active\":2"), "{json}");
        let back: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(back.active, 2);
        assert_eq!(back.tabs, session().tabs);
        // Sessions written by earlier builds only had these fields; they must keep loading.
        let old: Session = serde_json::from_str(r#"{"tabs":[{"url":"https://a/","title":"A","pinned":false,"favicon":null}],"active":0}"#).unwrap();
        assert_eq!(old.tabs.len(), 1);
    }

    #[test]
    fn restored_tabs_are_lazy_ordered_and_selected_like_the_saved_session() {
        // Mirrors Controller::restore_session on the pure tab model.
        let plan = plan_restore(session(), StartupBehavior::RestoreSession);
        let mut strip = crate::tabs::TabStrip::new();
        let mut active = None;
        for (i, t) in plan.tabs.into_iter().enumerate() {
            let id = strip.create(NewTab { url: t.url, pinned: t.pinned, title: Some(t.title), favicon: t.favicon, ..Default::default() }, 1);
            if plan.active == Some(i) {
                active = Some(id);
            }
        }
        strip.activate(active.unwrap(), 2);
        assert_eq!(strip.active().unwrap().url, "https://b.example/");
        assert!(strip.tabs()[0].pinned, "pinned tabs come first");
        assert!(strip.tabs().iter().all(|t| t.lifecycle == Lifecycle::Unloaded), "no webview exists until a tab is shown");
        assert_eq!(strip.tabs()[1].title, "title of https://b.example/");
    }
}
