//! Split view: two web tabs of one window shown side by side.
//!
//! A window has at most one split pair. The pair is shown whenever one of its two tabs is the active tab; activating
//! any other tab shows that tab alone, and activating either half again brings the pair back (as in Edge). Clicking
//! into one half makes it the active tab (`on_tab_focused`), so the toolbar, address bar and shortcuts follow the
//! half the user is working in. Both halves are web pages: browser:// pages are drawn by the shell and cannot share
//! the page area, so the pair ends when either half closes, moves to another window or becomes a browser page.
//! Split pairs are not saved with the session.

use serde::{Deserialize, Serialize};

use super::{ContentRect, Controller};
use crate::error::{Error, Result};
use crate::navigation::is_internal;
use crate::tabs::TabId;

/// Space between the two halves, in CSS pixels. The shell draws the divider handle in it.
pub const GAP: f64 = 6.0;
pub const MIN_RATIO: f64 = 0.2;
pub const MAX_RATIO: f64 = 0.8;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitView {
    pub left: TabId,
    pub right: TabId,
    /// Share of the page area's width taken by the left half.
    pub ratio: f64,
}

impl SplitView {
    pub fn contains(&self, id: TabId) -> bool {
        self.left == id || self.right == id
    }

    pub fn partner(&self, id: TabId) -> Option<TabId> {
        if self.left == id {
            Some(self.right)
        } else if self.right == id {
            Some(self.left)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "op")]
pub enum SplitOp {
    /// Splits `tab_id` (default: the active tab) with `with` (default: the nearest other web tab).
    Open {
        tab_id: Option<TabId>,
        with: Option<TabId>,
    },
    Close,
    Swap,
    Ratio {
        ratio: f64,
    },
}

pub fn clamp_ratio(ratio: f64) -> f64 {
    if ratio.is_finite() {
        ratio.clamp(MIN_RATIO, MAX_RATIO)
    } else {
        0.5
    }
}

/// The two halves of the page area.
pub fn split_rects(rect: ContentRect, ratio: f64) -> (ContentRect, ContentRect) {
    let usable = (rect.width - GAP).max(0.0);
    let left_w = (usable * clamp_ratio(ratio)).round();
    let right_w = (usable - left_w).max(0.0);
    let left = ContentRect { x: rect.x, y: rect.y, width: left_w, height: rect.height };
    let right = ContentRect { x: rect.x + left_w + GAP, y: rect.y, width: right_w, height: rect.height };
    (left, right)
}

/// Where each tab on screen goes: the pair when the active tab is half of it, otherwise the active tab alone.
pub fn placements(active: Option<TabId>, split: Option<SplitView>, rect: ContentRect) -> Vec<(TabId, ContentRect)> {
    match (active, split) {
        (Some(a), Some(s)) if s.contains(a) => {
            let (l, r) = split_rects(rect, s.ratio);
            vec![(s.left, l), (s.right, r)]
        }
        (Some(a), _) => vec![(a, rect)],
        (None, _) => Vec::new(),
    }
}

/// The tab to pair with `primary` when the user did not say: the nearest web tab to its right, else to its left.
pub fn default_partner(tabs: &[(TabId, bool)], primary: TabId) -> Option<TabId> {
    let pos = tabs.iter().position(|(id, _)| *id == primary)?;
    let web = |i: &usize| tabs[*i].1 && tabs[*i].0 != primary;
    ((pos + 1)..tabs.len()).find(web).or_else(|| (0..pos).rev().find(web)).map(|i| tabs[i].0)
}

impl Controller {
    fn is_web_tab(&self, id: TabId) -> bool {
        self.tabs.get(id).is_some_and(|t| !is_internal(&t.url))
    }

    pub fn split_view(&mut self, op: SplitOp) -> Result<()> {
        match op {
            SplitOp::Open { tab_id, with } => {
                let primary = tab_id.or(self.tabs.active_id()).ok_or_else(|| Error::msg("There is no tab to split."))?;
                if !self.is_web_tab(primary) {
                    return Err(Error::msg("Split view shows web pages. Open a site in this tab first."));
                }
                let list: Vec<(TabId, bool)> = self.tabs.tabs().iter().map(|t| (t.id, !is_internal(&t.url))).collect();
                let partner = match with {
                    Some(w) if w != primary && self.is_web_tab(w) => w,
                    Some(_) => return Err(Error::msg("Split view needs two different web pages.")),
                    None => default_partner(&list, primary).ok_or_else(|| Error::msg("Open a second web page in another tab, then split."))?,
                };
                let ratio = self.layout.split.map(|s| s.ratio).unwrap_or(0.5);
                self.layout.split = Some(SplitView { left: primary, right: partner, ratio });
                log::info!("split view: tabs {primary} and {partner}");
                if self.tabs.active_id() != Some(primary) {
                    self.activate(primary);
                } else {
                    self.relayout();
                }
            }
            SplitOp::Close => {
                if self.layout.split.take().is_some() {
                    log::info!("split view closed");
                }
                self.relayout();
            }
            SplitOp::Swap => {
                if let Some(s) = self.layout.split.as_mut() {
                    std::mem::swap(&mut s.left, &mut s.right);
                }
                self.relayout();
            }
            SplitOp::Ratio { ratio } => {
                if let Some(s) = self.layout.split.as_mut() {
                    s.ratio = clamp_ratio(ratio);
                }
                self.relayout();
            }
        }
        self.mark_state();
        Ok(())
    }

    /// Drops the split pair when one half is gone or is no longer a web page.
    pub(crate) fn validate_split(&mut self) {
        if let Some(s) = self.layout.split {
            if !self.is_web_tab(s.left) || !self.is_web_tab(s.right) || s.left == s.right {
                self.layout.split = None;
                self.mark_state();
            }
        }
    }

    /// The tab on screen together with the active tab (its split partner), if any.
    pub(crate) fn split_partner_on_screen(&self) -> Option<TabId> {
        let active = self.tabs.active_id()?;
        self.layout.split.and_then(|s| s.partner(active))
    }

    /// The user clicked into a tab's page. In split view that half becomes the active tab.
    pub fn on_tab_focused(&mut self, id: TabId) {
        if self.tabs.active_id() == Some(id) {
            return;
        }
        if self.split_partner_on_screen() == Some(id) {
            self.activate(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: ContentRect = ContentRect { x: 50.0, y: 100.0, width: 1006.0, height: 600.0 };

    #[test]
    fn halves_fill_the_area_with_a_gap() {
        let (l, r) = split_rects(AREA, 0.5);
        assert_eq!((l.x, l.width), (50.0, 500.0));
        assert_eq!((r.x, r.width), (50.0 + 500.0 + GAP, 500.0));
        assert_eq!(l.width + GAP + r.width, AREA.width);
        assert_eq!((l.y, l.height, r.y, r.height), (100.0, 600.0, 100.0, 600.0));
    }

    #[test]
    fn ratios_are_clamped() {
        assert_eq!(clamp_ratio(0.05), MIN_RATIO);
        assert_eq!(clamp_ratio(0.95), MAX_RATIO);
        assert_eq!(clamp_ratio(f64::NAN), 0.5);
        let (l, _) = split_rects(AREA, 0.0);
        assert_eq!(l.width, (1000.0 * MIN_RATIO).round());
    }

    #[test]
    fn the_pair_is_shown_only_while_one_half_is_active() {
        let s = Some(SplitView { left: 1, right: 2, ratio: 0.5 });
        assert_eq!(placements(Some(1), s, AREA).len(), 2);
        assert_eq!(placements(Some(2), s, AREA).iter().map(|p| p.0).collect::<Vec<_>>(), vec![1, 2], "left stays left");
        assert_eq!(placements(Some(3), s, AREA), vec![(3, AREA)]);
        assert_eq!(placements(Some(3), None, AREA), vec![(3, AREA)]);
        assert!(placements(None, s, AREA).is_empty());
    }

    #[test]
    fn the_default_partner_is_the_nearest_web_tab() {
        let tabs = [(1, true), (2, false), (3, true), (4, true)];
        assert_eq!(default_partner(&tabs, 1), Some(3), "skips the browser page");
        assert_eq!(default_partner(&tabs, 4), Some(3), "falls back to the left");
        assert_eq!(default_partner(&[(1, true), (2, false)], 1), None);
        assert_eq!(default_partner(&tabs, 9), None);
    }

    #[test]
    fn partners() {
        let s = SplitView { left: 1, right: 2, ratio: 0.5 };
        assert_eq!(s.partner(1), Some(2));
        assert_eq!(s.partner(2), Some(1));
        assert_eq!(s.partner(3), None);
    }

    #[test]
    fn ops_deserialize_from_the_shell() {
        let op: SplitOp = serde_json::from_str(r#"{"op":"open","tabId":4}"#).unwrap();
        assert!(matches!(op, SplitOp::Open { tab_id: Some(4), with: None }));
        let op: SplitOp = serde_json::from_str(r#"{"op":"ratio","ratio":0.3}"#).unwrap();
        assert!(matches!(op, SplitOp::Ratio { ratio } if (ratio - 0.3).abs() < 1e-9));
        assert!(matches!(serde_json::from_str::<SplitOp>(r#"{"op":"swap"}"#).unwrap(), SplitOp::Swap));
    }
}
