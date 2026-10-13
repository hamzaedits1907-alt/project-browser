//! WebView2 integration.
//!
//! Every function in this module must run on the main (UI) thread: WebView2
//! COM objects are single-threaded and are kept in thread-local registries.
//! Event handlers never block; they only post work to the controller thread.

pub mod context_menu;
pub mod dialog;
pub mod extensions;
pub mod metrics;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::webview::PlatformWebview;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{Interface, BOOL, HSTRING, PWSTR};
use windows::Win32::Foundation::HGLOBAL;
use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
use windows::Win32::System::Com::{IStream, STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET};
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;

use crate::controller::ControllerHandle;
use crate::shortcuts::{map_key, Mods};
use crate::tabs::TabId;

#[derive(Clone)]
struct TabCom {
    controller: ICoreWebView2Controller,
    webview: ICoreWebView2,
    env: ICoreWebView2Environment,
}

struct PendingPermission {
    args: ICoreWebView2PermissionRequestedEventArgs,
    deferral: ICoreWebView2Deferral,
}

thread_local! {
    static TABS: RefCell<HashMap<TabId, TabCom>> = RefCell::new(HashMap::new());
    static FIND_HOOKED: RefCell<std::collections::HashSet<TabId>> = RefCell::new(Default::default());
    static DOWNLOADS: RefCell<HashMap<i64, ICoreWebView2DownloadOperation>> = RefCell::new(HashMap::new());
    static PERMISSIONS: RefCell<HashMap<u64, PendingPermission>> = RefCell::new(HashMap::new());
    /// Downloads held (deferred) while the user picks where to save them ("Ask where to save each file").
    static ASKING: RefCell<HashMap<i64, AskingDownload>> = RefCell::new(HashMap::new());
    /// The shell webview of every browser window, by window label.
    static SHELLS: RefCell<HashMap<String, TabCom>> = RefCell::new(HashMap::new());
    /// Host of the page each tab is loading, for the Shield's first-party checks and the pop-up policy.
    static TOP_HOST: RefCell<HashMap<TabId, String>> = RefCell::new(HashMap::new());
    /// What WebView2 said about the latest new-window request of each tab (see `PopupIntent`).
    static POPUP_INTENT: RefCell<HashMap<TabId, PopupIntent>> = RefCell::new(HashMap::new());
    /// An address the user asked to load as it was ("Undo" on Link Cleaner's notice): not cleaned once.
    static CLEAN_BYPASS: RefCell<HashMap<TabId, String>> = RefCell::new(HashMap::new());
    /// The last address Link Cleaner produced per tab, and when: a site that adds the parameters back by redirecting
    /// gets its way the second time instead of looping.
    static LAST_CLEAN: RefCell<HashMap<TabId, (String, Instant)>> = RefCell::new(HashMap::new());
}

static NEXT_PERMISSION: AtomicU64 = AtomicU64::new(1);
/// Sites allowed to open pop-ups without a user action, and whether page-requested new windows open as browser windows.
static POPUP_ALLOWED: Mutex<Vec<String>> = Mutex::new(Vec::new());
static NEW_WINDOWS: AtomicBool = AtomicBool::new(false);
/// Link Cleaner on (see `linkclean.rs`).
static CLEAN_LINKS: AtomicBool = AtomicBool::new(true);
/// Sites Link Cleaner leaves alone (settings.clean_links_exceptions).
static CLEAN_EXCEPT: Mutex<Vec<String>> = Mutex::new(Vec::new());
static NEXT_DOWNLOAD: AtomicI64 = AtomicI64::new(0);
static DOWNLOAD_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
/// "Ask where to save each file" (settings.download_ask).
static DOWNLOAD_ASK: AtomicBool = AtomicBool::new(false);

struct AskingDownload {
    args: ICoreWebView2DownloadStartingEventArgs,
    deferral: ICoreWebView2Deferral,
    tab: Option<TabId>,
}

// ---- frame-info guard -------------------------------------------------------------------
//
// Mapping renderer processes to tabs uses `ICoreWebView2ProcessExtendedInfo::AssociatedFrameInfos`.
// On WebView2 runtime 154.0.4258.62 a call into that iterator crashed the whole process (an access
// violation inside EmbeddedBrowserWebView.dll, found with a debugger and symbolised in docs/CRASH-0.1.0.md).
// The code now keeps every COM object of that chain alive for the whole walk, but a native fault cannot
// be caught, so the first walk of each run leaves a marker file behind. If the app dies mid-walk the
// marker is still there on the next start and the API stays off for that runtime version (per-tab memory
// is then reported as unavailable instead of crashing). A different runtime version tries again.

static FRAME_INFO_ENABLED: AtomicBool = AtomicBool::new(true);
static FRAME_INFO_VERIFIED: AtomicBool = AtomicBool::new(false);
static FRAME_GUARD: Mutex<Option<(PathBuf, String)>> = Mutex::new(None);

const FRAME_GUARD_FILE: &str = "frame-info-guard.txt";

/// Environment switch: `off` never reads frame infos, `on` ignores a stored crash marker.
pub const FRAME_INFO_ENV: &str = "PROJECT_BROWSER_FRAME_INFO";

/// What to do at startup given the marker left by the previous run (pure, unit-tested).
/// Returns whether frame infos may be read and the marker to leave on disk (`None` = delete it).
pub fn guard_decision(marker: Option<&str>, runtime: &str) -> (bool, Option<String>) {
    match marker.map(str::trim) {
        Some(m) => match m.split_once(':') {
            // The previous run started a walk and never finished it: it crashed inside it.
            Some(("pending", v)) | Some(("disabled", v)) if v == runtime => (false, Some(format!("disabled:{runtime}"))),
            _ => (true, None),
        },
        None => (true, None),
    }
}

/// Reads the marker, decides, and remembers where to write the next one. Call once at startup.
pub fn init_frame_guard(dir: &std::path::Path, runtime: &str, env: Option<&str>) {
    let path = dir.join(FRAME_GUARD_FILE);
    let stored = std::fs::read_to_string(&path).ok();
    let (mut enabled, marker) = guard_decision(stored.as_deref(), runtime);
    match env.map(str::trim) {
        Some("off") => {
            enabled = false;
            log::warn!("frame info reading switched off by {FRAME_INFO_ENV}");
        }
        Some("on") if !enabled => {
            enabled = true;
            log::warn!("frame info reading forced on by {FRAME_INFO_ENV} despite an earlier crash");
        }
        _ => {}
    }
    match (&marker, enabled) {
        (Some(m), false) => {
            let _ = std::fs::write(&path, m);
            log::warn!("frame infos disabled for WebView2 {runtime}: the previous run crashed while reading them");
        }
        _ => {
            let _ = std::fs::remove_file(&path);
        }
    }
    FRAME_INFO_ENABLED.store(enabled, Ordering::SeqCst);
    if let Ok(mut g) = FRAME_GUARD.lock() {
        *g = Some((path, runtime.to_string()));
    }
    log::info!("frame info reading: {}", if enabled { "enabled" } else { "disabled" });
}

pub fn frame_info_enabled() -> bool {
    FRAME_INFO_ENABLED.load(Ordering::SeqCst)
}

/// Leaves the "walk in progress" marker (first walk of this run only).
fn frame_guard_begin() {
    if FRAME_INFO_VERIFIED.load(Ordering::SeqCst) {
        return;
    }
    if let Ok(g) = FRAME_GUARD.lock() {
        if let Some((path, runtime)) = g.as_ref() {
            let _ = std::fs::write(path, format!("pending:{runtime}"));
            log::info!("frame info walk starting (first of this run)");
        }
    }
}

/// The walk finished without crashing: remove the marker.
fn frame_guard_success() {
    if FRAME_INFO_VERIFIED.swap(true, Ordering::SeqCst) {
        return;
    }
    if let Ok(g) = FRAME_GUARD.lock() {
        if let Some((path, _)) = g.as_ref() {
            let _ = std::fs::remove_file(path);
        }
    }
    log::info!("frame info walk finished");
}

/// The part of a new-window request that only the native event knows: whether a user action caused it.
#[derive(Debug, Clone)]
pub struct PopupIntent {
    pub uri: String,
    pub user_initiated: bool,
}

/// Takes the intent recorded for the tab's latest `NewWindowRequested`. The wry handler that decides what to do with the
/// request runs after the native event handlers returned, on the same thread, so the intent is always there by then.
pub fn take_popup_intent(id: TabId) -> Option<PopupIntent> {
    POPUP_INTENT.with(|m| m.borrow_mut().remove(&id))
}

pub fn set_popup_policy(allowed_hosts: Vec<String>, new_windows: bool) {
    if let Ok(mut a) = POPUP_ALLOWED.lock() {
        *a = allowed_hosts;
    }
    NEW_WINDOWS.store(new_windows, Ordering::Relaxed);
}

/// May the page loaded in the tab open pop-ups that no user action asked for?
pub fn popups_allowed_for(id: TabId) -> bool {
    let host = TOP_HOST.with(|t| t.borrow().get(&id).cloned()).unwrap_or_default();
    !host.is_empty() && POPUP_ALLOWED.lock().map(|a| a.iter().any(|d| crate::shield::rules::host_is_or_under(&host, d))).unwrap_or(false)
}

pub fn set_clean_links(on: bool) {
    CLEAN_LINKS.store(on, Ordering::Relaxed);
}

pub fn set_clean_exceptions(sites: Vec<String>) {
    if let Ok(mut e) = CLEAN_EXCEPT.lock() {
        *e = sites;
    }
}

/// Loads `url` in tab `id` exactly as it was, without Link Cleaner touching it this once.
pub fn load_uncleaned(id: TabId, url: String) {
    CLEAN_BYPASS.with(|b| b.borrow_mut().insert(id, url.clone()));
    navigate(id, &url);
}

/// The address Link Cleaner loads instead of `uri`, if it applies (and is not looping on a redirect).
fn link_clean_target(id: TabId, uri: &str) -> Option<String> {
    if !CLEAN_LINKS.load(Ordering::Relaxed) {
        return None;
    }
    if CLEAN_BYPASS.with(|b| b.borrow().get(&id).is_some_and(|u| u == uri)) {
        CLEAN_BYPASS.with(|b| b.borrow_mut().remove(&id));
        return None;
    }
    let host = url::Url::parse(uri).ok().and_then(|u| u.host_str().map(str::to_ascii_lowercase)).unwrap_or_default();
    if CLEAN_EXCEPT.lock().map(|e| e.iter().any(|site| crate::shield::rules::host_is_or_under(&host, site))).unwrap_or(false) {
        return None;
    }
    let clean = crate::linkclean::clean(uri)?;
    let repeat = LAST_CLEAN.with(|m| m.borrow().get(&id).is_some_and(|(last, at)| *last == clean && at.elapsed() < Duration::from_secs(10)));
    if repeat {
        return None;
    }
    LAST_CLEAN.with(|m| m.borrow_mut().insert(id, (clean.clone(), Instant::now())));
    Some(clean)
}

pub fn new_windows_open_as_windows() -> bool {
    NEW_WINDOWS.load(Ordering::Relaxed)
}

/// Sets the folder new downloads are saved to (`None` = WebView2's default, the user's Downloads folder).
pub fn set_download_dir(dir: Option<PathBuf>) {
    if let Ok(mut d) = DOWNLOAD_DIR.lock() {
        *d = dir;
    }
}

pub fn set_download_ask(ask: bool) {
    DOWNLOAD_ASK.store(ask, Ordering::SeqCst);
}

fn next_download_id() -> i64 {
    // Seeded from the clock so ids stay unique across restarts without a DB round-trip.
    let _ = NEXT_DOWNLOAD.compare_exchange(0, crate::error::now_ms() * 1000, Ordering::SeqCst, Ordering::SeqCst);
    NEXT_DOWNLOAD.fetch_add(1, Ordering::SeqCst)
}

/// A shell webview to ask profile-wide things of (every window shares the profile). The main window's when it exists.
fn any_shell() -> Option<TabCom> {
    SHELLS.with(|s| {
        let shells = s.borrow();
        shells.get(crate::window::SHELL_LABEL).or_else(|| shells.values().next()).cloned()
    })
}

fn tab_com(id: TabId) -> Option<TabCom> {
    TABS.with(|t| t.borrow().get(&id).cloned())
}

pub(crate) fn pwstr(f: impl FnOnce(&mut PWSTR) -> windows::core::Result<()>) -> String {
    let mut p = PWSTR::null();
    match f(&mut p) {
        Ok(()) => take_pwstr(p),
        Err(_) => String::new(),
    }
}

pub(crate) fn read_bool(f: impl FnOnce(&mut BOOL) -> windows::core::Result<()>) -> bool {
    let mut b = BOOL::default();
    f(&mut b).map(|_| b.as_bool()).unwrap_or(false)
}

fn key_down(vk: i32) -> bool {
    unsafe { GetKeyState(vk) < 0 }
}

pub fn current_mods() -> Mods {
    Mods { ctrl: key_down(0x11), shift: key_down(0x10), alt: key_down(0x12) }
}

/// Which webview a shortcut was typed into, so it acts on that webview's window.
#[derive(Clone)]
enum AccelOrigin {
    Tab(TabId),
    Shell(String),
}

/// Routes browser shortcuts typed into any webview to the controller.
unsafe fn hook_accelerators(controller: &ICoreWebView2Controller, ctl: &ControllerHandle, origin: AccelOrigin) -> windows::core::Result<()> {
    let ctl = ctl.clone();
    let mut token = 0i64;
    controller.add_AcceleratorKeyPressed(
        &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
            args.KeyEventKind(&mut kind)?;
            if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN {
                return Ok(());
            }
            let mut vk = 0u32;
            args.VirtualKey(&mut vk)?;
            if let Some(cmd) = map_key(vk, current_mods()) {
                args.SetHandled(true)?;
                match &origin {
                    AccelOrigin::Tab(id) => ctl.post_tab(*id, move |c| c.run_command(cmd)),
                    AccelOrigin::Shell(label) => ctl.post_window(label.clone(), move |c| c.run_command(cmd)),
                }
            }
            Ok(())
        })),
        &mut token,
    )
}

pub struct TabInit {
    pub url: String,
    pub save_passwords: bool,
    pub autofill: bool,
    pub private: bool,
}

/// Wires up a freshly created tab webview and starts its first navigation.
pub fn install_tab(id: TabId, pw: PlatformWebview, ctl: ControllerHandle, init: TabInit) {
    let result = unsafe { install_tab_inner(id, &pw, &ctl, &init) };
    match result {
        Ok(()) => log::info!("tab {id}: native hooks installed, first navigation started"),
        Err(e) => {
            log::error!("failed to initialise tab {id}: {e}");
            let msg = e.message().to_string();
            ctl.post_tab(id, move |c| c.on_native_error(id, msg));
        }
    }
}

unsafe fn install_tab_inner(id: TabId, pw: &PlatformWebview, ctl: &ControllerHandle, init: &TabInit) -> windows::core::Result<()> {
    let controller = pw.controller();
    let env = pw.environment();
    let webview = controller.CoreWebView2()?;

    let settings = webview.Settings()?;
    settings.SetIsStatusBarEnabled(true)?; // link-hover URL bubble
    settings.SetIsZoomControlEnabled(true)?;
    settings.SetAreDevToolsEnabled(true)?;
    settings.SetAreDefaultContextMenusEnabled(true)?;
    if let Ok(s3) = settings.cast::<ICoreWebView2Settings3>() {
        s3.SetAreBrowserAcceleratorKeysEnabled(true)?;
    }
    if let Ok(s4) = settings.cast::<ICoreWebView2Settings4>() {
        s4.SetIsPasswordAutosaveEnabled(init.save_passwords && !init.private)?;
        s4.SetIsGeneralAutofillEnabled(init.autofill)?;
    }

    let mut token = 0i64;

    let c = ctl.clone();
    webview.add_NavigationStarting(
        &NavigationStartingEventHandler::create(Box::new(move |sender, args| {
            let Some(args) = args else { return Ok(()) };
            let uri = pwstr(|p| args.Uri(p));
            if let Some(clean) = link_clean_target(id, &uri) {
                // Cancelled here, loaded again without the tracking parameters (the controller navigates).
                args.SetCancel(true)?;
                let original = uri.clone();
                c.post_tab(id, move |ctl| ctl.on_link_cleaned(id, clean, original));
                return Ok(());
            }
            note_top_navigation(id, &uri, &c);
            if let Some(wv) = sender {
                sync_youtube_guard(id, &wv);
            }
            c.post_tab(id, move |ctl| ctl.on_nav_starting(id, uri));
            Ok(())
        })),
        &mut token,
    )?;

    // WebView2 knows whether a user action opened a new window; wry's handler (which decides what happens to the
    // request) does not. Record it here for that handler.
    webview.add_NewWindowRequested(
        &NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let uri = pwstr(|p| args.Uri(p));
            let user_initiated = read_bool(|b| args.IsUserInitiated(b));
            POPUP_INTENT.with(|m| m.borrow_mut().insert(id, PopupIntent { uri, user_initiated }));
            Ok(())
        })),
        &mut token,
    )?;

    let c = ctl.clone();
    webview.add_SourceChanged(
        &SourceChangedEventHandler::create(Box::new(move |sender, args| {
            let (Some(wv), Some(args)) = (sender, args) else { return Ok(()) };
            let uri = pwstr(|p| wv.Source(p));
            let new_doc = read_bool(|b| args.IsNewDocument(b));
            c.post_tab(id, move |ctl| ctl.on_source_changed(id, uri, new_doc));
            Ok(())
        })),
        &mut token,
    )?;

    let c = ctl.clone();
    webview.add_HistoryChanged(
        &HistoryChangedEventHandler::create(Box::new(move |sender, _| {
            let Some(wv) = sender else { return Ok(()) };
            let back = read_bool(|b| wv.CanGoBack(b));
            let fwd = read_bool(|b| wv.CanGoForward(b));
            c.post_tab(id, move |ctl| ctl.on_history_changed(id, back, fwd));
            Ok(())
        })),
        &mut token,
    )?;

    let c = ctl.clone();
    webview.add_NavigationCompleted(
        &NavigationCompletedEventHandler::create(Box::new(move |sender, args| {
            let (Some(wv), Some(args)) = (sender, args) else { return Ok(()) };
            let success = read_bool(|b| args.IsSuccess(b));
            let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
            let _ = args.WebErrorStatus(&mut status);
            let error = (!success).then(|| web_error(status));
            let source = pwstr(|p| wv.Source(p));
            let title = pwstr(|p| wv.DocumentTitle(p));
            c.post_tab(id, move |ctl| ctl.on_nav_completed(id, success, error, source, title));
            Ok(())
        })),
        &mut token,
    )?;

    let c = ctl.clone();
    webview.add_DocumentTitleChanged(
        &DocumentTitleChangedEventHandler::create(Box::new(move |sender, _| {
            let Some(wv) = sender else { return Ok(()) };
            let title = pwstr(|p| wv.DocumentTitle(p));
            c.post_tab(id, move |ctl| ctl.on_title(id, title));
            Ok(())
        })),
        &mut token,
    )?;

    let c = ctl.clone();
    webview.add_ContainsFullScreenElementChanged(
        &ContainsFullScreenElementChangedEventHandler::create(Box::new(move |sender, _| {
            let Some(wv) = sender else { return Ok(()) };
            let fs = read_bool(|b| wv.ContainsFullScreenElement(b));
            c.post_tab(id, move |ctl| ctl.on_fullscreen_element(id, fs));
            Ok(())
        })),
        &mut token,
    )?;

    let c = ctl.clone();
    webview.add_ProcessFailed(
        &ProcessFailedEventHandler::create(Box::new(move |_, _| {
            c.post_tab(id, move |ctl| ctl.on_crashed(id));
            Ok(())
        })),
        &mut token,
    )?;

    let c = ctl.clone();
    webview.add_PermissionRequested(
        &PermissionRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
            args.PermissionKind(&mut kind)?;
            let Some(kind) = permission_name(kind) else { return Ok(()) };
            if let Ok(a3) = args.cast::<ICoreWebView2PermissionRequestedEventArgs3>() {
                // We persist decisions ourselves so users can review and revoke them.
                let _ = a3.SetSavesInProfile(false);
            }
            let deferral = args.GetDeferral()?;
            let uri = pwstr(|p| args.Uri(p));
            let req = NEXT_PERMISSION.fetch_add(1, Ordering::SeqCst);
            PERMISSIONS.with(|p| p.borrow_mut().insert(req, PendingPermission { args: args.clone(), deferral }));
            c.post_tab(id, move |ctl| ctl.on_permission_request(req, id, uri, kind));
            Ok(())
        })),
        &mut token,
    )?;

    if let Ok(wv4) = webview.cast::<ICoreWebView2_4>() {
        let c = ctl.clone();
        wv4.add_DownloadStarting(
            &DownloadStartingEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let owner = tab_com(id).and_then(|t| {
                    let mut hwnd = windows::Win32::Foundation::HWND::default();
                    t.controller.ParentWindow(&mut hwnd).ok()?;
                    Some(windows::Win32::UI::WindowsAndMessaging::GetAncestor(hwnd, windows::Win32::UI::WindowsAndMessaging::GA_ROOT).0 as isize)
                });
                start_download(&args, Some(id), &c, owner.unwrap_or(0))
            })),
            &mut token,
        )?;
    }

    if let Ok(wv8) = webview.cast::<ICoreWebView2_8>() {
        let c = ctl.clone();
        let audio_handler = move |sender: Option<ICoreWebView2>| -> windows::core::Result<()> {
            let Some(wv8) = sender.and_then(|s| s.cast::<ICoreWebView2_8>().ok()) else { return Ok(()) };
            let playing = read_bool(|b| wv8.IsDocumentPlayingAudio(b));
            let muted = read_bool(|b| wv8.IsMuted(b));
            c.post_tab(id, move |ctl| ctl.on_audio(id, playing, muted));
            Ok(())
        };
        let h2 = audio_handler.clone();
        wv8.add_IsDocumentPlayingAudioChanged(
            &IsDocumentPlayingAudioChangedEventHandler::create(Box::new(move |s, _| audio_handler(s))),
            &mut token,
        )?;
        wv8.add_IsMutedChanged(&IsMutedChangedEventHandler::create(Box::new(move |s, _| h2(s))), &mut token)?;
    }

    if let Ok(wv14) = webview.cast::<ICoreWebView2_14>() {
        let c = ctl.clone();
        wv14.add_ServerCertificateErrorDetected(
            &ServerCertificateErrorDetectedEventHandler::create(Box::new(move |_, _| {
                // Leave the default action: WebView2 shows its TLS interstitial.
                c.post_tab(id, move |ctl| ctl.on_cert_error(id));
                Ok(())
            })),
            &mut token,
        )?;
    }

    if let Ok(wv15) = webview.cast::<ICoreWebView2_15>() {
        let c = ctl.clone();
        wv15.add_FaviconChanged(
            &FaviconChangedEventHandler::create(Box::new(move |sender, _| {
                let Some(wv15) = sender.and_then(|s| s.cast::<ICoreWebView2_15>().ok()) else { return Ok(()) };
                let uri = pwstr(|p| wv15.FaviconUri(p));
                c.post_tab(id, move |ctl| ctl.on_favicon(id, uri));
                Ok(())
            })),
            &mut token,
        )?;
    }

    let c = ctl.clone();
    controller.add_ZoomFactorChanged(
        &ZoomFactorChangedEventHandler::create(Box::new(move |sender, _| {
            let Some(ctrl) = sender else { return Ok(()) };
            let mut z = 1.0;
            ctrl.ZoomFactor(&mut z)?;
            c.post_tab(id, move |ctl| ctl.on_zoom(id, z));
            Ok(())
        })),
        &mut token,
    )?;

    hook_accelerators(&controller, ctl, AccelOrigin::Tab(id))?;

    // Split view: clicking into a half makes it the active tab.
    let c = ctl.clone();
    controller.add_GotFocus(
        &FocusChangedEventHandler::create(Box::new(move |_, _| {
            c.post_tab(id, move |ctl| ctl.on_tab_focused(id));
            Ok(())
        })),
        &mut token,
    )?;

    if let Err(e) = context_menu::hook(&webview, context_menu::MenuOwner::Tab(id), ctl) {
        log::warn!("tab {id}: the browser's own right-click menu is unavailable for this tab: {e}");
    }

    if let Ok(wv2) = webview.cast::<ICoreWebView2_2>() {
        // Project Shield's element hiding: one stylesheet of the site's own hiding rules, added once the document exists.
        wv2.add_DOMContentLoaded(
            &DOMContentLoadedEventHandler::create(Box::new(move |sender, _| {
                let Some(wv) = sender else { return Ok(()) };
                let host = TOP_HOST.with(|t| t.borrow().get(&id).cloned()).unwrap_or_default();
                if crate::shield::youtube::is_youtube(&host) && crate::shield::global().is_some_and(|s| s.on_for(&host)) {
                    // In case the document-creation script was not registered in time for this page (it does nothing
                    // when it already ran).
                    let script = HSTRING::from(crate::shield::youtube::SCRIPT);
                    let _ = wv.ExecuteScript(&script, &ExecuteScriptCompletedHandler::create(Box::new(|_, _| Ok(()))));
                }
                let Some(css) = crate::shield::global().and_then(|s| s.cosmetic_css(&host)) else { return Ok(()) };
                let Ok(text) = serde_json::to_string(&css) else { return Ok(()) };
                let script = format!(
                    "(()=>{{const s=document.createElement('style');s.dataset.projectShield='';s.textContent={text};(document.head||document.documentElement).appendChild(s);}})()"
                );
                let _ = wv.ExecuteScript(&HSTRING::from(script), &ExecuteScriptCompletedHandler::create(Box::new(|_, _| Ok(()))));
                Ok(())
            })),
            &mut token,
        )?;
    }

    if let Err(e) = hook_shield(&webview, &env, id, init.private, ctl) {
        log::warn!("tab {id}: request blocking is unavailable for this tab: {e}");
    }

    TABS.with(|t| t.borrow_mut().insert(id, TabCom { controller, webview: webview.clone(), env }));
    webview.Navigate(&HSTRING::from(init.url.as_str()))?;
    Ok(())
}

// ---- Project Shield: request blocking ---------------------------------------------------------

thread_local! {
    /// Tabs with Project Shield's YouTube script registered: its WebView2 script id, or `None` while registering.
    static YT_GUARD: RefCell<HashMap<TabId, Option<String>>> = RefCell::new(HashMap::new());
}

/// On a navigation to YouTube, registers the Shield's YouTube script for the tab if the Shield is on for YouTube, or
/// removes it if the Shield has been switched off for it. Other sites leave it as it is (the script checks the host).
fn sync_youtube_guard(id: TabId, webview: &ICoreWebView2) {
    let host = TOP_HOST.with(|t| t.borrow().get(&id).cloned()).unwrap_or_default();
    if !crate::shield::youtube::is_youtube(&host) {
        return;
    }
    let want = crate::shield::global().is_some_and(|s| s.on_for(&host));
    let have = YT_GUARD.with(|g| g.borrow().contains_key(&id));
    unsafe {
        if want && !have {
            YT_GUARD.with(|g| g.borrow_mut().insert(id, None));
            let wv = webview.clone();
            let _ = webview.AddScriptToExecuteOnDocumentCreated(
                &HSTRING::from(crate::shield::youtube::SCRIPT),
                &AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(move |res, script_id| {
                    let kept = YT_GUARD.with(|g| match g.borrow_mut().get_mut(&id) {
                        Some(slot) if res.is_ok() => {
                            *slot = Some(script_id.clone());
                            true
                        }
                        _ => false,
                    });
                    if !kept {
                        YT_GUARD.with(|g| g.borrow_mut().remove(&id));
                        if res.is_ok() {
                            // Switched off (or the tab went away) while registering.
                            let _ = wv.RemoveScriptToExecuteOnDocumentCreated(&HSTRING::from(script_id));
                        }
                    }
                    Ok(())
                })),
            );
        } else if !want && have {
            if let Some(Some(script_id)) = YT_GUARD.with(|g| g.borrow_mut().remove(&id)) {
                let _ = webview.RemoveScriptToExecuteOnDocumentCreated(&HSTRING::from(script_id));
            }
        }
    }
}

/// A page is starting to load in the tab: remember its host and start the tab's blocked count again.
fn note_top_navigation(id: TabId, uri: &str, ctl: &ControllerHandle) {
    TOP_HOST.with(|t| t.borrow_mut().insert(id, crate::shield::host_of_url(uri)));
    if let Some(shield) = crate::shield::global() {
        if shield.reset_tab(id) {
            ctl.post(|c| c.mark_shield());
        }
    }
}

fn resource_kind(context: COREWEBVIEW2_WEB_RESOURCE_CONTEXT) -> u16 {
    use crate::shield::rules::*;
    match context {
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_SCRIPT => T_SCRIPT,
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE => T_IMAGE,
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_STYLESHEET => T_STYLESHEET,
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_XML_HTTP_REQUEST | COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FETCH => T_XHR,
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_MEDIA => T_MEDIA,
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FONT => T_FONT,
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_PING | COREWEBVIEW2_WEB_RESOURCE_CONTEXT_CSP_VIOLATION_REPORT => T_PING,
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_WEBSOCKET => T_WEBSOCKET,
        _ => T_OTHER,
    }
}

/// Offers every sub-resource request and every sub-frame navigation of the tab to the Shield. A blocked
/// request is answered with an empty 403 before it leaves the machine; a blocked frame navigation is cancelled.
/// Main-frame page loads are never touched: WebView2 reports them with the same context as frames.
unsafe fn hook_shield(
    webview: &ICoreWebView2,
    env: &ICoreWebView2Environment,
    id: TabId,
    private: bool,
    ctl: &ControllerHandle,
) -> windows::core::Result<()> {
    use crate::shield::rules::{Verdict, T_SUBDOCUMENT};
    let mut filtered = false;
    if let Ok(w22) = webview.cast::<ICoreWebView2_22>() {
        // Also covers requests made by dedicated, shared and service workers.
        filtered = w22
            .AddWebResourceRequestedFilterWithRequestSourceKinds(
                windows::core::w!("*"),
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
            )
            .is_ok();
    }
    if !filtered {
        webview.AddWebResourceRequestedFilter(windows::core::w!("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)?;
    }

    let mut token = 0i64;
    let env = env.clone();
    let c = ctl.clone();
    webview.add_WebResourceRequested(
        &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let Some(shield) = crate::shield::global() else { return Ok(()) };
            if !shield.is_active() {
                return Ok(());
            }
            let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT::default();
            args.ResourceContext(&mut context)?;
            if context == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT {
                return Ok(());
            }
            let uri = pwstr(|p| args.Request().and_then(|r| r.Uri(p)));
            let top = TOP_HOST.with(|t| t.borrow().get(&id).cloned()).unwrap_or_default();
            if let Verdict::Block { .. } = shield.check(&uri, &top, resource_kind(context)).verdict {
                let response = env.CreateWebResourceResponse(
                    None::<&IStream>,
                    403,
                    windows::core::w!("Blocked by Project Shield"),
                    windows::core::w!("Content-Type: text/plain"),
                )?;
                args.SetResponse(&response)?;
                if shield.record_block(id, private, &top, &uri) {
                    c.post(|ctl| ctl.mark_shield());
                }
            }
            Ok(())
        })),
        &mut token,
    )?;

    let c = ctl.clone();
    webview.add_FrameNavigationStarting(
        &NavigationStartingEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let Some(shield) = crate::shield::global() else { return Ok(()) };
            if !shield.is_active() {
                return Ok(());
            }
            let uri = pwstr(|p| args.Uri(p));
            let top = TOP_HOST.with(|t| t.borrow().get(&id).cloned()).unwrap_or_default();
            if let Verdict::Block { .. } = shield.check(&uri, &top, T_SUBDOCUMENT).verdict {
                args.SetCancel(true)?;
                if shield.record_block(id, private, &top, &uri) {
                    c.post(|ctl| ctl.mark_shield());
                }
            }
            Ok(())
        })),
        &mut token,
    )?;
    Ok(())
}

/// Shell webview: browser accelerators (F5, Ctrl+F, Ctrl+P...) must not act on
/// the UI itself, and browser shortcuts are routed like in tabs.
pub fn install_shell(pw: PlatformWebview, ctl: ControllerHandle, devtools: bool, label: String) {
    let result: windows::core::Result<()> = unsafe {
        (|| {
            let controller = pw.controller();
            let webview = controller.CoreWebView2()?;
            let settings = webview.Settings()?;
            settings.SetIsZoomControlEnabled(false)?;
            settings.SetIsStatusBarEnabled(false)?;
            settings.SetAreDevToolsEnabled(devtools)?;
            if let Ok(s3) = settings.cast::<ICoreWebView2Settings3>() {
                s3.SetAreBrowserAcceleratorKeysEnabled(false)?;
            }
            if let Ok(s4) = settings.cast::<ICoreWebView2Settings4>() {
                s4.SetIsPasswordAutosaveEnabled(false)?;
                s4.SetIsGeneralAutofillEnabled(false)?;
            }
            hook_accelerators(&controller, &ctl, AccelOrigin::Shell(label.clone()))?;
            if let Err(e) = context_menu::hook(&webview, context_menu::MenuOwner::Shell(label.clone()), &ctl) {
                log::warn!("window {label}: the browser's own right-click menu is unavailable in the shell: {e}");
            }
            SHELLS.with(|s| s.borrow_mut().insert(label.clone(), TabCom { controller, webview, env: pw.environment() }));
            Ok(())
        })()
    };
    match result {
        Ok(()) => log::info!("shell webview of window {label} configured"),
        Err(e) => log::error!("failed to configure the shell webview of window {label}: {e}"),
    }
}

/// A window is gone: forget its shell webview.
pub fn remove_shell(label: &str) {
    SHELLS.with(|s| s.borrow_mut().remove(label));
    context_menu::forget(&context_menu::MenuOwner::Shell(label.to_string()));
}

/// The top-level window under a screen point (physical pixels), as a raw handle value. Used to find the browser
/// window a dragged tab is over; `WindowFromPoint` answers for the topmost window, so overlapping windows and other
/// programs covering ours are handled.
pub fn top_level_window_at(x: i32, y: i32) -> Option<isize> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::{GetAncestor, WindowFromPoint, GA_ROOT};
    unsafe {
        let hwnd = WindowFromPoint(POINT { x, y });
        if hwnd.0.is_null() {
            return None;
        }
        let root = GetAncestor(hwnd, GA_ROOT);
        (!root.0.is_null()).then_some(root.0 as isize)
    }
}

/// Makes `owner` the owner window of the top-level window `hwnd` (the menu window): it then stays above its owner and
/// Windows gives activation back to the owner when it hides.
pub fn set_owner(hwnd: isize, owner: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowLongPtrW, GWLP_HWNDPARENT};
    unsafe {
        SetWindowLongPtrW(HWND(hwnd as *mut _), GWLP_HWNDPARENT, owner);
    }
}

/// Makes a tab's webview render again after it moved to another browser window (tab tear-off or move).
///
/// wry moves a webview by re-parenting its container window, which WebView2 never hears about: the controller's
/// parent stays the same container, so WebView2 keeps tracking the old top-level window for visibility and
/// occlusion and the page can stay unpainted (the black page area seen in the 0.6.0 UI probe). Re-assigning the
/// parent window makes WebView2 re-evaluate its top-level window; the container is raised above the new window's
/// shell webview, told that its position changed, and the controller's visibility is cycled so a new frame is drawn.
pub fn rehost(id: TabId) {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE};
    let Some(t) = tab_com(id) else { return };
    unsafe {
        let mut container = windows::Win32::Foundation::HWND::default();
        if t.controller.ParentWindow(&mut container).is_err() || container.0.is_null() {
            return;
        }
        if let Err(e) = t.controller.SetParentWindow(container) {
            log::warn!("tab {id}: re-assigning the parent window failed: {e}");
        }
        let _ = SetWindowPos(container, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        let _ = t.controller.NotifyParentWindowPositionChanged();
        let visible = read_bool(|b| t.controller.IsVisible(b));
        if visible {
            let _ = t.controller.SetIsVisible(false);
            let _ = t.controller.SetIsVisible(true);
        }
        log::info!("tab {id}: webview re-hosted in its new window (visible={visible})");
    }
}

/// The user's Downloads folder (wherever Windows has it), for files the browser saves itself (screenshots).
pub fn downloads_folder() -> Option<PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None).ok()?;
        let path = p.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(p.0 as _));
        path.filter(|d| d.is_dir())
    }
}

pub fn remove_tab(id: TabId) {
    context_menu::forget(&context_menu::MenuOwner::Tab(id));
    TABS.with(|t| t.borrow_mut().remove(&id));
    TOP_HOST.with(|t| t.borrow_mut().remove(&id));
    YT_GUARD.with(|g| g.borrow_mut().remove(&id));
    POPUP_INTENT.with(|m| m.borrow_mut().remove(&id));
    LAST_CLEAN.with(|m| m.borrow_mut().remove(&id));
    CLEAN_BYPASS.with(|m| m.borrow_mut().remove(&id));
    if let Some(shield) = crate::shield::global() {
        shield.forget_tab(id);
    }
    FIND_HOOKED.with(|f| f.borrow_mut().remove(&id));
}

fn web_error(status: COREWEBVIEW2_WEB_ERROR_STATUS) -> (String, String) {
    let (code, desc) = match status {
        COREWEBVIEW2_WEB_ERROR_STATUS_HOST_NAME_NOT_RESOLVED => ("NAME_NOT_RESOLVED", "The server's address could not be found."),
        COREWEBVIEW2_WEB_ERROR_STATUS_CANNOT_CONNECT => ("CANNOT_CONNECT", "The connection to the server could not be established."),
        COREWEBVIEW2_WEB_ERROR_STATUS_SERVER_UNREACHABLE => ("SERVER_UNREACHABLE", "The server could not be reached."),
        COREWEBVIEW2_WEB_ERROR_STATUS_TIMEOUT => ("TIMED_OUT", "The server took too long to respond."),
        COREWEBVIEW2_WEB_ERROR_STATUS_DISCONNECTED => ("INTERNET_DISCONNECTED", "You appear to be offline."),
        COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_RESET => ("CONNECTION_RESET", "The connection was reset."),
        COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED => ("CONNECTION_ABORTED", "The connection was aborted."),
        COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED => ("CANCELED", "The navigation was canceled."),
        COREWEBVIEW2_WEB_ERROR_STATUS_REDIRECT_FAILED => ("REDIRECT_FAILED", "The page redirected too many times or to an invalid address."),
        COREWEBVIEW2_WEB_ERROR_STATUS_ERROR_HTTP_INVALID_SERVER_RESPONSE => ("INVALID_RESPONSE", "The server sent an invalid response."),
        COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_COMMON_NAME_IS_INCORRECT
        | COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_EXPIRED
        | COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_IS_INVALID
        | COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_REVOKED
        | COREWEBVIEW2_WEB_ERROR_STATUS_CLIENT_CERTIFICATE_CONTAINS_ERRORS => ("CERT_ERROR", "The site's security certificate is not valid."),
        COREWEBVIEW2_WEB_ERROR_STATUS_VALID_AUTHENTICATION_CREDENTIALS_REQUIRED
        | COREWEBVIEW2_WEB_ERROR_STATUS_VALID_PROXY_AUTHENTICATION_REQUIRED => ("AUTH_REQUIRED", "Authentication is required."),
        _ => ("FAILED", "The page could not be loaded."),
    };
    (code.to_string(), desc.to_string())
}

fn permission_name(kind: COREWEBVIEW2_PERMISSION_KIND) -> Option<&'static str> {
    Some(match kind {
        COREWEBVIEW2_PERMISSION_KIND_MICROPHONE => "microphone",
        COREWEBVIEW2_PERMISSION_KIND_CAMERA => "camera",
        COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION => "geolocation",
        COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS => "notifications",
        COREWEBVIEW2_PERMISSION_KIND_OTHER_SENSORS => "sensors",
        COREWEBVIEW2_PERMISSION_KIND_CLIPBOARD_READ => "clipboard",
        COREWEBVIEW2_PERMISSION_KIND_MULTIPLE_AUTOMATIC_DOWNLOADS => "downloads",
        COREWEBVIEW2_PERMISSION_KIND_FILE_READ_WRITE => "files",
        COREWEBVIEW2_PERMISSION_KIND_LOCAL_FONTS => "fonts",
        COREWEBVIEW2_PERMISSION_KIND_MIDI_SYSTEM_EXCLUSIVE_MESSAGES => "midi",
        COREWEBVIEW2_PERMISSION_KIND_WINDOW_MANAGEMENT => "windowManagement",
        // Autoplay and unknown kinds keep WebView2's default behaviour.
        _ => return None,
    })
}

/// Completes a deferred permission request. `None` (dismissed) denies for this request only.
pub fn respond_permission(req: u64, allow: Option<bool>) {
    let Some(p) = PERMISSIONS.with(|m| m.borrow_mut().remove(&req)) else { return };
    let state = if allow == Some(true) { COREWEBVIEW2_PERMISSION_STATE_ALLOW } else { COREWEBVIEW2_PERMISSION_STATE_DENY };
    unsafe {
        let _ = p.args.SetState(state);
        let _ = p.deferral.Complete();
    }
}

fn choose_download_path(suggested: &str) -> Option<String> {
    let dir = DOWNLOAD_DIR.lock().ok()?.clone()?;
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("download folder {} is not usable ({e}); using the default folder", dir.display());
        return None;
    }
    let name = std::path::Path::new(suggested).file_name()?.to_string_lossy().to_string();
    let path = crate::db::downloads::unique_path(&dir, &name, |p| p.exists());
    Some(path.to_string_lossy().to_string())
}

fn host_of(url: &str) -> String {
    url::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_else(|| "unknown host".into())
}

unsafe fn start_download(
    args: &ICoreWebView2DownloadStartingEventArgs,
    tab: Option<TabId>,
    ctl: &ControllerHandle,
    owner: isize,
) -> windows::core::Result<()> {
    // We show our own downloads UI instead of the Edge flyout.
    if let Err(e) = args.SetHandled(true) {
        log::warn!("download: could not take over the download UI: {e}");
    }
    let mut path = pwstr(|p| args.ResultFilePath(p));
    if let Some(custom) = choose_download_path(&path) {
        match args.SetResultFilePath(&HSTRING::from(custom.as_str())) {
            Ok(()) => path = custom,
            Err(e) => log::warn!("download: could not set the target path, keeping WebView2's choice: {e}"),
        }
    }
    let id = next_download_id();
    if DOWNLOAD_ASK.load(Ordering::SeqCst) {
        // Hold the download (nothing is written yet) while the Save dialog is open on a worker thread; WebView2 waits
        // for the deferral. Cancelling the dialog cancels the download before it starts.
        let deferral = args.GetDeferral()?;
        ASKING.with(|a| a.borrow_mut().insert(id, AskingDownload { args: args.clone(), deferral, tab }));
        let c = ctl.clone();
        std::thread::spawn(move || {
            let chosen = dialog::save_file(owner, &path);
            c.post(move |ctl| {
                let h = ctl.handle.clone();
                let _ = ctl.app.run_on_main_thread(move || finish_ask(id, chosen, &h));
            });
        });
        return Ok(());
    }
    track_download(args, id, tab, ctl)
}

/// The Save dialog for download `id` closed: start it at the chosen path, or cancel it.
fn finish_ask(id: i64, chosen: Option<PathBuf>, ctl: &ControllerHandle) {
    let Some(a) = ASKING.with(|m| m.borrow_mut().remove(&id)) else { return };
    unsafe {
        match chosen {
            Some(p) => {
                if let Err(e) = a.args.SetResultFilePath(&HSTRING::from(p.to_string_lossy().as_ref())) {
                    log::warn!("download {id}: the chosen path was not accepted ({e}); using the default folder");
                }
                if let Err(e) = track_download(&a.args, id, a.tab, ctl) {
                    log::warn!("download {id}: could not be tracked: {e}");
                }
            }
            None => {
                log::info!("download {id}: cancelled in the Save dialog");
                let _ = a.args.SetCancel(true);
            }
        }
        let _ = a.deferral.Complete();
    }
}

/// Follows a starting download: progress and state updates go to the controller, which lists and persists it.
unsafe fn track_download(
    args: &ICoreWebView2DownloadStartingEventArgs,
    id: i64,
    tab: Option<TabId>,
    ctl: &ControllerHandle,
) -> windows::core::Result<()> {
    let op = args.DownloadOperation()?;
    let path = pwstr(|p| args.ResultFilePath(p));
    let url = pwstr(|p| op.Uri(p));
    let mime = pwstr(|p| op.MimeType(p));
    let mut total = 0i64;
    let _ = op.TotalBytesToReceive(&mut total);
    let total = (total > 0).then_some(total);
    log::info!(
        "download {id}: started from {} into {} ({}, {})",
        host_of(&url),
        std::path::Path::new(&path).parent().map(|p| p.display().to_string()).unwrap_or_default(),
        total.map(|t| format!("{t} bytes")).unwrap_or_else(|| "size unknown".into()),
        if mime.is_empty() { "no content type" } else { mime.as_str() }
    );

    let c = ctl.clone();
    let mut last = Instant::now().checked_sub(Duration::from_secs(1)).unwrap_or_else(Instant::now);
    let mut all_received = false;
    let mut token = 0i64;
    if let Err(e) = op.add_BytesReceivedChanged(
        &BytesReceivedChangedEventHandler::create(Box::new(move |sender, _| {
            let Some(op) = sender else { return Ok(()) };
            let mut received = 0i64;
            let mut total = 0i64;
            let _ = op.BytesReceived(&mut received);
            let _ = op.TotalBytesToReceive(&mut total);
            let done = total > 0 && received >= total;
            if done && !all_received {
                // Every byte is here; WebView2 still has to finish (Windows' security check of the file comes now).
                all_received = true;
                log::info!("download {id}: all {received} bytes received");
                c.post(move |ctl| ctl.on_download_progress(id, received, Some(total)));
                return Ok(());
            }
            // Progress fires very often; forward at most 4 updates per second.
            if last.elapsed() < Duration::from_millis(250) {
                return Ok(());
            }
            last = Instant::now();
            c.post(move |ctl| ctl.on_download_progress(id, received, (total > 0).then_some(total)));
            Ok(())
        })),
        &mut token,
    ) {
        log::warn!("download {id}: progress updates unavailable: {e}");
    }

    let c = ctl.clone();
    if let Err(e) = op.add_StateChanged(
        &StateChangedEventHandler::create(Box::new(move |sender, _| {
            let Some(op) = sender else { return Ok(()) };
            let mut state = COREWEBVIEW2_DOWNLOAD_STATE::default();
            let _ = op.State(&mut state);
            let mut reason = COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON::default();
            let _ = op.InterruptReason(&mut reason);
            let mut received = 0i64;
            let _ = op.BytesReceived(&mut received);
            let path = pwstr(|p| op.ResultFilePath(p));
            let native = match state {
                COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED => NativeDownloadState::Completed,
                COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS => NativeDownloadState::InProgress,
                _ if reason == COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_PAUSED => NativeDownloadState::Paused,
                _ if reason == COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_CANCELED => NativeDownloadState::Cancelled,
                _ => NativeDownloadState::Interrupted(interrupt_text(reason)),
            };
            log::info!("download {id}: state {} (reason {}), {received} bytes received", state.0, reason.0);
            if matches!(native, NativeDownloadState::Completed | NativeDownloadState::Cancelled | NativeDownloadState::Interrupted(_)) {
                DOWNLOADS.with(|d| d.borrow_mut().remove(&id));
            }
            c.post(move |ctl| ctl.on_download_state(id, native, path, received));
            Ok(())
        })),
        &mut token,
    ) {
        log::warn!("download {id}: state updates unavailable: {e}");
    }

    DOWNLOADS.with(|d| d.borrow_mut().insert(id, op));
    let started = NativeDownload { id, url, path, mime: (!mime.is_empty()).then_some(mime), total };
    ctl.post(move |c| c.on_download_started(started, tab));
    Ok(())
}

fn interrupt_text(reason: COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON) -> String {
    match reason {
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_NO_SPACE => "Disk full",
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_ACCESS_DENIED => "Access denied",
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_MALICIOUS => "Blocked: file flagged as malicious",
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_BLOCKED_BY_POLICY => "Blocked by policy",
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_SECURITY_CHECK_FAILED => "Security check failed",
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_DISCONNECTED => "Network disconnected",
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_TIMEOUT => "Network timeout",
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_FAILED => "Network error",
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_FORBIDDEN => "Server refused the download",
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_UNAUTHORIZED => "Not authorised",
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_SHUTDOWN => "Browser closed",
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_FAILED => "The file could not be written or was rejected by a Windows security check",
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_CONTENT_LENGTH_MISMATCH => "The server sent less than it announced (connection cut off)",
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_NAME_TOO_LONG => "File name too long",
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_FAILED => "The server failed",
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_SERVER_DOWN => "The server is unreachable",
        _ => return format!("Download failed (code {})", reason.0),
    }
    .to_string()
}

#[derive(Debug, Clone)]
pub struct NativeDownload {
    pub id: i64,
    pub url: String,
    pub path: String,
    pub mime: Option<String>,
    pub total: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NativeDownloadState {
    InProgress,
    Paused,
    Completed,
    Cancelled,
    Interrupted(String),
}

pub enum DownloadAction {
    Pause,
    Resume,
    Cancel,
}

/// Stops tracking download `id` (an attempt WebView2 replaced with a new one).
pub fn forget_download(id: i64) {
    DOWNLOADS.with(|d| d.borrow_mut().remove(&id));
}

/// Reads WebView2's own state for download `id` and reports it, so the list never keeps showing a download as
/// running that WebView2 has already ended (or restarted under a new id).
pub fn refresh_download_state(id: i64, ctl: ControllerHandle) {
    let Some(op) = DOWNLOADS.with(|d| d.borrow().get(&id).cloned()) else { return };
    unsafe {
        let mut state = COREWEBVIEW2_DOWNLOAD_STATE::default();
        if op.State(&mut state).is_err() {
            // The owning webview is gone: the operation can no longer be asked or controlled.
            DOWNLOADS.with(|d| d.borrow_mut().remove(&id));
            ctl.post(move |c| {
                c.on_download_state(id, NativeDownloadState::Interrupted("The download was cut off before it finished".into()), String::new(), 0)
            });
            return;
        }
        if state == COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS {
            return;
        }
        let mut reason = COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON::default();
        let _ = op.InterruptReason(&mut reason);
        let mut received = 0i64;
        let _ = op.BytesReceived(&mut received);
        let path = pwstr(|p| op.ResultFilePath(p));
        let native = match state {
            COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED => NativeDownloadState::Completed,
            _ if reason == COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_CANCELED => NativeDownloadState::Cancelled,
            _ => NativeDownloadState::Interrupted(interrupt_text(reason)),
        };
        log::info!("download {id}: state {} read back (reason {})", state.0, reason.0);
        DOWNLOADS.with(|d| d.borrow_mut().remove(&id));
        ctl.post(move |c| c.on_download_state(id, native, path, received));
    }
}

/// Opens WebView2's own download panel over tab `id` (top right, under the toolbar). It is where Windows SmartScreen
/// asks to keep or delete a file it could not vouch for; nothing in it is bypassed or answered for the user.
pub fn open_download_dialog(id: TabId) -> bool {
    let Some(t) = tab_com(id) else { return false };
    let Ok(wv9) = t.webview.cast::<ICoreWebView2_9>() else { return false };
    unsafe {
        let _ = wv9.SetDefaultDownloadDialogCornerAlignment(COREWEBVIEW2_DEFAULT_DOWNLOAD_DIALOG_CORNER_ALIGNMENT_TOP_RIGHT);
        let _ = wv9.SetDefaultDownloadDialogMargin(windows::Win32::Foundation::POINT { x: 12, y: 8 });
        wv9.OpenDefaultDownloadDialog().is_ok()
    }
}

/// `ERROR_INVALID_STATE`: what WebView2 answers when the webview that owns a download has been closed.
const E_INVALID_STATE: i32 = 0x8007139Fu32 as i32;

pub fn download_action(id: i64, action: DownloadAction, ctl: ControllerHandle) {
    let Some(op) = DOWNLOADS.with(|d| d.borrow().get(&id).cloned()) else { return };
    unsafe {
        let r = match action {
            DownloadAction::Pause => op.Pause(),
            DownloadAction::Resume => {
                if read_bool(|b| op.CanResume(b)) {
                    op.Resume()
                } else {
                    // Never pretend: a download WebView2 cannot resume is reported as failed, and Retry starts it
                    // again from the beginning.
                    DOWNLOADS.with(|d| d.borrow_mut().remove(&id));
                    let _ = op.Cancel();
                    let mut received = 0i64;
                    let _ = op.BytesReceived(&mut received);
                    ctl.post(move |c| {
                        c.on_download_state(
                            id,
                            NativeDownloadState::Interrupted("This download can't be resumed. Retry starts it again from the beginning.".into()),
                            String::new(),
                            received,
                        )
                    });
                    return;
                }
            }
            DownloadAction::Cancel => op.Cancel(),
        };
        if let Err(e) = r {
            log::warn!("download {id} action failed: {e}");
            if e.code().0 == E_INVALID_STATE {
                // The transfer can no longer be controlled (its tab's webview is gone). Stop showing it as running.
                DOWNLOADS.with(|d| d.borrow_mut().remove(&id));
                let mut received = 0i64;
                let _ = op.BytesReceived(&mut received);
                let cancelled = matches!(action, DownloadAction::Cancel);
                ctl.post(move |c| {
                    let state = if cancelled {
                        NativeDownloadState::Cancelled
                    } else {
                        NativeDownloadState::Interrupted("The download was cut off before it finished".into())
                    };
                    c.on_download_state(id, state, String::new(), received)
                });
            }
        }
    }
}

/// Reports the address the tab is really showing. A link that turned into a download makes the tab announce the
/// download's address first, but the tab stays on its previous page.
pub fn report_source(id: TabId, ctl: ControllerHandle) {
    let Some(t) = tab_com(id) else { return };
    let uri = unsafe { pwstr(|p| t.webview.Source(p)) };
    if !uri.is_empty() {
        ctl.post_tab(id, move |c| c.on_source_changed(id, uri, true));
    }
}

pub fn navigate(id: TabId, url: &str) {
    if let Some(t) = tab_com(id) {
        unsafe {
            let _ = t.webview.Navigate(&HSTRING::from(url));
        }
    }
}

pub enum NavAction {
    Back,
    Forward,
    Reload,
    HardReload,
    Stop,
}

pub fn nav_action(id: TabId, action: NavAction) {
    let Some(t) = tab_com(id) else { return };
    unsafe {
        let _ = match action {
            NavAction::Back => t.webview.GoBack(),
            NavAction::Forward => t.webview.GoForward(),
            NavAction::Reload => t.webview.Reload(),
            NavAction::Stop => t.webview.Stop(),
            // Cache-bypassing reload via the DevTools protocol (WebView2 has no direct API).
            NavAction::HardReload => t.webview.CallDevToolsProtocolMethod(
                windows::core::w!("Page.reload"),
                windows::core::w!("{\"ignoreCache\":true}"),
                &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(|_, _| Ok(()))),
            ),
        };
    }
}

pub fn show_error_page(id: TabId, html: String) {
    if let Some(t) = tab_com(id) {
        unsafe {
            let _ = t.webview.NavigateToString(&HSTRING::from(html));
        }
    }
}

pub fn set_zoom(id: TabId, factor: f64) {
    if let Some(t) = tab_com(id) {
        unsafe {
            let _ = t.controller.SetZoomFactor(factor);
        }
    }
}

pub fn set_muted(id: TabId, muted: bool) {
    if let Some(wv8) = tab_com(id).and_then(|t| t.webview.cast::<ICoreWebView2_8>().ok()) {
        unsafe {
            let _ = wv8.SetIsMuted(muted);
        }
    }
}

/// Suspends a hidden tab's renderer (WebView2 requires the webview to be hidden).
pub fn try_suspend(id: TabId, ctl: ControllerHandle, epoch: u64) {
    let Some(wv3) = tab_com(id).and_then(|t| t.webview.cast::<ICoreWebView2_3>().ok()) else {
        ctl.post_tab(id, move |c| c.on_suspended(id, false, epoch));
        return;
    };
    let c = ctl.clone();
    let r = unsafe {
        wv3.TrySuspend(&TrySuspendCompletedHandler::create(Box::new(move |res, ok| {
            let ok = res.is_ok() && ok;
            c.post_tab(id, move |ctl| ctl.on_suspended(id, ok, epoch));
            Ok(())
        })))
    };
    if r.is_err() {
        ctl.post_tab(id, move |c| c.on_suspended(id, false, epoch));
    }
}

/// Asks WebView2 to trim memory for a background tab (or restore normal behaviour).
/// Tells sites whether the user prefers dark pages (`prefers-color-scheme`), for the whole profile (Dark pages: Auto).
pub fn set_preferred_dark(dark: bool) {
    let Some(shell) = any_shell() else { return };
    unsafe {
        if let Ok(profile) = shell.webview.cast::<ICoreWebView2_13>().and_then(|w| w.Profile()) {
            let scheme = if dark { COREWEBVIEW2_PREFERRED_COLOR_SCHEME_DARK } else { COREWEBVIEW2_PREFERRED_COLOR_SCHEME_AUTO };
            if let Err(e) = profile.SetPreferredColorScheme(scheme) {
                log::warn!("dark pages: the preferred colour scheme could not be set: {e}");
            }
        }
    }
}

/// WebView2's memory target for a webview that is not a tab (the hidden menu window).
pub fn set_view_memory_target(pw: &PlatformWebview, low: bool) {
    unsafe {
        if let Ok(wv19) = pw.controller().CoreWebView2().and_then(|w| w.cast::<ICoreWebView2_19>()) {
            let level = if low { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW } else { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL };
            let _ = wv19.SetMemoryUsageTargetLevel(level);
        }
    }
}

pub fn set_memory_target(id: TabId, low: bool) {
    if let Some(wv19) = tab_com(id).and_then(|t| t.webview.cast::<ICoreWebView2_19>().ok()) {
        let level = if low { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW } else { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL };
        unsafe {
            let _ = wv19.SetMemoryUsageTargetLevel(level);
        }
    }
}

pub fn apply_tab_settings(save_passwords: bool, autofill: bool, private_tabs: Vec<TabId>) {
    let tabs: Vec<(TabId, TabCom)> = TABS.with(|t| t.borrow().iter().map(|(k, v)| (*k, v.clone())).collect());
    for (id, t) in tabs {
        unsafe {
            if let Ok(s4) = t.webview.Settings().and_then(|s| s.cast::<ICoreWebView2Settings4>()) {
                let _ = s4.SetIsPasswordAutosaveEnabled(save_passwords && !private_tabs.contains(&id));
                let _ = s4.SetIsGeneralAutofillEnabled(autofill);
            }
        }
    }
}

/// Starts or updates find-in-page using WebView2's Find API with our own find bar.
pub fn find_start(id: TabId, text: String, case_sensitive: bool, ctl: ControllerHandle) {
    let Some(t) = tab_com(id) else { return };
    let result: windows::core::Result<()> = unsafe {
        (|| {
            let find = t.webview.cast::<ICoreWebView2_28>()?.Find()?;
            let hooked = FIND_HOOKED.with(|f| f.borrow().contains(&id));
            if !hooked {
                let mut token = 0i64;
                let c = ctl.clone();
                find.add_MatchCountChanged(
                    &FindMatchCountChangedEventHandler::create(Box::new(move |sender, _| {
                        if let Some(f) = sender {
                            post_find(&f, id, &c);
                        }
                        Ok(())
                    })),
                    &mut token,
                )?;
                let c = ctl.clone();
                find.add_ActiveMatchIndexChanged(
                    &FindActiveMatchIndexChangedEventHandler::create(Box::new(move |sender, _| {
                        if let Some(f) = sender {
                            post_find(&f, id, &c);
                        }
                        Ok(())
                    })),
                    &mut token,
                )?;
                FIND_HOOKED.with(|f| f.borrow_mut().insert(id));
            }
            let options = t.env.cast::<ICoreWebView2Environment15>()?.CreateFindOptions()?;
            options.SetFindTerm(&HSTRING::from(text.as_str()))?;
            options.SetIsCaseSensitive(case_sensitive)?;
            options.SetShouldHighlightAllMatches(true)?;
            options.SetSuppressDefaultFindDialog(true)?;
            let f2 = find.clone();
            let c = ctl.clone();
            find.Start(
                &options,
                &FindStartCompletedHandler::create(Box::new(move |_| {
                    post_find(&f2, id, &c);
                    Ok(())
                })),
            )
        })()
    };
    if let Err(e) = result {
        log::warn!("find unavailable: {e}");
        ctl.post_tab(id, move |c| c.on_find_unavailable(id));
    }
}

fn post_find(find: &ICoreWebView2Find, id: TabId, ctl: &ControllerHandle) {
    let (mut count, mut active) = (0i32, 0i32);
    unsafe {
        let _ = find.MatchCount(&mut count);
        let _ = find.ActiveMatchIndex(&mut active);
    }
    ctl.post_tab(id, move |c| c.on_find_update(id, count, active));
}

pub fn find_step(id: TabId, forward: bool) {
    if let Some(find) = tab_com(id).and_then(|t| unsafe { t.webview.cast::<ICoreWebView2_28>().and_then(|w| w.Find()).ok() }) {
        unsafe {
            let _ = if forward { find.FindNext() } else { find.FindPrevious() };
        }
    }
}

pub fn find_stop(id: TabId) {
    if let Some(find) = tab_com(id).and_then(|t| unsafe { t.webview.cast::<ICoreWebView2_28>().and_then(|w| w.Find()).ok() }) {
        unsafe {
            let _ = find.Stop();
        }
    }
}

unsafe fn read_stream(stream: &IStream) -> Option<Vec<u8>> {
    let mut stat = STATSTG::default();
    stream.Stat(&mut stat, STATFLAG_NONAME).ok()?;
    let len = stat.cbSize as usize;
    if len == 0 || len > 64 * 1024 * 1024 {
        return None;
    }
    stream.Seek(0, STREAM_SEEK_SET, None).ok()?;
    let mut buf = vec![0u8; len];
    let mut read_total = 0usize;
    while read_total < len {
        let mut read = 0u32;
        let hr = stream.Read(buf[read_total..].as_mut_ptr() as *mut _, (len - read_total) as u32, Some(&mut read));
        if hr.is_err() || read == 0 {
            break;
        }
        read_total += read as usize;
    }
    buf.truncate(read_total);
    Some(buf)
}

/// Captures the visible page as JPEG bytes (used as a backdrop while the shell shows menus over the page area).
pub fn capture_preview(id: TabId, reply: tokio::sync::oneshot::Sender<Option<Vec<u8>>>) {
    let Some(t) = tab_com(id) else {
        let _ = reply.send(None);
        return;
    };
    unsafe {
        let Ok(stream) = CreateStreamOnHGlobal(HGLOBAL::default(), true) else {
            let _ = reply.send(None);
            return;
        };
        let s2 = stream.clone();
        let _ = t.webview.CapturePreview(
            COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_JPEG,
            &stream,
            &CapturePreviewCompletedHandler::create(Box::new(move |res| {
                let _ = reply.send(res.ok().and_then(|_| read_stream(&s2)));
                Ok(())
            })),
        );
        // If CapturePreview failed synchronously the handler (and `reply`) is dropped,
        // which the awaiting side treats as "no snapshot".
    }
}

type PendingCapture = (TabId, Instant, tokio::sync::oneshot::Receiver<Option<Vec<u8>>>);

/// A capture started the moment a page asked for a right-click menu, so the menu does not wait for a fresh one after
/// the round trip through the shell.
static PREFETCHED: Mutex<Option<PendingCapture>> = Mutex::new(None);

/// How long a prefetched capture may be used: only for the menu it was started for.
const PREFETCH_TTL: Duration = Duration::from_millis(1500);

/// Starts capturing tab `id` now (UI thread). The next `take_prefetched` for that tab gets the result.
pub fn prefetch_capture(id: TabId) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    if let Ok(mut p) = PREFETCHED.lock() {
        *p = Some((id, Instant::now(), rx));
    }
    capture_preview(id, tx);
}

/// The capture `prefetch_capture` started for tab `id`, if it is recent; a stale or other tab's capture is dropped.
pub fn take_prefetched(id: TabId) -> Option<tokio::sync::oneshot::Receiver<Option<Vec<u8>>>> {
    let mut p = PREFETCHED.lock().ok()?;
    match p.take() {
        Some((tab, at, rx)) if tab == id && at.elapsed() < PREFETCH_TTL => Some(rx),
        _ => None,
    }
}

/// Script that returns the readable text of the page (boilerplate left out, form contents never read) and the current selection.
const PAGE_TEXT_JS: &str = include_str!("page_extract.js");

/// Reads the visible text of a tab for the AI assistant. Replies with the JSON
/// the script returned, or `None` if the page could not be read.
pub fn page_text(id: TabId, reply: tokio::sync::oneshot::Sender<Option<String>>) {
    let Some(t) = tab_com(id) else {
        let _ = reply.send(None);
        return;
    };
    unsafe {
        let _ = t.webview.ExecuteScript(
            &HSTRING::from(PAGE_TEXT_JS),
            &ExecuteScriptCompletedHandler::create(Box::new(move |res, json| {
                let _ = reply.send(res.ok().map(|_| json));
                Ok(())
            })),
        );
        // A synchronous failure drops the handler (and `reply`); the caller treats that as "no text".
    }
}

/// Runs a script (a saved login or address being filled in) in a tab's top frame and replies with the JSON
/// it returned, or `None` if it could not run. The script text may hold a password, so it is neither
/// logged nor kept: it is handed to WebView2 and dropped.
pub fn run_script(id: TabId, script: String, reply: tokio::sync::oneshot::Sender<Option<String>>) {
    let Some(t) = tab_com(id) else {
        let _ = reply.send(None);
        return;
    };
    unsafe {
        let _ = t.webview.ExecuteScript(
            &HSTRING::from(script),
            &ExecuteScriptCompletedHandler::create(Box::new(move |res, json| {
                let _ = reply.send(res.ok().map(|_| json));
                Ok(())
            })),
        );
        // A synchronous failure drops the handler (and `reply`); the caller treats that as "could not fill".
    }
}

/// Runs a fixed script of the browser's own (SponsorSkip data, media controls) in a tab's top frame, ignoring the result.
pub fn exec_script(id: TabId, script: String) {
    if let Some(t) = tab_com(id) {
        unsafe {
            let _ = t.webview.ExecuteScript(&HSTRING::from(script), &ExecuteScriptCompletedHandler::create(Box::new(|_, _| Ok(()))));
        }
    }
}

const READER_JS: &str = include_str!("reader_extract.js");

/// Reads the article of a page for the reader view (see reader_extract.js). Replies with the script's JSON.
pub fn reader_extract(id: TabId, reply: tokio::sync::oneshot::Sender<Option<String>>) {
    run_script(id, READER_JS.to_string(), reply);
}

/// The largest page height a full-page screenshot covers, in CSS pixels (Chromium's own capture limit is 16384).
pub const MAX_SCREENSHOT_HEIGHT: f64 = 16000.0;

/// Takes a PNG screenshot of a tab through the DevTools protocol: the visible part, or the whole page (`full`).
/// Replies with the PNG bytes, or `None` when the page could not be captured.
pub fn capture_screenshot(id: TabId, full: bool, reply: tokio::sync::oneshot::Sender<Option<Vec<u8>>>) {
    let Some(t) = tab_com(id) else {
        let _ = reply.send(None);
        return;
    };
    let webview = t.webview.clone();
    let capture = move |params: String, reply: tokio::sync::oneshot::Sender<Option<Vec<u8>>>| unsafe {
        let _ = webview.CallDevToolsProtocolMethod(
            &HSTRING::from("Page.captureScreenshot"),
            &HSTRING::from(params),
            &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |res, json| {
                use base64::Engine as _;
                let png = res
                    .ok()
                    .and_then(|_| serde_json::from_str::<serde_json::Value>(&json).ok())
                    .and_then(|v| v.get("data").and_then(|d| d.as_str()).and_then(|d| base64::engine::general_purpose::STANDARD.decode(d).ok()));
                let _ = reply.send(png);
                Ok(())
            })),
        );
    };
    if !full {
        capture(r#"{"format":"png"}"#.to_string(), reply);
        return;
    }
    unsafe {
        let _ = t.webview.CallDevToolsProtocolMethod(
            &HSTRING::from("Page.getLayoutMetrics"),
            &HSTRING::from("{}"),
            &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |res, json| {
                let size = res.ok().and_then(|_| serde_json::from_str::<serde_json::Value>(&json).ok()).and_then(|v| {
                    let s = v.get("cssContentSize")?;
                    Some((s.get("width")?.as_f64()?, s.get("height")?.as_f64()?))
                });
                let params = match size {
                    Some((w, h)) if w > 0.0 && h > 0.0 => serde_json::json!({
                        "format": "png",
                        "captureBeyondViewport": true,
                        "clip": { "x": 0, "y": 0, "width": w.ceil(), "height": h.min(MAX_SCREENSHOT_HEIGHT).ceil(), "scale": 1 }
                    })
                    .to_string(),
                    _ => r#"{"format":"png"}"#.to_string(),
                };
                capture(params, reply);
                Ok(())
            })),
        );
    }
}

/// Script that answers whether the page holds data the user typed and has not submitted (see the file for the rules).
const UNSAVED_INPUT_JS: &str = include_str!("unsaved_input.js");

/// Asks a tab whether it holds unsaved typed-in data before its webview is destroyed.
/// Replies `Some(true/false)`, or `None` when the page could not be asked (the caller treats that as "yes").
pub fn page_has_unsaved_input(id: TabId, reply: tokio::sync::oneshot::Sender<Option<bool>>) {
    let Some(t) = tab_com(id) else {
        let _ = reply.send(None);
        return;
    };
    unsafe {
        let _ = t.webview.ExecuteScript(
            &HSTRING::from(UNSAVED_INPUT_JS),
            &ExecuteScriptCompletedHandler::create(Box::new(move |res, json| {
                let _ = reply.send(res.ok().and_then(|_| match json.trim() {
                    "true" => Some(true),
                    "false" => Some(false),
                    _ => None,
                }));
                Ok(())
            })),
        );
        // A synchronous failure drops the handler (and `reply`); the caller treats that as "has input".
    }
}

pub struct ClearKinds {
    pub cookies: bool,
    pub cache: bool,
    pub passwords: bool,
    pub autofill: bool,
}

/// Clears profile data through WebView2. Replies `true` on success.
pub fn clear_browsing_data(kinds: ClearKinds, reply: tokio::sync::oneshot::Sender<bool>) {
    let Some(shell) = any_shell() else {
        let _ = reply.send(false);
        return;
    };
    let mut k = COREWEBVIEW2_BROWSING_DATA_KINDS(0);
    if kinds.cookies {
        k = k | COREWEBVIEW2_BROWSING_DATA_KINDS_COOKIES | COREWEBVIEW2_BROWSING_DATA_KINDS_ALL_DOM_STORAGE;
    }
    if kinds.cache {
        k = k | COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE | COREWEBVIEW2_BROWSING_DATA_KINDS_CACHE_STORAGE;
    }
    if kinds.passwords {
        k |= COREWEBVIEW2_BROWSING_DATA_KINDS_PASSWORD_AUTOSAVE;
    }
    if kinds.autofill {
        k |= COREWEBVIEW2_BROWSING_DATA_KINDS_GENERAL_AUTOFILL;
    }
    if k.0 == 0 {
        let _ = reply.send(true);
        return;
    }
    unsafe {
        let profile = shell.webview.cast::<ICoreWebView2_13>().and_then(|w| w.Profile()).and_then(|p| p.cast::<ICoreWebView2Profile2>());
        match profile {
            Ok(p) => {
                let _ = p.ClearBrowsingData(
                    k,
                    &ClearBrowsingDataCompletedHandler::create(Box::new(move |res| {
                        let _ = reply.send(res.is_ok());
                        Ok(())
                    })),
                );
            }
            Err(_) => {
                let _ = reply.send(false);
            }
        }
    }
}

/// Raw process data for the performance sampler.
#[derive(Debug, Default, Clone)]
pub struct ProcSnapshot {
    pub processes: Vec<(u32, &'static str)>,
    pub renderer_frames: Vec<(u32, Vec<u32>)>,
    pub tab_frames: Vec<(TabId, u32)>,
    pub shell_frame: Option<u32>,
    /// Frame infos were not read (guard or switch), so processes cannot be attributed to tabs.
    pub frames_unavailable: bool,
}

fn process_kind(kind: COREWEBVIEW2_PROCESS_KIND) -> &'static str {
    match kind {
        COREWEBVIEW2_PROCESS_KIND_BROWSER => "browser",
        COREWEBVIEW2_PROCESS_KIND_RENDERER => "renderer",
        COREWEBVIEW2_PROCESS_KIND_GPU => "gpu",
        COREWEBVIEW2_PROCESS_KIND_UTILITY => "utility",
        COREWEBVIEW2_PROCESS_KIND_SANDBOX_HELPER => "sandbox",
        _ => "other",
    }
}

fn main_frame_id(webview: &ICoreWebView2) -> Option<u32> {
    let wv20 = webview.cast::<ICoreWebView2_20>().ok()?;
    let mut id = 0u32;
    unsafe { wv20.FrameId(&mut id).ok()? };
    Some(id)
}

/// Enumerates WebView2 processes and which tab frames each renderer hosts.
pub fn collect_processes(ctl: ControllerHandle) {
    let Some(shell) = any_shell() else {
        ctl.post(|c| c.on_process_snapshot(ProcSnapshot::default()));
        return;
    };
    let mut snap = ProcSnapshot {
        frames_unavailable: !frame_info_enabled(),
        shell_frame: main_frame_id(&shell.webview),
        tab_frames: TABS.with(|t| t.borrow().iter().filter_map(|(id, tc)| main_frame_id(&tc.webview).map(|f| (*id, f))).collect()),
        ..Default::default()
    };
    unsafe {
        if let Ok(coll) = shell.env.cast::<ICoreWebView2Environment8>().and_then(|e| e.GetProcessInfos()) {
            let mut n = 0u32;
            let _ = coll.Count(&mut n);
            for i in 0..n {
                if let Ok(info) = coll.GetValueAtIndex(i) {
                    let (mut pid, mut kind) = (0i32, COREWEBVIEW2_PROCESS_KIND::default());
                    if info.ProcessId(&mut pid).is_ok() && info.Kind(&mut kind).is_ok() {
                        snap.processes.push((pid as u32, process_kind(kind)));
                    }
                }
            }
        }
        let Ok(env13) = shell.env.cast::<ICoreWebView2Environment13>() else {
            ctl.post(move |c| c.on_process_snapshot(snap));
            return;
        };
        let c = ctl.clone();
        let fallback = snap.clone();
        let started = env13.GetProcessExtendedInfos(&GetProcessExtendedInfosCompletedHandler::create(Box::new(move |res, coll| {
            let mut snap = snap;
            if let (Ok(()), Some(coll)) = (res, coll) {
                let frames_enabled = frame_info_enabled();
                if frames_enabled {
                    frame_guard_begin();
                }
                let mut n = 0u32;
                let _ = coll.Count(&mut n);
                for i in 0..n {
                    let Ok(ext) = coll.GetValueAtIndex(i) else { continue };
                    let Ok(info) = ext.ProcessInfo() else { continue };
                    let mut pid = 0i32;
                    if info.ProcessId(&mut pid).is_err() {
                        continue;
                    }
                    let mut frames = Vec::new();
                    if frames_enabled {
                        // The collection must outlive its iterator, and every object stays referenced until the walk
                        // is over: dropping the collection right after `GetIterator` left the iterator pointing at
                        // freed memory (the crash fixed in 0.1.1).
                        if let Ok(collection) = ext.AssociatedFrameInfos() {
                            if let Ok(iter) = collection.GetIterator() {
                                while read_bool(|b| iter.HasCurrent(b)) {
                                    if let Ok(current) = iter.GetCurrent() {
                                        if let Ok(fi) = current.cast::<ICoreWebView2FrameInfo2>() {
                                            let mut fid = 0u32;
                                            if fi.FrameId(&mut fid).is_ok() {
                                                frames.push(fid);
                                            }
                                        }
                                    }
                                    if !read_bool(|b| iter.MoveNext(b)) {
                                        break;
                                    }
                                }
                                drop(iter);
                            }
                            drop(collection);
                        }
                    }
                    snap.renderer_frames.push((pid as u32, frames));
                }
                if frames_enabled {
                    frame_guard_success();
                }
            }
            c.post(move |ctl| ctl.on_process_snapshot(snap));
            Ok(())
        })));
        if started.is_err() {
            ctl.post(move |c| c.on_process_snapshot(fallback));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clean_start_keeps_frame_infos_on() {
        assert_eq!(guard_decision(None, "154.0.1"), (true, None));
        assert_eq!(guard_decision(Some("garbage"), "154.0.1"), (true, None));
    }

    #[test]
    fn a_walk_that_never_finished_disables_frame_infos_for_that_runtime_only() {
        // The previous run wrote "pending" and then died inside the walk.
        assert_eq!(guard_decision(Some("pending:154.0.1"), "154.0.1"), (false, Some("disabled:154.0.1".into())));
        // The decision sticks on later runs of the same runtime...
        assert_eq!(guard_decision(Some("disabled:154.0.1"), "154.0.1"), (false, Some("disabled:154.0.1".into())));
        // ...but a different runtime version gets another chance and the marker is cleared.
        assert_eq!(guard_decision(Some("disabled:154.0.1"), "155.0.0"), (true, None));
        assert_eq!(guard_decision(Some("pending:154.0.1"), "155.0.0"), (true, None));
    }

    #[test]
    fn marker_file_lifecycle_across_simulated_runs() {
        let dir = std::env::temp_dir().join(format!("pb-guard-{}-{}", std::process::id(), crate::error::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let marker = dir.join(FRAME_GUARD_FILE);

        // Run 1: starts clean, begins the walk (marker appears) and finishes it (marker removed).
        init_frame_guard(&dir, "154.0.1", None);
        assert!(frame_info_enabled());
        frame_guard_begin();
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "pending:154.0.1");
        frame_guard_success();
        assert!(!marker.exists(), "a completed walk leaves no marker");

        // Run 2: the process died mid-walk, so the pending marker is still there at the next start.
        std::fs::write(&marker, "pending:154.0.1").unwrap();
        FRAME_INFO_VERIFIED.store(false, Ordering::SeqCst);
        init_frame_guard(&dir, "154.0.1", None);
        assert!(!frame_info_enabled(), "crash during the walk disables it");
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "disabled:154.0.1");

        // Explicit switches win in both directions.
        init_frame_guard(&dir, "154.0.1", Some("on"));
        assert!(frame_info_enabled());
        init_frame_guard(&dir, "154.0.1", Some("off"));
        assert!(!frame_info_enabled());

        // A new runtime version retries.
        init_frame_guard(&dir, "155.0.0", None);
        assert!(frame_info_enabled());
        assert!(!marker.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
