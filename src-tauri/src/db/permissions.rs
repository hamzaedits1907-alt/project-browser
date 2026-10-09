use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::Db;
use crate::error::Result;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SitePermission {
    pub origin: String,
    pub kind: String,
    pub decision: String,
    pub updated_at: i64,
}

impl Db {
    /// Stored decision for an origin: `Some(true)` allow, `Some(false)` block, `None` ask.
    pub fn permission_get(&self, origin: &str, kind: &str) -> Result<Option<bool>> {
        let d: Option<String> = self
            .conn
            .query_row("SELECT decision FROM site_permissions WHERE origin = ?1 AND kind = ?2", params![origin, kind], |r| r.get(0))
            .optional()?;
        Ok(d.map(|d| d == "allow"))
    }

    pub fn permission_set(&self, origin: &str, kind: &str, allow: bool, now: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO site_permissions (origin, kind, decision, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(origin, kind) DO UPDATE SET decision = excluded.decision, updated_at = excluded.updated_at",
            params![origin, kind, if allow { "allow" } else { "block" }, now],
        )?;
        Ok(())
    }

    pub fn permission_delete(&self, origin: &str, kind: &str) -> Result<()> {
        self.conn.execute("DELETE FROM site_permissions WHERE origin = ?1 AND kind = ?2", params![origin, kind])?;
        Ok(())
    }

    pub fn permissions_list(&self) -> Result<Vec<SitePermission>> {
        let mut stmt = self.conn.prepare_cached("SELECT origin, kind, decision, updated_at FROM site_permissions ORDER BY origin, kind")?;
        let rows = stmt.query_map([], |r| Ok(SitePermission { origin: r.get(0)?, kind: r.get(1)?, decision: r.get(2)?, updated_at: r.get(3)? }))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decisions_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.permission_get("https://a", "camera").unwrap(), None);
        db.permission_set("https://a", "camera", true, 1).unwrap();
        assert_eq!(db.permission_get("https://a", "camera").unwrap(), Some(true));
        db.permission_set("https://a", "camera", false, 2).unwrap();
        assert_eq!(db.permission_get("https://a", "camera").unwrap(), Some(false));
        assert_eq!(db.permissions_list().unwrap().len(), 1);
        db.permission_delete("https://a", "camera").unwrap();
        assert_eq!(db.permission_get("https://a", "camera").unwrap(), None);
    }
}
