//! Browser commands and their keyboard shortcuts.
//!
//! Keys are intercepted natively through WebView2's `AcceleratorKeyPressed`
//! event in both the shell and every tab, so this table is the single source
//! of truth regardless of which webview has focus.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "value")]
pub enum Command {
    NewTab,
    NewPrivateTab,
    NewWindow,
    NewPrivateWindow,
    CloseWindow,
    ReopenWindow,
    Quit,
    CloseTab,
    ReopenTab,
    NextTab,
    PrevTab,
    SelectTab(u8),
    FocusOmnibox,
    Reload,
    HardReload,
    Back,
    Forward,
    Stop,
    Find,
    FindNext,
    FindPrev,
    BookmarkPage,
    ToggleBookmarksBar,
    OpenHistory,
    OpenDownloads,
    OpenBookmarks,
    OpenSettings,
    OpenPerformance,
    OpenGaming,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Fullscreen,
    ToggleGamingMode,
    /// Command Center: one search box for tabs, history, bookmarks and browser commands.
    CommandPalette,
    Screenshot,
    ReaderMode,
    ToggleSplit,
    ShowShortcuts,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

// Windows virtual-key codes.
const VK_TAB: u32 = 0x09;
const VK_ESCAPE: u32 = 0x1B;
const VK_PRIOR: u32 = 0x21;
const VK_NEXT: u32 = 0x22;
const VK_LEFT: u32 = 0x25;
const VK_RIGHT: u32 = 0x27;
const VK_NUMPAD0: u32 = 0x60;
const VK_ADD: u32 = 0x6B;
const VK_SUBTRACT: u32 = 0x6D;
const VK_F3: u32 = 0x72;
const VK_F4: u32 = 0x73;
const VK_F1: u32 = 0x70;
const VK_F5: u32 = 0x74;
const VK_F6: u32 = 0x75;
const VK_F9: u32 = 0x78;
const VK_F11: u32 = 0x7A;
const VK_OEM_PLUS: u32 = 0xBB;
const VK_OEM_COMMA: u32 = 0xBC;
const VK_OEM_MINUS: u32 = 0xBD;

const fn key(c: u8) -> u32 {
    c as u32
}

/// Maps a key-down to a browser command. Keys not listed here pass through
/// to the page (Ctrl+C/V, Ctrl+P print, F12 devtools, plain Esc, ...).
pub fn map_key(vk: u32, m: Mods) -> Option<Command> {
    use Command::*;
    let Mods { ctrl, shift, alt } = m;
    match (ctrl, shift, alt, vk) {
        (true, false, false, k) if k == key(b'T') => Some(NewTab),
        (true, true, false, k) if k == key(b'T') => Some(ReopenTab),
        (true, false, false, k) if k == key(b'N') => Some(NewWindow),
        (true, true, false, k) if k == key(b'N') => Some(NewPrivateWindow),
        (true, true, false, k) if k == key(b'W') => Some(CloseWindow),
        (true, false, false, k) if k == key(b'W') || k == VK_F4 => Some(CloseTab),
        (true, false, false, VK_TAB) | (true, false, false, VK_NEXT) => Some(NextTab),
        (true, true, false, VK_TAB) | (true, false, false, VK_PRIOR) => Some(PrevTab),
        (true, false, false, k) if (key(b'1')..=key(b'9')).contains(&k) => Some(SelectTab((k - key(b'0')) as u8)),
        (true, false, false, k) if k == key(b'K') => Some(CommandPalette),
        (true, true, false, k) if k == key(b'A') => Some(CommandPalette),
        (true, false, false, k) if k == key(b'L') || k == key(b'E') => Some(FocusOmnibox),
        (true, true, false, k) if k == key(b'S') => Some(Screenshot),
        (false, false, false, VK_F9) => Some(ReaderMode),
        (true, true, false, k) if k == key(b'E') => Some(ToggleSplit),
        (false, false, false, VK_F1) => Some(ShowShortcuts),
        (false, false, true, k) if k == key(b'D') => Some(FocusOmnibox),
        (false, false, false, VK_F6) => Some(FocusOmnibox),
        (true, false, false, k) if k == key(b'R') => Some(Reload),
        (false, false, false, VK_F5) => Some(Reload),
        (true, true, false, k) if k == key(b'R') => Some(HardReload),
        (true, false, false, VK_F5) | (false, true, false, VK_F5) => Some(HardReload),
        (false, false, true, VK_LEFT) => Some(Back),
        (false, false, true, VK_RIGHT) => Some(Forward),
        (true, false, false, k) if k == key(b'F') => Some(Find),
        (false, false, false, VK_F3) => Some(FindNext),
        (true, false, false, k) if k == key(b'G') => Some(FindNext),
        (false, true, false, VK_F3) => Some(FindPrev),
        (true, false, false, k) if k == key(b'D') => Some(BookmarkPage),
        (true, true, false, k) if k == key(b'B') => Some(ToggleBookmarksBar),
        (true, false, false, k) if k == key(b'H') => Some(OpenHistory),
        (true, false, false, k) if k == key(b'J') => Some(OpenDownloads),
        (true, true, false, k) if k == key(b'O') => Some(OpenBookmarks),
        (true, false, false, VK_OEM_COMMA) => Some(OpenSettings),
        (false, true, false, VK_ESCAPE) => Some(OpenPerformance),
        (true, true, false, k) if k == key(b'G') => Some(ToggleGamingMode),
        (true, true, false, k) if k == key(b'D') => Some(OpenGaming),
        (true, _, false, VK_OEM_PLUS) | (true, _, false, VK_ADD) => Some(ZoomIn),
        (true, _, false, VK_OEM_MINUS) | (true, _, false, VK_SUBTRACT) => Some(ZoomOut),
        (true, false, false, k) if k == key(b'0') || k == VK_NUMPAD0 => Some(ZoomReset),
        (false, false, false, VK_F11) => Some(Fullscreen),
        _ => None,
    }
}

/// Chrome's zoom ladder.
pub const ZOOM_STEPS: &[f64] = &[0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 5.0];

pub fn zoom_step(current: f64, up: bool) -> f64 {
    if up {
        ZOOM_STEPS.iter().copied().find(|z| *z > current + 0.001).unwrap_or(5.0)
    } else {
        ZOOM_STEPS.iter().rev().copied().find(|z| *z < current - 0.001).unwrap_or(0.25)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: Mods = Mods { ctrl: true, shift: false, alt: false };
    const CS: Mods = Mods { ctrl: true, shift: true, alt: false };
    const A: Mods = Mods { ctrl: false, shift: false, alt: true };
    const NONE: Mods = Mods { ctrl: false, shift: false, alt: false };
    const S: Mods = Mods { ctrl: false, shift: true, alt: false };

    #[test]
    fn tab_shortcuts() {
        assert_eq!(map_key(key(b'T'), C), Some(Command::NewTab));
        assert_eq!(map_key(key(b'T'), CS), Some(Command::ReopenTab));
        assert_eq!(map_key(key(b'W'), C), Some(Command::CloseTab));
        assert_eq!(map_key(VK_TAB, C), Some(Command::NextTab));
        assert_eq!(map_key(VK_TAB, CS), Some(Command::PrevTab));
        assert_eq!(map_key(key(b'3'), C), Some(Command::SelectTab(3)));
        assert_eq!(map_key(key(b'9'), C), Some(Command::SelectTab(9)));
        assert_eq!(map_key(key(b'N'), CS), Some(Command::NewPrivateWindow));
    }

    #[test]
    fn feature_shortcuts() {
        assert_eq!(map_key(key(b'K'), C), Some(Command::CommandPalette));
        assert_eq!(map_key(key(b'A'), CS), Some(Command::CommandPalette));
        assert_eq!(map_key(key(b'S'), CS), Some(Command::Screenshot));
        assert_eq!(map_key(VK_F9, NONE), Some(Command::ReaderMode));
        assert_eq!(map_key(key(b'E'), CS), Some(Command::ToggleSplit));
        assert_eq!(map_key(VK_F1, NONE), Some(Command::ShowShortcuts));
        assert_eq!(map_key(key(b'E'), C), Some(Command::FocusOmnibox));
    }

    #[test]
    fn window_shortcuts() {
        assert_eq!(map_key(key(b'N'), C), Some(Command::NewWindow));
        assert_eq!(map_key(key(b'N'), CS), Some(Command::NewPrivateWindow));
        assert_eq!(map_key(key(b'W'), CS), Some(Command::CloseWindow));
        assert_eq!(map_key(key(b'W'), C), Some(Command::CloseTab), "Ctrl+W still closes only the tab");
    }

    #[test]
    fn navigation_shortcuts() {
        assert_eq!(map_key(VK_LEFT, A), Some(Command::Back));
        assert_eq!(map_key(VK_RIGHT, A), Some(Command::Forward));
        assert_eq!(map_key(VK_F5, NONE), Some(Command::Reload));
        assert_eq!(map_key(VK_F5, C), Some(Command::HardReload));
        assert_eq!(map_key(key(b'R'), CS), Some(Command::HardReload));
        assert_eq!(map_key(key(b'L'), C), Some(Command::FocusOmnibox));
        assert_eq!(map_key(key(b'D'), A), Some(Command::FocusOmnibox));
    }

    #[test]
    fn find_and_pages() {
        assert_eq!(map_key(key(b'F'), C), Some(Command::Find));
        assert_eq!(map_key(VK_F3, NONE), Some(Command::FindNext));
        assert_eq!(map_key(VK_F3, S), Some(Command::FindPrev));
        assert_eq!(map_key(key(b'G'), C), Some(Command::FindNext));
        assert_eq!(map_key(key(b'G'), CS), Some(Command::ToggleGamingMode));
        assert_eq!(map_key(key(b'H'), C), Some(Command::OpenHistory));
        assert_eq!(map_key(VK_ESCAPE, S), Some(Command::OpenPerformance));
    }

    #[test]
    fn editing_and_page_keys_pass_through() {
        for c in *b"CVXAZPSU" {
            assert_eq!(map_key(key(c), C), None, "Ctrl+{}", c as char);
        }
        assert_eq!(map_key(VK_ESCAPE, NONE), None, "plain Esc belongs to the page");
        assert_eq!(map_key(0x7B, NONE), None, "F12 opens devtools natively");
    }

    #[test]
    fn zoom_ladder() {
        assert_eq!(map_key(VK_OEM_PLUS, C), Some(Command::ZoomIn));
        assert_eq!(map_key(VK_OEM_PLUS, CS), Some(Command::ZoomIn));
        assert_eq!(map_key(key(b'0'), C), Some(Command::ZoomReset));
        assert_eq!(zoom_step(1.0, true), 1.1);
        assert_eq!(zoom_step(1.0, false), 0.9);
        assert_eq!(zoom_step(1.05, true), 1.1);
        assert_eq!(zoom_step(5.0, true), 5.0);
        assert_eq!(zoom_step(0.25, false), 0.25);
    }
}
