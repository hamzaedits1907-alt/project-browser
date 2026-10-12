//! The standard Windows "Open" dialog, used to choose the password export file.
//!
//! The browser reads the file itself: the path never travels through the shell UI, so the shell cannot
//! ask this code to read an arbitrary file.

use std::path::PathBuf;

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Shows the dialog and returns the chosen file, or `None` when the user cancels. It blocks until the
/// dialog is closed, so call it from a worker thread (never the UI thread). `owner` is the window the
/// dialog belongs to (a raw `HWND`, 0 for none).
pub fn pick_file(owner: isize, title: &str, filter_name: &str, filter_pattern: &str) -> Option<PathBuf> {
    // The filter is a list of "<label>\0<pattern>\0" pairs that ends with an extra NUL.
    let mut filter: Vec<u16> = Vec::new();
    for part in [format!("{filter_name} ({filter_pattern})"), filter_pattern.to_string(), "All files (*.*)".to_string(), "*.*".to_string()] {
        filter.extend(part.encode_utf16());
        filter.push(0);
    }
    filter.push(0);
    let title = wide(title);
    let mut buffer = vec![0u16; 4096];
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: HWND(owner as *mut core::ffi::c_void),
        lpstrFilter: PCWSTR(filter.as_ptr()),
        nFilterIndex: 1,
        lpstrFile: PWSTR(buffer.as_mut_ptr()),
        nMaxFile: buffer.len() as u32,
        lpstrTitle: PCWSTR(title.as_ptr()),
        Flags: OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    // SAFETY: this runs on a thread of its own. Every pointer in `dialog` refers to a buffer that lives
    // until after the call; COM is initialised for the dialog's shell extensions and released afterwards.
    let chosen = unsafe {
        let com_ready = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let ok = GetOpenFileNameW(&mut dialog).as_bool();
        if com_ready {
            CoUninitialize();
        }
        ok
    };
    if !chosen {
        return None;
    }
    let len = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
    Some(PathBuf::from(String::from_utf16_lossy(&buffer[..len])))
}
