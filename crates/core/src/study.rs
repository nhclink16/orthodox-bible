//! Local study data: highlights, notes, bookmarks and settings, in one SQLite file.
//!
//! Everything is keyed by canonical verse reference (not by translation), so a highlight made
//! while reading the KJV shows up in the WEB too. The tables are mirrored in memory — they hold
//! hundreds of rows, not millions — so the reader can look them up per verse every frame; writes go
//! straight through to SQLite (WAL mode, a few hundred microseconds each).
//!
//! Every write also logs the record's latest state in `changes` (one row per record), which is
//! what [`crate::sync`] sends to the user's other devices.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::books::{BookId, ChapterIdx, VerseRef};
use crate::sync::Change;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HighlightColor {
    Yellow,
    Green,
    Blue,
    Pink,
    Purple,
    Orange,
}

impl HighlightColor {
    /// In key order: 1–6 apply these, 0 clears. The stored value is the index, so new colours
    /// only ever go at the end.
    pub const ALL: [HighlightColor; 6] = [
        HighlightColor::Yellow,
        HighlightColor::Green,
        HighlightColor::Blue,
        HighlightColor::Pink,
        HighlightColor::Purple,
        HighlightColor::Orange,
    ];

    pub fn name(self) -> &'static str {
        match self {
            HighlightColor::Yellow => "yellow",
            HighlightColor::Green => "green",
            HighlightColor::Blue => "blue",
            HighlightColor::Pink => "pink",
            HighlightColor::Purple => "purple",
            HighlightColor::Orange => "orange",
        }
    }

    fn from_i64(v: i64) -> Option<Self> {
        Self::ALL.get(usize::try_from(v).ok()?).copied()
    }

    fn to_i64(self) -> i64 {
        Self::ALL.iter().position(|c| *c == self).unwrap() as i64
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub body: String,
    pub created: i64,
    pub updated: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bookmark {
    pub label: String,
    pub created: i64,
}

pub struct StudyStore {
    conn: Connection,
    highlights: HashMap<u32, (HighlightColor, i64)>,
    notes: HashMap<u32, Note>,
    bookmarks: BTreeMap<u32, Bookmark>,
    generation: u64,
    /// This device's id in `changes`, kept in the settings.
    device: String,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS highlights(
    book TEXT NOT NULL, chapter INTEGER NOT NULL, verse INTEGER NOT NULL,
    color INTEGER NOT NULL, created INTEGER NOT NULL,
    PRIMARY KEY(book, chapter, verse));
CREATE TABLE IF NOT EXISTS notes(
    book TEXT NOT NULL, chapter INTEGER NOT NULL, verse INTEGER NOT NULL,
    body TEXT NOT NULL, created INTEGER NOT NULL, updated INTEGER NOT NULL,
    PRIMARY KEY(book, chapter, verse));
CREATE TABLE IF NOT EXISTS bookmarks(
    book TEXT NOT NULL, chapter INTEGER NOT NULL, verse INTEGER NOT NULL,
    label TEXT NOT NULL, created INTEGER NOT NULL,
    PRIMARY KEY(book, chapter, verse));
CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS reading(
    started INTEGER PRIMARY KEY, book TEXT NOT NULL, chapter INTEGER NOT NULL,
    translation TEXT NOT NULL, seconds INTEGER NOT NULL, furthest INTEGER NOT NULL,
    verses INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS plans(
    id TEXT PRIMARY KEY, started INTEGER NOT NULL, finished INTEGER);
CREATE TABLE IF NOT EXISTS plan_reads(
    plan TEXT NOT NULL, day INTEGER NOT NULL, passage INTEGER NOT NULL, at INTEGER NOT NULL,
    PRIMARY KEY(plan, day, passage));
CREATE TABLE IF NOT EXISTS rule_done(
    day INTEGER NOT NULL, item TEXT NOT NULL, at INTEGER NOT NULL,
    PRIMARY KEY(day, item));
CREATE TABLE IF NOT EXISTS changes(
    key TEXT PRIMARY KEY, value TEXT, at INTEGER NOT NULL, device TEXT NOT NULL);
";

/// A stretch of reading one chapter: when it started, for how long, and how far it got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadingSession {
    /// Unix seconds; also the session's identity.
    pub started: i64,
    pub book: BookId,
    pub chapter: u8,
    pub translation: String,
    /// Time actually spent reading (idle time is not counted).
    pub seconds: u32,
    /// The furthest verse reached, and how many verses the chapter has.
    pub furthest: u8,
    pub verses: u8,
}

impl ReadingSession {
    pub fn chapter_idx(&self) -> Option<ChapterIdx> {
        ChapterIdx::new(self.book, self.chapter)
    }

    /// Reached the chapter's last verse.
    pub fn finished(&self) -> bool {
        self.verses > 0 && self.furthest >= self.verses
    }

    /// How much of the chapter was read, 0–1.
    pub fn progress(&self) -> f32 {
        if self.verses == 0 { 0.0 } else { (self.furthest as f32 / self.verses as f32).min(1.0) }
    }
}

/// A reading plan under way (or finished): which, and when it began (the start of that day).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanProgress {
    pub id: String,
    pub started: i64,
    pub finished: Option<i64>,
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn verse_from_row(book: &str, chapter: i64, verse: i64) -> Option<VerseRef> {
    let book = BookId::from_code(book)?;
    Some(VerseRef::new(book, u8::try_from(chapter).ok()?, u8::try_from(verse).ok()?))
}

/// A record that syncs, as named by its key in `changes`. Verses are written `JHN.3.16` (the
/// book code the tables use, which stays the same when books are added).
#[derive(Debug)]
enum Record {
    Highlight(VerseRef),
    Note(VerseRef),
    Bookmark(VerseRef),
    Plan(String),
    /// Plan, day and passage.
    PlanRead(String, u16, u8),
    /// Day and item.
    RuleDone(i64, String),
    /// A reading session, by when it started.
    Reading(i64),
    /// The prayer rule (the only setting that syncs; the rest are per device).
    Rule,
}

impl Record {
    fn key(&self) -> String {
        let verse = |v: &VerseRef| format!("{}.{}.{}", v.book.info().code, v.chapter, v.verse);
        match self {
            Record::Highlight(v) => format!("highlight:{}", verse(v)),
            Record::Note(v) => format!("note:{}", verse(v)),
            Record::Bookmark(v) => format!("bookmark:{}", verse(v)),
            Record::Plan(id) => format!("plan:{id}"),
            Record::PlanRead(plan, day, passage) => format!("plan-read:{plan}:{day}:{passage}"),
            Record::RuleDone(day, item) => format!("rule-done:{day}:{item}"),
            Record::Reading(started) => format!("reading:{started}"),
            Record::Rule => "setting:rule".into(),
        }
    }

    fn parse(key: &str) -> Option<Record> {
        let verse = |s: &str| {
            let mut parts = s.split('.');
            let (book, chapter, verse) = (parts.next()?, parts.next()?, parts.next()?);
            if parts.next().is_some() {
                return None;
            }
            Some(VerseRef::new(BookId::from_code(book)?, chapter.parse().ok()?, verse.parse().ok()?))
        };
        let (kind, rest) = key.split_once(':')?;
        Some(match kind {
            "highlight" => Record::Highlight(verse(rest)?),
            "note" => Record::Note(verse(rest)?),
            "bookmark" => Record::Bookmark(verse(rest)?),
            "plan" => Record::Plan(rest.to_string()),
            "plan-read" => {
                // From the right, so a plan id may contain colons.
                let mut parts = rest.rsplitn(3, ':');
                let (passage, day, plan) = (parts.next()?, parts.next()?, parts.next()?);
                Record::PlanRead(plan.to_string(), day.parse().ok()?, passage.parse().ok()?)
            }
            "rule-done" => {
                let (day, item) = rest.split_once(':')?;
                Record::RuleDone(day.parse().ok()?, item.to_string())
            }
            "reading" => Record::Reading(rest.parse().ok()?),
            "setting" if rest == "rule" => Record::Rule,
            _ => return None,
        })
    }
}

impl StudyStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(SCHEMA)?;
        let mut s = Self {
            conn,
            highlights: HashMap::new(),
            notes: HashMap::new(),
            bookmarks: BTreeMap::new(),
            generation: 1,
            device: String::new(),
        };
        s.device = match s.setting("device-id") {
            Some(id) => id,
            None => {
                let id = crate::online::random_id();
                s.set_setting("device-id", &id)?;
                id
            }
        };
        s.backfill_changes()?;
        s.reload()?;
        Ok(s)
    }

    fn reload(&mut self) -> Result<()> {
        self.highlights.clear();
        self.notes.clear();
        self.bookmarks.clear();
        let mut st = self.conn.prepare("SELECT book, chapter, verse, color, created FROM highlights")?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get(4)?,
            ))
        })?;
        for row in rows {
            let (b, c, v, color, created) = row?;
            if let (Some(vr), Some(color)) = (verse_from_row(&b, c, v), HighlightColor::from_i64(color)) {
                self.highlights.insert(vr.key(), (color, created));
            }
        }
        let mut st = self.conn.prepare("SELECT book, chapter, verse, body, created, updated FROM notes")?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })?;
        for row in rows {
            let (b, c, v, body, created, updated) = row?;
            if let Some(vr) = verse_from_row(&b, c, v) {
                self.notes.insert(vr.key(), Note { body, created, updated });
            }
        }
        let mut st = self.conn.prepare("SELECT book, chapter, verse, label, created FROM bookmarks")?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get(4)?,
            ))
        })?;
        for row in rows {
            let (b, c, v, label, created) = row?;
            if let Some(vr) = verse_from_row(&b, c, v) {
                self.bookmarks.insert(vr.key(), Bookmark { label, created });
            }
        }
        self.generation += 1;
        Ok(())
    }

    /// Increments whenever study data changes (the reader uses it to invalidate layouts).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn highlight(&self, v: VerseRef) -> Option<HighlightColor> {
        self.highlights.get(&v.key()).map(|(c, _)| *c)
    }

    pub fn note(&self, v: VerseRef) -> Option<&Note> {
        self.notes.get(&v.key())
    }

    pub fn bookmark(&self, v: VerseRef) -> Option<&Bookmark> {
        self.bookmarks.get(&v.key())
    }

    pub fn is_bookmarked(&self, v: VerseRef) -> bool {
        self.bookmarks.contains_key(&v.key())
    }

    pub fn set_highlight(&mut self, v: VerseRef, color: Option<HighlightColor>) -> Result<()> {
        let code = v.book.info().code;
        let created = now();
        // The row and its change are saved together (the same in every method that writes).
        let tx = self.conn.unchecked_transaction()?;
        match color {
            Some(c) => tx.execute(
                "INSERT INTO highlights(book, chapter, verse, color, created) VALUES(?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(book, chapter, verse) DO UPDATE SET color = excluded.color",
                params![code, v.chapter, v.verse, c.to_i64(), created],
            )?,
            None => tx.execute(
                "DELETE FROM highlights WHERE book = ?1 AND chapter = ?2 AND verse = ?3",
                params![code, v.chapter, v.verse],
            )?,
        };
        self.log_change(&Record::Highlight(v))?;
        tx.commit()?;
        match color {
            Some(c) => {
                let created = self.highlights.get(&v.key()).map_or(created, |(_, t)| *t);
                self.highlights.insert(v.key(), (c, created));
            }
            None => {
                self.highlights.remove(&v.key());
            }
        }
        self.generation += 1;
        Ok(())
    }

    /// Adds `text` to the verse's note, after a blank line if there is one already.
    pub fn append_note(&mut self, v: VerseRef, text: &str) -> Result<()> {
        let body = match self.note(v) {
            Some(n) if !n.body.trim().is_empty() => format!("{}\n\n{text}", n.body.trim_end()),
            _ => text.to_string(),
        };
        self.set_note(v, &body)
    }

    /// Saves a note; an empty (whitespace-only) body deletes it.
    pub fn set_note(&mut self, v: VerseRef, body: &str) -> Result<()> {
        let code = v.book.info().code;
        let (delete, t) = (body.trim().is_empty(), now());
        let tx = self.conn.unchecked_transaction()?;
        if delete {
            tx.execute(
                "DELETE FROM notes WHERE book = ?1 AND chapter = ?2 AND verse = ?3",
                params![code, v.chapter, v.verse],
            )?;
        } else {
            tx.execute(
                "INSERT INTO notes(book, chapter, verse, body, created, updated) VALUES(?1, ?2, ?3, ?4, ?5, ?5)
                 ON CONFLICT(book, chapter, verse) DO UPDATE SET body = excluded.body, updated = excluded.updated",
                params![code, v.chapter, v.verse, body, t],
            )?;
        }
        self.log_change(&Record::Note(v))?;
        tx.commit()?;
        if delete {
            self.notes.remove(&v.key());
        } else {
            let created = self.notes.get(&v.key()).map_or(t, |n| n.created);
            self.notes.insert(v.key(), Note { body: body.to_string(), created, updated: t });
        }
        self.generation += 1;
        Ok(())
    }

    pub fn set_bookmark(&mut self, v: VerseRef, label: Option<&str>) -> Result<()> {
        let code = v.book.info().code;
        let t = now();
        let tx = self.conn.unchecked_transaction()?;
        match label {
            Some(label) => tx.execute(
                "INSERT INTO bookmarks(book, chapter, verse, label, created) VALUES(?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(book, chapter, verse) DO UPDATE SET label = excluded.label",
                params![code, v.chapter, v.verse, label, t],
            )?,
            None => tx.execute(
                "DELETE FROM bookmarks WHERE book = ?1 AND chapter = ?2 AND verse = ?3",
                params![code, v.chapter, v.verse],
            )?,
        };
        self.log_change(&Record::Bookmark(v))?;
        tx.commit()?;
        match label {
            Some(label) => {
                let created = self.bookmarks.get(&v.key()).map_or(t, |b| b.created);
                self.bookmarks.insert(v.key(), Bookmark { label: label.to_string(), created });
            }
            None => {
                self.bookmarks.remove(&v.key());
            }
        }
        self.generation += 1;
        Ok(())
    }

    pub fn toggle_bookmark(&mut self, v: VerseRef) -> Result<bool> {
        let on = !self.is_bookmarked(v);
        self.set_bookmark(v, on.then_some(""))?;
        Ok(on)
    }

    pub fn highlights(&self) -> Vec<(VerseRef, HighlightColor)> {
        let mut v: Vec<_> =
            self.highlights.iter().filter_map(|(k, (c, _))| Some((VerseRef::from_key(*k)?, *c))).collect();
        v.sort_by_key(|(r, _)| *r);
        v
    }

    /// Highlights, the most recently made first.
    pub fn highlights_newest_first(&self) -> Vec<(VerseRef, HighlightColor)> {
        let mut v: Vec<_> = self
            .highlights
            .iter()
            .filter_map(|(k, (c, t))| Some((VerseRef::from_key(*k)?, *c, *t)))
            .collect();
        v.sort_by_key(|(r, _, t)| (std::cmp::Reverse(*t), *r));
        v.into_iter().map(|(r, c, _)| (r, c)).collect()
    }

    /// Highlights with when they were made, the most recent first.
    pub fn highlights_dated(&self) -> Vec<(VerseRef, HighlightColor, i64)> {
        let mut v: Vec<_> = self
            .highlights
            .iter()
            .filter_map(|(k, (c, t))| Some((VerseRef::from_key(*k)?, *c, *t)))
            .collect();
        v.sort_by_key(|(r, _, t)| (std::cmp::Reverse(*t), *r));
        v
    }

    pub fn notes(&self) -> Vec<(VerseRef, &Note)> {
        let mut v: Vec<_> =
            self.notes.iter().filter_map(|(k, n)| Some((VerseRef::from_key(*k)?, n))).collect();
        v.sort_by_key(|(r, _)| *r);
        v
    }

    pub fn bookmarks(&self) -> Vec<(VerseRef, &Bookmark)> {
        self.bookmarks.iter().filter_map(|(k, b)| Some((VerseRef::from_key(*k)?, b))).collect()
    }

    pub fn counts(&self) -> (usize, usize, usize) {
        (self.highlights.len(), self.notes.len(), self.bookmarks.len())
    }

    /// Saves a reading session, replacing an earlier save of the same session.
    pub fn record_reading(&self, s: &ReadingSession) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO reading(started, book, chapter, translation, seconds, furthest, verses)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(started) DO UPDATE SET seconds = excluded.seconds, furthest = excluded.furthest,
               verses = excluded.verses, translation = excluded.translation",
            params![s.started, s.book.info().code, s.chapter, s.translation, s.seconds, s.furthest, s.verses],
        )?;
        self.log_change(&Record::Reading(s.started))?;
        Ok(tx.commit()?)
    }

    /// Reading sessions that started at or after `since` (Unix seconds), newest first.
    pub fn reading_since(&self, since: i64) -> Vec<ReadingSession> {
        let Ok(mut st) = self.conn.prepare_cached(
            "SELECT started, book, chapter, translation, seconds, furthest, verses FROM reading
             WHERE started >= ?1 ORDER BY started DESC",
        ) else {
            return Vec::new();
        };
        let rows = st.query_map([since], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
            ))
        });
        let Ok(rows) = rows else { return Vec::new() };
        rows.filter_map(|r| r.ok())
            .filter_map(|(started, book, chapter, translation, seconds, furthest, verses)| {
                Some(ReadingSession {
                    started,
                    book: BookId::from_code(&book)?,
                    chapter: u8::try_from(chapter).ok()?,
                    translation,
                    seconds: u32::try_from(seconds).ok()?,
                    furthest: u8::try_from(furthest).ok()?,
                    verses: u8::try_from(verses).ok()?,
                })
            })
            .collect()
    }

    /// Forgets reading history from `since` on (all of it for 0).
    pub fn clear_reading(&self, since: i64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let started: Vec<i64> = tx
            .prepare("SELECT started FROM reading WHERE started >= ?1")?
            .query_map([since], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        tx.execute("DELETE FROM reading WHERE started >= ?1", [since])?;
        for t in started {
            self.log_change(&Record::Reading(t))?;
        }
        Ok(tx.commit()?)
    }

    /// Plans started, newest first.
    pub fn plans(&self) -> Vec<PlanProgress> {
        let Ok(mut st) =
            self.conn.prepare_cached("SELECT id, started, finished FROM plans ORDER BY started DESC")
        else {
            return Vec::new();
        };
        st.query_map([], |r| Ok(PlanProgress { id: r.get(0)?, started: r.get(1)?, finished: r.get(2)? }))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    /// Starts (or restarts) a plan on the day containing `started`, forgetting earlier progress.
    pub fn start_plan(&mut self, id: &str, started: i64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let reads = self.plan_reads(id);
        tx.execute("DELETE FROM plan_reads WHERE plan = ?1", [id])?;
        tx.execute(
            "INSERT OR REPLACE INTO plans(id, started, finished) VALUES(?1, ?2, NULL)",
            params![id, started],
        )?;
        self.log_plan(id, reads)?;
        tx.commit()?;
        self.generation += 1;
        Ok(())
    }

    pub fn finish_plan(&mut self, id: &str, at: Option<i64>) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("UPDATE plans SET finished = ?2 WHERE id = ?1", params![id, at])?;
        self.log_plan(id, Default::default())?;
        tx.commit()?;
        self.generation += 1;
        Ok(())
    }

    pub fn stop_plan(&mut self, id: &str) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let reads = self.plan_reads(id);
        tx.execute("DELETE FROM plans WHERE id = ?1", [id])?;
        tx.execute("DELETE FROM plan_reads WHERE plan = ?1", [id])?;
        self.log_plan(id, reads)?;
        tx.commit()?;
        self.generation += 1;
        Ok(())
    }

    /// Logs a change to a plan and to the passages it had ticked (now cleared).
    fn log_plan(&self, id: &str, cleared: std::collections::HashSet<(u16, u8)>) -> Result<()> {
        for (day, passage) in cleared {
            self.log_change(&Record::PlanRead(id.to_string(), day, passage))?;
        }
        self.log_change(&Record::Plan(id.to_string()))
    }

    /// The passages ticked off in a plan, as (day, passage index) pairs.
    pub fn plan_reads(&self, id: &str) -> std::collections::HashSet<(u16, u8)> {
        let Ok(mut st) = self.conn.prepare_cached("SELECT day, passage FROM plan_reads WHERE plan = ?1")
        else {
            return Default::default();
        };
        st.query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
            .map(|rows| {
                rows.filter_map(|r| r.ok())
                    .filter_map(|(d, p)| Some((u16::try_from(d).ok()?, u8::try_from(p).ok()?)))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn set_plan_read(&mut self, id: &str, day: u16, passage: u8, read: bool) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        if read {
            tx.execute(
                "INSERT OR REPLACE INTO plan_reads(plan, day, passage, at) VALUES(?1, ?2, ?3, ?4)",
                params![id, day, passage, now()],
            )?;
        } else {
            tx.execute(
                "DELETE FROM plan_reads WHERE plan = ?1 AND day = ?2 AND passage = ?3",
                params![id, day, passage],
            )?;
        }
        self.log_change(&Record::PlanRead(id.to_string(), day, passage))?;
        tx.commit()?;
        self.generation += 1;
        Ok(())
    }

    /// The prayer rule; the starter rule until one has been saved.
    pub fn rule(&self) -> crate::rule::Rule {
        self.setting("rule")
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_else(crate::rule::Rule::starter)
    }

    pub fn set_rule(&mut self, rule: &crate::rule::Rule) -> Result<()> {
        self.set_setting("rule", &serde_json::to_string(rule)?)?;
        self.generation += 1;
        Ok(())
    }

    /// The rule's items checked off on `day` (a [`crate::calendar::Day`]), by id.
    pub fn rule_done(&self, day: i64) -> std::collections::HashSet<String> {
        self.rule_done_between(day, day).remove(&day).unwrap_or_default()
    }

    /// Items checked off on each day from `from` to `to` (inclusive), for the calendar.
    pub fn rule_done_between(
        &self,
        from: i64,
        to: i64,
    ) -> std::collections::BTreeMap<i64, std::collections::HashSet<String>> {
        let mut out: std::collections::BTreeMap<i64, std::collections::HashSet<String>> = Default::default();
        let Ok(mut st) =
            self.conn.prepare_cached("SELECT day, item FROM rule_done WHERE day >= ?1 AND day <= ?2")
        else {
            return out;
        };
        if let Ok(rows) =
            st.query_map(params![from, to], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        {
            for (day, item) in rows.filter_map(|r| r.ok()) {
                out.entry(day).or_default().insert(item);
            }
        }
        out
    }

    pub fn set_rule_done(&mut self, day: i64, item: &str, done: bool) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        if done {
            tx.execute(
                "INSERT OR REPLACE INTO rule_done(day, item, at) VALUES(?1, ?2, ?3)",
                params![day, item, now()],
            )?;
        } else {
            tx.execute("DELETE FROM rule_done WHERE day = ?1 AND item = ?2", params![day, item])?;
        }
        self.log_change(&Record::RuleDone(day, item.to_string()))?;
        tx.commit()?;
        self.generation += 1;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
    }

    /// Saves a setting. Only the prayer rule syncs; the others belong to this device.
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO settings(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        if key == "rule" {
            self.log_change(&Record::Rule)?;
        }
        Ok(tx.commit()?)
    }

    /// This device's id in the sync files (made once, then kept in the settings).
    pub fn device_id(&self) -> String {
        self.device.clone()
    }

    /// JSON export of all study data. `text` supplies verse text for context (may return `None`).
    pub fn export_json(
        &self,
        translation: &str,
        text: impl Fn(VerseRef) -> Option<String>,
    ) -> Result<String> {
        self.export_json_with(translation, text, Include::STUDY)
    }

    /// The study data as JSON (re-importable), with what `include` asks for.
    pub fn export_json_with(
        &self,
        translation: &str,
        text: impl Fn(VerseRef) -> Option<String>,
        include: Include,
    ) -> Result<String> {
        let entry = |v: VerseRef| ExportVerse {
            reference: v.to_string(),
            book: v.book.info().code.to_string(),
            chapter: v.chapter,
            verse: v.verse,
            text: text(v),
        };
        let mut doc = ExportDoc {
            app: "bible-rs".into(),
            format: 1,
            exported_at: rfc3339(now()),
            translation: translation.to_string(),
            history: Vec::new(),
            plans: Vec::new(),
            highlights: self
                .highlights()
                .into_iter()
                .map(|(v, c)| ExportHighlight {
                    verse: entry(v),
                    color: c,
                    created: rfc3339(self.highlights[&v.key()].1),
                })
                .collect(),
            notes: self
                .notes()
                .into_iter()
                .map(|(v, n)| ExportNote {
                    verse: entry(v),
                    note: n.body.clone(),
                    created: rfc3339(n.created),
                    updated: rfc3339(n.updated),
                })
                .collect(),
            bookmarks: self
                .bookmarks()
                .into_iter()
                .map(|(v, b)| ExportBookmark {
                    verse: entry(v),
                    label: b.label.clone(),
                    created: rfc3339(b.created),
                })
                .collect(),
        };
        if !include.highlights {
            doc.highlights.clear();
        }
        if !include.notes {
            doc.notes.clear();
        }
        if !include.bookmarks {
            doc.bookmarks.clear();
        }
        if include.history {
            doc.history = self
                .reading_since(0)
                .into_iter()
                .map(|s| ExportReading {
                    started: rfc3339(s.started),
                    book: s.book.info().code.to_string(),
                    chapter: s.chapter,
                    translation: s.translation,
                    seconds: s.seconds,
                    furthest: s.furthest,
                    verses: s.verses,
                })
                .collect();
            doc.plans = self
                .plans()
                .into_iter()
                .map(|p| ExportPlan {
                    read: self.plan_reads(&p.id).into_iter().collect(),
                    id: p.id,
                    started: rfc3339(p.started),
                    finished: p.finished.map(rfc3339),
                })
                .collect();
        }
        Ok(serde_json::to_string_pretty(&doc)?)
    }

    pub fn export_markdown(&self, translation: &str, text: impl Fn(VerseRef) -> Option<String>) -> String {
        self.export_markdown_with(translation, text, Include::STUDY)
    }

    /// The study data as readable Markdown, with what `include` asks for.
    pub fn export_markdown_with(
        &self,
        translation: &str,
        text: impl Fn(VerseRef) -> Option<String>,
        include: Include,
    ) -> String {
        use std::fmt::Write;
        let mut md = String::new();
        let _ = writeln!(md, "# Bible study notes\n");
        let _ =
            writeln!(md, "Exported from Bible on {} · verse text: {translation}\n", &rfc3339(now())[..10]);
        let bookmarks = if include.bookmarks { self.bookmarks() } else { Vec::new() };
        if !bookmarks.is_empty() {
            let _ = writeln!(md, "## Bookmarks\n");
            for (v, b) in bookmarks {
                if b.label.is_empty() {
                    let _ = writeln!(md, "- **{v}**");
                } else {
                    let _ = writeln!(md, "- **{v}** — {}", b.label);
                }
            }
            md.push('\n');
        }
        let highlights = if include.highlights { self.highlights() } else { Vec::new() };
        if !highlights.is_empty() {
            let _ = writeln!(md, "## Highlights\n");
            let mut book = None;
            for (v, c) in highlights {
                if book != Some(v.book) {
                    book = Some(v.book);
                    let _ = writeln!(md, "### {}\n", v.book.name());
                }
                let t = text(v).unwrap_or_default();
                let _ = writeln!(md, "- **{v}** ({}) {t}", c.name());
            }
            md.push('\n');
        }
        let notes = if include.notes { self.notes() } else { Vec::new() };
        if !notes.is_empty() {
            let _ = writeln!(md, "## Notes\n");
            for (v, n) in notes {
                let _ = writeln!(md, "### {v}\n");
                if let Some(t) = text(v) {
                    let _ = writeln!(md, "> {t}\n");
                }
                let _ = writeln!(md, "{}\n", n.body.trim_end());
            }
        }
        let history = if include.history { self.reading_since(0) } else { Vec::new() };
        if !history.is_empty() {
            let _ = writeln!(md, "## Reading history\n");
            for s in history {
                let minutes = (s.seconds + 30) / 60;
                let done = if s.finished() {
                    "finished".to_string()
                } else {
                    format!("{:.0} %", s.progress() * 100.0)
                };
                let _ = writeln!(
                    md,
                    "- {} — {} {} ({}), {minutes} min, {done}",
                    &rfc3339(s.started)[..10],
                    s.book.name(),
                    s.chapter,
                    s.translation
                );
            }
            md.push('\n');
        }
        md
    }

    /// Merges a JSON export back in (existing entries for the same verse are overwritten).
    pub fn import_json(&mut self, json: &str) -> Result<(usize, usize, usize)> {
        let doc: ExportDoc = serde_json::from_str(json).context("not a bible-rs export")?;
        if doc.app != "bible-rs" {
            bail!("not a bible-rs export");
        }
        let verse = |e: &ExportVerse| -> Result<VerseRef> {
            let book = BookId::from_code(&e.book).with_context(|| format!("unknown book {}", e.book))?;
            Ok(VerseRef::new(book, e.chapter, e.verse))
        };
        let tx = self.conn.unchecked_transaction()?;
        for h in &doc.highlights {
            let v = verse(&h.verse)?;
            tx.execute(
                "INSERT INTO highlights(book, chapter, verse, color, created) VALUES(?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(book, chapter, verse) DO UPDATE SET color = excluded.color",
                params![
                    v.book.info().code,
                    v.chapter,
                    v.verse,
                    h.color.to_i64(),
                    parse_rfc3339(&h.created).unwrap_or_else(now)
                ],
            )?;
            self.log_change(&Record::Highlight(v))?;
        }
        for n in &doc.notes {
            let v = verse(&n.verse)?;
            tx.execute(
                "INSERT INTO notes(book, chapter, verse, body, created, updated) VALUES(?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(book, chapter, verse) DO UPDATE SET body = excluded.body, updated = excluded.updated",
                params![
                    v.book.info().code,
                    v.chapter,
                    v.verse,
                    n.note,
                    parse_rfc3339(&n.created).unwrap_or_else(now),
                    parse_rfc3339(&n.updated).unwrap_or_else(now)
                ],
            )?;
            self.log_change(&Record::Note(v))?;
        }
        for b in &doc.bookmarks {
            let v = verse(&b.verse)?;
            tx.execute(
                "INSERT INTO bookmarks(book, chapter, verse, label, created) VALUES(?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(book, chapter, verse) DO UPDATE SET label = excluded.label",
                params![
                    v.book.info().code,
                    v.chapter,
                    v.verse,
                    b.label,
                    parse_rfc3339(&b.created).unwrap_or_else(now)
                ],
            )?;
            self.log_change(&Record::Bookmark(v))?;
        }
        for r in &doc.history {
            let Some(book) = BookId::from_code(&r.book) else { continue };
            let started = parse_rfc3339(&r.started).unwrap_or_else(now);
            tx.execute(
                "INSERT OR REPLACE INTO reading(started, book, chapter, translation, seconds, furthest, verses)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![started, book.info().code, r.chapter, r.translation, r.seconds, r.furthest, r.verses],
            )?;
            self.log_change(&Record::Reading(started))?;
        }
        for p in &doc.plans {
            tx.execute(
                "INSERT OR REPLACE INTO plans(id, started, finished) VALUES(?1, ?2, ?3)",
                params![
                    p.id,
                    parse_rfc3339(&p.started).unwrap_or_else(now),
                    p.finished.as_deref().and_then(parse_rfc3339)
                ],
            )?;
            self.log_change(&Record::Plan(p.id.clone()))?;
            for &(day, passage) in &p.read {
                tx.execute(
                    "INSERT OR REPLACE INTO plan_reads(plan, day, passage, at) VALUES(?1, ?2, ?3, ?4)",
                    params![p.id, day, passage, now()],
                )?;
                self.log_change(&Record::PlanRead(p.id.clone(), day, passage))?;
            }
        }
        tx.commit()?;
        self.reload()?;
        Ok((doc.highlights.len(), doc.notes.len(), doc.bookmarks.len()))
    }

    /// Erases every highlight, note, bookmark, plan and the reading history.
    ///
    /// This resets this device only: it forgets their changes instead of logging deletions, so
    /// other devices keep their copies and the next sync brings them back here. Deletions are
    /// kept, so one made here before erasing still reaches the other devices.
    pub fn erase_all(&mut self) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        // The latest change this device had seen, so the next edit of a forgotten record still
        // comes after it (see `log_change`).
        let floor = self.sync_floor().max(tx.query_row("SELECT MAX(at) FROM changes", [], |r| r.get(0))?);
        if let Some(floor) = floor {
            tx.execute(
                "INSERT OR REPLACE INTO settings(key, value) VALUES('sync-floor', ?1)",
                [floor.to_string()],
            )?;
        }
        tx.execute_batch(
            "DELETE FROM highlights; DELETE FROM notes; DELETE FROM bookmarks;
             DELETE FROM plans; DELETE FROM plan_reads; DELETE FROM reading;
             DELETE FROM changes
               WHERE value IS NOT NULL AND key NOT LIKE 'rule-done:%' AND key <> 'setting:rule';",
        )?;
        tx.commit()?;
        self.reload()
    }

    /// The latest change time [`StudyStore::erase_all`] forgot.
    fn sync_floor(&self) -> Option<i64> {
        self.setting("sync-floor")?.parse().ok()
    }

    /// Logs the record's current state (or its deletion) as this device's latest change to it.
    /// The time is at least a second past the change it replaces, so an edit made after a sync
    /// still wins when this device's clock is behind, or when two edits fall in the same second.
    /// A record with no change (new, or forgotten by `erase_all`) goes past the erase's floor.
    fn log_change(&self, r: &Record) -> Result<()> {
        let key = r.key();
        let prev: Option<i64> = self
            .conn
            .query_row("SELECT at FROM changes WHERE key = ?1", [&key], |row| row.get(0))
            .optional()?;
        let at = prev.or_else(|| self.sync_floor()).map_or(now(), |p| now().max(p + 1));
        self.put_change(&key, self.record_value(r)?.as_deref(), at, &self.device)
    }

    fn put_change(&self, key: &str, value: Option<&str>, at: i64, device: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO changes(key, value, at, device) VALUES(?1, ?2, ?3, ?4)",
            params![key, value, at, device],
        )?;
        Ok(())
    }

    /// The record's row as JSON, or `None` when there is none.
    fn record_value(&self, r: &Record) -> Result<Option<String>> {
        let c = &self.conn;
        let verse = |v: &VerseRef| (v.book.info().code, v.chapter, v.verse);
        let value = match r {
            Record::Highlight(v) => c
                .query_row(
                    "SELECT color, created FROM highlights WHERE book = ?1 AND chapter = ?2 AND verse = ?3",
                    verse(v),
                    |r| Ok(json!({"color": r.get::<_, i64>(0)?, "created": r.get::<_, i64>(1)?})),
                )
                .optional()?,
            Record::Note(v) => c
                .query_row(
                    "SELECT body, created, updated FROM notes WHERE book = ?1 AND chapter = ?2 AND verse = ?3",
                    verse(v),
                    |r| {
                        Ok(json!({
                            "body": r.get::<_, String>(0)?,
                            "created": r.get::<_, i64>(1)?,
                            "updated": r.get::<_, i64>(2)?,
                        }))
                    },
                )
                .optional()?,
            Record::Bookmark(v) => c
                .query_row(
                    "SELECT label, created FROM bookmarks WHERE book = ?1 AND chapter = ?2 AND verse = ?3",
                    verse(v),
                    |r| Ok(json!({"label": r.get::<_, String>(0)?, "created": r.get::<_, i64>(1)?})),
                )
                .optional()?,
            Record::Plan(id) => c
                .query_row("SELECT started, finished FROM plans WHERE id = ?1", [id], |r| {
                    Ok(json!({"started": r.get::<_, i64>(0)?, "finished": r.get::<_, Option<i64>>(1)?}))
                })
                .optional()?,
            Record::PlanRead(plan, day, passage) => c
                .query_row(
                    "SELECT at FROM plan_reads WHERE plan = ?1 AND day = ?2 AND passage = ?3",
                    params![plan, day, passage],
                    |r| Ok(json!({"at": r.get::<_, i64>(0)?})),
                )
                .optional()?,
            Record::RuleDone(day, item) => c
                .query_row("SELECT at FROM rule_done WHERE day = ?1 AND item = ?2", params![day, item], |r| {
                    Ok(json!({"at": r.get::<_, i64>(0)?}))
                })
                .optional()?,
            Record::Reading(started) => c
                .query_row(
                    "SELECT book, chapter, translation, seconds, furthest, verses FROM reading WHERE started = ?1",
                    [started],
                    |r| {
                        Ok(json!({
                            "book": r.get::<_, String>(0)?,
                            "chapter": r.get::<_, i64>(1)?,
                            "translation": r.get::<_, String>(2)?,
                            "seconds": r.get::<_, i64>(3)?,
                            "furthest": r.get::<_, i64>(4)?,
                            "verses": r.get::<_, i64>(5)?,
                        }))
                    },
                )
                .optional()?,
            Record::Rule => self.setting("rule").map(|v| json!({ "value": v })),
        };
        Ok(value.map(|v| v.to_string()))
    }

    /// Writes a record that came from another device (`None` deletes it). `false`, with nothing
    /// written, when the value isn't one this version understands, or is a plan tick from before
    /// the plan was last (re)started here.
    fn apply_record(&self, r: &Record, value: Option<&str>) -> Result<bool> {
        let c = &self.conn;
        let verse = |v: &VerseRef| (v.book.info().code, v.chapter, v.verse);
        let Some(value) = value else {
            match r {
                Record::Highlight(v) => c.execute(
                    "DELETE FROM highlights WHERE book = ?1 AND chapter = ?2 AND verse = ?3",
                    verse(v),
                )?,
                Record::Note(v) => {
                    c.execute("DELETE FROM notes WHERE book = ?1 AND chapter = ?2 AND verse = ?3", verse(v))?
                }
                Record::Bookmark(v) => c.execute(
                    "DELETE FROM bookmarks WHERE book = ?1 AND chapter = ?2 AND verse = ?3",
                    verse(v),
                )?,
                Record::Plan(id) => c.execute("DELETE FROM plans WHERE id = ?1", [id])?,
                Record::PlanRead(plan, day, passage) => c.execute(
                    "DELETE FROM plan_reads WHERE plan = ?1 AND day = ?2 AND passage = ?3",
                    params![plan, day, passage],
                )?,
                Record::RuleDone(day, item) => {
                    c.execute("DELETE FROM rule_done WHERE day = ?1 AND item = ?2", params![day, item])?
                }
                Record::Reading(started) => c.execute("DELETE FROM reading WHERE started = ?1", [started])?,
                Record::Rule => c.execute("DELETE FROM settings WHERE key = 'rule'", [])?,
            };
            return Ok(true);
        };
        let Ok(j) = serde_json::from_str::<serde_json::Value>(value) else { return Ok(false) };
        let int = |k: &str| j.get(k).and_then(|x| x.as_i64());
        let text = |k: &str| j.get(k).and_then(|x| x.as_str());
        match r {
            Record::Highlight(v) => {
                let (Some(color), Some(created)) =
                    (int("color").and_then(HighlightColor::from_i64), int("created"))
                else {
                    return Ok(false);
                };
                let (book, chapter, verse) = verse(v);
                c.execute(
                    "INSERT OR REPLACE INTO highlights(book, chapter, verse, color, created) VALUES(?1, ?2, ?3, ?4, ?5)",
                    params![book, chapter, verse, color.to_i64(), created],
                )?;
            }
            Record::Note(v) => {
                let (Some(body), Some(created), Some(updated)) =
                    (text("body"), int("created"), int("updated"))
                else {
                    return Ok(false);
                };
                let (book, chapter, verse) = verse(v);
                c.execute(
                    "INSERT OR REPLACE INTO notes(book, chapter, verse, body, created, updated)
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                    params![book, chapter, verse, body, created, updated],
                )?;
            }
            Record::Bookmark(v) => {
                let (Some(label), Some(created)) = (text("label"), int("created")) else { return Ok(false) };
                let (book, chapter, verse) = verse(v);
                c.execute(
                    "INSERT OR REPLACE INTO bookmarks(book, chapter, verse, label, created) VALUES(?1, ?2, ?3, ?4, ?5)",
                    params![book, chapter, verse, label, created],
                )?;
            }
            Record::Plan(id) => {
                let Some(started) = int("started") else { return Ok(false) };
                c.execute(
                    "INSERT OR REPLACE INTO plans(id, started, finished) VALUES(?1, ?2, ?3)",
                    params![id, started, int("finished")],
                )?;
                // A restart elsewhere also clears ticks made here before it that it never saw.
                // (`started` is the start of that day, so ticks from earlier that day stay.)
                c.execute("DELETE FROM plan_reads WHERE plan = ?1 AND at < ?2", params![id, started])?;
            }
            Record::PlanRead(plan, day, passage) => {
                let Some(at) = int("at") else { return Ok(false) };
                let started: Option<i64> = c
                    .query_row("SELECT started FROM plans WHERE id = ?1", [plan], |r| r.get(0))
                    .optional()?;
                if started.is_some_and(|started| at < started) {
                    return Ok(false);
                }
                c.execute(
                    "INSERT OR REPLACE INTO plan_reads(plan, day, passage, at) VALUES(?1, ?2, ?3, ?4)",
                    params![plan, day, passage, at],
                )?;
            }
            Record::RuleDone(day, item) => {
                let Some(at) = int("at") else { return Ok(false) };
                c.execute(
                    "INSERT OR REPLACE INTO rule_done(day, item, at) VALUES(?1, ?2, ?3)",
                    params![day, item, at],
                )?;
            }
            Record::Reading(started) => {
                let (
                    Some(book),
                    Some(chapter),
                    Some(translation),
                    Some(seconds),
                    Some(furthest),
                    Some(verses),
                ) = (
                    text("book").and_then(BookId::from_code),
                    int("chapter"),
                    text("translation"),
                    int("seconds"),
                    int("furthest"),
                    int("verses"),
                )
                else {
                    return Ok(false);
                };
                c.execute(
                    "INSERT OR REPLACE INTO reading(started, book, chapter, translation, seconds, furthest, verses)
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![started, book.info().code, chapter, translation, seconds, furthest, verses],
                )?;
            }
            Record::Rule => {
                let Some(rule) = text("value") else { return Ok(false) };
                c.execute("INSERT OR REPLACE INTO settings(key, value) VALUES('rule', ?1)", [rule])?;
            }
        }
        Ok(true)
    }

    /// Every record's latest change, by key.
    pub(crate) fn changes(&self) -> Result<Vec<Change>> {
        let mut st = self.conn.prepare("SELECT key, value, at, device FROM changes ORDER BY key")?;
        let rows = st.query_map([], |r| {
            Ok(Change { key: r.get(0)?, value: r.get(1)?, at: r.get(2)?, device: r.get(3)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Applies changes from other devices and logs them as they came (with their time and
    /// device, so they travel on unchanged). Returns how many were applied: ones with a key or
    /// value this version doesn't know are skipped.
    pub(crate) fn apply_changes(&mut self, changes: &[Change]) -> Result<usize> {
        let tx = self.conn.unchecked_transaction()?;
        let mut applied = 0;
        for change in changes {
            let Some(r) = Record::parse(&change.key) else { continue };
            if self.apply_record(&r, change.value.as_deref())? {
                self.put_change(&change.key, change.value.as_deref(), change.at, &change.device)?;
                applied += 1;
            }
        }
        tx.commit()?;
        if applied > 0 {
            self.reload()?;
        }
        Ok(applied)
    }

    /// Seeds an empty `changes` from the rows already there (a database from before sync), each
    /// at the row's own time, so the first sync sends everything.
    fn backfill_changes(&self) -> Result<()> {
        let any: bool = self.conn.query_row("SELECT EXISTS(SELECT 1 FROM changes)", [], |r| r.get(0))?;
        if any {
            return Ok(());
        }
        let mut records: Vec<(Record, i64)> = Vec::new();
        for (sql, record) in [
            ("SELECT book, chapter, verse, created FROM highlights", Record::Highlight as fn(_) -> _),
            ("SELECT book, chapter, verse, updated FROM notes", Record::Note),
            ("SELECT book, chapter, verse, created FROM bookmarks", Record::Bookmark),
        ] {
            let mut st = self.conn.prepare(sql)?;
            let rows = st.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?))
            })?;
            for row in rows {
                let (b, c, v, at) = row?;
                if let Some(v) = verse_from_row(&b, c, v) {
                    records.push((record(v), at));
                }
            }
        }
        let mut st = self.conn.prepare("SELECT id, MAX(started, COALESCE(finished, started)) FROM plans")?;
        for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
            let (id, at) = row?;
            records.push((Record::Plan(id), at));
        }
        let mut st = self.conn.prepare("SELECT plan, day, passage, at FROM plan_reads")?;
        let rows = st.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?))
        })?;
        for row in rows {
            let (plan, day, passage, at) = row?;
            if let (Ok(day), Ok(passage)) = (u16::try_from(day), u8::try_from(passage)) {
                records.push((Record::PlanRead(plan, day, passage), at));
            }
        }
        let mut st = self.conn.prepare("SELECT day, item, at FROM rule_done")?;
        for row in
            st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?
        {
            let (day, item, at) = row?;
            records.push((Record::RuleDone(day, item), at));
        }
        let mut st = self.conn.prepare("SELECT started FROM reading")?;
        for row in st.query_map([], |r| r.get::<_, i64>(0))? {
            let started = row?;
            records.push((Record::Reading(started), started));
        }
        // Settings carry no time; 0 lets any real edit on another device win.
        if self.setting("rule").is_some() {
            records.push((Record::Rule, 0));
        }
        let tx = self.conn.unchecked_transaction()?;
        for (r, at) in records {
            self.put_change(&r.key(), self.record_value(&r)?.as_deref(), at, &self.device)?;
        }
        tx.commit()?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct ExportDoc {
    app: String,
    format: u32,
    exported_at: String,
    translation: String,
    #[serde(default)]
    highlights: Vec<ExportHighlight>,
    #[serde(default)]
    notes: Vec<ExportNote>,
    #[serde(default)]
    bookmarks: Vec<ExportBookmark>,
    /// Reading sessions and plans, when the export includes history.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    history: Vec<ExportReading>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    plans: Vec<ExportPlan>,
}

#[derive(Serialize, Deserialize)]
struct ExportReading {
    started: String,
    book: String,
    chapter: u8,
    translation: String,
    seconds: u32,
    furthest: u8,
    verses: u8,
}

#[derive(Serialize, Deserialize)]
struct ExportPlan {
    id: String,
    started: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    finished: Option<String>,
    /// Passages ticked, as (day, index).
    read: Vec<(u16, u8)>,
}

/// What an export carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Include {
    pub highlights: bool,
    pub notes: bool,
    pub bookmarks: bool,
    /// Reading history and plans.
    pub history: bool,
}

impl Include {
    /// Highlights, notes and bookmarks.
    pub const STUDY: Include = Include { highlights: true, notes: true, bookmarks: true, history: false };
    pub const EVERYTHING: Include = Include { highlights: true, notes: true, bookmarks: true, history: true };

    pub const fn default_none() -> Include {
        Include { highlights: false, notes: false, bookmarks: false, history: false }
    }
}

#[derive(Serialize, Deserialize)]
struct ExportVerse {
    reference: String,
    book: String,
    chapter: u8,
    verse: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ExportHighlight {
    #[serde(flatten)]
    verse: ExportVerse,
    color: HighlightColor,
    created: String,
}

#[derive(Serialize, Deserialize)]
struct ExportNote {
    #[serde(flatten)]
    verse: ExportVerse,
    note: String,
    created: String,
    updated: String,
}

#[derive(Serialize, Deserialize)]
struct ExportBookmark {
    #[serde(flatten)]
    verse: ExportVerse,
    label: String,
    created: String,
}

/// Unix seconds → `YYYY-MM-DDTHH:MM:SSZ` (proleptic Gregorian, UTC).
pub fn rfc3339(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

fn parse_rfc3339(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let days = days_from_civil(num(0..4)?, num(5..7)? as u32, num(8..10)? as u32);
    Some(days * 86_400 + num(11..13)? * 3600 + num(14..16)? * 60 + num(17..19)?)
}

// Howard Hinnant's date algorithms.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod plan_tests {
    use super::*;

    #[test]
    fn plans_start_tick_and_stop() {
        let mut s = StudyStore::open_in_memory().unwrap();
        s.start_plan("john-21", 1_000).unwrap();
        s.set_plan_read("john-21", 1, 0, true).unwrap();
        s.set_plan_read("john-21", 2, 0, true).unwrap();
        s.set_plan_read("john-21", 2, 0, false).unwrap();
        assert_eq!(s.plans()[0].id, "john-21");
        assert_eq!(s.plan_reads("john-21").into_iter().collect::<Vec<_>>(), vec![(1, 0)]);
        // Restarting forgets progress.
        s.start_plan("john-21", 2_000).unwrap();
        assert!(s.plan_reads("john-21").is_empty());
        s.finish_plan("john-21", Some(3_000)).unwrap();
        assert_eq!(s.plans()[0].finished, Some(3_000));
        s.stop_plan("john-21").unwrap();
        assert!(s.plans().is_empty());
    }

    #[test]
    fn notes_append() {
        let mut s = StudyStore::open_in_memory().unwrap();
        let v = VerseRef::new(BookId::GENESIS, 1, 1);
        s.append_note(v, "A quote.").unwrap();
        assert_eq!(s.note(v).unwrap().body, "A quote.");
        s.append_note(v, "Another.").unwrap();
        assert_eq!(s.note(v).unwrap().body, "A quote.\n\nAnother.");
    }

    #[test]
    fn rule_is_saved_and_days_are_ticked() {
        let mut s = StudyStore::open_in_memory().unwrap();
        assert_eq!(s.rule(), crate::rule::Rule::starter());
        let mut rule = s.rule();
        rule.blocks.truncate(1);
        s.set_rule(&rule).unwrap();
        assert_eq!(s.rule().blocks.len(), 1);
        s.set_rule_done(20_000, "morning-prayers", true).unwrap();
        s.set_rule_done(20_001, "morning-prayers", true).unwrap();
        s.set_rule_done(20_001, "morning-reading", true).unwrap();
        s.set_rule_done(20_001, "morning-reading", false).unwrap();
        assert_eq!(s.rule_done(20_001).into_iter().collect::<Vec<_>>(), ["morning-prayers"]);
        assert_eq!(s.rule_done_between(19_000, 21_000).len(), 2);
        assert!(s.rule_done(19_999).is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jn316() -> VerseRef {
        VerseRef::new(BookId(58), 3, 16)
    }

    #[test]
    fn reading_sessions() {
        let s = StudyStore::open_in_memory().unwrap();
        let mut session = ReadingSession {
            started: 1_000,
            book: BookId(58),
            chapter: 3,
            translation: "kjv".into(),
            seconds: 30,
            furthest: 10,
            verses: 36,
        };
        s.record_reading(&session).unwrap();
        session.seconds = 400;
        session.furthest = 36;
        s.record_reading(&session).unwrap();
        s.record_reading(&ReadingSession { started: 2_000, chapter: 4, furthest: 3, ..session.clone() })
            .unwrap();
        let all = s.reading_since(0);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].chapter, 4, "newest first");
        assert_eq!(all[1].seconds, 400, "a session is saved in place");
        assert!(all[1].finished() && !all[0].finished());
        assert_eq!(s.reading_since(1_500).len(), 1);
        s.clear_reading(1_500).unwrap();
        assert_eq!(s.reading_since(0).len(), 1);
    }

    #[test]
    fn crud_and_persistence() {
        let dir = std::env::temp_dir().join(format!("bible-rs-study-{}", std::process::id()));
        let path = dir.join("study.sqlite");
        let _ = std::fs::remove_dir_all(&dir);
        {
            let mut s = StudyStore::open(&path).unwrap();
            let g = s.generation();
            s.set_highlight(jn316(), Some(HighlightColor::Yellow)).unwrap();
            s.set_highlight(jn316(), Some(HighlightColor::Green)).unwrap();
            s.set_note(jn316(), "God's love").unwrap();
            assert!(s.toggle_bookmark(VerseRef::new(BookId(0), 1, 1)).unwrap());
            s.set_setting("theme", "dark").unwrap();
            assert!(s.generation() > g);
        }
        let mut s = StudyStore::open(&path).unwrap();
        assert_eq!(s.highlight(jn316()), Some(HighlightColor::Green));
        assert_eq!(s.note(jn316()).unwrap().body, "God's love");
        assert!(s.is_bookmarked(VerseRef::new(BookId(0), 1, 1)));
        assert_eq!(s.setting("theme").as_deref(), Some("dark"));
        assert_eq!(s.counts(), (1, 1, 1));
        s.set_note(jn316(), "  ").unwrap();
        s.set_highlight(jn316(), None).unwrap();
        assert!(!s.toggle_bookmark(VerseRef::new(BookId(0), 1, 1)).unwrap());
        assert_eq!(s.counts(), (0, 0, 0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn json_round_trip_and_markdown() {
        let mut a = StudyStore::open_in_memory().unwrap();
        a.set_highlight(jn316(), Some(HighlightColor::Pink)).unwrap();
        a.set_note(jn316(), "line one\nline two").unwrap();
        a.set_bookmark(VerseRef::new(BookId(25), 23, 1), Some("Shepherd")).unwrap();
        let text = |v: VerseRef| Some(format!("text of {v}"));
        let json = a.export_json("KJV", text).unwrap();
        assert!(json.contains("\"reference\": \"John 3:16\""));
        assert!(json.contains("\"color\": \"pink\""));

        let mut b = StudyStore::open_in_memory().unwrap();
        assert_eq!(b.import_json(&json).unwrap(), (1, 1, 1));
        assert_eq!(b.highlight(jn316()), Some(HighlightColor::Pink));
        assert_eq!(b.note(jn316()).unwrap().body, "line one\nline two");
        assert_eq!(b.bookmark(VerseRef::new(BookId(25), 23, 1)).unwrap().label, "Shepherd");
        assert_eq!(b.note(jn316()).unwrap().created, a.note(jn316()).unwrap().created);

        let md = b.export_markdown("KJV", text);
        assert!(md.contains("## Bookmarks"));
        assert!(md.contains("- **Psalm 23:1** — Shepherd"));
        assert!(md.contains("- **John 3:16** (pink) text of John 3:16"));
        assert!(md.contains("### John 3:16\n\n> text of John 3:16\n\nline one\nline two"));
        assert!(b.import_json("{}").is_err());
        // History and plans travel when asked for, and erasing clears everything.
        a.record_reading(&ReadingSession {
            started: 1_000,
            book: BookId(58),
            chapter: 3,
            translation: "kjv".into(),
            seconds: 300,
            furthest: 36,
            verses: 36,
        })
        .unwrap();
        a.start_plan("john-21", 900).unwrap();
        a.set_plan_read("john-21", 1, 0, true).unwrap();
        let all = a.export_json_with("KJV", text, Include::EVERYTHING).unwrap();
        let only_notes =
            a.export_json_with("KJV", text, Include { notes: true, ..Include::default_none() }).unwrap();
        assert!(!only_notes.contains("\"highlights\": [\n"), "{only_notes}");
        let mut c = StudyStore::open_in_memory().unwrap();
        c.import_json(&all).unwrap();
        assert_eq!(c.reading_since(0).len(), 1);
        assert_eq!(c.plan_reads("john-21").len(), 1);
        c.erase_all().unwrap();
        assert_eq!((c.counts(), c.reading_since(0).len(), c.plans().len()), ((0, 0, 0), 0, 0));
    }

    #[test]
    fn dates() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(1_791_158_400), "2026-10-05T00:00:00Z");
        for t in [0, 951_782_400, 1_791_158_399, 4_102_444_800] {
            assert_eq!(parse_rfc3339(&rfc3339(t)), Some(t));
        }
    }
}
