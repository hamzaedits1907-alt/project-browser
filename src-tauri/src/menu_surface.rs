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

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

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
static OPEN: AtomicBool = AtomicBool::new(false);
static MENU_HWND: AtomicIsize = AtomicIsize::new(0);
/// Counts menus shown, for the log (one session per shown menu).
static SESSION: AtomicU64 = AtomicU64::new(0);
static APP: OnceLock<AppHandle> = OnceLock::new();

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
        Ok(w) => {
            let _ = APP.set(app.clone());
            if let Ok(h) = w.hwnd() {
                crate::native::make_no_activate(h.0 as isize);
            }
            log::info!("menu window created (hidden, never takes focus)");
        }
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
///
/// The menu window never takes keyboard focus (it is a non-activating window, like a system menu): the page keeps
/// focus, nothing has to be "given back" afterwards, and WebView2 moving focus around after a right-click cannot close
/// it. While it is open, a click anywhere outside it closes it (a low-level mouse hook, installed only for that time),
/// Escape and the arrow keys typed into the page or the shell are routed to it (`route_key`), and the browser window
/// going to the background closes it (`owner_deactivated`).
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
    let menu_hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    if let Some(owner) = app.get_window(&a.owner) {
        if let Ok(owner) = owner.hwnd() {
            crate::native::set_owner(menu_hwnd, owner.0 as isize);
        }
    }
    window.set_size(PhysicalSize::new(size.0 as u32, size.1 as u32)).map_err(|e| e.to_string())?;
    window.set_position(PhysicalPosition::new(x, y)).map_err(|e| e.to_string())?;
    let _ = window.with_webview(|pw| crate::native::set_view_memory_target(&pw, false));
    crate::native::show_without_focus(menu_hwnd);
    OPEN.store(true, Ordering::SeqCst);
    MENU_HWND.store(menu_hwnd, Ordering::SeqCst);
    crate::native::outside_click_watch(true);
    let n = SESSION.fetch_add(1, Ordering::SeqCst) + 1;
    log::info!("menu window: menu session {n} shown");
    Ok(())
}

/// A menu is open in the menu window.
pub fn is_open() -> bool {
    OPEN.load(Ordering::SeqCst)
}

/// The menu window's handle while a menu is open (0 otherwise), for the outside-click check.
pub fn open_hwnd() -> isize {
    if is_open() {
        MENU_HWND.load(Ordering::SeqCst)
    } else {
        0
    }
}

/// Asks the menu window to close its menu as dismissed (`reason` goes to the log). Safe to call at any time.
pub fn request_dismiss(reason: &'static str) {
    if !is_open() {
        return;
    }
    let Some(app) = APP.get() else { return };
    log::info!("menu window: dismissed ({reason})");
    use tauri::Emitter;
    let _ = app.emit_to(LABEL, "pb:menu-dismiss", reason);
}

/// A key typed into a page or a shell while a menu is open: Escape, arrows, Home/End and Enter belong to the menu.
/// Returns true when the key was taken.
pub fn route_key(vk: u32) -> bool {
    if !is_open() {
        return false;
    }
    let key = match vk {
        0x1B => "Escape",
        0x26 => "ArrowUp",
        0x28 => "ArrowDown",
        0x24 => "Home",
        0x23 => "End",
        0x0D => "Enter",
        _ => return false,
    };
    let Some(app) = APP.get() else { return false };
    use tauri::Emitter;
    let _ = app.emit_to(LABEL, "pb:menu-key", key);
    true
}

/// A browser window stopped being the window in front. If that took the user to another program, an open menu closes.
pub fn owner_deactivated(app: &AppHandle) {
    if !is_open() {
        return;
    }
    let Some(owner) = anchor().and_then(|a| app.get_window(&a.owner)) else { return };
    let Ok(hwnd) = owner.hwnd() else { return };
    if !crate::native::is_foreground(hwnd.0 as isize) {
        request_dismiss("another window came to the front");
    }
}

/// Hides the menu window (a pick or a dismissal; the shell that owns the menu is told separately by the menu window).
pub fn dismiss(app: &AppHandle) {
    OPEN.store(false, Ordering::SeqCst);
    crate::native::outside_click_watch(false);
    if let Some(w) = app.get_webview_window(LABEL) {
        // Shown natively (without activation), so hidden natively too: Tauri's hide() believes it is already hidden.
        if let Ok(h) = w.hwnd() {
            crate::native::hide_window(h.0 as isize);
        }
        // Hidden most of the time: WebView2 may trim its memory meanwhile.
        let _ = w.with_webview(|pw| crate::native::set_view_memory_target(&pw, true));
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
