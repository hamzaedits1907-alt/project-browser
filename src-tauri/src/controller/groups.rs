//! Tab groups: what the shell can do with them (the rules themselves live in the tab model, `tabs.rs`).

use serde::Deserialize;

use super::Controller;
use crate::error::{Error, Result};
use crate::tabs::{GroupId, TabId};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", tag = "op")]
pub enum GroupOp {
    /// New group made of these tabs.
    Create {
        tabs: Vec<TabId>,
        title: String,
        color: String,
    },
    Add {
        tab: TabId,
        group: GroupId,
    },
    /// Take one tab out of its group.
    Remove {
        tab: TabId,
    },
    Update {
        group: GroupId,
        title: Option<String>,
        color: Option<String>,
        collapsed: Option<bool>,
    },
    /// Dissolve the group; its tabs stay open.
    Ungroup {
        group: GroupId,
    },
    /// Close every tab of the group.
    Close {
        group: GroupId,
    },
}

impl Controller {
    /// Applies a group operation to the current window. Returns the id of a group that was created.
    pub fn group_op(&mut self, op: GroupOp) -> Result<Option<GroupId>> {
        let created = match op {
            GroupOp::Create { tabs, title, color } => {
                Some(self.tabs.group_create(&tabs, &title, &color).ok_or_else(|| Error::msg("Pinned tabs can't be grouped."))?)
            }
            GroupOp::Add { tab, group } => {
                if !self.tabs.group_add(tab, group) {
                    return Err(Error::msg("That tab can't join this group."));
                }
                None
            }
            GroupOp::Remove { tab } => {
                self.tabs.group_remove_tab(tab);
                None
            }
            GroupOp::Update { group, title, color, collapsed } => {
                if collapsed == Some(true) && self.tabs.active().is_some_and(|t| t.group == Some(group)) {
                    // Like Chrome: collapsing the group you are in moves you to the nearest tab outside it.
                    self.leave_group(group);
                }
                self.tabs.group_update(group, title.as_deref(), color.as_deref(), collapsed);
                None
            }
            GroupOp::Ungroup { group } => {
                self.tabs.group_dissolve(group);
                None
            }
            GroupOp::Close { group } => {
                for id in self.tabs.group_tabs(group) {
                    self.close_tab(id);
                }
                None
            }
        };
        self.mark_session();
        Ok(created)
    }

    /// Activates the nearest tab outside `group` (to the right first), or a new tab when every tab is in it.
    fn leave_group(&mut self, group: GroupId) {
        let tabs = self.tabs.tabs();
        let Some(active) = self.tabs.active_id().and_then(|a| self.tabs.index_of(a)) else { return };
        let outside = |i: &usize| tabs[*i].group != Some(group);
        let next = (active + 1..tabs.len()).find(outside).or_else(|| (0..active).rev().find(outside)).map(|i| tabs[i].id);
        match next {
            Some(id) => self.activate(id),
            None => {
                self.new_tab(false, false);
            }
        }
    }
}
