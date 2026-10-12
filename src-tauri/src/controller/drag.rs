//! Dragging a tab out of its window: into another browser window, or out into a new one.
//!
//! The shell that owns the dragged tab keeps the mouse captured for the whole drag (pointer capture keeps reporting
//! moves outside the window), so it always knows when the drag starts, leaves its tab strip and ends. While the tab
//! is outside its own strip the shell asks the core where the pointer is. The core finds the browser window under
//! the pointer with `WindowFromPoint` (so the topmost window wins where windows overlap, and another program's window
//! on top hides ours) and tells that window's shell, which draws where the tab would land (`pb:tabdrag`). On release
//! the target window's shell decides the position and moves the tab with `tab_to_window`; with no browser window
//! under the pointer the tab becomes a new window at the pointer. The page keeps running throughout: its native
//! webview is re-parented, never recreated.

use serde::Serialize;
use tauri::{Emitter, Manager};

use super::window_set::WindowSpec;
use super::{Controller, WindowGeometry};
use crate::error::{Error, Result};
use crate::tabs::TabId;

/// `pb:tabdrag`: a tab from another window is over this window ("over"), has left it ("leave"), or was dropped on it
/// ("drop"). Coordinates are the pointer in this window's CSS pixels.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabDragEvent {
    pub phase: &'static str,
    pub tab_id: TabId,
    pub x: f64,
    pub y: f64,
}

/// What became of a dragged tab, for the shell that started the drag.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum DragOutcome {
    /// Released over another window, whose shell places it.
    Window {
        label: String,
    },
    /// Became a new window at the pointer.
    NewWindow {
        label: String,
    },
    /// The only tab of its window was dropped in empty space: the window moved there instead.
    MovedWindow,
    Cancelled,
}

/// Where a window dragged out by a tab goes: its tab strip under the pointer (the pointer is in logical pixels).
pub fn new_window_origin(pointer: (f64, f64), size: (f64, f64)) -> (f64, f64) {
    let x = pointer.0 - (size.0 * 0.12).min(140.0);
    let y = pointer.1 - 18.0;
    (x, y.max(0.0))
}

impl Controller {
    /// The browser window under the mouse pointer and the pointer's position in it (CSS pixels).
    fn window_at_pointer(&self) -> Option<(String, f64, f64)> {
        let cursor = self.app.cursor_position().ok()?;
        let hwnd = crate::native::top_level_window_at(cursor.x.round() as i32, cursor.y.round() as i32)?;
        for label in &self.window_order {
            let Some(window) = self.app.get_window(label) else { continue };
            let Ok(handle) = window.hwnd() else { continue };
            if handle.0 as isize != hwnd {
                continue;
            }
            let inner = window.inner_position().ok()?;
            let scale = window.scale_factor().ok().filter(|s| *s > 0.0)?;
            return Some((label.clone(), (cursor.x - inner.x as f64) / scale, (cursor.y - inner.y as f64) / scale));
        }
        None
    }

    fn emit_drag(&self, label: &str, phase: &'static str, tab_id: TabId, x: f64, y: f64) {
        if let Err(e) = self.app.emit_to(label, "pb:tabdrag", TabDragEvent { phase, tab_id, x, y }) {
            log::warn!("tab drag event to {label} failed: {e}");
        }
    }

    /// The dragged tab moved outside its strip. Returns the window it is over (not counting its own).
    pub fn tab_drag_hover(&mut self, source: &str, tab_id: TabId) -> Option<String> {
        let over = self.window_at_pointer().filter(|(label, _, _)| label != source);
        let previous = self.drag_hover.take();
        if let Some(prev) = previous.as_ref().filter(|p| over.as_ref().map(|o| &o.0) != Some(*p)) {
            self.emit_drag(prev, "leave", tab_id, 0.0, 0.0);
        }
        if let Some((label, x, y)) = &over {
            self.emit_drag(label, "over", tab_id, *x, *y);
            self.drag_hover = Some(label.clone());
        }
        over.map(|o| o.0)
    }

    /// The drag ended. `detached`: the tab was pulled out of its strip (otherwise the shell already reordered it).
    pub fn tab_drag_end(&mut self, source: &str, tab_id: TabId, detached: bool, cancel: bool) -> Result<DragOutcome> {
        if let Some(prev) = self.drag_hover.take() {
            if cancel || !detached {
                self.emit_drag(&prev, "leave", tab_id, 0.0, 0.0);
            }
        }
        if cancel || !detached {
            return Ok(DragOutcome::Cancelled);
        }
        if !self.select_tab_window(tab_id) {
            return Err(Error::msg("That tab is no longer open."));
        }
        if let Some((label, x, y)) = self.window_at_pointer().filter(|(label, _, _)| label != source) {
            // The window under the pointer places the tab where its strip shows the drop marker.
            self.emit_drag(&label, "drop", tab_id, x, y);
            return Ok(DragOutcome::Window { label });
        }
        let cursor = self.app.cursor_position().map_err(|e| Error::msg(e.to_string()))?;
        let scale = self.app.monitor_from_point(cursor.x, cursor.y).ok().flatten().map(|m| m.scale_factor()).unwrap_or(1.0);
        let pointer = (cursor.x / scale, cursor.y / scale);
        let v = self.layout.viewport;
        let size = if v.width >= 640.0 && v.height >= 420.0 { (v.width, v.height) } else { (1280.0, 800.0) };
        let (x, y) = new_window_origin(pointer, size);
        if self.tabs.len() <= 1 {
            // Pulling out the only tab moves its window, as other browsers do.
            if let Some(w) = self.app.get_window(&self.current) {
                if self.layout.viewport.maximized {
                    let _ = w.unmaximize();
                }
                let _ = w.set_position(tauri::LogicalPosition::new(x, y));
                let _ = w.set_focus();
            }
            return Ok(DragOutcome::MovedWindow);
        }
        let geometry = WindowGeometry { x: Some(x), y: Some(y), width: size.0, height: size.1, maximized: false };
        let label = self.move_tab_to_window_at(tab_id, None, None, Some(geometry))?;
        if let Some(w) = self.app.get_window(&label) {
            let _ = w.set_focus();
        }
        Ok(DragOutcome::NewWindow { label })
    }

    /// Opens `url` (already resolved by the caller) in a new browser window.
    pub fn open_in_new_window(&mut self, url: Option<String>, private: bool) -> Option<String> {
        let tabs = url.map(|u| vec![super::window_set::session_tab(u)]).unwrap_or_default();
        self.open_window(WindowSpec { tabs, active: 0, geometry: None, private, empty: false, groups: Vec::new() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_torn_off_window_puts_its_tab_strip_under_the_pointer() {
        assert_eq!(new_window_origin((800.0, 400.0), (1280.0, 800.0)), (800.0 - 140.0, 382.0));
        // Small windows keep the pointer proportionally closer to their left edge.
        assert_eq!(new_window_origin((800.0, 400.0), (700.0, 500.0)), (800.0 - 84.0, 382.0));
        // Never above the top of the screen.
        assert_eq!(new_window_origin((50.0, 5.0), (1280.0, 800.0)).1, 0.0);
    }

    #[test]
    fn outcomes_serialise_with_a_kind_tag() {
        let json = serde_json::to_string(&DragOutcome::Window { label: "win-2".into() }).unwrap();
        assert_eq!(json, r#"{"kind":"window","label":"win-2"}"#);
        assert_eq!(serde_json::to_string(&DragOutcome::Cancelled).unwrap(), r#"{"kind":"cancelled"}"#);
    }
}
