//! The right-click menu's own window.
//!
//! Tab pages are native webviews drawn above the shell, so a menu drawn by the shell can only be seen if the page is
//! hidden while it is open (lib/overlay.ts: the page is replaced by a still capture). That froze the page for the
//! whole time a menu was open (video, animations, typing), and the capture itself delayed the menu.
//!
//! Instead the menu gets a small borderless, always-on-top window of its own ("ctxmenu"), created once after startup
//! and kept hidden. The shell that received the right-click sends it the menu model; the menu window renders it,
//! measures itself and calls `menu_present`, which places it at the click (inside the monitor's work area), makes the
//! clicked browser window its owner and shows it. The page underneath keeps running and stays visible. The pick (or
//! a dismissal) goes back to that shell, which carries it out exactly as before. If the window is not ready yet, the
//! shell falls back to the old overlay menu.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

use crate::profile::Profile;

pub const LABEL: &str = "ctxmenu";

/// Where the next menu opens: the browser window it belongs to and the point, in physical screen pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Anchor {
    pub owner: String,
    pub x: i32,
    pub y: i32,
}

static ANCHOR: Mutex<Option<Anchor>> = Mutex::new(None);
static READY: AtomicBool = AtomicBool::new(false);
static CREATING: AtomicBool = AtomicBool::new(false);

/// The menu window has loaded and can show menus.
pub fn is_ready() -> bool {
    READY.load(Ordering::SeqCst)
}

pub fn set_ready(ready: bool) {
    READY.store(ready, Ordering::SeqCst);
}

pub fn set_anchor(anchor: Anchor) {
    if let Ok(mut a) = ANCHOR.lock() {
        *a = Some(anchor);
    }
}

fn anchor() -> Option<Anchor> {
    ANCHOR.lock().ok().and_then(|a| a.clone())
}

/// Creates the hidden menu window once (it must share the shell's WebView2 environment options exactly).
pub fn ensure(app: &AppHandle, profile: &Profile) {
    if app.get_webview_window(LABEL).is_some() || CREATING.swap(true, Ordering::SeqCst) {
        return;
    }
    let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
        .title("Project Browser")
        .decorations(false)
        .shadow(true)
        .visible(false)
        .focused(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .inner_size(288.0, 120.0)
        .background_color(tauri::window::Color(20, 23, 31, 255))
        .data_directory(profile.webview_dir())
        .additional_browser_args(&crate::window::browser_args())
        .browser_extensions_enabled(true)
        .disable_drag_drop_handler()
        .build();
    match built {
        Ok(_) => log::info!("menu window created (hidden)"),
        Err(e) => {
            log::warn!("menu window could not be created, menus use the overlay: {e}");
            CREATING.store(false, Ordering::SeqCst);
        }
    }
}

/// Top-left corner for a menu of `size` opened at `point` inside `work` (all physical pixels): to the right of and
/// below the point when it fits, flipped to the other side when it does not, and never outside the work area.
pub fn place(point: (i32, i32), size: (i32, i32), work: (i32, i32, i32, i32)) -> (i32, i32) {
    let (left, top, right, bottom) = work;
    let mut x = point.0;
    let mut y = point.1;
    if x + size.0 > right {
        x = point.0 - size.0;
    }
    if y + size.1 > bottom {
        y = point.1 - size.1;
    }
    x = x.min(right - size.0).max(left);
    y = y.min(bottom - size.1).max(top);
    (x, y)
}

/// Shows the menu window at the current anchor with the given content size (CSS pixels).
pub fn present(app: &AppHandle, width: f64, height: f64) -> Result<(), String> {
    let window = app.get_webview_window(LABEL).ok_or("no menu window")?;
    let a = anchor().ok_or("no menu to show")?;
    let monitor = app.monitor_from_point(a.x as f64, a.y as f64).ok().flatten().or_else(|| window.current_monitor().ok().flatten());
    let (scale, work) = match &monitor {
        Some(m) => {
            let w = m.work_area();
            (m.scale_factor(), (w.position.x, w.position.y, w.position.x + w.size.width as i32, w.position.y + w.size.height as i32))
        }
        None => (1.0, (i32::MIN / 2, i32::MIN / 2, i32::MAX / 2, i32::MAX / 2)),
    };
    let size = ((width.max(40.0) * scale).ceil() as i32, (height.max(20.0) * scale).ceil() as i32);
    let (x, y) = place((a.x, a.y), size, work);
    if let Some(owner) = app.get_window(&a.owner) {
        if let (Ok(menu), Ok(owner)) = (window.hwnd(), owner.hwnd()) {
            crate::native::set_owner(menu.0 as isize, owner.0 as isize);
        }
    }
    window.set_size(PhysicalSize::new(size.0 as u32, size.1 as u32)).map_err(|e| e.to_string())?;
    window.set_position(PhysicalPosition::new(x, y)).map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

/// Hides the menu window. `refocus`: the menu still had focus (a pick or Escape), so the browser window it belongs
/// to gets it back; a menu that closed because the user clicked elsewhere leaves focus where it went.
pub fn dismiss(app: &AppHandle, refocus: bool) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.hide();
    }
    if refocus {
        if let Some(owner) = anchor().and_then(|a| app.get_window(&a.owner)) {
            let _ = owner.set_focus();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: (i32, i32, i32, i32) = (0, 0, 1920, 1040);

    #[test]
    fn a_menu_opens_below_and_right_of_the_point_when_it_fits() {
        assert_eq!(place((100, 100), (300, 400), WORK), (100, 100));
    }

    #[test]
    fn a_menu_flips_left_or_up_near_the_work_area_edges() {
        assert_eq!(place((1800, 100), (300, 400), WORK), (1500, 100));
        assert_eq!(place((100, 900), (300, 400), WORK), (100, 500));
        assert_eq!(place((1800, 900), (300, 400), WORK), (1500, 500));
    }

    #[test]
    fn a_menu_never_leaves_the_work_area() {
        // Taller than the space on either side: pinned to the top of the work area.
        assert_eq!(place((100, 500), (300, 1200), WORK), (100, 0));
        // A second monitor left of the primary one (negative coordinates).
        assert_eq!(place((-50, 10), (300, 200), (-1920, 0, 0, 1080)), (-350, 10));
    }
}
