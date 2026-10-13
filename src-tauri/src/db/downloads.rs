use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::Db;
use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadState {
    InProgress,
    Paused,
    Completed,
    Cancelled,
    Interrupted,
}

impl DownloadState {
    pub fn as_str(self) -> &'static str {
        match self {
            DownloadState::InProgress => "inprogress",
            DownloadState::Paused => "paused",
            DownloadState::Completed => "completed",
            DownloadState::Cancelled => "cancelled",
            DownloadState::Interrupted => "interrupted",
        }
    }

    fn parse(s: &str) -> DownloadState {
        match s {
            "inprogress" => DownloadState::InProgress,
            "paused" => DownloadState::Paused,
            "completed" => DownloadState::Completed,
            "cancelled" => DownloadState::Cancelled,
            _ => DownloadState::Interrupted,
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, DownloadState::InProgress | DownloadState::Paused)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    pub id: i64,
    pub url: String,
    pub path: String,
    pub filename: String,
    pub mime: Option<String>,
    pub total_bytes: Option<i64>,
    pub received_bytes: i64,
    pub state: DownloadState,
    /// Set for file types that can run code when opened (executables, scripts, installers).
    pub danger: Option<String>,
    pub error: Option<String>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    /// Downloads started from private tabs are kept in memory only.
    #[serde(default)]
    pub private: bool,
    /// True while Gaming Mode holds this download paused.
    #[serde(default)]
    pub paused_by_gaming: bool,
    /// A completed download whose file is no longer where it was saved (moved or deleted since). Checked, not stored.
    #[serde(default)]
    pub missing: bool,
    /// Every byte arrived but WebView2 has not finished: Windows is checking the file, or SmartScreen waits for the
    /// user to keep or delete it (Review opens that prompt). Not stored.
    #[serde(default)]
    pub checking: bool,
}

const DANGEROUS_EXTENSIONS: &[&str] = &[
    "exe",
    "msi",
    "msix",
    "msixbundle",
    "appx",
    "bat",
    "cmd",
    "com",
    "scr",
    "pif",
    "ps1",
    "psm1",
    "vbs",
    "vbe",
    "js",
    "jse",
    "wsf",
    "wsh",
    "hta",
    "jar",
    "lnk",
    "reg",
    "cpl",
    "dll",
    "sys",
    "inf",
    "application",
    "gadget",
    "msp",
    "mst",
    "iso",
    "img",
    "vhd",
    "vhdx",
];

/// Classifies a filename by whether opening it can execute code.
pub fn danger_for(filename: &str) -> Option<String> {
    let ext = filename.rsplit_once('.')?.1.to_ascii_lowercase();
    DANGEROUS_EXTENSIONS.contains(&ext.as_str()).then(|| "executable".to_string())
}

/// `dir/name`, or `dir/stem (n).ext` for the first `n` that doesn't exist yet.
pub fn unique_path(dir: &std::path::Path, name: &str, exists: impl Fn(&std::path::Path) -> bool) -> std::path::PathBuf {
    // Never allow a suggested name to escape the target directory.
    let name = std::path::Path::new(name).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let name = if name.is_empty() { "download".to_string() } else { name };
    let candidate = dir.join(&name);
    if !exists(&candidate) {
        return candidate;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.clone(), String::new()),
    };
    (1..10_000).map(|n| dir.join(format!("{stem} ({n}){ext}"))).find(|p| !exists(p)).unwrap_or(candidate)
}

impl Db {
    pub fn download_insert(&self, d: &Download) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO downloads (id, url, path, filename, mime, total_bytes, received_bytes, state, danger, error, started_at, ended_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                d.id, d.url, d.path, d.filename, d.mime, d.total_bytes, d.received_bytes,
                d.state.as_str(), d.danger, d.error, d.started_at, d.ended_at
            ],
        )?;
        Ok(())
    }

    pub fn download_update(&self, d: &Download) -> Result<()> {
        self.conn.execute(
            "UPDATE downloads SET path = ?2, filename = ?3, total_bytes = ?4, received_bytes = ?5, state = ?6, error = ?7, ended_at = ?8 WHERE id = ?1",
            params![d.id, d.path, d.filename, d.total_bytes, d.received_bytes, d.state.as_str(), d.error, d.ended_at],
        )?;
        Ok(())
    }

    pub fn downloads_list(&self, limit: u32) -> Result<Vec<Download>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, url, path, filename, mime, total_bytes, received_bytes, state, danger, error, started_at, ended_at
               FROM downloads ORDER BY started_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit], |r| {
            Ok(Download {
                id: r.get(0)?,
                url: r.get(1)?,
                path: r.get(2)?,
                filename: r.get(3)?,
                mime: r.get(4)?,
                total_bytes: r.get(5)?,
                received_bytes: r.get(6)?,
                state: DownloadState::parse(&r.get::<_, String>(7)?),
                danger: r.get(8)?,
                error: r.get(9)?,
                started_at: r.get(10)?,
                ended_at: r.get(11)?,
                private: false,
                paused_by_gaming: false,
                missing: false,
                checking: false,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn download_remove(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM downloads WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn downloads_clear_finished(&self) -> Result<()> {
        self.conn.execute("DELETE FROM downloads WHERE state NOT IN ('inprogress', 'paused')", [])?;
        Ok(())
    }

    /// Downloads still marked active from a previous run cannot be resumed
    /// (WebView2 download operations do not survive a restart).
    pub fn downloads_mark_stale_interrupted(&self, now: i64) -> Result<usize> {
        Ok(self.conn.execute(
            "UPDATE downloads SET state = 'interrupted', error = 'Browser closed before the download finished', ended_at = ?1
              WHERE state IN ('inprogress', 'paused')",
            [now],
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: i64, state: DownloadState) -> Download {
        Download {
            id,
            url: "https://x/file.zip".into(),
            path: "C:\\Downloads\\file.zip".into(),
            filename: "file.zip".into(),
            mime: Some("application/zip".into()),
            total_bytes: Some(100),
            received_bytes: 10,
            state,
            danger: None,
            error: None,
            started_at: id,
            ended_at: None,
            private: false,
            paused_by_gaming: false,
            missing: false,
            checking: false,
        }
    }

    #[test]
    fn unique_paths() {
        use std::path::{Path, PathBuf};
        let dir = Path::new("C:\\dl");
        let taken: Vec<PathBuf> = vec![dir.join("a.zip"), dir.join("a (1).zip"), dir.join("README")];
        let exists = |p: &Path| taken.iter().any(|t| t == p);
        assert_eq!(unique_path(dir, "b.zip", exists), dir.join("b.zip"));
        assert_eq!(unique_path(dir, "a.zip", exists), dir.join("a (2).zip"));
        assert_eq!(unique_path(dir, "README", exists), dir.join("README (1)"));
        assert_eq!(unique_path(dir, "..\\..\\evil.exe", exists), dir.join("evil.exe"));
        assert_eq!(unique_path(dir, "", exists), dir.join("download"));
    }

    #[test]
    fn danger_classification() {
        assert_eq!(danger_for("setup.EXE").as_deref(), Some("executable"));
        assert_eq!(danger_for("script.ps1").as_deref(), Some("executable"));
        assert_eq!(danger_for("photo.png"), None);
        assert_eq!(danger_for("noextension"), None);
    }

    #[test]
    fn lifecycle_roundtrip_and_stale_marking() {
        let db = Db::open_in_memory().unwrap();
        db.download_insert(&sample(1, DownloadState::InProgress)).unwrap();
        let mut done = sample(2, DownloadState::InProgress);
        db.download_insert(&done).unwrap();
        done.state = DownloadState::Completed;
        done.received_bytes = 100;
        db.download_update(&done).unwrap();
        assert_eq!(db.downloads_mark_stale_interrupted(50).unwrap(), 1);
        let list = db.downloads_list(10).unwrap();
        assert_eq!(list[0].id, 2, "newest first");
        assert_eq!(list[0].state, DownloadState::Completed);
        assert_eq!(list[1].state, DownloadState::Interrupted);
        db.downloads_clear_finished().unwrap();
        assert!(db.downloads_list(10).unwrap().is_empty());
    }
}
