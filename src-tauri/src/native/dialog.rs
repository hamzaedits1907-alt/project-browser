//! The standard Windows "Open" and "Save as" dialogs (the password export file; where to save a download).
//!
//! The browser reads the file itself: the path never travels through the shell UI, so the shell cannot
//! ask this code to read an arbitrary file.

use std::path::PathBuf;

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, GetSaveFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST,
    OPENFILENAMEW,
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

/// The standard "Save as" dialog, starting at `suggested` (a full path: its folder and file name). Returns the chosen
/// path, or `None` when the user cancels. Replacing an existing file needs the dialog's own confirmation. Blocks until
/// the dialog closes: call it from a worker thread.
pub fn save_file(owner: isize, suggested: &str) -> Option<PathBuf> {
    let title = wide("Save download as");
    let mut buffer = vec![0u16; 4096];
    let initial: Vec<u16> = suggested.encode_utf16().take(buffer.len() - 1).collect();
    buffer[..initial.len()].copy_from_slice(&initial);
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: HWND(owner as *mut core::ffi::c_void),
        lpstrFile: PWSTR(buffer.as_mut_ptr()),
        nMaxFile: buffer.len() as u32,
        lpstrTitle: PCWSTR(title.as_ptr()),
        Flags: OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY | OFN_NOCHANGEDIR | OFN_OVERWRITEPROMPT,
        ..Default::default()
    };
    // SAFETY: as in `pick_file`: every pointer refers to a buffer that outlives the call, on a thread of its own.
    let chosen = unsafe {
        let com_ready = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let ok = GetSaveFileNameW(&mut dialog).as_bool();
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
