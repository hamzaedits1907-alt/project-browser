//! Builders for the shell window and tab webviews.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use tauri::webview::{NewWindowResponse, WebviewBuilder};
use tauri::{AppHandle, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Wry};

use crate::controller::{ControllerHandle, WindowGeometry};
use crate::navigation::is_blocked_tab_navigation;
use crate::profile::Profile;
use crate::tabs::TabId;

/// Browser arguments shared by every webview. WebView2 requires all webviews
/// using one user-data folder to pass identical arguments. Unlike wry's
/// default this keeps Microsoft Defender SmartScreen enabled.
pub const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI";

static ARGS: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Fixes the start-up arguments for the whole run, proxy included (see `tunnel.rs`). Call once, before any webview.
pub fn init_browser_args(proxy: Option<&crate::tunnel::TunnelConfig>) {
    let _ = ARGS.set(crate::tunnel::browser_args(BROWSER_ARGS, proxy));
}

/// The arguments every webview must share (WebView2 requires identical arguments for one profile).
pub fn browser_args() -> String {
    ARGS.get_or_init(|| BROWSER_ARGS.to_string()).clone()
}

pub const SHELL_LABEL: &str = "main";

static POPUP_SEQ: AtomicU64 = AtomicU64::new(1);

/// The labels of the browser windows that exist. Events and shutdown logic only ever address these, never popups
/// or tab webviews.
static SHELLS: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn register_shell(label: &str) {
    if let Ok(mut s) = SHELLS.lock() {
        if !s.iter().any(|l| l == label) {
            s.push(label.to_string());
        }
    }
}

pub fn unregister_shell(label: &str) {
    if let Ok(mut s) = SHELLS.lock() {
        s.retain(|l| l != label);
    }
}

pub fn is_shell_label(label: &str) -> bool {
    SHELLS.lock().map(|s| s.iter().any(|l| l == label)).unwrap_or(false)
}

pub fn shell_count() -> usize {
    SHELLS.lock().map(|s| s.len()).unwrap_or(0)
}

pub fn shell_labels() -> Vec<String> {
    SHELLS.lock().map(|s| s.clone()).unwrap_or_default()
}

/// Sends an event to every browser window's shell webview.
pub fn emit_to_shells<S: serde::Serialize + Clone>(app: &AppHandle, event: &str, payload: S) {
    use tauri::Emitter;
    for label in shell_labels() {
        let _ = app.emit_to(label.as_str(), event, payload.clone());
    }
}

pub fn build_shell_window(
    app: &AppHandle,
    profile: &Profile,
    label: &str,
    geometry: Option<WindowGeometry>,
    private: bool,
) -> tauri::Result<WebviewWindow> {
    let mut builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title(if private { "Project Browser (Private)" } else { "Project Browser" })
        .decorations(false)
        .shadow(true)
        .visible(false)
        .min_inner_size(640.0, 420.0)
        .background_color(tauri::window::Color(11, 13, 18, 255))
        .data_directory(profile.webview_dir())
        .additional_browser_args(&browser_args())
        // The shell uses HTML5 drag-and-drop for tab reordering.
        .disable_drag_drop_handler();
    match geometry {
        Some(g) if g.width >= 640.0 && g.height >= 420.0 => {
            builder = builder.inner_size(g.width, g.height);
            if let (Some(x), Some(y)) = (g.x, g.y) {
                // Ignore positions that would put the window entirely off-screen.
                if x > -g.width + 80.0 && y > -40.0 {
                    builder = builder.position(x, y);
                }
            }
            builder = builder.maximized(g.maximized);
        }
        _ => {
            builder = builder.inner_size(1280.0, 800.0).center();
        }
    }
    let window = builder.build()?;
    register_shell(label);
    Ok(window)
}

/// A tab webview. It loads `about:blank`; the real URL is navigated after the
/// native event hooks are installed so no early event is missed.
pub fn tab_webview_builder(app: &AppHandle, id: TabId, private: bool, profile: &Profile, ctl: ControllerHandle) -> WebviewBuilder<Wry> {
    let app = app.clone();
    let data_dir = profile.webview_dir();
    WebviewBuilder::new(format!("tab-{id}"), WebviewUrl::External("about:blank".parse().expect("valid url")))
        .data_directory(data_dir.clone())
        .additional_browser_args(&browser_args())
        .incognito(private)
        .devtools(true)
        .zoom_hotkeys_enabled(true)
        .disable_drag_drop_handler()
        .on_navigation(|url| !is_blocked_tab_navigation(url))
        .on_new_window(move |url, features| {
            // Whether a user action caused this request is known only to the native event (see native::PopupIntent).
            let intent = crate::native::take_popup_intent(id);
            let user_initiated = intent.map(|i| i.user_initiated).unwrap_or(true);
            if is_blocked_tab_navigation(&url) {
                return NewWindowResponse::Deny;
            }
            // A pop-up nobody asked for is blocked unless the user allowed this site; the shell says so and offers it.
            if !user_initiated && !crate::native::popups_allowed_for(id) {
                let target = url.to_string();
                ctl.post_tab(id, move |c| c.on_popup_blocked(id, target));
                return NewWindowResponse::Deny;
            }
            // Scripted popups that request a size (OAuth/sign-in flows) get a real window so `window.opener` keeps
            // working. It shares the opener's profile (or its private session), so a sign-in made in it is the
            // sign-in the opener sees. When it closes, the opener gets the focus back.
            if features.size().is_some() {
                let label = format!("popup-{}", POPUP_SEQ.fetch_add(1, Ordering::Relaxed));
                let title = url.host_str().unwrap_or("Popup").to_string();
                let popup = WebviewWindowBuilder::new(&app, label, WebviewUrl::External(url.clone()))
                    .window_features(features)
                    .title(title)
                    .data_directory(data_dir.clone())
                    .additional_browser_args(&browser_args())
                    .incognito(private)
                    .disable_drag_drop_handler()
                    .on_navigation(|u| !is_blocked_tab_navigation(u))
                    .build();
                match popup {
                    Ok(window) => {
                        let ctl_for_focus = ctl.clone();
                        window.on_window_event(move |event| {
                            if matches!(event, tauri::WindowEvent::Destroyed) {
                                ctl_for_focus.post_tab(id, move |c| c.focus_tab(id));
                            }
                        });
                        return NewWindowResponse::Create { window };
                    }
                    Err(e) => log::warn!("popup window failed, opening as tab: {e}"),
                }
            }
            // Ctrl+click opens in the background, like other browsers.
            let background = crate::native::current_mods().ctrl;
            let target = url.to_string();
            if crate::native::new_windows_open_as_windows() && !background {
                ctl.post_tab(id, move |c| c.open_from_page_in_window(id, target));
            } else {
                ctl.post_tab(id, move |c| c.open_from_page(id, target, background));
            }
            NewWindowResponse::Deny
        })
}
