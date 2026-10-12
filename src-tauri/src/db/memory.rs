//! Project Memory: what the user chose to keep.
//!
//! Notes they wrote, pages they asked to have summarised and saved, research results they decided to keep, and
//! the sources and links between them. Nothing is added automatically: every record is created by an explicit
//! Save in the shell, and the user can search, edit and delete all of it. Nothing here is ever sent to the AI
//! provider unless the user puts it into a request themselves.
//!
//! Stored as plain text in the profile database (it is the user's own notes, not credentials). Searching is done
//! in Rust rather than with SQL `LIKE` so that case-insensitive matching works for every language, not just ASCII.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::Db;
use crate::error::{Error, Result};

pub const MAX_TITLE: usize = 200;
pub const MAX_BODY: usize = 50_000;
const MAX_TAGS: usize = 20;
const MAX_TAG_LEN: usize = 40;
const MAX_SOURCES: usize = 30;
const MAX_RELATED: usize = 50;
const MAX_URL: usize = 2000;
const MAX_TERMS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum MemoryKind {
    /// Written by the user.
    #[default]
    Note,
    /// A page summary the user chose to keep.
    Page,
    /// A comparison or claim check the user chose to keep.
    Research,
    /// A group of pages the user saved together.
    Session,
}

impl MemoryKind {
    fn column_value(self) -> &'static str {
        match self {
            MemoryKind::Note => "note",
            MemoryKind::Page => "page",
            MemoryKind::Research => "research",
            MemoryKind::Session => "session",
        }
    }

    fn parse(s: &str) -> MemoryKind {
        match s {
            "page" => MemoryKind::Page,
            "research" => MemoryKind::Research,
            "session" => MemoryKind::Session,
            _ => MemoryKind::Note,
        }
    }
}

/// A web page a record came from. Only the address and the title the page gave itself are kept, never its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySource {
    pub url: String,
    #[serde(default)]
    pub title: String,
}

/// What the shell sends to create or replace a record.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MemoryDraft {
    pub kind: MemoryKind,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub sources: Vec<MemorySource>,
    /// Ids of other records this one is linked to (a topic association chosen by the user).
    pub related: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryItem {
    pub id: i64,
    pub kind: MemoryKind,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub sources: Vec<MemorySource>,
    pub related: Vec<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

impl MemoryDraft {
    /// Trims and bounds everything the shell sent. Rejects only what cannot be saved sensibly (nothing to
    /// remember, or a body larger than the limit); everything else is quietly tidied.
    pub fn cleaned(self) -> Result<MemoryDraft> {
        let body = self.body.trim().to_string();
        if body.chars().count() > MAX_BODY {
            return Err(Error::msg(format!("That is too long to keep as one record (the limit is {MAX_BODY} characters). Shorten it or split it.")));
        }
        let mut title = self.title.trim().to_string();
        if title.is_empty() {
            title = body.lines().map(str::trim).find(|l| !l.is_empty()).map(|l| truncate(l, 80)).unwrap_or_default();
        }
        if title.is_empty() {
            return Err(Error::msg("Write something to remember first."));
        }
        let title = truncate(&title, MAX_TITLE);

        let mut tags: Vec<String> = Vec::new();
        for t in &self.tags {
            let t = truncate(t.trim().trim_start_matches('#').trim(), MAX_TAG_LEN).to_lowercase();
            if !t.is_empty() && !tags.contains(&t) {
                tags.push(t);
            }
        }
        tags.truncate(MAX_TAGS);

        let mut sources: Vec<MemorySource> = Vec::new();
        for s in &self.sources {
            let url = s.url.trim();
            let web = url.len() <= MAX_URL && url::Url::parse(url).is_ok_and(|u| matches!(u.scheme(), "http" | "https"));
            if web && !sources.iter().any(|x| x.url == url) {
                sources.push(MemorySource { url: url.to_string(), title: truncate(s.title.trim(), MAX_TITLE) });
            }
        }
        sources.truncate(MAX_SOURCES);

        let mut related: Vec<i64> = Vec::new();
        for r in &self.related {
            if !related.contains(r) {
                related.push(*r);
            }
        }
        related.truncate(MAX_RELATED);

        Ok(MemoryDraft { kind: self.kind, title, body, tags, sources, related })
    }
}

/// How well a record answers a search. `None` when some word is not found anywhere in it. Title matches count
/// most, then tags, then the text and sources.
pub fn match_score(item: &MemoryItem, query: &str) -> Option<u32> {
    let terms: Vec<String> = query.split_whitespace().take(MAX_TERMS).map(str::to_lowercase).collect();
    if terms.is_empty() {
        return Some(0);
    }
    let title = item.title.to_lowercase();
    let body = item.body.to_lowercase();
    let tags = item.tags.join(" ").to_lowercase();
    let sources = item.sources.iter().map(|s| format!("{} {}", s.title, s.url)).collect::<Vec<_>>().join(" ").to_lowercase();
    let mut score = 0;
    for term in &terms {
        let mut best = 0;
        if title.contains(term) {
            best = best.max(8);
        }
        if tags.contains(term) {
            best = best.max(5);
        }
        if body.contains(term) {
            best = best.max(2);
        }
        if sources.contains(term) {
            best = best.max(2);
        }
        if best == 0 {
            return None;
        }
        score += best;
    }
    Some(score)
}

fn map_item(r: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryItem> {
    let kind: String = r.get(1)?;
    let tags: String = r.get(4)?;
    let sources: String = r.get(5)?;
    let related: String = r.get(6)?;
    Ok(MemoryItem {
        id: r.get(0)?,
        kind: MemoryKind::parse(&kind),
        title: r.get(2)?,
        body: r.get(3)?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        sources: serde_json::from_str(&sources).unwrap_or_default(),
        related: serde_json::from_str(&related).unwrap_or_default(),
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
    })
}

const COLUMNS: &str = "id, kind, title, body, tags_json, sources_json, related_json, created_at, updated_at";

impl Db {
    pub fn memory_get(&self, id: i64) -> Result<Option<MemoryItem>> {
        let mut stmt = self.conn.prepare(&format!("SELECT {COLUMNS} FROM memory_items WHERE id = ?1"))?;
        let mut rows = stmt.query_map([id], map_item)?;
        Ok(rows.next().transpose()?)
    }

    /// Keeps only the ids of records that exist (and that are not the record itself).
    fn memory_existing(&self, ids: &[i64], except: Option<i64>) -> Result<Vec<i64>> {
        let mut out = Vec::new();
        for id in ids {
            if Some(*id) != except && self.memory_get(*id)?.is_some() {
                out.push(*id);
            }
        }
        Ok(out)
    }

    /// Creates a record and returns it. The draft is cleaned first.
    pub fn memory_create(&self, draft: MemoryDraft, now: i64) -> Result<MemoryItem> {
        let d = draft.cleaned()?;
        let related = self.memory_existing(&d.related, None)?;
        self.conn.execute(
            "INSERT INTO memory_items (kind, title, body, tags_json, sources_json, related_json, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![
                d.kind.column_value(),
                d.title,
                d.body,
                serde_json::to_string(&d.tags)?,
                serde_json::to_string(&d.sources)?,
                serde_json::to_string(&related)?,
                now
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        self.memory_get(id)?.ok_or_else(|| Error::msg("The record could not be read back after saving."))
    }

    /// Replaces a record's content (its creation time is kept) and returns it.
    pub fn memory_replace(&self, id: i64, draft: MemoryDraft, now: i64) -> Result<MemoryItem> {
        let d = draft.cleaned()?;
        let related = self.memory_existing(&d.related, Some(id))?;
        let changed = self.conn.execute(
            "UPDATE memory_items SET kind = ?1, title = ?2, body = ?3, tags_json = ?4, sources_json = ?5, related_json = ?6, updated_at = ?7 WHERE id = ?8",
            params![d.kind.column_value(), d.title, d.body, serde_json::to_string(&d.tags)?, serde_json::to_string(&d.sources)?, serde_json::to_string(&related)?, now, id],
        )?;
        if changed == 0 {
            return Err(Error::msg("That record no longer exists."));
        }
        self.memory_get(id)?.ok_or_else(|| Error::msg("The record could not be read back after saving."))
    }

    /// Records matching `query` (every word must be found), best match first; with no query, newest first.
    pub fn memory_search(&self, query: &str, limit: usize) -> Result<Vec<MemoryItem>> {
        let mut stmt = self.conn.prepare(&format!("SELECT {COLUMNS} FROM memory_items ORDER BY updated_at DESC, id DESC"))?;
        let all: Vec<MemoryItem> = stmt.query_map([], map_item)?.collect::<rusqlite::Result<_>>()?;
        if query.trim().is_empty() {
            return Ok(all.into_iter().take(limit).collect());
        }
        let mut scored: Vec<(u32, MemoryItem)> = all.into_iter().filter_map(|i| match_score(&i, query).map(|s| (s, i))).collect();
        // `all` is already newest first and the sort is stable, so equal scores stay newest first.
        scored.sort_by_key(|(s, _)| std::cmp::Reverse(*s));
        Ok(scored.into_iter().take(limit).map(|(_, i)| i).collect())
    }

    /// Deletes a record and removes links that pointed at it. `false` if it did not exist.
    pub fn memory_delete(&self, id: i64) -> Result<bool> {
        let removed = self.conn.execute("DELETE FROM memory_items WHERE id = ?1", [id])?;
        if removed > 0 {
            let mut stmt = self.conn.prepare("SELECT id, related_json FROM memory_items WHERE related_json != '[]'")?;
            let rows: Vec<(i64, String)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
            for (other, json) in rows {
                let links: Vec<i64> = serde_json::from_str(&json).unwrap_or_default();
                if links.contains(&id) {
                    let kept: Vec<i64> = links.into_iter().filter(|l| *l != id).collect();
                    self.conn.execute("UPDATE memory_items SET related_json = ?1 WHERE id = ?2", params![serde_json::to_string(&kept)?, other])?;
                }
            }
        }
        Ok(removed > 0)
    }

    /// Deletes every record. Returns how many there were.
    pub fn memory_clear(&self) -> Result<usize> {
        Ok(self.conn.execute("DELETE FROM memory_items", [])?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(title: &str, body: &str) -> MemoryDraft {
        MemoryDraft { title: title.into(), body: body.into(), ..Default::default() }
    }

    #[test]
    fn a_saved_record_can_be_read_edited_and_deleted() {
        let db = Db::open_in_memory().unwrap();
        let a = db.memory_create(note("Rust notes", "Ownership and borrowing"), 100).unwrap();
        assert_eq!((a.kind, a.title.as_str(), a.created_at, a.updated_at), (MemoryKind::Note, "Rust notes", 100, 100));
        let b = db.memory_replace(a.id, note("Rust notes v2", "Lifetimes"), 200).unwrap();
        assert_eq!((b.id, b.title.as_str(), b.body.as_str(), b.created_at, b.updated_at), (a.id, "Rust notes v2", "Lifetimes", 100, 200));
        assert!(db.memory_replace(999, note("x", "y"), 1).is_err(), "editing something that is gone fails");
        assert!(db.memory_delete(a.id).unwrap());
        assert!(!db.memory_delete(a.id).unwrap());
        assert!(db.memory_get(a.id).unwrap().is_none());
    }

    #[test]
    fn nothing_is_saved_without_content_and_the_title_is_derived_when_missing() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.memory_create(note("  ", "  "), 1).is_err());
        let item = db.memory_create(note("", "\n  First line here\nsecond"), 1).unwrap();
        assert_eq!(item.title, "First line here");
        let long = "x".repeat(MAX_BODY + 1);
        assert!(db.memory_create(note("t", &long), 1).is_err());
        assert_eq!(db.memory_search("", 10).unwrap().len(), 1, "the rejected records were not stored");
    }

    #[test]
    fn tags_and_sources_are_tidied_and_only_web_addresses_are_kept() {
        let db = Db::open_in_memory().unwrap();
        let draft = MemoryDraft {
            title: "t".into(),
            tags: vec!["  #Rust ".into(), "rust".into(), "".into(), "GPU".into()],
            sources: vec![
                MemorySource { url: "https://example.com/a".into(), title: " A ".into() },
                MemorySource { url: "https://example.com/a".into(), title: "dup".into() },
                MemorySource { url: "javascript:alert(1)".into(), title: "bad".into() },
                MemorySource { url: "file:///C:/secret.txt".into(), title: "bad".into() },
                MemorySource { url: "not a url".into(), title: "bad".into() },
            ],
            ..Default::default()
        };
        let item = db.memory_create(draft, 1).unwrap();
        assert_eq!(item.tags, vec!["rust".to_string(), "gpu".to_string()]);
        assert_eq!(item.sources, vec![MemorySource { url: "https://example.com/a".into(), title: "A".into() }]);
    }

    #[test]
    fn search_needs_every_word_ranks_titles_first_and_ignores_case() {
        let db = Db::open_in_memory().unwrap();
        db.memory_create(note("Graphics cards", "RTX comparison and benchmarks"), 1).unwrap();
        db.memory_create(note("Shopping", "buy a graphics card later; RTX is pricey"), 2).unwrap();
        db.memory_create(note("Unrelated", "bread recipe"), 3).unwrap();
        let hits = db.memory_search("RTX GRAPHICS", 10).unwrap();
        assert_eq!(hits.iter().map(|h| h.title.as_str()).collect::<Vec<_>>(), vec!["Graphics cards", "Shopping"]);
        assert!(db.memory_search("graphics bread", 10).unwrap().is_empty(), "all words must match");
        assert_eq!(db.memory_search("", 2).unwrap().iter().map(|h| h.title.as_str()).collect::<Vec<_>>(), vec!["Unrelated", "Shopping"]);
        db.memory_create(note("Ärger mit Übersicht", "größer"), 4).unwrap();
        assert_eq!(db.memory_search("ÜBERSICHT", 10).unwrap().len(), 1, "matching is not limited to ASCII");
    }

    #[test]
    fn search_finds_tags_and_sources_and_treats_sql_wildcards_literally() {
        let db = Db::open_in_memory().unwrap();
        let draft = MemoryDraft {
            title: "Site".into(),
            tags: vec!["research".into()],
            sources: vec![MemorySource { url: "https://docs.example.org/guide".into(), title: "Guide".into() }],
            ..Default::default()
        };
        db.memory_create(draft, 1).unwrap();
        assert_eq!(db.memory_search("research", 10).unwrap().len(), 1);
        assert_eq!(db.memory_search("docs.example.org", 10).unwrap().len(), 1);
        assert!(db.memory_search("%", 10).unwrap().is_empty());
        assert!(db.memory_search("'; DROP TABLE memory_items; --", 10).unwrap().is_empty());
        assert_eq!(db.memory_search("", 10).unwrap().len(), 1, "the table is intact");
    }

    #[test]
    fn links_between_records_survive_until_one_is_deleted() {
        let db = Db::open_in_memory().unwrap();
        let a = db.memory_create(note("A", "a"), 1).unwrap();
        let b = db.memory_create(MemoryDraft { related: vec![a.id, 4242, a.id], ..note("B", "b") }, 2).unwrap();
        assert_eq!(b.related, vec![a.id], "unknown ids and repeats are dropped");
        let a2 = db.memory_replace(a.id, MemoryDraft { related: vec![a.id, b.id], ..note("A", "a") }, 3).unwrap();
        assert_eq!(a2.related, vec![b.id], "a record cannot link to itself");
        db.memory_delete(a.id).unwrap();
        assert!(db.memory_get(b.id).unwrap().unwrap().related.is_empty(), "links to a deleted record are removed");
    }

    #[test]
    fn clearing_removes_everything() {
        let db = Db::open_in_memory().unwrap();
        db.memory_create(note("a", "a"), 1).unwrap();
        db.memory_create(note("b", "b"), 2).unwrap();
        assert_eq!(db.memory_clear().unwrap(), 2);
        assert!(db.memory_search("", 10).unwrap().is_empty());
    }

    #[test]
    fn records_persist_in_a_file_database() {
        let dir = std::env::temp_dir().join(format!("pb-memory-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("browser.db");
        {
            let db = Db::open(&path).unwrap();
            db.memory_create(note("Kept", "across restarts"), 5).unwrap();
        }
        let db = Db::open(&path).unwrap();
        assert_eq!(db.memory_search("restarts", 10).unwrap().len(), 1);
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }
}
