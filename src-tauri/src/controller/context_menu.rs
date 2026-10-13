//! Right-click menus: places the menu WebView2 asked for in the window's shell (see `native::context_menu`).

use serde::Serialize;
use tauri::Manager;

use super::Controller;
use crate::native::context_menu::{ContextRequest, ContextTarget, MenuOwner, NativeMenuItem};
use crate::tabs::TabId;

/// A menu for the shell to draw (`pb:contextmenu`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextMenuEvent {
    pub request_id: u64,
    /// The tab whose page was clicked; `None` for the shell's own text fields.
    pub tab_id: Option<TabId>,
    /// Where to open the menu, in the shell's CSS pixels.
    pub x: f64,
    pub y: f64,
    /// The page is in a private tab (no AI, no saving to Project Memory).
    pub private: bool,
    pub target: ContextTarget,
    pub items: Vec<NativeMenuItem>,
    /// The menu window is ready: the shell sends the menu there (drawn over the live page) instead of drawing it itself.
    pub popup: bool,
}

/// Picks the menu position: the mouse pointer when it is inside the clicked area (a right-click), otherwise the point
/// WebView2 reported (the menu key or Shift+F10). `origin` is the clicked area's top-left in CSS pixels, `area` its size.
pub fn menu_position(cursor: Option<(f64, f64)>, reported: (f64, f64), origin: (f64, f64), area: (f64, f64)) -> (f64, f64) {
    if let Some((x, y)) = cursor {
        let inside = x >= origin.0 && y >= origin.1 && x <= origin.0 + area.0 && y <= origin.1 + area.1;
        if inside {
            return (x, y);
        }
    }
    (origin.0 + reported.0, origin.1 + reported.1)
}

impl Controller {
    /// The mouse pointer in the current window's CSS pixels, if it can be read.
    fn cursor_in_window(&self) -> Option<(f64, f64)> {
        let window = self.app.get_window(&self.current)?;
        let cursor = self.app.cursor_position().ok()?;
        let inner = window.inner_position().ok()?;
        let scale = window.scale_factor().ok().filter(|s| *s > 0.0)?;
        Some(((cursor.x - inner.x as f64) / scale, (cursor.y - inner.y as f64) / scale))
    }

    pub fn on_context_menu(&mut self, req: ContextRequest) {
        let scale = self.app.get_window(&self.current).and_then(|w| w.scale_factor().ok()).filter(|s| *s > 0.0).unwrap_or(1.0);
        let reported = (req.location.0 as f64 / scale, req.location.1 as f64 / scale);
        let (tab_id, origin, area) = match &req.owner {
            MenuOwner::Tab(id) => {
                let Some(rect) = self.layout.content else {
                    self.close_context_menu(req.id);
                    return;
                };
                (Some(*id), (rect.x, rect.y), (rect.width, rect.height))
            }
            MenuOwner::Shell(_) => (None, (0.0, 0.0), (self.layout.viewport.width, self.layout.viewport.height)),
        };
        if let Some(id) = tab_id {
            // Only the page the user is looking at can ask for a menu.
            if self.tabs.active_id() != Some(id) {
                self.close_context_menu(req.id);
                return;
            }
        }
        // A menu that is still open in this window is superseded by the new one.
        if let Some(previous) = self.layout.context_menu.replace(req.id) {
            self.close_context_menu(previous);
        }
        let (x, y) = menu_position(self.cursor_in_window(), reported, origin, area);
        log::info!(
            "context menu {}: {} target{}{}, {} WebView2 commands offered",
            req.id,
            req.target.kind,
            if req.target.editable { " (editable)" } else { "" },
            if req.target.link_url.is_some() { " (link)" } else { "" },
            req.items.len()
        );
        let private = tab_id.and_then(|id| self.tabs.get(id)).map(|t| t.private).unwrap_or(self.private_window);
        let popup = crate::menu_surface::is_ready() && self.set_menu_anchor(x, y);
        self.emit("pb:contextmenu", ContextMenuEvent { request_id: req.id, tab_id, x, y, private, target: req.target, items: req.items, popup });
    }

    /// Records where the menu window should open: point `(x, y)` of this window (CSS pixels) on the screen.
    fn set_menu_anchor(&self, x: f64, y: f64) -> bool {
        let Some(window) = self.app.get_window(&self.current) else { return false };
        let (Ok(inner), Ok(scale)) = (window.inner_position(), window.scale_factor()) else { return false };
        crate::menu_surface::set_anchor(crate::menu_surface::Anchor {
            owner: self.current.clone(),
            x: inner.x + (x * scale).round() as i32,
            y: inner.y + (y * scale).round() as i32,
        });
        true
    }

    /// Completes a menu without running a WebView2 command (dismissed, superseded or not for this window).
    pub(crate) fn close_context_menu(&mut self, id: u64) {
        if self.layout.context_menu == Some(id) {
            self.layout.context_menu = None;
        }
        let _ = self.app.run_on_main_thread(move || {
            crate::native::context_menu::choose(id, None);
        });
    }

    /// The shell closed menu `id`, with one of WebView2's commands chosen or none.
    pub fn context_menu_done(&mut self, id: u64, command: Option<i32>) {
        log::info!("context menu {id}: closed{}", if command.is_some() { " with a WebView2 command" } else { "" });
        if self.layout.context_menu == Some(id) {
            self.layout.context_menu = None;
        }
        if command.is_some() && !self.layout.overlay {
            // Menu window path: the page never lost its place, but it lost keyboard focus to the menu. Commands such
            // as Paste act on the focused field, so the page gets focus back first.
            if let Some(v) = self.tabs.active_id().and_then(|a| self.views.get(&a)).filter(|v| v.visible) {
                let _ = v.webview.set_focus();
            }
        }
        let _ = self.app.run_on_main_thread(move || {
            crate::native::context_menu::choose(id, command);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_right_click_opens_at_the_pointer_and_the_menu_key_at_the_reported_point() {
        // Page area at (54, 118), 1000x600. Pointer inside it: use the pointer.
        assert_eq!(menu_position(Some((300.0, 400.0)), (5.0, 5.0), (54.0, 118.0), (1000.0, 600.0)), (300.0, 400.0));
        // Pointer outside (menu key pressed while the mouse rests on the toolbar): the reported point, offset by the area.
        assert_eq!(menu_position(Some((10.0, 10.0)), (100.0, 50.0), (54.0, 118.0), (1000.0, 600.0)), (154.0, 168.0));
        // No pointer reading at all.
        assert_eq!(menu_position(None, (0.0, 0.0), (54.0, 118.0), (1000.0, 600.0)), (54.0, 118.0));
    }
}
