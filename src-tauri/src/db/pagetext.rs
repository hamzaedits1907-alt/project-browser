//! Full-text history: the words of pages the user visited (normal tabs only, on this computer), so history can be
//! searched by what a page said, not only by its title and address. One row per address, the newest text wins; the
//! table never grows past `MAX_PAGES` and loses a page's text when that page leaves history.

use rusqlite::params;
use serde::Serialize;

use super::Db;
use crate::error::Result;

/// Most pages kept; the oldest go first.
pub const MAX_PAGES: i64 = 5000;
/// Most characters kept per page.
pub const MAX_CHARS: usize = 40_000;

/// Marks around matched words in a snippet (the shell turns them into highlights).
pub const MARK_OPEN: char = '\u{2}';
pub const MARK_CLOSE: char = '\u{3}';

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TextHit {
    pub url: String,
    pub title: String,
    pub snippet: String,
    pub visited_at: i64,
}

/// An FTS5 query for the user's words: every word must appear (as a word prefix). Anything that could be FTS5 syntax
/// is quoted away, so typing never causes a query error.
pub fn fts_query(text: &str) -> Option<String> {
    let words: Vec<String> =
        text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).take(12).map(|w| format!("\"{}\"*", w.replace('"', ""))).collect();
    (!words.is_empty()).then(|| words.join(" AND "))
}

impl Db {
    pub fn index_page_text(&self, url: &str, title: &str, body: &str, at: i64) -> Result<()> {
        let body: String = body.chars().take(MAX_CHARS).collect();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM page_text WHERE url = ?1", [url])?;
        tx.execute("INSERT INTO page_text (url, title, body, visited_at) VALUES (?1, ?2, ?3, ?4)", params![url, title, body, at])?;
        tx.execute(
            "DELETE FROM page_text WHERE rowid IN (SELECT rowid FROM page_text ORDER BY CAST(visited_at AS INTEGER) DESC LIMIT -1 OFFSET ?1)",
            [MAX_PAGES],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn search_page_text(&self, text: &str, limit: u32) -> Result<Vec<TextHit>> {
        let Some(q) = fts_query(text) else { return Ok(Vec::new()) };
        let mut stmt = self.conn.prepare(
            "SELECT url, title, snippet(page_text, 2, char(2), char(3), '…', 14), CAST(visited_at AS INTEGER)
             FROM page_text WHERE page_text MATCH ?1 ORDER BY rank LIMIT ?2",
        )?;
        let rows =
            stmt.query_map(params![q, limit], |r| Ok(TextHit { url: r.get(0)?, title: r.get(1)?, snippet: r.get(2)?, visited_at: r.get(3)? }))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Drops the text of pages that are no longer in history (after history was deleted).
    pub fn prune_page_text(&self) -> Result<()> {
        self.conn.execute("DELETE FROM page_text WHERE url NOT IN (SELECT url FROM urls)", [])?;
        Ok(())
    }

    pub fn clear_page_text(&self) -> Result<()> {
        self.conn.execute("DELETE FROM page_text", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_are_found_by_what_they_said() {
        let db = Db::open_in_memory().unwrap();
        db.index_page_text("https://a.test/", "Recipes", "A slow cooked tomato sauce with basil and garlic.", 1).unwrap();
        db.index_page_text("https://b.test/", "Cars", "The engine has four cylinders.", 2).unwrap();
        let hits = db.search_page_text("tomato bas", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, "https://a.test/");
        assert!(hits[0].snippet.contains(MARK_OPEN));
        // The newest text of an address replaces the old one.
        db.index_page_text("https://a.test/", "Recipes", "Now about bread.", 3).unwrap();
        assert!(db.search_page_text("tomato", 10).unwrap().is_empty());
        assert_eq!(db.search_page_text("bread", 10).unwrap().len(), 1);
    }

    #[test]
    fn odd_input_never_breaks_the_query() {
        let db = Db::open_in_memory().unwrap();
        db.index_page_text("https://a.test/", "T", "hello world", 1).unwrap();
        for q in ["\"", "AND OR NOT", "a*b", "(x", "-", "NEAR(", ""] {
            let _ = db.search_page_text(q, 5).unwrap();
        }
        assert_eq!(fts_query("  "), None);
        assert_eq!(fts_query("he\"llo wor").as_deref(), Some("\"he\"* AND \"llo\"* AND \"wor\"*"));
    }

    #[test]
    fn text_leaves_with_its_history() {
        let db = Db::open_in_memory().unwrap();
        db.index_page_text("https://gone.test/", "T", "unique words here", 1).unwrap();
        db.prune_page_text().unwrap();
        assert!(db.search_page_text("unique", 5).unwrap().is_empty());
    }
}
