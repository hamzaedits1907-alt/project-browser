use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::history::ImportCounts;
use super::Db;
use crate::error::{Error, Result};

pub const BAR_ID: i64 = 1;
pub const OTHER_ID: i64 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub kind: String,
    pub title: String,
    pub url: Option<String>,
    pub position: i64,
    pub created_at: i64,
}

/// Input tree used by importers.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportNode {
    Folder { title: String, children: Vec<ImportNode> },
    Url { title: String, url: String, created_at: i64 },
}

impl ImportNode {
    pub fn count_urls(&self) -> usize {
        match self {
            ImportNode::Url { .. } => 1,
            ImportNode::Folder { children, .. } => children.iter().map(ImportNode::count_urls).sum(),
        }
    }
}

impl Db {
    pub fn bookmarks_all(&self) -> Result<Vec<Bookmark>> {
        let mut stmt =
            self.conn.prepare_cached("SELECT id, parent_id, kind, title, url, position, created_at FROM bookmarks ORDER BY parent_id, position")?;
        let rows = stmt.query_map([], map_bookmark)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn bookmark_get(&self, id: i64) -> Result<Option<Bookmark>> {
        Ok(self
            .conn
            .query_row("SELECT id, parent_id, kind, title, url, position, created_at FROM bookmarks WHERE id = ?1", [id], map_bookmark)
            .optional()?)
    }

    pub fn bookmark_ids_for_url(&self, url: &str) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare_cached("SELECT id FROM bookmarks WHERE kind = 'url' AND url = ?1")?;
        let rows = stmt.query_map([url], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn is_bookmarked(&self, url: &str) -> Result<bool> {
        Ok(!self.bookmark_ids_for_url(url)?.is_empty())
    }

    fn ensure_folder(&self, id: i64) -> Result<()> {
        match self.bookmark_get(id)? {
            Some(b) if b.kind == "folder" => Ok(()),
            _ => Err(Error::msg("parent folder does not exist")),
        }
    }

    fn next_position(&self, parent: i64) -> Result<i64> {
        Ok(self.conn.query_row("SELECT COALESCE(MAX(position) + 1, 0) FROM bookmarks WHERE parent_id = ?1", [parent], |r| r.get(0))?)
    }

    pub fn bookmark_add(&self, parent: i64, title: &str, url: Option<&str>, now: i64) -> Result<i64> {
        self.ensure_folder(parent)?;
        let kind = if url.is_some() { "url" } else { "folder" };
        let pos = self.next_position(parent)?;
        self.conn.execute(
            "INSERT INTO bookmarks (parent_id, kind, title, url, position, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![parent, kind, title, url, pos, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn bookmark_update(&self, id: i64, title: &str, url: Option<&str>) -> Result<()> {
        if id == BAR_ID || id == OTHER_ID {
            return Err(Error::msg("root folders cannot be edited"));
        }
        let b = self.bookmark_get(id)?.ok_or_else(|| Error::msg("bookmark not found"))?;
        let url = if b.kind == "url" { url.or(b.url.as_deref()) } else { None };
        self.conn.execute("UPDATE bookmarks SET title = ?2, url = ?3 WHERE id = ?1", params![id, title, url])?;
        Ok(())
    }

    /// Moves `id` into `parent` at `index`, renumbering siblings densely.
    pub fn bookmark_move(&self, id: i64, parent: i64, index: usize) -> Result<()> {
        if id == BAR_ID || id == OTHER_ID {
            return Err(Error::msg("root folders cannot be moved"));
        }
        self.ensure_folder(parent)?;
        // Refuse to move a folder into itself or one of its descendants.
        let mut cursor = Some(parent);
        while let Some(c) = cursor {
            if c == id {
                return Err(Error::msg("cannot move a folder into itself"));
            }
            cursor = self.bookmark_get(c)?.and_then(|b| b.parent_id);
        }
        let tx = self.conn.unchecked_transaction()?;
        let mut siblings: Vec<i64> = {
            let mut stmt = tx.prepare("SELECT id FROM bookmarks WHERE parent_id = ?1 AND id != ?2 ORDER BY position")?;
            let rows = stmt.query_map(params![parent, id], |r| r.get(0))?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        siblings.insert(index.min(siblings.len()), id);
        for (pos, sid) in siblings.iter().enumerate() {
            tx.execute("UPDATE bookmarks SET parent_id = ?2, position = ?3 WHERE id = ?1", params![sid, parent, pos as i64])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn bookmark_delete(&self, id: i64) -> Result<()> {
        if id == BAR_ID || id == OTHER_ID {
            return Err(Error::msg("root folders cannot be deleted"));
        }
        self.conn.execute("DELETE FROM bookmarks WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Removes every bookmark pointing at `url` (Ctrl+D toggle off).
    pub fn bookmark_delete_url(&self, url: &str) -> Result<usize> {
        Ok(self.conn.execute("DELETE FROM bookmarks WHERE kind = 'url' AND url = ?1", [url])?)
    }

    /// Imports a tree under the folder `folder_title` in `parent` (created if it does not exist yet).
    ///
    /// Idempotent: folders with the same name are merged and a URL already present in the same folder
    /// is skipped, so importing the same source twice adds nothing the second time.
    pub fn bookmarks_import(&self, parent: i64, folder_title: &str, nodes: &[ImportNode], now: i64) -> Result<ImportCounts> {
        let tx = self.conn.unchecked_transaction()?;
        let root = match self.find_child_folder(parent, folder_title)? {
            Some(id) => id,
            None => self.bookmark_add(parent, folder_title, None, now)?,
        };
        let mut counts = ImportCounts::default();
        self.import_into(root, nodes, now, &mut counts)?;
        tx.commit()?;
        Ok(counts)
    }

    fn find_child_folder(&self, parent: i64, title: &str) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id FROM bookmarks WHERE parent_id = ?1 AND kind = 'folder' AND title = ?2 ORDER BY position LIMIT 1",
                params![parent, title],
                |r| r.get(0),
            )
            .optional()?)
    }

    fn has_child_url(&self, parent: i64, url: &str) -> Result<bool> {
        Ok(self
            .conn
            .query_row("SELECT 1 FROM bookmarks WHERE parent_id = ?1 AND kind = 'url' AND url = ?2 LIMIT 1", params![parent, url], |_| Ok(()))
            .optional()?
            .is_some())
    }

    fn import_into(&self, parent: i64, nodes: &[ImportNode], now: i64, counts: &mut ImportCounts) -> Result<()> {
        for node in nodes {
            match node {
                ImportNode::Url { title, url, created_at } => {
                    let safe = url.starts_with("http://") || url.starts_with("https://") || url.starts_with("file:");
                    if !safe || self.has_child_url(parent, url)? {
                        counts.skipped += 1;
                        continue;
                    }
                    let id = self.bookmark_add(parent, title, Some(url), now)?;
                    self.conn.execute("UPDATE bookmarks SET created_at = ?2 WHERE id = ?1", params![id, created_at])?;
                    counts.added += 1;
                }
                ImportNode::Folder { title, children } => {
                    let id = match self.find_child_folder(parent, title)? {
                        Some(id) => id,
                        None => self.bookmark_add(parent, title, None, now)?,
                    };
                    self.import_into(id, children, now, counts)?;
                }
            }
        }
        Ok(())
    }

    pub fn bookmark_suggest(&self, text: &str, limit: u32) -> Result<Vec<Bookmark>> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(Vec::new());
        }
        let pattern = format!("%{}%", text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, parent_id, kind, title, url, position, created_at FROM bookmarks
              WHERE kind = 'url' AND (title LIKE ?1 ESCAPE '\\' OR url LIKE ?1 ESCAPE '\\')
              ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![pattern, limit], map_bookmark)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn recent_bookmarks(&self, limit: u32) -> Result<Vec<Bookmark>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, parent_id, kind, title, url, position, created_at FROM bookmarks
              WHERE kind = 'url' ORDER BY created_at DESC, id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit], map_bookmark)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

fn map_bookmark(r: &rusqlite::Row<'_>) -> rusqlite::Result<Bookmark> {
    Ok(Bookmark {
        id: r.get(0)?,
        parent_id: r.get(1)?,
        kind: r.get(2)?,
        title: r.get(3)?,
        url: r.get(4)?,
        position: r.get(5)?,
        created_at: r.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roots_exist_and_are_protected() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.bookmarks_all().unwrap().len(), 2);
        assert!(db.bookmark_delete(BAR_ID).is_err());
        assert!(db.bookmark_move(OTHER_ID, BAR_ID, 0).is_err());
        assert!(db.bookmark_update(BAR_ID, "x", None).is_err());
    }

    #[test]
    fn add_update_toggle() {
        let db = Db::open_in_memory().unwrap();
        let id = db.bookmark_add(BAR_ID, "Ex", Some("https://ex.com/"), 1).unwrap();
        assert!(db.is_bookmarked("https://ex.com/").unwrap());
        db.bookmark_update(id, "Example", Some("https://example.com/")).unwrap();
        let b = db.bookmark_get(id).unwrap().unwrap();
        assert_eq!((b.title.as_str(), b.url.as_deref()), ("Example", Some("https://example.com/")));
        assert_eq!(db.bookmark_delete_url("https://example.com/").unwrap(), 1);
        assert!(!db.is_bookmarked("https://example.com/").unwrap());
        assert!(db.bookmark_add(9999, "x", Some("https://x"), 1).is_err());
    }

    #[test]
    fn move_renumbers_and_rejects_cycles() {
        let db = Db::open_in_memory().unwrap();
        let a = db.bookmark_add(BAR_ID, "a", Some("https://a/"), 1).unwrap();
        let b = db.bookmark_add(BAR_ID, "b", Some("https://b/"), 1).unwrap();
        let f = db.bookmark_add(BAR_ID, "folder", None, 1).unwrap();
        let inner = db.bookmark_add(f, "inner", None, 1).unwrap();
        db.bookmark_move(b, BAR_ID, 0).unwrap();
        let order: Vec<i64> = db.bookmarks_all().unwrap().into_iter().filter(|x| x.parent_id == Some(BAR_ID)).map(|x| x.id).collect();
        assert_eq!(order, vec![b, a, f]);
        assert!(db.bookmark_move(f, inner, 0).is_err());
        assert!(db.bookmark_move(f, f, 0).is_err());
        db.bookmark_move(a, f, 5).unwrap();
        assert_eq!(db.bookmark_get(a).unwrap().unwrap().parent_id, Some(f));
    }

    #[test]
    fn deleting_folder_cascades() {
        let db = Db::open_in_memory().unwrap();
        let f = db.bookmark_add(OTHER_ID, "f", None, 1).unwrap();
        db.bookmark_add(f, "x", Some("https://x/"), 1).unwrap();
        db.bookmark_delete(f).unwrap();
        assert!(!db.is_bookmarked("https://x/").unwrap());
    }

    #[test]
    fn import_tree_skips_unsafe_urls() {
        let db = Db::open_in_memory().unwrap();
        let nodes = vec![
            ImportNode::Url { title: "a".into(), url: "https://a/".into(), created_at: 5 },
            ImportNode::Url { title: "js".into(), url: "javascript:alert(1)".into(), created_at: 5 },
            ImportNode::Folder { title: "sub".into(), children: vec![ImportNode::Url { title: "b".into(), url: "http://b/".into(), created_at: 6 }] },
        ];
        assert_eq!(nodes.iter().map(ImportNode::count_urls).sum::<usize>(), 3);
        assert_eq!(db.bookmarks_import(OTHER_ID, "Imported", &nodes, 1).unwrap(), ImportCounts { added: 2, skipped: 1 });
        assert!(db.is_bookmarked("http://b/").unwrap());
        assert!(!db.is_bookmarked("javascript:alert(1)").unwrap());
        assert_eq!(db.bookmark_suggest("b", 5).unwrap().len(), 1);
    }

    #[test]
    fn importing_twice_adds_nothing_the_second_time() {
        let db = Db::open_in_memory().unwrap();
        let nodes = vec![
            ImportNode::Url { title: "a".into(), url: "https://a/".into(), created_at: 5 },
            ImportNode::Folder {
                title: "sub".into(),
                children: vec![ImportNode::Url { title: "b".into(), url: "https://b/".into(), created_at: 6 }],
            },
        ];
        assert_eq!(db.bookmarks_import(OTHER_ID, "Imported", &nodes, 1).unwrap().added, 2);
        let folders_before = db.bookmarks_all().unwrap().iter().filter(|b| b.kind == "folder").count();
        let again = db.bookmarks_import(OTHER_ID, "Imported", &nodes, 2).unwrap();
        assert_eq!(again, ImportCounts { added: 0, skipped: 2 });
        assert_eq!(db.bookmarks_all().unwrap().iter().filter(|b| b.kind == "folder").count(), folders_before);
        assert_eq!(db.bookmark_ids_for_url("https://b/").unwrap().len(), 1);
        // New entries in a later import are still picked up.
        let more = vec![ImportNode::Url { title: "c".into(), url: "https://c/".into(), created_at: 7 }];
        assert_eq!(db.bookmarks_import(OTHER_ID, "Imported", &more, 3).unwrap().added, 1);
    }
}
