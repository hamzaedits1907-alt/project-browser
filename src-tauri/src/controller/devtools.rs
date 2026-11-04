//! DevTools docked in the browser window, below or beside the page (how the DevTools window is adopted is in
//! `native/devtools.rs`).
//!
//! The controller owns the layout: when the active tab has docked DevTools, the page area the shell reported is
//! split into the page, a thin bar the shell draws (title, dock buttons, close; dragging it resizes), and the
//! DevTools window. Each tab keeps its own DevTools, shown only while that tab is active, as in Chrome. While a
//! shell popup covers the page, or a video is in fullscreen, DevTools is hidden with the page.
//!
//! An open DevTools window costs memory only while it exists: closing it (the bar's ×, F12 again) destroys it.

use serde::Serialize;

use super::{ContentRect, Controller};
use crate::settings::DevtoolsDock;
use crate::tabs::TabId;

/// Height of the bar the shell draws above docked DevTools.
pub const BAR: f64 = 30.0;
/// Width of the grip between the page and DevTools docked on the right.
pub const GRIP: f64 = 6.0;
/// Neither the page nor DevTools gets smaller than this while there is room for both.
const MIN_SIDE: f64 = 140.0;

/// What the shell draws for docked DevTools (window CSS pixels).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevtoolsView {
    pub tab_id: TabId,
    pub side: DevtoolsDock,
    pub size: u32,
    /// The bar above DevTools.
    pub bar: ContentRect,
    /// The strip between the page and DevTools that resizes them (the bar itself when docked at the bottom).
    pub grip: ContentRect,
    /// Where the DevTools window is.
    pub tools: ContentRect,
}

/// The page area split for docked DevTools: (page, bar, grip, DevTools window).
pub fn dock_split(area: ContentRect, side: DevtoolsDock, percent: u32) -> (ContentRect, ContentRect, ContentRect, ContentRect) {
    let share = f64::from(percent.clamp(15, 85)) / 100.0;
    match side {
        DevtoolsDock::Right => {
            let room = (area.width - GRIP).max(0.0);
            let mut tools_w = (room * share).round();
            if room >= 2.0 * MIN_SIDE {
                tools_w = tools_w.clamp(MIN_SIDE, room - MIN_SIDE);
            }
            let page_w = (room - tools_w).max(0.0);
            let page = ContentRect { x: area.x, y: area.y, width: page_w, height: area.height };
            let grip = ContentRect { x: area.x + page_w, y: area.y, width: GRIP, height: area.height };
            let x = grip.x + GRIP;
            let bar = ContentRect { x, y: area.y, width: tools_w, height: BAR.min(area.height) };
            let tools = ContentRect { x, y: area.y + bar.height, width: tools_w, height: (area.height - bar.height).max(0.0) };
            (page, bar, grip, tools)
        }
        _ => {
            let room = (area.height - BAR).max(0.0);
            let mut tools_h = (room * share).round();
            if room >= 2.0 * MIN_SIDE {
                tools_h = tools_h.clamp(MIN_SIDE, room - MIN_SIDE);
            }
            let page_h = (room - tools_h).max(0.0);
            let page = ContentRect { x: area.x, y: area.y, width: area.width, height: page_h };
            let bar = ContentRect { x: area.x, y: area.y + page_h, width: area.width, height: BAR.min(area.height) };
            let tools = ContentRect { x: area.x, y: bar.y + bar.height, width: area.width, height: tools_h };
            (page, bar, bar, tools)
        }
    }
}

/// x, y, width, height in physical pixels of a window's client area.
pub type PhysicalRect = (i32, i32, i32, i32);

/// `rect` in physical pixels of the window's client area.
fn physical(rect: ContentRect, scale: f64) -> PhysicalRect {
    let s = if scale > 0.0 { scale } else { 1.0 };
    let x = (rect.x * s).round() as i32;
    let y = (rect.y * s).round() as i32;
    let r = ((rect.x + rect.width) * s).round() as i32;
    let b = ((rect.y + rect.height) * s).round() as i32;
    (x, y, (r - x).max(0), (b - y).max(0))
}

/// The percentage `pointer` (CSS pixels, along the dock axis) asks for when the user drags the bar or grip.
pub fn size_for_pointer(area: ContentRect, side: DevtoolsDock, pointer: f64) -> u32 {
    let pct = match side {
        DevtoolsDock::Right => {
            let room = (area.width - GRIP).max(1.0);
            (area.x + area.width - pointer - GRIP / 2.0) / room
        }
        _ => {
            let room = (area.height - BAR).max(1.0);
            (area.y + area.height - pointer - BAR / 2.0) / room
        }
    };
    ((pct * 100.0).round().clamp(15.0, 85.0)) as u32
}

impl Controller {
    /// The active tab's docked DevTools window, if it has one.
    fn active_devtools(&self) -> Option<(TabId, isize)> {
        let id = self.tabs.active_id()?;
        self.layout.devtools.get(&id).map(|h| (id, *h))
    }

    /// The docked DevTools the shell must draw a bar for.
    pub(crate) fn devtools_view(&self) -> Option<DevtoolsView> {
        let (tab_id, _) = self.active_devtools()?;
        let area = self.layout.content?;
        if self.layout.element_fullscreen.is_some() || self.settings.devtools_dock == DevtoolsDock::Window {
            return None;
        }
        let side = self.settings.devtools_dock;
        let (_, bar, grip, tools) = dock_split(area, side, self.settings.devtools_size);
        Some(DevtoolsView { tab_id, side, size: self.settings.devtools_size, bar, grip, tools })
    }

    /// Called by `relayout`: shrinks `area` for the active tab's DevTools and places every DevTools window of this
    /// window (the active tab's shown, the others hidden). Returns the area left for the page.
    pub(crate) fn layout_devtools(&mut self, area: ContentRect) -> ContentRect {
        // Forget windows that are gone, and DevTools of tabs that left this window (their window was a child of it).
        let tabs: Vec<TabId> = self.layout.devtools.keys().copied().collect();
        for tab in tabs {
            let hwnd = self.layout.devtools[&tab];
            let here = self.tabs.get(tab).is_some();
            if !here || !crate::native::devtools::is_alive(hwnd) {
                self.layout.devtools.remove(&tab);
                self.layout.devtools_placed.remove(&hwnd);
                if here {
                    continue;
                }
                let _ = self.app.run_on_main_thread(move || crate::native::devtools::close(hwnd));
            }
        }
        if self.layout.devtools.is_empty() {
            return area;
        }
        let view = self.devtools_view();
        let hidden = self.layout.overlay || area.width <= 0.0 || area.height <= 0.0;
        let scale = self.layout.viewport.scale;
        let mut page = area;
        let mut targets: Vec<(isize, Option<PhysicalRect>)> = Vec::new();
        for (tab, hwnd) in &self.layout.devtools {
            let target = match view {
                Some(v) if v.tab_id == *tab => {
                    let (p, _, _, tools) = dock_split(area, v.side, v.size);
                    page = p;
                    (!hidden).then(|| physical(tools, scale))
                }
                _ => None,
            };
            targets.push((*hwnd, target));
        }
        for (hwnd, target) in targets {
            if self.layout.devtools_placed.get(&hwnd) == Some(&target) {
                continue;
            }
            self.layout.devtools_placed.insert(hwnd, target);
            let _ = self.app.run_on_main_thread(move || crate::native::devtools::place(hwnd, target));
        }
        page
    }

    /// F12: opens DevTools for the active tab, or closes it when it is already open.
    pub fn toggle_devtools(&mut self) {
        let Some(id) = self.active_tab_with_view() else {
            self.toast("Developer tools work on web pages.");
            return;
        };
        if let Some(hwnd) = self.layout.devtools.get(&id).copied() {
            self.close_devtools_window(id, hwnd);
            return;
        }
        let dock = self.settings.devtools_dock != DevtoolsDock::Window;
        let ctl = self.handle.clone();
        let _ = self.app.run_on_main_thread(move || crate::native::devtools::open(id, dock, ctl));
    }

    fn close_devtools_window(&mut self, id: TabId, hwnd: isize) {
        self.layout.devtools.remove(&id);
        self.layout.devtools_placed.remove(&hwnd);
        let _ = self.app.run_on_main_thread(move || crate::native::devtools::close(hwnd));
        self.relayout();
        self.mark_state();
    }

    /// The native side adopted tab `id`'s DevTools window.
    pub fn on_devtools_docked(&mut self, id: TabId, hwnd: isize) {
        if self.tabs.get(id).is_none() {
            let _ = self.app.run_on_main_thread(move || crate::native::devtools::close(hwnd));
            return;
        }
        if let Some(old) = self.layout.devtools.insert(id, hwnd).filter(|old| *old != hwnd) {
            let _ = self.app.run_on_main_thread(move || crate::native::devtools::close(old));
        }
        self.relayout();
        self.mark_state();
    }

    /// Tab `id`'s docked DevTools window was destroyed.
    pub fn on_devtools_closed(&mut self, id: TabId, hwnd: isize) {
        if self.layout.devtools.get(&id) == Some(&hwnd) {
            self.layout.devtools.remove(&id);
            self.layout.devtools_placed.remove(&hwnd);
            self.relayout();
            self.mark_state();
        }
    }

    /// The DevTools bar's buttons and drag.
    pub fn devtools_action(&mut self, action: DevtoolsAction) -> crate::error::Result<()> {
        match action {
            DevtoolsAction::Close => {
                if let Some((id, hwnd)) = self.active_devtools() {
                    self.close_devtools_window(id, hwnd);
                }
            }
            DevtoolsAction::Dock { side } => {
                let mut next = self.settings.clone();
                next.devtools_dock = side;
                self.update_settings(next)?;
                if side == DevtoolsDock::Window {
                    self.undock_active_devtools();
                }
            }
            DevtoolsAction::Resize { pointer } => {
                let Some(area) = self.layout.content else { return Ok(()) };
                let size = size_for_pointer(area, self.settings.devtools_dock, pointer);
                if size != self.settings.devtools_size {
                    // Kept in memory while dragging; saved with the next settings write (or at the end of the drag).
                    self.settings.devtools_size = size;
                    self.relayout();
                    self.mark_state();
                }
            }
            DevtoolsAction::ResizeEnd => {
                let next = self.settings.clone();
                self.update_settings(next)?;
            }
        }
        Ok(())
    }

    /// Gives the active tab's docked DevTools its own window again, over the right half of the browser window.
    fn undock_active_devtools(&mut self) {
        let Some((id, hwnd)) = self.active_devtools() else { return };
        self.layout.devtools.remove(&id);
        self.layout.devtools_placed.remove(&hwnd);
        let v = self.layout.viewport;
        let s = if v.scale > 0.0 { v.scale } else { 1.0 };
        let (w, h) = ((v.width * 0.5).max(480.0), v.height.max(420.0));
        let screen = (((v.screen_x + v.width - w) * s) as i32, (v.screen_y * s) as i32, (w * s) as i32, (h * s) as i32);
        let _ = self.app.run_on_main_thread(move || crate::native::devtools::undock(hwnd, screen));
        self.relayout();
        self.mark_state();
    }
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum DevtoolsAction {
    Close,
    Dock {
        side: DevtoolsDock,
    },
    /// The bar or grip is being dragged; `pointer` is the pointer along the dock axis (window CSS pixels).
    Resize {
        pointer: f64,
    },
    ResizeEnd,
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: ContentRect = ContentRect { x: 0.0, y: 80.0, width: 1200.0, height: 700.0 };

    #[test]
    fn docked_at_the_bottom_the_page_bar_and_tools_stack_without_gaps() {
        let (page, bar, grip, tools) = dock_split(AREA, DevtoolsDock::Bottom, 40);
        assert_eq!(page.y, AREA.y);
        assert_eq!(bar.y, page.y + page.height);
        assert_eq!(tools.y, bar.y + BAR);
        assert_eq!(tools.y + tools.height, AREA.y + AREA.height);
        assert_eq!(grip, bar);
        assert_eq!(tools.height, ((700.0 - BAR) * 0.4_f64).round());
        assert_eq!(page.width, AREA.width);
    }

    #[test]
    fn docked_on_the_right_the_columns_fill_the_width() {
        let (page, bar, grip, tools) = dock_split(AREA, DevtoolsDock::Right, 30);
        assert_eq!(page.x, 0.0);
        assert_eq!(grip.x, page.width);
        assert_eq!(bar.x, grip.x + GRIP);
        assert_eq!(tools.x, bar.x);
        assert_eq!(tools.x + tools.width, AREA.width);
        assert_eq!(tools.y, AREA.y + BAR);
        assert_eq!(page.height, AREA.height);
    }

    #[test]
    fn neither_side_collapses_while_there_is_room() {
        let (page, _, _, tools) = dock_split(AREA, DevtoolsDock::Bottom, 85);
        assert!(page.height >= MIN_SIDE);
        assert!(tools.height >= MIN_SIDE);
        let tiny = ContentRect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 };
        let (page, _, _, tools) = dock_split(tiny, DevtoolsDock::Right, 50);
        assert!(page.width >= 0.0 && tools.width >= 0.0);
    }

    #[test]
    fn dragging_the_bar_maps_back_to_the_same_size() {
        let (_, bar, _, _) = dock_split(AREA, DevtoolsDock::Bottom, 40);
        assert_eq!(size_for_pointer(AREA, DevtoolsDock::Bottom, bar.y + BAR / 2.0), 40);
        let (_, _, grip, _) = dock_split(AREA, DevtoolsDock::Right, 35);
        assert_eq!(size_for_pointer(AREA, DevtoolsDock::Right, grip.x + GRIP / 2.0), 35);
        assert_eq!(size_for_pointer(AREA, DevtoolsDock::Bottom, -500.0), 85);
        assert_eq!(size_for_pointer(AREA, DevtoolsDock::Bottom, 5000.0), 15);
    }

    #[test]
    fn physical_rects_round_edges_not_sizes() {
        assert_eq!(physical(ContentRect { x: 0.5, y: 10.0, width: 100.0, height: 50.0 }, 1.5), (1, 15, 150, 75));
    }
}
