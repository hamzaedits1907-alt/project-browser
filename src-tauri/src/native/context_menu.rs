//! Project Browser's own right-click menu.
//!
//! WebView2's default menu (the Microsoft-styled one) is replaced. `ContextMenuRequested` is marked handled so that menu
//! never appears; the event's deferral is kept open while the shell shows its menu. What was clicked (page, link, image,
//! selection, editable field, audio or video) and the commands WebView2 itself offers for it go to the shell of the
//! window. When the user picks one of WebView2's commands (copy, paste, save image as, picture in picture, …) its id
//! goes back through `SelectedCommandId` and WebView2 runs it exactly as its own menu would: nothing is imitated with
//! page scripts. Menu items that are the browser's own (open link in a new tab, search, bookmark, AI) are carried out
//! by the shell through the ordinary commands, and the deferral is completed without a WebView2 command.
//!
//! A page can neither open this menu nor choose from it: the event is raised by WebView2 for a real right-click or
//! the context-menu key, and only the shell webview can call `context_menu_choose`, which only accepts a command id
//! that WebView2 offered for that very request.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::Interface;
use windows::Win32::Foundation::POINT;

use super::{pwstr, read_bool};
use crate::controller::ControllerHandle;
use crate::tabs::TabId;

/// Who asked for the menu: a tab's page, or a window's shell (its own text fields).
#[derive(Debug, Clone, PartialEq)]
pub enum MenuOwner {
    Tab(TabId),
    Shell(String),
}

struct Pending {
    args: ICoreWebView2ContextMenuRequestedEventArgs,
    deferral: ICoreWebView2Deferral,
    owner: MenuOwner,
    offered: Vec<i32>,
}

thread_local! {
    static PENDING: RefCell<HashMap<u64, Pending>> = RefCell::new(HashMap::new());
}

static NEXT: AtomicU64 = AtomicU64::new(1);

const MAX_ITEMS: u32 = 80;
const MAX_SELECTION: usize = 10_000;
const MAX_LINK_TEXT: usize = 500;

/// One of WebView2's own menu commands, as it offered it for this request.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NativeMenuItem {
    pub command_id: i32,
    /// WebView2's stable name ("copy", "paste", "saveImageAs", …); the shell labels known names itself.
    pub name: String,
    /// WebView2's label in the system language, without the `&` mnemonic markers.
    pub label: String,
    pub kind: &'static str,
    pub enabled: bool,
    pub checked: bool,
    pub shortcut: String,
    pub children: Vec<NativeMenuItem>,
}

/// What was right-clicked.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextTarget {
    /// "page", "image", "selection", "audio" or "video".
    pub kind: &'static str,
    pub editable: bool,
    pub main_frame: bool,
    pub page_url: String,
    pub frame_url: String,
    pub link_url: Option<String>,
    pub link_text: Option<String>,
    pub source_url: Option<String>,
    pub selection: Option<String>,
}

pub struct ContextRequest {
    pub id: u64,
    pub owner: MenuOwner,
    pub target: ContextTarget,
    /// Where the menu was asked for, relative to the webview, in physical pixels.
    pub location: (i32, i32),
    pub items: Vec<NativeMenuItem>,
}

/// Removes the `&` that marks a keyboard mnemonic in a Windows menu label (`&&` is a literal `&`).
pub fn clean_label(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut chars = label.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            if chars.peek() == Some(&'&') {
                out.push('&');
                chars.next();
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn bounded(text: String, max: usize) -> String {
    if text.chars().count() <= max {
        text
    } else {
        text.chars().take(max).collect()
    }
}

fn item_kind(kind: COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND) -> &'static str {
    match kind {
        COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_CHECK_BOX => "checkbox",
        COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_RADIO => "radio",
        COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR => "separator",
        COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SUBMENU => "submenu",
        _ => "command",
    }
}

fn target_kind(kind: COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND) -> &'static str {
    match kind {
        COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND_IMAGE => "image",
        COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND_SELECTED_TEXT => "selection",
        COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND_AUDIO => "audio",
        COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND_VIDEO => "video",
        _ => "page",
    }
}

unsafe fn read_items(coll: &ICoreWebView2ContextMenuItemCollection, depth: u32, offered: &mut Vec<i32>) -> Vec<NativeMenuItem> {
    let mut out = Vec::new();
    let mut n = 0u32;
    let _ = coll.Count(&mut n);
    for i in 0..n.min(MAX_ITEMS) {
        let Ok(item) = coll.GetValueAtIndex(i) else { continue };
        let mut kind = COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND::default();
        let _ = item.Kind(&mut kind);
        let mut command_id = -1i32;
        let _ = item.CommandId(&mut command_id);
        let children = if kind == COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SUBMENU && depth < 3 {
            item.Children().map(|c| read_items(&c, depth + 1, offered)).unwrap_or_default()
        } else {
            Vec::new()
        };
        let enabled = read_bool(|b| item.IsEnabled(b));
        if enabled && kind != COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR && kind != COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SUBMENU {
            offered.push(command_id);
        }
        out.push(NativeMenuItem {
            command_id,
            name: pwstr(|p| item.Name(p)),
            label: clean_label(&pwstr(|p| item.Label(p))),
            kind: item_kind(kind),
            enabled,
            checked: read_bool(|b| item.IsChecked(b)),
            shortcut: pwstr(|p| item.ShortcutKeyDescription(p)),
            children,
        });
    }
    out
}

unsafe fn read_target(t: &ICoreWebView2ContextMenuTarget) -> ContextTarget {
    let mut kind = COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND::default();
    let _ = t.Kind(&mut kind);
    let opt = |has: bool, value: String, max: usize| (has && !value.is_empty()).then(|| bounded(value, max));
    ContextTarget {
        kind: target_kind(kind),
        editable: read_bool(|b| t.IsEditable(b)),
        main_frame: read_bool(|b| t.IsRequestedForMainFrame(b)),
        page_url: pwstr(|p| t.PageUri(p)),
        frame_url: pwstr(|p| t.FrameUri(p)),
        link_url: opt(read_bool(|b| t.HasLinkUri(b)), pwstr(|p| t.LinkUri(p)), 8192),
        link_text: opt(read_bool(|b| t.HasLinkText(b)), pwstr(|p| t.LinkText(p)), MAX_LINK_TEXT),
        source_url: opt(read_bool(|b| t.HasSourceUri(b)), pwstr(|p| t.SourceUri(p)), 8192),
        selection: opt(read_bool(|b| t.HasSelection(b)), pwstr(|p| t.SelectionText(p)), MAX_SELECTION),
    }
}

/// Replaces WebView2's menu on `webview`. Runtimes without the API (older than ICoreWebView2_11) keep the default menu.
pub unsafe fn hook(webview: &ICoreWebView2, owner: MenuOwner, ctl: &ControllerHandle) -> windows::core::Result<()> {
    let Ok(wv11) = webview.cast::<ICoreWebView2_11>() else {
        log::warn!("this WebView2 runtime has no ContextMenuRequested; its own menu stays in use");
        return Ok(());
    };
    let ctl = ctl.clone();
    let mut token = 0i64;
    wv11.add_ContextMenuRequested(
        &ContextMenuRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            // Never WebView2's menu: from here on either the shell shows ours or nothing is shown.
            args.SetHandled(true)?;
            let target = args.ContextMenuTarget().map(|t| read_target(&t)).unwrap_or_default();
            if let MenuOwner::Shell(_) = &owner {
                // The shell draws its own menus for tabs, bookmarks and the toolbar; only its text fields and selected
                // text get this one.
                if !target.editable && target.selection.is_none() {
                    return Ok(());
                }
            }
            let mut offered = Vec::new();
            let items = args.MenuItems().map(|c| read_items(&c, 0, &mut offered)).unwrap_or_default();
            let mut pt = POINT::default();
            let _ = args.Location(&mut pt);
            let deferral = args.GetDeferral()?;
            let id = NEXT.fetch_add(1, Ordering::SeqCst);
            PENDING.with(|p| p.borrow_mut().insert(id, Pending { args: args.clone(), deferral, owner: owner.clone(), offered }));
            let req = ContextRequest { id, owner: owner.clone(), target, location: (pt.x, pt.y), items };
            match &owner {
                MenuOwner::Tab(tab) => {
                    let tab = *tab;
                    ctl.post_tab(tab, move |c| c.on_context_menu(req));
                }
                MenuOwner::Shell(label) => ctl.post_window(label.clone(), move |c| c.on_context_menu(req)),
            }
            Ok(())
        })),
        &mut token,
    )
}

/// Closes menu `id`: runs WebView2's command `command` if one was chosen (and was offered), else nothing.
pub fn choose(id: u64, command: Option<i32>) -> bool {
    let Some(p) = PENDING.with(|m| m.borrow_mut().remove(&id)) else { return false };
    let accepted = match command {
        Some(cmd) if p.offered.contains(&cmd) => {
            unsafe {
                let _ = p.args.SetSelectedCommandId(cmd);
            }
            true
        }
        Some(cmd) => {
            log::warn!("context menu {id}: command {cmd} was not offered for this menu; ignored");
            false
        }
        None => true,
    };
    unsafe {
        let _ = p.deferral.Complete();
    }
    accepted
}

/// Completes every open menu of a tab that is going away (or of a window that closed).
pub fn forget(owner: &MenuOwner) {
    let ids: Vec<u64> = PENDING.with(|m| m.borrow().iter().filter(|(_, p)| &p.owner == owner).map(|(id, _)| *id).collect());
    for id in ids {
        choose(id, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mnemonic_markers_are_removed_and_literal_ampersands_kept() {
        assert_eq!(clean_label("&Kopieren"), "Kopieren");
        assert_eq!(clean_label("Save &image as..."), "Save image as...");
        assert_eq!(clean_label("Tom && Jerry"), "Tom & Jerry");
        assert_eq!(clean_label("plain"), "plain");
        assert_eq!(clean_label("trailing&"), "trailing");
    }

    #[test]
    fn long_page_text_is_bounded_by_characters_not_bytes() {
        let text = "ä".repeat(MAX_LINK_TEXT + 10);
        let b = bounded(text, MAX_LINK_TEXT);
        assert_eq!(b.chars().count(), MAX_LINK_TEXT);
        assert_eq!(bounded("short".into(), 10), "short");
    }

    #[test]
    fn target_and_item_kinds_have_stable_names() {
        assert_eq!(target_kind(COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND_IMAGE), "image");
        assert_eq!(target_kind(COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND_SELECTED_TEXT), "selection");
        assert_eq!(target_kind(COREWEBVIEW2_CONTEXT_MENU_TARGET_KIND_PAGE), "page");
        assert_eq!(item_kind(COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SUBMENU), "submenu");
        assert_eq!(item_kind(COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_COMMAND), "command");
    }
}
