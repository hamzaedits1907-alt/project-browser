//! Browser backup and restore: one local JSON file with the data that is safe to keep in plain text.
//!
//! In a backup: settings, bookmarks (workspaces are bookmark folders, so they come along), Project Shield's allowlist,
//! switched-off lists and the user's own rules, notes and other Project Memory records, and the shell's per-site
//! preferences (zoom, dark pages) and Focus settings. Never in a backup: passwords and saved logins, cookies and site
//! storage, the AI key, history, downloads. The file is written where the user chooses with the Save dialog; nothing
//! is uploaded anywhere.
//!
//! Restoring adds instead of replacing where data could be lost: bookmarks go into a new folder "Restored from
//! backup", notes that already exist (same title and text) are skipped; settings and the Shield configuration are
//! replaced by the backup's.

use serde::{Deserialize, Serialize};

use crate::db::bookmarks::{Bookmark, ImportNode, BAR_ID, OTHER_ID};
use crate::db::memory::{MemoryDraft, MemoryItem, MemoryKind, MemorySource};
use crate::settings::Settings;

pub const FORMAT: &str = "project-browser-backup";
pub const VERSION: u32 = 1;
/// A backup bigger than this is not read (a real one is a few hundred KB).
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackupBookmark {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub folder: bool,
    pub title: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct BackupShield {
    pub disabled_sites: Vec<String>,
    pub disabled_lists: Vec<String>,
    pub user_rules: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackupNote {
    pub kind: MemoryKind,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub sources: Vec<MemorySource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub format: String,
    pub version: u32,
    pub created_at: i64,
    pub app_version: String,
    pub settings: serde_json::Value,
    #[serde(default)]
    pub bookmarks: Vec<BackupBookmark>,
    #[serde(default)]
    pub shield: BackupShield,
    #[serde(default)]
    pub notes: Vec<BackupNote>,
    /// The shell's own preferences (per-site zoom and dark pages, Focus), as the shell gave them.
    #[serde(default)]
    pub shell: serde_json::Value,
}

/// What a restore did, for the shell to show.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    pub bookmarks: usize,
    pub notes: usize,
    pub notes_skipped: usize,
    pub settings: bool,
    pub shield: bool,
    pub created_at: i64,
    /// The shell's part of the backup, for the shell to apply.
    pub shell: serde_json::Value,
}

pub fn build(
    settings: &Settings,
    bookmarks: &[Bookmark],
    shield: BackupShield,
    notes: &[MemoryItem],
    shell: serde_json::Value,
    now: i64,
    app_version: &str,
) -> Backup {
    let mut sorted: Vec<&Bookmark> = bookmarks.iter().collect();
    sorted.sort_by_key(|b| (b.parent_id.unwrap_or(0), b.position));
    Backup {
        format: FORMAT.into(),
        version: VERSION,
        created_at: now,
        app_version: app_version.into(),
        settings: serde_json::to_value(settings).unwrap_or(serde_json::Value::Null),
        bookmarks: sorted
            .into_iter()
            .map(|b| BackupBookmark { id: b.id, parent_id: b.parent_id, folder: b.kind == "folder", title: b.title.clone(), url: b.url.clone() })
            .collect(),
        shield,
        notes: notes
            .iter()
            .map(|n| BackupNote { kind: n.kind, title: n.title.clone(), body: n.body.clone(), tags: n.tags.clone(), sources: n.sources.clone() })
            .collect(),
        shell: if shell.is_object() { shell } else { serde_json::Value::Null },
    }
}

/// Reads a backup file's text. Anything that is not a Project Browser backup of a known version is refused.
pub fn parse(text: &str) -> Result<Backup, String> {
    let b: Backup = serde_json::from_str(text).map_err(|_| "This file is not a Project Browser backup.".to_string())?;
    if b.format != FORMAT {
        return Err("This file is not a Project Browser backup.".into());
    }
    if b.version > VERSION {
        return Err("This backup was made by a newer version of Project Browser. Update first, then restore it.".into());
    }
    Ok(b)
}

/// The backup's settings applied over the defaults (unknown fields ignored). A download folder that does not exist on
/// this computer is dropped (downloads then go to the Windows Downloads folder).
pub fn restored_settings(b: &Backup) -> Option<Settings> {
    if !b.settings.is_object() {
        return None;
    }
    let mut s = Settings::default().with_overrides(&b.settings.to_string()).ok()?;
    if s.download_dir.as_deref().map(|d| !std::path::Path::new(d).is_dir()).unwrap_or(false) {
        s.download_dir = None;
    }
    Some(s)
}

/// The bookmark bar and "Other bookmarks" of the backup as two folders to import (empty ones left out).
pub fn bookmark_tree(b: &Backup, now: i64) -> Vec<ImportNode> {
    fn children(all: &[BackupBookmark], parent: i64, now: i64, depth: usize) -> Vec<ImportNode> {
        if depth > 64 {
            return Vec::new();
        }
        all.iter()
            .filter(|b| b.parent_id == Some(parent) && b.id != parent)
            .filter_map(|b| {
                if b.folder {
                    Some(ImportNode::Folder { title: b.title.clone(), children: children(all, b.id, now, depth + 1) })
                } else {
                    let url = b.url.clone()?;
                    let ok = url::Url::parse(&url).map(|u| matches!(u.scheme(), "http" | "https" | "file")).unwrap_or(false);
                    ok.then(|| ImportNode::Url { title: b.title.clone(), url, created_at: now })
                }
            })
            .collect()
    }
    let mut out = Vec::new();
    for (root, title) in [(BAR_ID, "Bookmarks bar"), (OTHER_ID, "Other bookmarks")] {
        let c = children(&b.bookmarks, root, now, 0);
        if !c.is_empty() {
            out.push(ImportNode::Folder { title: title.into(), children: c });
        }
    }
    out
}

pub fn count_urls(nodes: &[ImportNode]) -> usize {
    nodes
        .iter()
        .map(|n| match n {
            ImportNode::Folder { children, .. } => count_urls(children),
            ImportNode::Url { .. } => 1,
        })
        .sum()
}

/// Notes of the backup that are not already in Project Memory (same title and text).
pub fn new_notes(b: &Backup, existing: &[MemoryItem]) -> (Vec<MemoryDraft>, usize) {
    let have: std::collections::HashSet<(String, String)> = existing.iter().map(|m| (m.title.clone(), m.body.clone())).collect();
    let mut skipped = 0;
    let drafts = b
        .notes
        .iter()
        .filter(|n| {
            let dup = have.contains(&(n.title.clone(), n.body.clone()));
            if dup {
                skipped += 1;
            }
            !dup
        })
        .map(|n| MemoryDraft {
            kind: n.kind,
            title: n.title.clone(),
            body: n.body.clone(),
            tags: n.tags.clone(),
            sources: n.sources.clone(),
            related: Vec::new(),
        })
        .collect();
    (drafts, skipped)
}

pub fn file_name(now_ms: i64) -> String {
    let date = chrono_like_date(now_ms);
    format!("Project Browser backup {date}.json")
}

/// `YYYY-MM-DD` (UTC) without a date library.
fn chrono_like_date(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bm(id: i64, parent: i64, folder: bool, title: &str, url: Option<&str>) -> Bookmark {
        Bookmark {
            id,
            parent_id: Some(parent),
            kind: if folder { "folder" } else { "url" }.into(),
            title: title.into(),
            url: url.map(Into::into),
            position: id,
            created_at: 0,
        }
    }

    fn sample() -> Backup {
        let marks = vec![
            bm(10, BAR_ID, false, "Ex", Some("https://example.com/")),
            bm(11, OTHER_ID, true, "Workspaces", None),
            bm(12, 11, false, "Docs", Some("https://docs.example/")),
            bm(13, OTHER_ID, false, "bad", Some("javascript:alert(1)")),
        ];
        let mut settings = Settings::default();
        settings.gaming_mode = true;
        settings.download_dir = Some("Z:\\does\\not\\exist".into());
        build(
            &settings,
            &marks,
            BackupShield { disabled_sites: vec!["a.com".into()], ..Default::default() },
            &[],
            serde_json::json!({"site.prefs": {}}),
            1_791_825_005_000,
            "0.8.0",
        )
    }

    #[test]
    fn a_backup_round_trips_and_holds_no_secrets() {
        let b = sample();
        let text = serde_json::to_string(&b).unwrap();
        assert_eq!(parse(&text).unwrap(), b);
        // Only these parts exist: no logins, cookies, site storage, AI key or history.
        let keys: Vec<String> = serde_json::from_str::<serde_json::Value>(&text).unwrap().as_object().unwrap().keys().cloned().collect();
        let mut keys = keys;
        keys.sort();
        assert_eq!(keys, ["appVersion", "bookmarks", "createdAt", "format", "notes", "settings", "shell", "shield", "version"]);
        let lower = text.to_lowercase();
        for secret in ["cookie", "apikey", "api_key", "nvapi-", "token"] {
            assert!(!lower.contains(secret), "backup mentions {secret}");
        }
    }

    #[test]
    fn other_files_and_newer_versions_are_refused() {
        assert!(parse("{}").is_err());
        assert!(parse("not json").is_err());
        let mut b = sample();
        b.version = VERSION + 1;
        assert!(parse(&serde_json::to_string(&b).unwrap()).unwrap_err().contains("newer version"));
        b.format = "x".into();
        b.version = 1;
        assert!(parse(&serde_json::to_string(&b).unwrap()).is_err());
    }

    #[test]
    fn restored_settings_keep_values_but_drop_a_missing_download_folder() {
        let s = restored_settings(&sample()).unwrap();
        assert!(s.gaming_mode);
        assert_eq!(s.download_dir, None);
    }

    #[test]
    fn bookmarks_come_back_as_folders_without_unsafe_addresses() {
        let tree = bookmark_tree(&sample(), 5);
        assert_eq!(tree.len(), 2);
        assert_eq!(count_urls(&tree), 2);
        let text = format!("{tree:?}");
        assert!(text.contains("Workspaces") && text.contains("docs.example") && !text.contains("javascript"));
    }

    #[test]
    fn file_names_carry_the_date() {
        assert_eq!(file_name(1_791_825_005_000), "Project Browser backup 2026-10-12.json");
        assert_eq!(chrono_like_date(0), "1970-01-01");
    }
}
