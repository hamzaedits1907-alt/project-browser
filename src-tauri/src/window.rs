//! Builders for the shell window and tab webviews.

use std::sync::atomic::{AtomicU64, Ordering};

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

pub const SHELL_LABEL: &str = "main";

static POPUP_SEQ: AtomicU64 = AtomicU64::new(1);

pub fn build_shell_window(app: &AppHandle, profile: &Profile, geometry: Option<WindowGeometry>) -> tauri::Result<WebviewWindow> {
    let mut builder = WebviewWindowBuilder::new(app, SHELL_LABEL, WebviewUrl::App("index.html".into()))
        .title("Project Browser")
        .decorations(false)
        .shadow(true)
        .visible(false)
        .min_inner_size(640.0, 420.0)
        .background_color(tauri::window::Color(11, 13, 18, 255))
        .data_directory(profile.webview_dir())
        .additional_browser_args(BROWSER_ARGS)
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
    builder.build()
}

/// A tab webview. It loads `about:blank`; the real URL is navigated after the
/// native event hooks are installed so no early event is missed.
pub fn tab_webview_builder(app: &AppHandle, id: TabId, private: bool, profile: &Profile, ctl: ControllerHandle) -> WebviewBuilder<Wry> {
    let app = app.clone();
    let data_dir = profile.webview_dir();
    WebviewBuilder::new(format!("tab-{id}"), WebviewUrl::External("about:blank".parse().expect("valid url")))
        .data_directory(data_dir.clone())
        .additional_browser_args(BROWSER_ARGS)
        .incognito(private)
        .devtools(true)
        .zoom_hotkeys_enabled(true)
        .disable_drag_drop_handler()
        .on_navigation(|url| !is_blocked_tab_navigation(url))
        .on_new_window(move |url, features| {
            // Scripted popups that request a size (OAuth/sign-in flows) get a real
            // window so `window.opener` keeps working; everything else becomes a tab.
            if features.size().is_some() && !is_blocked_tab_navigation(&url) {
                let label = format!("popup-{}", POPUP_SEQ.fetch_add(1, Ordering::Relaxed));
                let title = url.host_str().unwrap_or("Popup").to_string();
                let popup = WebviewWindowBuilder::new(&app, label, WebviewUrl::External(url.clone()))
                    .window_features(features)
                    .title(title)
                    .data_directory(data_dir.clone())
                    .additional_browser_args(BROWSER_ARGS)
                    .incognito(private)
                    .disable_drag_drop_handler()
                    .on_navigation(|u| !is_blocked_tab_navigation(u))
                    .build();
                match popup {
                    Ok(window) => return NewWindowResponse::Create { window },
                    Err(e) => log::warn!("popup window failed, opening as tab: {e}"),
                }
            }
            // Ctrl+click opens in the background, like other browsers.
            let background = crate::native::current_mods().ctrl;
            let target = url.to_string();
            ctl.post(move |c| c.open_from_page(id, target, background));
            NewWindowResponse::Deny
        })
}
