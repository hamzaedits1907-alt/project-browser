//! The card that follows the mouse while a tab is pulled out of its window.
//!
//! The shell can only draw inside its own window, so once a dragged tab leaves the window nothing showed what was
//! happening until the button was released. This is a small native window (no webview, so it costs next to nothing)
//! that shows the tab's title and what releasing it will do ("Release to open in a new window" / "… to move it into
//! this window"). It follows the pointer on its own timer, never takes focus, lets clicks pass through and is
//! destroyed when the drag ends.
//!
//! UI thread only.

use std::cell::RefCell;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, FrameRect, InvalidateRect, SelectObject, SetBkMode,
    SetTextColor, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, FONT_CHARSET, FONT_CLIP_PRECISION, FONT_OUTPUT_PRECISION,
    FONT_QUALITY, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetCursorPos, KillTimer, RegisterClassW, SetLayeredWindowAttributes, SetTimer, SetWindowPos,
    HWND_TOPMOST, LWA_ALPHA, SWP_NOACTIVATE, SWP_NOSIZE, SWP_SHOWWINDOW, WM_PAINT, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

thread_local! {
    static GHOST: RefCell<Option<isize>> = const { RefCell::new(None) };
    static TEXT: RefCell<(String, String)> = const { RefCell::new((String::new(), String::new())) };
}

const CLASS: PCWSTR = w!("ProjectBrowserTabGhost");
const TIMER: usize = 1;

fn scale() -> f64 {
    f64::from(unsafe { GetDpiForSystem() }.max(96)) / 96.0
}

fn size() -> (i32, i32) {
    let s = scale();
    ((300.0 * s) as i32, (54.0 * s) as i32)
}

fn follow(hwnd: HWND) {
    let mut p = POINT::default();
    if unsafe { GetCursorPos(&mut p) }.is_ok() {
        let off = (14.0 * scale()) as i32;
        let _ = unsafe { SetWindowPos(hwnd, Some(HWND_TOPMOST), p.x + off, p.y + off, 0, 0, SWP_NOACTIVATE | SWP_NOSIZE | SWP_SHOWWINDOW) };
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => {
            follow(hwnd);
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = unsafe { BeginPaint(hwnd, &mut ps) };
            let (w, h) = size();
            let s = scale();
            unsafe {
                let bg = CreateSolidBrush(COLORREF(0x0016_110F)); // #0f1116 (BGR)
                let border = CreateSolidBrush(COLORREF(0x00F0_D22E)); // accent #2ed2f0 (BGR)
                let full = RECT { left: 0, top: 0, right: w, bottom: h };
                FillRect(dc, &full, bg);
                FrameRect(dc, &full, border);
                let font = |px: f64, weight: i32| {
                    CreateFontW(
                        -((px * s) as i32),
                        0,
                        0,
                        0,
                        weight,
                        0,
                        0,
                        0,
                        FONT_CHARSET(1),
                        FONT_OUTPUT_PRECISION(0),
                        FONT_CLIP_PRECISION(0),
                        FONT_QUALITY(5),
                        0,
                        w!("Segoe UI"),
                    )
                };
                let title_font = font(13.0, 600);
                let hint_font = font(11.5, 400);
                SetBkMode(dc, TRANSPARENT);
                let pad = (12.0 * s) as i32;
                let (title, hint) = TEXT.with(|t| t.borrow().clone());
                let old = SelectObject(dc, title_font.into());
                SetTextColor(dc, COLORREF(0x00F6_EEEA));
                let mut r1 = RECT { left: pad, top: (6.0 * s) as i32, right: w - pad, bottom: (28.0 * s) as i32 };
                DrawTextW(dc, &mut wide(&title), &mut r1, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX);
                SelectObject(dc, hint_font.into());
                SetTextColor(dc, COLORREF(0x00F0_D22E));
                let mut r2 = RECT { left: pad, top: (28.0 * s) as i32, right: w - pad, bottom: h - (6.0 * s) as i32 };
                DrawTextW(dc, &mut wide(&hint), &mut r2, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX);
                SelectObject(dc, old);
                let _ = DeleteObject(title_font.into());
                let _ = DeleteObject(hint_font.into());
                let _ = DeleteObject(bg.into());
                let _ = DeleteObject(border.into());
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Shows (or updates) the card for a tab titled `title` with `hint` under it.
pub fn show(title: &str, hint: &str) {
    TEXT.with(|t| *t.borrow_mut() = (title.chars().take(120).collect(), hint.to_string()));
    if let Some(h) = GHOST.with(|g| *g.borrow()) {
        unsafe {
            let _ = InvalidateRect(Some(HWND(h as *mut _)), None, true);
        }
        return;
    }
    unsafe {
        let Ok(module) = GetModuleHandleW(None) else { return };
        let class = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: module.into(), lpszClassName: CLASS, ..Default::default() };
        RegisterClassW(&class); // fails harmlessly when already registered
        let (w, h) = size();
        let Ok(hwnd) = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            CLASS,
            w!(""),
            WS_POPUP,
            0,
            0,
            w,
            h,
            None,
            None,
            Some(module.into()),
            None,
        ) else {
            return;
        };
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 235, LWA_ALPHA);
        follow(hwnd);
        SetTimer(Some(hwnd), TIMER, 15, None);
        GHOST.with(|g| *g.borrow_mut() = Some(hwnd.0 as isize));
    }
}

/// Removes the card.
pub fn hide() {
    if let Some(h) = GHOST.with(|g| g.borrow_mut().take()) {
        unsafe {
            let hwnd = HWND(h as *mut _);
            let _ = KillTimer(Some(hwnd), TIMER);
            let _ = DestroyWindow(hwnd);
        }
    }
}
