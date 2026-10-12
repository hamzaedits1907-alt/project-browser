//! Storage for the credential vault. This layer only moves opaque `sealed` blobs in and out of
//! SQLite; sealing and opening them is `crate::vault`'s job, so nothing in here ever sees a
//! plaintext password.

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::Db;
use crate::error::Result;

/// One stored login with its sealed secrets.
#[derive(Debug, Clone)]
pub struct LoginRecord {
    pub id: i64,
    pub origin: String,
    pub display_url: String,
    pub username: String,
    pub sealed: Vec<u8>,
    pub note_sealed: Option<Vec<u8>>,
}

/// A login as the shell may see it: never the password, never the note.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LoginSummary {
    pub id: i64,
    pub origin: String,
    pub display_url: String,
    pub username: String,
    pub source: String,
    pub created_at: i64,
    pub last_used: Option<i64>,
}

/// An address profile with its sealed JSON.
#[derive(Debug, Clone)]
pub struct ProfileRecord {
    pub id: i64,
    pub source_id: String,
    pub sealed: Vec<u8>,
}

fn map_summary(r: &rusqlite::Row<'_>) -> rusqlite::Result<LoginSummary> {
    Ok(LoginSummary {
        id: r.get(0)?,
        origin: r.get(1)?,
        display_url: r.get(2)?,
        username: r.get(3)?,
        source: r.get(4)?,
        created_at: r.get(5)?,
        last_used: r.get(6)?,
    })
}

const SUMMARY_COLUMNS: &str = "id, origin, display_url, username, source, created_at, last_used";

impl Db {
    pub fn login_find(&self, origin: &str, username: &str) -> Result<Option<LoginRecord>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, origin, display_url, username, sealed, note_sealed FROM vault_logins WHERE origin = ?1 AND username = ?2",
                params![origin, username],
                map_record,
            )
            .optional()?)
    }

    pub fn login_get(&self, id: i64) -> Result<Option<LoginRecord>> {
        Ok(self
            .conn
            .query_row("SELECT id, origin, display_url, username, sealed, note_sealed FROM vault_logins WHERE id = ?1", [id], map_record)
            .optional()?)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn login_insert(
        &self,
        origin: &str,
        display_url: &str,
        username: &str,
        sealed: &[u8],
        note_sealed: Option<&[u8]>,
        source: &str,
        now: i64,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO vault_logins (origin, display_url, username, sealed, note_sealed, source, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![origin, display_url, username, sealed, note_sealed, source, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn login_replace_secret(&self, id: i64, display_url: &str, sealed: &[u8], note_sealed: Option<&[u8]>, now: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE vault_logins SET display_url = ?2, sealed = ?3, note_sealed = ?4, updated_at = ?5 WHERE id = ?1",
            params![id, display_url, sealed, note_sealed, now],
        )?;
        Ok(())
    }

    pub fn login_touch(&self, id: i64, now: i64) -> Result<()> {
        self.conn.execute("UPDATE vault_logins SET last_used = ?2 WHERE id = ?1", params![id, now])?;
        Ok(())
    }

    pub fn logins_list(&self) -> Result<Vec<LoginSummary>> {
        let mut stmt = self.conn.prepare_cached(&format!("SELECT {SUMMARY_COLUMNS} FROM vault_logins ORDER BY origin, username"))?;
        let rows = stmt.query_map([], map_summary)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn logins_for_origin(&self, origin: &str) -> Result<Vec<LoginSummary>> {
        let mut stmt =
            self.conn.prepare_cached(&format!("SELECT {SUMMARY_COLUMNS} FROM vault_logins WHERE origin = ?1 ORDER BY last_used DESC, username"))?;
        let rows = stmt.query_map([origin], map_summary)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn login_delete(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM vault_logins WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn logins_clear(&self) -> Result<usize> {
        Ok(self.conn.execute("DELETE FROM vault_logins", [])?)
    }

    pub fn profile_find_by_source(&self, source_id: &str) -> Result<Option<ProfileRecord>> {
        Ok(self.conn.query_row("SELECT id, source_id, sealed FROM vault_profiles WHERE source_id = ?1", [source_id], map_profile).optional()?)
    }

    pub fn profile_insert(&self, source_id: &str, sealed: &[u8], now: i64) -> Result<i64> {
        self.conn.execute("INSERT INTO vault_profiles (source_id, sealed, created_at) VALUES (?1, ?2, ?3)", params![source_id, sealed, now])?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn profile_get(&self, id: i64) -> Result<Option<ProfileRecord>> {
        Ok(self.conn.query_row("SELECT id, source_id, sealed FROM vault_profiles WHERE id = ?1", [id], map_profile).optional()?)
    }

    pub fn profiles_list(&self) -> Result<Vec<ProfileRecord>> {
        let mut stmt = self.conn.prepare_cached("SELECT id, source_id, sealed FROM vault_profiles ORDER BY id")?;
        let rows = stmt.query_map([], map_profile)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn profile_delete(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM vault_profiles WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn profiles_clear(&self) -> Result<usize> {
        Ok(self.conn.execute("DELETE FROM vault_profiles", [])?)
    }
}

fn map_record(r: &rusqlite::Row<'_>) -> rusqlite::Result<LoginRecord> {
    Ok(LoginRecord { id: r.get(0)?, origin: r.get(1)?, display_url: r.get(2)?, username: r.get(3)?, sealed: r.get(4)?, note_sealed: r.get(5)? })
}

fn map_profile(r: &rusqlite::Row<'_>) -> rusqlite::Result<ProfileRecord> {
    Ok(ProfileRecord { id: r.get(0)?, source_id: r.get(1)?, sealed: r.get(2)? })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logins_are_unique_per_site_and_user_and_summaries_carry_no_secret() {
        let db = Db::open_in_memory().unwrap();
        let id = db.login_insert("https://a.example", "https://a.example/login", "me", b"sealed-bytes", None, "test", 5).unwrap();
        assert!(db.login_insert("https://a.example", "https://a.example/", "me", b"x", None, "test", 6).is_err(), "same site+user is a duplicate");
        db.login_insert("https://a.example", "https://a.example/", "other", b"y", None, "test", 7).unwrap();
        let found = db.login_find("https://a.example", "me").unwrap().unwrap();
        assert_eq!((found.id, found.sealed.as_slice()), (id, &b"sealed-bytes"[..]));

        let summaries = db.logins_for_origin("https://a.example").unwrap();
        assert_eq!(summaries.len(), 2);
        let json = serde_json::to_string(&summaries).unwrap();
        assert!(!json.contains("sealed"), "{json}");

        db.login_touch(id, 99).unwrap();
        assert_eq!(db.logins_for_origin("https://a.example").unwrap()[0].last_used, Some(99), "recently used logins come first");
        db.login_delete(id).unwrap();
        assert_eq!(db.logins_list().unwrap().len(), 1);
        assert_eq!(db.logins_clear().unwrap(), 1);
    }

    #[test]
    fn profiles_dedupe_on_their_source_id() {
        let db = Db::open_in_memory().unwrap();
        db.profile_insert("guid-1", b"blob", 1).unwrap();
        assert!(db.profile_insert("guid-1", b"blob2", 2).is_err());
        assert!(db.profile_find_by_source("guid-1").unwrap().is_some());
        assert_eq!(db.profiles_list().unwrap().len(), 1);
        assert_eq!(db.profiles_clear().unwrap(), 1);
    }
}
