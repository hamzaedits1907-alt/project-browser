//! WebView2 integration.
//!
//! Every function in this module must run on the main (UI) thread: WebView2
//! COM objects are single-threaded and are kept in thread-local registries.
//! Event handlers never block; they only post work to the controller thread.

pub mod dialog;
pub mod metrics;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine as _;
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
    static SHELL: RefCell<Option<TabCom>> = const { RefCell::new(None) };
    /// Host of the page each tab is loading, for the Shield's first-party checks.
    static TOP_HOST: RefCell<HashMap<TabId, String>> = RefCell::new(HashMap::new());
}

static NEXT_PERMISSION: AtomicU64 = AtomicU64::new(1);
static NEXT_DOWNLOAD: AtomicI64 = AtomicI64::new(0);
static DOWNLOAD_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

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

/// Sets the folder new downloads are saved to (`None` = WebView2's default, the user's Downloads folder).
pub fn set_download_dir(dir: Option<PathBuf>) {
    if let Ok(mut d) = DOWNLOAD_DIR.lock() {
        *d = dir;
    }
}

fn next_download_id() -> i64 {
    // Seeded from the clock so ids stay unique across restarts without a DB round-trip.
    let _ = NEXT_DOWNLOAD.compare_exchange(0, crate::error::now_ms() * 1000, Ordering::SeqCst, Ordering::SeqCst);
    NEXT_DOWNLOAD.fetch_add(1, Ordering::SeqCst)
}

fn tab_com(id: TabId) -> Option<TabCom> {
    TABS.with(|t| t.borrow().get(&id).cloned())
}

fn pwstr(f: impl FnOnce(&mut PWSTR) -> windows::core::Result<()>) -> String {
    let mut p = PWSTR::null();
    match f(&mut p) {
        Ok(()) => take_pwstr(p),
        Err(_) => String::new(),
    }
}

fn read_bool(f: impl FnOnce(&mut BOOL) -> windows::core::Result<()>) -> bool {
    let mut b = BOOL::default();
    f(&mut b).map(|_| b.as_bool()).unwrap_or(false)
}

fn key_down(vk: i32) -> bool {
    unsafe { GetKeyState(vk) < 0 }
}

pub fn current_mods() -> Mods {
    Mods { ctrl: key_down(0x11), shift: key_down(0x10), alt: key_down(0x12) }
}

/// Routes browser shortcuts typed into any webview to the controller.
unsafe fn hook_accelerators(controller: &ICoreWebView2Controller, ctl: &ControllerHandle) -> windows::core::Result<()> {
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
                ctl.post(move |c| c.run_command(cmd));
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
            ctl.post(move |c| c.on_native_error(id, msg));
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
        &NavigationStartingEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let uri = pwstr(|p| args.Uri(p));
            note_top_navigation(id, &uri, &c);
            c.post(move |ctl| ctl.on_nav_starting(id, uri));
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
            c.post(move |ctl| ctl.on_source_changed(id, uri, new_doc));
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
            c.post(move |ctl| ctl.on_history_changed(id, back, fwd));
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
            c.post(move |ctl| ctl.on_nav_completed(id, success, error, source, title));
            Ok(())
        })),
        &mut token,
    )?;

    let c = ctl.clone();
    webview.add_DocumentTitleChanged(
        &DocumentTitleChangedEventHandler::create(Box::new(move |sender, _| {
            let Some(wv) = sender else { return Ok(()) };
            let title = pwstr(|p| wv.DocumentTitle(p));
            c.post(move |ctl| ctl.on_title(id, title));
            Ok(())
        })),
        &mut token,
    )?;

    let c = ctl.clone();
    webview.add_ContainsFullScreenElementChanged(
        &ContainsFullScreenElementChangedEventHandler::create(Box::new(move |sender, _| {
            let Some(wv) = sender else { return Ok(()) };
            let fs = read_bool(|b| wv.ContainsFullScreenElement(b));
            c.post(move |ctl| ctl.on_fullscreen_element(id, fs));
            Ok(())
        })),
        &mut token,
    )?;

    let c = ctl.clone();
    webview.add_ProcessFailed(
        &ProcessFailedEventHandler::create(Box::new(move |_, _| {
            c.post(move |ctl| ctl.on_crashed(id));
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
            c.post(move |ctl| ctl.on_permission_request(req, id, uri, kind));
            Ok(())
        })),
        &mut token,
    )?;

    if let Ok(wv4) = webview.cast::<ICoreWebView2_4>() {
        let c = ctl.clone();
        wv4.add_DownloadStarting(
            &DownloadStartingEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                start_download(&args, Some(id), &c)
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
            c.post(move |ctl| ctl.on_audio(id, playing, muted));
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
                c.post(move |ctl| ctl.on_cert_error(id));
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
                c.post(move |ctl| ctl.on_favicon(id, uri));
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
            c.post(move |ctl| ctl.on_zoom(id, z));
            Ok(())
        })),
        &mut token,
    )?;

    hook_accelerators(&controller, ctl)?;

    if let Err(e) = hook_shield(&webview, &env, id, init.private, ctl) {
        log::warn!("tab {id}: request blocking is unavailable for this tab: {e}");
    }

    TABS.with(|t| t.borrow_mut().insert(id, TabCom { controller, webview: webview.clone(), env }));
    webview.Navigate(&HSTRING::from(init.url.as_str()))?;
    Ok(())
}

// ---- Project Shield: request blocking ---------------------------------------------------------

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
pub fn install_shell(pw: PlatformWebview, ctl: ControllerHandle, devtools: bool) {
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
            hook_accelerators(&controller, &ctl)?;
            SHELL.with(|s| *s.borrow_mut() = Some(TabCom { controller, webview, env: pw.environment() }));
            Ok(())
        })()
    };
    match result {
        Ok(()) => log::info!("startup: shell webview configured"),
        Err(e) => log::error!("failed to configure shell webview: {e}"),
    }
}

pub fn remove_tab(id: TabId) {
    TABS.with(|t| t.borrow_mut().remove(&id));
    TOP_HOST.with(|t| t.borrow_mut().remove(&id));
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

unsafe fn start_download(args: &ICoreWebView2DownloadStartingEventArgs, tab: Option<TabId>, ctl: &ControllerHandle) -> windows::core::Result<()> {
    let op = args.DownloadOperation()?;
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
    let mut token = 0i64;
    if let Err(e) = op.add_BytesReceivedChanged(
        &BytesReceivedChangedEventHandler::create(Box::new(move |sender, _| {
            // Progress fires very often; forward at most 4 updates per second.
            if last.elapsed() < Duration::from_millis(250) {
                return Ok(());
            }
            last = Instant::now();
            let Some(op) = sender else { return Ok(()) };
            let mut received = 0i64;
            let mut total = 0i64;
            let _ = op.BytesReceived(&mut received);
            let _ = op.TotalBytesToReceive(&mut total);
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
                    Ok(())
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
        ctl.post(move |c| c.on_source_changed(id, uri, true));
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
        ctl.post(move |c| c.on_suspended(id, false, epoch));
        return;
    };
    let c = ctl.clone();
    let r = unsafe {
        wv3.TrySuspend(&TrySuspendCompletedHandler::create(Box::new(move |res, ok| {
            let ok = res.is_ok() && ok;
            c.post(move |ctl| ctl.on_suspended(id, ok, epoch));
            Ok(())
        })))
    };
    if r.is_err() {
        ctl.post(move |c| c.on_suspended(id, false, epoch));
    }
}

/// Asks WebView2 to trim memory for a background tab (or restore normal behaviour).
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
        ctl.post(move |c| c.on_find_unavailable(id));
    }
}

fn post_find(find: &ICoreWebView2Find, id: TabId, ctl: &ControllerHandle) {
    let (mut count, mut active) = (0i32, 0i32);
    unsafe {
        let _ = find.MatchCount(&mut count);
        let _ = find.ActiveMatchIndex(&mut active);
    }
    ctl.post(move |c| c.on_find_update(id, count, active));
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

/// Captures the visible page as a JPEG data URL (used as a backdrop while the
/// shell shows menus over the page area).
pub fn capture_preview(id: TabId, reply: tokio::sync::oneshot::Sender<Option<String>>) {
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
                let data = res
                    .ok()
                    .and_then(|_| read_stream(&s2))
                    .map(|bytes| format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)));
                let _ = reply.send(data);
                Ok(())
            })),
        );
        // If CapturePreview failed synchronously the handler (and `reply`) is dropped,
        // which the awaiting side treats as "no snapshot".
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
    let Some(shell) = SHELL.with(|s| s.borrow().clone()) else {
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
    let Some(shell) = SHELL.with(|s| s.borrow().clone()) else {
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
