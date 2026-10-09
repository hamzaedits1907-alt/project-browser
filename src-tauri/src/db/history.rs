use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::Db;
use crate::error::Result;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub visit_id: i64,
    pub url_id: i64,
    pub url: String,
    pub title: String,
    pub visited_at: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UrlRow {
    pub url: String,
    pub title: String,
    pub visit_count: i64,
    pub last_visit: i64,
}

/// Reloads and in-page navigations within this window don't create new visits.
const DEDUPE_WINDOW_MS: i64 = 30_000;

fn like_pattern(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    format!("%{escaped}%")
}

/// Only real web pages belong in history.
pub fn is_recordable(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://") || url.starts_with("file:")
}

impl Db {
    pub fn record_visit(&self, url: &str, title: &str, now: i64) -> Result<()> {
        if !is_recordable(url) {
            return Ok(());
        }
        let existing: Option<(i64, i64)> =
            self.conn.query_row("SELECT id, last_visit FROM urls WHERE url = ?1", [url], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
        match existing {
            Some((id, last)) if now - last < DEDUPE_WINDOW_MS => {
                if !title.is_empty() {
                    self.conn.execute("UPDATE urls SET title = ?2 WHERE id = ?1", params![id, title])?;
                }
            }
            Some((id, _)) => {
                self.conn.execute(
                    "UPDATE urls SET visit_count = visit_count + 1, last_visit = ?2,
                       title = CASE WHEN ?3 = '' THEN title ELSE ?3 END WHERE id = ?1",
                    params![id, now, title],
                )?;
                self.conn.execute("INSERT INTO visits (url_id, visited_at) VALUES (?1, ?2)", params![id, now])?;
            }
            None => {
                self.conn.execute("INSERT INTO urls (url, title, visit_count, last_visit) VALUES (?1, ?2, 1, ?3)", params![url, title, now])?;
                let id = self.conn.last_insert_rowid();
                self.conn.execute("INSERT INTO visits (url_id, visited_at) VALUES (?1, ?2)", params![id, now])?;
            }
        }
        Ok(())
    }

    pub fn update_title(&self, url: &str, title: &str) -> Result<()> {
        if !title.is_empty() {
            self.conn.execute("UPDATE urls SET title = ?2 WHERE url = ?1", params![url, title])?;
        }
        Ok(())
    }

    /// Visits newest-first, optionally filtered by text and paged by `before` (exclusive timestamp).
    pub fn history_query(&self, text: &str, limit: u32, before: Option<i64>) -> Result<Vec<HistoryEntry>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT v.id, u.id, u.url, u.title, v.visited_at
               FROM visits v JOIN urls u ON u.id = v.url_id
              WHERE (?1 = '' OR u.url LIKE ?2 ESCAPE '\\' OR u.title LIKE ?2 ESCAPE '\\')
                AND v.visited_at < ?3
              ORDER BY v.visited_at DESC
              LIMIT ?4",
        )?;
        let rows = stmt.query_map(params![text, like_pattern(text), before.unwrap_or(i64::MAX), limit], |r| {
            Ok(HistoryEntry { visit_id: r.get(0)?, url_id: r.get(1)?, url: r.get(2)?, title: r.get(3)?, visited_at: r.get(4)? })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn delete_visits(&self, visit_ids: &[i64]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for id in visit_ids {
            tx.execute("DELETE FROM visits WHERE id = ?1", [id])?;
        }
        // Drop urls that no longer have visits so they stop appearing in suggestions.
        tx.execute("DELETE FROM urls WHERE id NOT IN (SELECT DISTINCT url_id FROM visits)", [])?;
        tx.execute(
            "UPDATE urls SET visit_count = (SELECT COUNT(*) FROM visits WHERE url_id = urls.id),
                             last_visit = (SELECT MAX(visited_at) FROM visits WHERE url_id = urls.id)",
            [],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Removes all history recorded at or after `since` (Unix ms); `None` clears everything.
    pub fn clear_history(&self, since: Option<i64>) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        match since {
            None => {
                tx.execute("DELETE FROM visits", [])?;
                tx.execute("DELETE FROM urls", [])?;
            }
            Some(t) => {
                tx.execute("DELETE FROM visits WHERE visited_at >= ?1", [t])?;
                tx.execute("DELETE FROM urls WHERE id NOT IN (SELECT DISTINCT url_id FROM visits)", [])?;
                tx.execute(
                    "UPDATE urls SET visit_count = (SELECT COUNT(*) FROM visits WHERE url_id = urls.id),
                                     last_visit = (SELECT MAX(visited_at) FROM visits WHERE url_id = urls.id)",
                    [],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Most visited pages, one per host, weighted toward recent activity.
    pub fn top_sites(&self, limit: usize, now: i64) -> Result<Vec<UrlRow>> {
        let month_ago = now - 30 * 24 * 3600 * 1000;
        let mut stmt = self.conn.prepare_cached(
            "SELECT url, title, visit_count, last_visit FROM urls
              WHERE last_visit >= ?1
              ORDER BY visit_count * (CASE WHEN last_visit >= ?2 THEN 2 ELSE 1 END) DESC, last_visit DESC
              LIMIT 200",
        )?;
        let week_ago = now - 7 * 24 * 3600 * 1000;
        let rows: Vec<UrlRow> = stmt.query_map(params![month_ago, week_ago], map_url_row)?.collect::<std::result::Result<_, _>>()?;
        let mut seen = std::collections::HashSet::new();
        Ok(rows
            .into_iter()
            .filter(|r| {
                let host = crate::navigation::host_of(&r.url).unwrap_or_default();
                !host.is_empty() && seen.insert(host.trim_start_matches("www.").to_string())
            })
            .take(limit)
            .collect())
    }

    /// Most recently visited distinct pages ("continue browsing").
    pub fn recent_pages(&self, limit: u32) -> Result<Vec<UrlRow>> {
        let mut stmt = self.conn.prepare_cached("SELECT url, title, visit_count, last_visit FROM urls ORDER BY last_visit DESC LIMIT ?1")?;
        let rows = stmt.query_map([limit], map_url_row)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Omnibox suggestions from history: host-prefix matches rank first, then popularity.
    pub fn history_suggest(&self, text: &str, limit: u32) -> Result<Vec<UrlRow>> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(Vec::new());
        }
        let stripped = text.trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("www.");
        let prefix = format!("{}%", stripped.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        let mut stmt = self.conn.prepare_cached(
            "SELECT url, title, visit_count, last_visit,
                    (CASE WHEN url LIKE 'https://' || ?2 ESCAPE '\\' OR url LIKE 'http://' || ?2 ESCAPE '\\'
                            OR url LIKE 'https://www.' || ?2 ESCAPE '\\' OR url LIKE 'http://www.' || ?2 ESCAPE '\\'
                          THEN 1 ELSE 0 END) AS prefix_hit
               FROM urls
              WHERE url LIKE ?1 ESCAPE '\\' OR title LIKE ?1 ESCAPE '\\'
              ORDER BY prefix_hit DESC, visit_count DESC, last_visit DESC
              LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![like_pattern(text), prefix, limit], map_url_row)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Bulk import of url rows (browser migration), in one transaction.
    ///
    /// Idempotent: a row whose (url, last visit time) is already stored is skipped, so running the
    /// same import twice neither duplicates visits nor inflates visit counts.
    pub fn import_urls(&self, rows: &[UrlRow]) -> Result<ImportCounts> {
        let tx = self.conn.unchecked_transaction()?;
        let mut counts = ImportCounts::default();
        for r in rows {
            if !is_recordable(&r.url) {
                counts.skipped += 1;
                continue;
            }
            let existing: Option<i64> = tx.query_row("SELECT id FROM urls WHERE url = ?1", [&r.url], |row| row.get(0)).optional()?;
            let id = match existing {
                None => {
                    tx.execute(
                        "INSERT INTO urls (url, title, visit_count, last_visit) VALUES (?1, ?2, ?3, ?4)",
                        params![r.url, r.title, r.visit_count.max(1), r.last_visit],
                    )?;
                    tx.last_insert_rowid()
                }
                Some(id) => {
                    let seen: Option<i64> = tx
                        .query_row("SELECT id FROM visits WHERE url_id = ?1 AND visited_at = ?2", params![id, r.last_visit], |row| row.get(0))
                        .optional()?;
                    if seen.is_some() {
                        counts.skipped += 1;
                        continue;
                    }
                    tx.execute(
                        "UPDATE urls SET visit_count = visit_count + ?2, last_visit = MAX(last_visit, ?3),
                           title = CASE WHEN title = '' THEN ?4 ELSE title END WHERE id = ?1",
                        params![id, r.visit_count.max(1), r.last_visit, r.title],
                    )?;
                    id
                }
            };
            // One visit row so imported pages show up in the history page.
            tx.execute("INSERT INTO visits (url_id, visited_at) VALUES (?1, ?2)", params![id, r.last_visit])?;
            counts.added += 1;
        }
        tx.commit()?;
        Ok(counts)
    }
}

/// Outcome of a bulk import into the database.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportCounts {
    pub added: usize,
    /// Rows that were not stored: already present, or not a web address.
    pub skipped: usize,
}

fn map_url_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<UrlRow> {
    Ok(UrlRow { url: r.get(0)?, title: r.get(1)?, visit_count: r.get(2)?, last_visit: r.get(3)? })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visits_are_recorded_and_deduped() {
        let db = Db::open_in_memory().unwrap();
        db.record_visit("https://a.com/", "A", 1_000).unwrap();
        db.record_visit("https://a.com/", "A2", 2_000).unwrap(); // within dedupe window
        db.record_visit("https://a.com/", "", 100_000).unwrap();
        db.record_visit("browser://settings", "S", 100_000).unwrap(); // ignored
        let h = db.history_query("", 10, None).unwrap();
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].title, "A2", "title kept when the new one is empty");
        let top = db.top_sites(10, 100_000).unwrap();
        assert_eq!(top[0].visit_count, 2);
    }

    #[test]
    fn search_paging_and_like_escaping() {
        let db = Db::open_in_memory().unwrap();
        db.record_visit("https://rust-lang.org/", "Rust", 1).unwrap();
        db.record_visit("https://example.com/100%25", "Percent", 100_000).unwrap();
        db.record_visit("https://example.com/b", "B", 200_000).unwrap();
        assert_eq!(db.history_query("rust", 10, None).unwrap().len(), 1);
        assert_eq!(db.history_query("%", 10, None).unwrap().len(), 1, "literal percent");
        let page = db.history_query("", 1, Some(200_000)).unwrap();
        assert_eq!(page[0].url, "https://example.com/100%25");
    }

    #[test]
    fn deleting_and_clearing() {
        let db = Db::open_in_memory().unwrap();
        db.record_visit("https://a.com/", "A", 1).unwrap();
        db.record_visit("https://b.com/", "B", 50_000).unwrap();
        db.record_visit("https://b.com/", "B", 100_000).unwrap();
        let h = db.history_query("", 10, None).unwrap();
        db.delete_visits(&[h[0].visit_id]).unwrap();
        let top = db.top_sites(10, 100_000).unwrap();
        let b = top.iter().find(|r| r.url == "https://b.com/").unwrap();
        assert_eq!(b.visit_count, 1);
        db.clear_history(Some(40_000)).unwrap();
        assert_eq!(db.history_query("", 10, None).unwrap().len(), 1);
        db.clear_history(None).unwrap();
        assert!(db.history_query("", 10, None).unwrap().is_empty());
        assert!(db.history_suggest("a", 5).unwrap().is_empty());
    }

    #[test]
    fn suggestions_prefer_host_prefix() {
        let db = Db::open_in_memory().unwrap();
        for t in 0..5 {
            db.record_visit("https://blog.example.com/github-tips", "Tips about github", t * 60_000).unwrap();
        }
        db.record_visit("https://github.com/", "GitHub", 1).unwrap();
        let s = db.history_suggest("git", 5).unwrap();
        assert_eq!(s[0].url, "https://github.com/");
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn top_sites_one_per_host() {
        let db = Db::open_in_memory().unwrap();
        db.record_visit("https://www.a.com/1", "", 1).unwrap();
        db.record_visit("https://a.com/2", "", 1).unwrap();
        db.record_visit("https://b.com/", "", 1).unwrap();
        assert_eq!(db.top_sites(10, 1).unwrap().len(), 2);
    }

    #[test]
    fn import_merges_with_existing() {
        let db = Db::open_in_memory().unwrap();
        db.record_visit("https://a.com/", "A", 5).unwrap();
        let rows = vec![
            UrlRow { url: "https://a.com/".into(), title: "Other".into(), visit_count: 3, last_visit: 10 },
            UrlRow { url: "https://b.com/".into(), title: "B".into(), visit_count: 2, last_visit: 7 },
            UrlRow { url: "chrome://settings".into(), title: "".into(), visit_count: 1, last_visit: 7 },
        ];
        assert_eq!(db.import_urls(&rows).unwrap(), ImportCounts { added: 2, skipped: 1 });
        let top = db.top_sites(10, 10).unwrap();
        let a = top.iter().find(|r| r.url == "https://a.com/").unwrap();
        assert_eq!((a.visit_count, a.title.as_str()), (4, "A"));
        // Importing twice neither duplicates visits nor inflates visit counts.
        assert_eq!(db.import_urls(&rows).unwrap(), ImportCounts { added: 0, skipped: 3 });
        let top = db.top_sites(10, 10).unwrap();
        let a = top.iter().find(|r| r.url == "https://a.com/").unwrap();
        assert_eq!(a.visit_count, 4);
        let visits: i64 = db.conn.query_row("SELECT COUNT(*) FROM visits", [], |r| r.get(0)).unwrap();
        assert_eq!(visits, 3, "one recorded visit plus two imported");
    }
}
