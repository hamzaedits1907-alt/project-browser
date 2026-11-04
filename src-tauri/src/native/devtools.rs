//! Developer tools docked inside the browser window, like Chrome's.
//!
//! WebView2 has no API to dock its DevTools: `OpenDevToolsWindow` always opens a separate top-level window, owned by
//! the WebView2 browser process. Docking therefore works on that window itself:
//!
//! 1. Before DevTools is opened for a tab (F12, Ctrl+Shift+I/J/C, or "Inspect" in the right-click menu), the tab
//!    *expects* a DevTools window: the DevTools windows that already exist are noted and a WinEvent hook watches the
//!    browser process for windows being shown or renamed.
//! 2. The first new `Chrome_WidgetWin_1` window whose title starts with "DevTools" is adopted: its frame styles are
//!    removed, it becomes a child window of the browser window (`SetParent`), and it is hidden until the controller
//!    gives it its place below or beside the page (`place`).
//!    The window goes into a plain holder window of ours, placed a caption's height higher than the holder's top:
//!    Chromium keeps drawing its own title bar ("DevTools - …" with minimise/maximise/close) inside the window even
//!    without a frame, and the holder cuts that strip off.
//! 3. Chromium sometimes moves the window again on its own (restoring the size it last had); the same hook sees the
//!    location change and puts it back. When it is destroyed (closed, or its tab went away) the controller is told.
//!
//! Cross-process child windows are what WebView2 itself uses to show pages, so keyboard and mouse input work as in
//! a page. Nothing here is visible to pages; it only moves a window the user asked to open. If no DevTools window
//! shows up within a few seconds (a WebView2 that names it differently), nothing happens and it stays a separate
//! window, which is exactly the old behaviour.
//!
//! Every function runs on the UI thread except `is_alive`, which only asks Windows whether a window still exists.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::core::{w, BOOL, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, EnumWindows, GetAncestor, GetClassNameW, GetWindowLongPtrW, GetWindowRect, GetWindowTextW,
    GetWindowThreadProcessId, IsWindow, PostMessageW, SetParent, SetWindowLongPtrW, SetWindowPos, ShowWindow, EVENT_OBJECT_DESTROY,
    EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_SHOW, GA_ROOT, GWL_EXSTYLE, GWL_STYLE, HWND_TOP, SM_CXPADDEDBORDER,
    SM_CYCAPTION, SM_CYSIZEFRAME, SWP_ASYNCWINDOWPOS, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, WINDOW_EX_STYLE,
    WINEVENT_OUTOFCONTEXT, WM_CLOSE, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_APPWINDOW, WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME,
    WS_EX_WINDOWEDGE, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
};

use super::tab_com;
use crate::controller::ControllerHandle;
use crate::tabs::TabId;

/// Docking on (settings.devtools_dock is not "window"); set by the controller.
static DOCKING: AtomicBool = AtomicBool::new(true);

pub fn set_docking(on: bool) {
    DOCKING.store(on, Ordering::Relaxed);
}

pub fn docking_enabled() -> bool {
    DOCKING.load(Ordering::Relaxed)
}

/// How long a tab waits for its DevTools window to appear before giving up (it then stays a separate window).
const EXPECT_FOR: Duration = Duration::from_secs(6);
/// Chromium's window class for every top-level window it creates.
const CHROME_CLASS: &str = "Chrome_WidgetWin_1";

struct Expectation {
    tab: TabId,
    parent: isize,
    pid: u32,
    since: Instant,
    /// DevTools windows that already existed (another tab's, or a separate one): never adopted.
    known: Vec<isize>,
}

struct Docked {
    tab: TabId,
    /// Our holder window (child of the browser window) that the DevTools window sits in.
    container: isize,
    /// Where the DevTools window must be inside the holder (its title bar above the top edge).
    inner: Option<(i32, i32, i32, i32)>,
    /// Styles before docking, put back when the window is undocked.
    style: isize,
    ex_style: isize,
    /// Where it belongs in its parent's client area (physical pixels); `None` = hidden.
    target: Option<(i32, i32, i32, i32)>,
    /// Corrections made recently (Chromium moving it back), to give up instead of fighting forever.
    corrections: u32,
    window_since: Instant,
}

thread_local! {
    static EXPECT: RefCell<Option<Expectation>> = const { RefCell::new(None) };
    static DOCKED: RefCell<HashMap<isize, Docked>> = RefCell::new(HashMap::new());
    /// The WinEvent hook and the browser process it watches.
    static HOOK: RefCell<Option<(isize, u32)>> = const { RefCell::new(None) };
    static CTL: RefCell<Option<ControllerHandle>> = const { RefCell::new(None) };
}

fn h(v: isize) -> HWND {
    HWND(v as *mut _)
}

fn text_of(hwnd: HWND, class: bool) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe {
        if class {
            GetClassNameW(hwnd, &mut buf)
        } else {
            GetWindowTextW(hwnd, &mut buf)
        }
    };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Whether `title` is a DevTools window's title ("DevTools - example.com/" in every Chromium language).
pub fn is_devtools_title(title: &str) -> bool {
    title.starts_with("DevTools")
}

fn process_of(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

fn is_devtools_window(hwnd: HWND, pid: u32) -> bool {
    process_of(hwnd) == pid && text_of(hwnd, true) == CHROME_CLASS && is_devtools_title(&text_of(hwnd, false))
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let out = unsafe { &mut *(lparam.0 as *mut (u32, Vec<isize>)) };
    if is_devtools_window(hwnd, out.0) {
        out.1.push(hwnd.0 as isize);
    }
    BOOL(1)
}

/// The DevTools windows of browser process `pid` that are top-level right now.
fn devtools_windows(pid: u32) -> Vec<isize> {
    let mut out: (u32, Vec<isize>) = (pid, Vec::new());
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut out as *mut _ as isize));
    }
    out.1
}

/// Turns the top-level DevTools window `hwnd` into a hidden child of `parent`.
fn adopt(hwnd: isize, parent: isize, tab: TabId) {
    unsafe {
        let w = h(hwnd);
        let style = GetWindowLongPtrW(w, GWL_STYLE);
        let ex_style = GetWindowLongPtrW(w, GWL_EXSTYLE);
        let frame = (WS_POPUP.0 | WS_CAPTION.0 | WS_THICKFRAME.0 | WS_SYSMENU.0 | WS_MINIMIZEBOX.0 | WS_MAXIMIZEBOX.0) as isize;
        let ex_frame = (WS_EX_APPWINDOW.0 | WS_EX_WINDOWEDGE.0 | WS_EX_CLIENTEDGE.0 | WS_EX_DLGMODALFRAME.0) as isize;
        let _ = ShowWindow(w, SW_HIDE);
        let container = match CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            PCWSTR::null(),
            WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            0,
            0,
            0,
            0,
            Some(h(parent)),
            None,
            None,
            None,
        ) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("devtools: no holder window ({e}); it stays separate");
                let _ = ShowWindow(w, windows::Win32::UI::WindowsAndMessaging::SW_SHOW);
                return;
            }
        };
        SetWindowLongPtrW(w, GWL_STYLE, (style & !frame) | WS_CHILD.0 as isize);
        SetWindowLongPtrW(w, GWL_EXSTYLE, ex_style & !ex_frame);
        if let Err(e) = SetParent(w, Some(container)) {
            log::warn!("devtools: could not dock the window ({e}); it stays separate");
            let _ = DestroyWindow(container);
            SetWindowLongPtrW(w, GWL_STYLE, style);
            SetWindowLongPtrW(w, GWL_EXSTYLE, ex_style);
            let _ = ShowWindow(w, windows::Win32::UI::WindowsAndMessaging::SW_SHOW);
            return;
        }
        let _ = SetWindowPos(w, Some(HWND_TOP), 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOACTIVATE);
        DOCKED.with(|d| {
            d.borrow_mut().insert(
                hwnd,
                Docked {
                    tab,
                    container: container.0 as isize,
                    inner: None,
                    style,
                    ex_style,
                    target: None,
                    corrections: 0,
                    window_since: Instant::now(),
                },
            )
        });
    }
    log::info!("devtools: docked the DevTools window of tab {tab}");
    if let Some(ctl) = CTL.with(|c| c.borrow().clone()) {
        ctl.post_tab(tab, move |c| c.on_devtools_docked(tab, hwnd));
    }
}

/// Where `hwnd` is in its parent's client area (physical pixels).
fn client_rect(hwnd: HWND) -> Option<(i32, i32, i32, i32)> {
    let parent = unsafe { windows::Win32::UI::WindowsAndMessaging::GetParent(hwnd) }.ok()?;
    let mut r = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut r) }.ok()?;
    let mut p = POINT { x: r.left, y: r.top };
    let _ = unsafe { ScreenToClient(parent, &mut p) };
    Some((p.x, p.y, r.right - r.left, r.bottom - r.top))
}

unsafe extern "system" fn on_event(_hook: HWINEVENTHOOK, event: u32, hwnd: HWND, id_object: i32, id_child: i32, _thread: u32, _time: u32) {
    // Only the windows themselves (OBJID_WINDOW, CHILDID_SELF), not their accessible parts.
    if id_object != 0 || id_child != 0 || hwnd.0.is_null() {
        return;
    }
    let key = hwnd.0 as isize;
    match event {
        EVENT_OBJECT_DESTROY => {
            if let Some(d) = DOCKED.with(|d| d.borrow_mut().remove(&key)) {
                let _ = DestroyWindow(h(d.container));
                let tab = d.tab;
                log::info!("devtools: the docked DevTools of tab {tab} closed");
                if let Some(ctl) = CTL.with(|c| c.borrow().clone()) {
                    ctl.post_tab(tab, move |c| c.on_devtools_closed(tab, key));
                }
                unhook_if_idle();
            }
        }
        EVENT_OBJECT_LOCATIONCHANGE => {
            let target = DOCKED.with(|d| {
                let mut d = d.borrow_mut();
                let entry = d.get_mut(&key)?;
                if entry.window_since.elapsed() > Duration::from_secs(1) {
                    entry.window_since = Instant::now();
                    entry.corrections = 0;
                }
                entry.corrections += 1;
                // Give up after many corrections in one second rather than fight Chromium in a loop.
                (entry.corrections <= 20).then_some(entry.inner.zip(entry.target)).flatten()
            });
            if let Some((inner, outer)) = target {
                if client_rect(hwnd) != Some(inner) {
                    apply(key, Some(outer));
                }
            }
        }
        EVENT_OBJECT_SHOW | EVENT_OBJECT_NAMECHANGE => {
            let found = EXPECT.with(|e| {
                let e = e.borrow();
                let x = e.as_ref()?;
                (x.since.elapsed() < EXPECT_FOR && !x.known.contains(&key) && is_devtools_window(hwnd, x.pid)).then_some((x.tab, x.parent))
            });
            if let Some((tab, parent)) = found {
                EXPECT.with(|e| e.borrow_mut().take());
                adopt(key, parent, tab);
            } else if EXPECT.with(|e| e.borrow().as_ref().is_some_and(|x| x.since.elapsed() >= EXPECT_FOR)) {
                EXPECT.with(|e| e.borrow_mut().take());
                log::info!("devtools: no DevTools window appeared to dock; it stays separate");
                unhook_if_idle();
            }
        }
        _ => {}
    }
}

fn ensure_hook(pid: u32) {
    let current = HOOK.with(|x| *x.borrow());
    if let Some((_, p)) = current {
        if p == pid {
            return;
        }
        unhook();
    }
    let hook = unsafe { SetWinEventHook(EVENT_OBJECT_DESTROY, EVENT_OBJECT_NAMECHANGE, None, Some(on_event), pid, 0, WINEVENT_OUTOFCONTEXT) };
    if hook.0.is_null() {
        log::warn!("devtools: cannot watch for the DevTools window; it opens separately");
        return;
    }
    HOOK.with(|x| *x.borrow_mut() = Some((hook.0 as isize, pid)));
}

fn unhook() {
    if let Some((hook, _)) = HOOK.with(|x| x.borrow_mut().take()) {
        unsafe {
            let _ = UnhookWinEvent(HWINEVENTHOOK(hook as *mut _));
        }
    }
}

fn unhook_if_idle() {
    let idle = DOCKED.with(|d| d.borrow().is_empty()) && EXPECT.with(|e| e.borrow().is_none());
    if idle {
        unhook();
    }
}

/// Opens DevTools for tab `id`. `dock`: adopt the window into the tab's browser window (otherwise it stays separate).
pub fn open(id: TabId, dock: bool, ctl: ControllerHandle) {
    let Some(t) = tab_com(id) else { return };
    if dock {
        expect(id, ctl);
    }
    unsafe {
        if let Err(e) = t.webview.OpenDevToolsWindow() {
            log::warn!("devtools: WebView2 refused to open DevTools for tab {id}: {e}");
            EXPECT.with(|e| e.borrow_mut().take());
            unhook_if_idle();
        }
    }
}

/// Tab `id` is about to open DevTools by itself ("Inspect" in WebView2's own command list): dock the window it opens.
pub fn expect(id: TabId, ctl: ControllerHandle) {
    let Some(t) = tab_com(id) else { return };
    unsafe {
        let mut container = HWND::default();
        if t.controller.ParentWindow(&mut container).is_err() || container.0.is_null() {
            return;
        }
        let root = GetAncestor(container, GA_ROOT);
        let mut pid = 0u32;
        if root.0.is_null() || t.webview.BrowserProcessId(&mut pid).is_err() || pid == 0 {
            return;
        }
        CTL.with(|c| *c.borrow_mut() = Some(ctl));
        let known = devtools_windows(pid);
        EXPECT.with(|e| *e.borrow_mut() = Some(Expectation { tab: id, parent: root.0 as isize, pid, since: Instant::now(), known }));
        ensure_hook(pid);
    }
}

/// Height of the title bar Chromium draws at the top of a DevTools window, at the DPI of `hwnd`.
fn caption_height(hwnd: HWND) -> i32 {
    unsafe {
        let dpi = match GetDpiForWindow(hwnd) {
            0 => 96,
            d => d,
        };
        GetSystemMetricsForDpi(SM_CYCAPTION, dpi) + GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi)
    }
}

fn apply(hwnd: isize, target: Option<(i32, i32, i32, i32)>) {
    let Some(container) = DOCKED.with(|d| d.borrow().get(&hwnd).map(|e| e.container)) else { return };
    unsafe {
        let holder = h(container);
        match target {
            Some((x, y, width, height)) => {
                let (width, height) = (width.max(1), height.max(1));
                let cap = caption_height(holder);
                let inner = (0, -cap, width, height + cap);
                DOCKED.with(|d| {
                    if let Some(e) = d.borrow_mut().get_mut(&hwnd) {
                        e.inner = Some(inner);
                    }
                });
                let _ = SetWindowPos(holder, Some(HWND_TOP), x, y, width, height, SWP_NOACTIVATE | SWP_SHOWWINDOW);
                let _ = SetWindowPos(
                    h(hwnd),
                    None,
                    inner.0,
                    inner.1,
                    inner.2,
                    inner.3,
                    SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW | SWP_ASYNCWINDOWPOS,
                );
            }
            None => {
                let _ = ShowWindow(holder, SW_HIDE);
            }
        }
    }
}

/// Puts the docked DevTools window `hwnd` at `target` in its browser window's client area (physical pixels), or
/// hides it (`None`).
pub fn place(hwnd: isize, target: Option<(i32, i32, i32, i32)>) {
    let known = DOCKED.with(|d| {
        let mut d = d.borrow_mut();
        let Some(entry) = d.get_mut(&hwnd) else { return false };
        entry.target = target;
        entry.corrections = 0;
        true
    });
    if known {
        apply(hwnd, target);
    }
}

/// Closes a DevTools window (docked or not).
pub fn close(hwnd: isize) {
    unsafe {
        let _ = PostMessageW(Some(h(hwnd)), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

/// Gives the docked window back its frame as a separate window at `screen` (physical pixels).
pub fn undock(hwnd: isize, screen: (i32, i32, i32, i32)) {
    let Some(d) = DOCKED.with(|d| d.borrow_mut().remove(&hwnd)) else { return };
    unsafe {
        let w = h(hwnd);
        let _ = ShowWindow(w, SW_HIDE);
        let _ = SetParent(w, None);
        let _ = DestroyWindow(h(d.container));
        SetWindowLongPtrW(w, GWL_STYLE, d.style);
        SetWindowLongPtrW(w, GWL_EXSTYLE, d.ex_style);
        let (x, y, width, height) = screen;
        let _ = SetWindowPos(w, Some(HWND_TOP), x, y, width, height, SWP_FRAMECHANGED | SWP_SHOWWINDOW);
    }
    unhook_if_idle();
}

/// Whether window `hwnd` still exists (any thread).
pub fn is_alive(hwnd: isize) -> bool {
    unsafe { IsWindow(Some(h(hwnd))).as_bool() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devtools_titles_are_recognised() {
        assert!(is_devtools_title("DevTools - example.com/"));
        assert!(is_devtools_title("DevTools"));
        assert!(!is_devtools_title("Example Domain"));
        assert!(!is_devtools_title(""));
    }
}
