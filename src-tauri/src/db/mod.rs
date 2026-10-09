//! Per-profile SQLite database (`browser.db`).
//!
//! Owned exclusively by the controller thread, so a single connection is used
//! without locking. WAL mode keeps writes cheap and crash-safe.

pub mod bookmarks;
pub mod downloads;
pub mod history;
pub mod permissions;

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};

use crate::error::Result;

pub struct Db {
    pub(crate) conn: Connection,
}

/// Ordered list of schema migrations; index + 1 is the schema version.
const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE kv (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE urls (
  id          INTEGER PRIMARY KEY,
  url         TEXT NOT NULL UNIQUE,
  title       TEXT NOT NULL DEFAULT '',
  visit_count INTEGER NOT NULL DEFAULT 0,
  last_visit  INTEGER NOT NULL
);
CREATE INDEX urls_last_visit ON urls(last_visit DESC);

CREATE TABLE visits (
  id         INTEGER PRIMARY KEY,
  url_id     INTEGER NOT NULL REFERENCES urls(id) ON DELETE CASCADE,
  visited_at INTEGER NOT NULL
);
CREATE INDEX visits_time ON visits(visited_at DESC);
CREATE INDEX visits_url ON visits(url_id);

CREATE TABLE bookmarks (
  id         INTEGER PRIMARY KEY,
  parent_id  INTEGER REFERENCES bookmarks(id) ON DELETE CASCADE,
  kind       TEXT NOT NULL CHECK (kind IN ('folder', 'url')),
  title      TEXT NOT NULL,
  url        TEXT,
  position   INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE INDEX bookmarks_parent ON bookmarks(parent_id, position);
CREATE INDEX bookmarks_url ON bookmarks(url);
INSERT INTO bookmarks (id, parent_id, kind, title, url, position, created_at)
VALUES (1, NULL, 'folder', 'Bookmarks bar', NULL, 0, 0),
       (2, NULL, 'folder', 'Other bookmarks', NULL, 1, 0);

CREATE TABLE downloads (
  id             INTEGER PRIMARY KEY,
  url            TEXT NOT NULL,
  path           TEXT NOT NULL,
  filename       TEXT NOT NULL,
  mime           TEXT,
  total_bytes    INTEGER,
  received_bytes INTEGER NOT NULL DEFAULT 0,
  state          TEXT NOT NULL,
  danger         TEXT,
  error          TEXT,
  started_at     INTEGER NOT NULL,
  ended_at       INTEGER
);
CREATE INDEX downloads_started ON downloads(started_at DESC);

CREATE TABLE site_permissions (
  origin     TEXT NOT NULL,
  kind       TEXT NOT NULL,
  decision   TEXT NOT NULL CHECK (decision IN ('allow', 'block')),
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (origin, kind)
);
"#];

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Db> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Db> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(2))?;
        let mut db = Db { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&mut self) -> Result<()> {
        let version: usize = self.conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", i + 1)?;
            tx.commit()?;
            log::info!("database migrated to schema v{}", i + 1);
        }
        Ok(())
    }

    pub fn kv_get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let raw: Option<String> = self.conn.query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| r.get(0)).optional()?;
        Ok(match raw {
            Some(s) => match serde_json::from_str(&s) {
                Ok(v) => Some(v),
                Err(e) => {
                    log::warn!("ignoring unreadable kv entry {key}: {e}");
                    None
                }
            },
            None => None,
        })
    }

    pub fn kv_set<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        let s = serde_json::to_string(value)?;
        self.conn.execute("INSERT INTO kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value", params![key, s])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_idempotent_and_kv_roundtrips() {
        let db = Db::open_in_memory().unwrap();
        let v: usize = db.conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(v, MIGRATIONS.len());
        db.kv_set("k", &vec![1, 2, 3]).unwrap();
        assert_eq!(db.kv_get::<Vec<i32>>("k").unwrap(), Some(vec![1, 2, 3]));
        db.kv_set("k", &vec![4]).unwrap();
        assert_eq!(db.kv_get::<Vec<i32>>("k").unwrap(), Some(vec![4]));
        assert_eq!(db.kv_get::<Vec<i32>>("missing").unwrap(), None);
    }

    #[test]
    fn file_database_persists_across_reopen() {
        let dir = std::env::temp_dir().join(format!("pb-db-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("browser.db");
        {
            let db = Db::open(&path).unwrap();
            db.kv_set("session", &"saved").unwrap();
            db.record_visit("https://example.com/", "Example", 1_000).unwrap();
        }
        let db = Db::open(&path).unwrap();
        assert_eq!(db.kv_get::<String>("session").unwrap().as_deref(), Some("saved"));
        assert_eq!(db.history_query("", 10, None).unwrap().len(), 1);
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }
}
