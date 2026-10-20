//! The Church Fathers on each verse: quotes from the Historical Christian Faith Commentaries
//! Database (<https://github.com/HistoricalChristianFaith/Commentaries-Database>), kept to the
//! Fathers the Orthodox Church receives.
//!
//! The database mixes public-domain translations with fair-use excerpts of modern ones, so it is
//! never bundled or committed: [`install`] builds `fathers.sqlite` in the user's data directory
//! from a `commentaries.sqlite` they downloaded, and [`Fathers`] reads it.
//!
//! The source keys verses as `chapter × 1,000,000 + verse` with Hebrew psalm numbers, which is
//! the grid's numbering, so quotes need no remapping.

use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, params};

use crate::books::{BOOKS, BookId, VerseRef};

/// The file in the data directory.
pub const FILE: &str = "fathers.sqlite";

/// The East and West before the Great Schism of 1054.
const BEFORE: i64 = 1054;

/// Kept out although the source doesn't mark them condemned: Origen, Theodore of Mopsuestia,
/// Didymus and Evagrius were condemned at the Fifth Ecumenical Council (553); Diodore of Tarsus,
/// Severus of Antioch and the Donatist Ticonius by other councils; the rest wrote in churches
/// outside Orthodox communion. Isaac of Nineveh stays: the Orthodox Church venerates him.
const LEFT_OUT: &[&str] = &[
    "Origen of Alexandria",
    "Theodore of Mopsuestia",
    "Didymus the Blind",
    "Evagrius Ponticus",
    "Diodorus of Tarsus",
    "Severus of Antioch",
    "Ticonius",
    "Philoxenus of Mabbug",
    "Jacob of Serugh",
    "Jacob of Edessa",
    "Shenoute the Archimandrite",
    "Besa The Copt",
    "Ishodad of Merv",
    "Sahdona the Syrian",
    "John of Dalyatha",
    "Abraham of Nathpar",
];

/// Source categories that aren't Fathers: pseudepigrapha, Philo and Josephus, cross-references.
const NOT_FATHERS: &[&str] = &[
    "Pseudonymous Works",
    "Apocrypha, Pseudepigrapha & Early Documents",
    "Second Temple Judaism",
    "Canonical Scriptures",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Quote {
    /// "John Chrysostom", or "Bede (as quoted by Aquinas, AD 1274)".
    pub author: String,
    /// The year the source gives for the author (usually their death).
    pub year: i32,
    /// "Homily on the Gospel of John 27".
    pub source: String,
    pub url: String,
    pub text: String,
}

impl Quote {
    /// The quote as it goes into a note: the text in quotation marks, then who said it, where.
    pub fn as_note(&self) -> String {
        let source = if self.source.is_empty() { String::new() } else { format!(", {}", self.source) };
        format!("\u{201c}{}\u{201d}\n\u{2014} {}{source}", self.text.trim(), self.author)
    }
}

/// What [`install`] kept.
#[derive(Debug)]
pub struct Installed {
    pub quotes: usize,
    pub authors: usize,
}

/// `chapter << 8 | verse`: grid order within a book.
fn key(chapter: i64, verse: i64) -> i64 {
    chapter.clamp(0, 255) << 8 | verse.clamp(0, 255)
}

/// The source's book names ("1corinthians", "songofsolomon", "wisdom") as grid books.
fn book_for(name: &str) -> Option<BookId> {
    BOOKS
        .iter()
        .position(|b| {
            let full: String = b.name.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
            full.eq_ignore_ascii_case(name) || b.aliases.contains(&name)
        })
        .map(|i| BookId(i as u8))
}

/// Builds `dest` from the Commentaries Database at `src`, keeping the Fathers before 1054 the
/// Orthodox Church receives. Replaces any earlier copy.
pub fn install(src: &Path, dest: &Path) -> Result<Installed> {
    let from = Connection::open_with_flags(src, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening {}", src.display()))?;
    let tmp = dest.with_extension("sqlite.tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut to = Connection::open(&tmp)?;
    to.execute_batch(
        "CREATE TABLE quotes(book TEXT NOT NULL, start INTEGER NOT NULL, end INTEGER NOT NULL,
             author TEXT NOT NULL, year INTEGER NOT NULL, source TEXT NOT NULL, url TEXT NOT NULL,
             text TEXT NOT NULL);",
    )?;
    let mut st = from.prepare(
        "SELECT c.father_name, coalesce(c.append_to_author_name, ''), c.book, c.location_start,
                c.location_end, coalesce(c.source_title, ''), coalesce(c.source_url, ''), c.txt,
                cast(m.default_year AS INTEGER), coalesce(m.father_category, '')
         FROM commentary c JOIN father_meta m ON m.name = c.father_name
         WHERE cast(m.default_year AS INTEGER) < ?1 AND coalesce(m.condemned_by_council, 0) = 0",
    )?;
    let tx = to.transaction()?;
    let (mut quotes, mut authors) = (0, std::collections::HashSet::new());
    {
        let mut insert = tx.prepare("INSERT INTO quotes VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)")?;
        let mut rows = st.query([BEFORE])?;
        while let Some(r) = rows.next()? {
            let (father, suffix, book): (String, String, String) = (r.get(0)?, r.get(1)?, r.get(2)?);
            let category: String = r.get(9)?;
            if LEFT_OUT.contains(&father.as_str()) || NOT_FATHERS.contains(&category.as_str()) {
                continue;
            }
            let Some(book) = book_for(&book) else { continue };
            let text: String = r.get(7)?;
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            let (start, end): (i64, i64) = (r.get(3)?, r.get(4)?);
            let start = key(start / 1_000_000, (start % 1_000_000).max(1));
            let end = key(end / 1_000_000, (end % 1_000_000).max(1)).max(start);
            insert.execute(params![
                book.info().code,
                start,
                end,
                format!("{father}{suffix}"),
                r.get::<_, i64>(8)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                text
            ])?;
            quotes += 1;
            authors.insert(father);
        }
    }
    tx.execute_batch("CREATE INDEX quotes_at ON quotes(book, start);")?;
    tx.commit()?;
    drop(to);
    std::fs::rename(&tmp, dest)?;
    Ok(Installed { quotes, authors: authors.len() })
}

/// The installed quotes, read on demand.
pub struct Fathers {
    conn: Mutex<Connection>,
}

impl Fathers {
    /// Opens `fathers.sqlite` in `data_dir`; `None` until it has been installed.
    pub fn open(data_dir: &Path) -> Option<Self> {
        let path = data_dir.join(FILE);
        if !path.exists() {
            return None;
        }
        let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
        Some(Self { conn: Mutex::new(conn) })
    }

    /// The quotes on `v`, oldest author first.
    pub fn quotes(&self, v: VerseRef) -> Vec<Quote> {
        let at = key(v.chapter as i64, v.verse as i64);
        let conn = self.conn.lock().unwrap();
        // `start` is indexed; a quote starts at most a few chapters before the verse it covers.
        let Ok(mut st) = conn.prepare_cached(
            "SELECT author, year, source, url, text FROM quotes
             WHERE book = ?1 AND start <= ?2 AND start >= ?3 AND end >= ?2
             ORDER BY year, author, start",
        ) else {
            return Vec::new();
        };
        let rows = st.query_map(params![v.book.info().code, at, at - (8 << 8)], |r| {
            Ok(Quote {
                author: r.get(0)?,
                year: r.get(1)?,
                source: r.get(2)?,
                url: r.get(3)?,
                text: r.get(4)?,
            })
        });
        rows.map(|rows| rows.filter_map(Result::ok).collect()).unwrap_or_default()
    }

    /// How many quotes there are on `v` (for the verse menu).
    pub fn count(&self, v: VerseRef) -> usize {
        let at = key(v.chapter as i64, v.verse as i64);
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT count(*) FROM quotes WHERE book = ?1 AND start <= ?2 AND start >= ?3 AND end >= ?2",
            params![v.book.info().code, at, at - (8 << 8)],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n as usize)
        .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("bible-rs-fathers-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A small database in the Commentaries Database's schema.
    fn source(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("commentaries.sqlite");
        let c = Connection::open(&path).unwrap();
        c.execute_batch(
            "CREATE TABLE father_meta(name, default_year, wiki_url, father_category, condemned_by_council);
             CREATE TABLE commentary(id, father_name, file_name, append_to_author_name, ts, book,
                 location_start, location_end, txt, source_url, source_title);
             INSERT INTO father_meta VALUES
                 ('John Chrysostom', '407', '', 'Eastern & Byzantine Theology', 0),
                 ('Basil of Caesarea', '379', '', 'Eastern & Byzantine Theology', 0),
                 ('Origen of Alexandria', '253', '', 'Early Fathers (Pre-Nicaea)', 0),
                 ('Arius', '336', '', 'Eastern & Byzantine Theology', 1),
                 ('Thomas Aquinas', '1274', '', 'Western & Medieval Theology', 0),
                 ('Philo of Alexandria', '50', '', 'Second Temple Judaism', 0);
             INSERT INTO commentary VALUES
                 (1, 'John Chrysostom', '', '', 0, 'john', 3000016, 3000016, ' He so loved. ', '', 'Homily 27'),
                 (2, 'Basil of Caesarea', '', '', 0, 'john', 3000014, 3000017, 'A range.', '', 'On the Spirit'),
                 (3, 'Origen of Alexandria', '', '', 0, 'john', 3000016, 3000016, 'Left out.', '', ''),
                 (4, 'Arius', '', '', 0, 'john', 3000016, 3000016, 'Condemned.', '', ''),
                 (5, 'Thomas Aquinas', '', '', 0, 'john', 3000016, 3000016, 'Too late.', '', ''),
                 (6, 'Philo of Alexandria', '', '', 0, 'genesis', 1000001, 1000001, 'Not a Father.', '', ''),
                 (7, 'John Chrysostom', '', '', 0, 'psalms', 51000001, 51000001, 'Have mercy.', '', ''),
                 (8, 'John Chrysostom', '', '', 0, 'sirach', 2000001, 2000001, 'My son.', '', '');",
        )
        .unwrap();
        path
    }

    #[test]
    fn installs_the_fathers_and_finds_them_by_verse() {
        let d = tmp("install");
        let installed = install(&source(&d), &d.join(FILE)).unwrap();
        assert_eq!((installed.quotes, installed.authors), (4, 2));
        let f = Fathers::open(&d).unwrap();
        let john = BookId::from_code("JHN").unwrap();
        let q = f.quotes(VerseRef::new(john, 3, 16));
        let authors: Vec<&str> = q.iter().map(|q| q.author.as_str()).collect();
        assert_eq!(authors, ["Basil of Caesarea", "John Chrysostom"]);
        assert_eq!(q[1].text, "He so loved.");
        assert_eq!(q[1].as_note(), "\u{201c}He so loved.\u{201d}\n\u{2014} John Chrysostom, Homily 27");
        assert_eq!(f.count(VerseRef::new(john, 3, 17)), 1);
        assert_eq!(f.count(VerseRef::new(john, 3, 18)), 0);
        assert_eq!(f.quotes(VerseRef::new(BookId::PSALMS, 51, 1))[0].text, "Have mercy.");
        let sirach = BookId::from_code("SIR").unwrap();
        assert_eq!(f.count(VerseRef::new(sirach, 2, 1)), 1);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn source_book_names_map_to_the_grid() {
        for (name, code) in [
            ("1corinthians", "1CO"),
            ("songofsolomon", "SNG"),
            ("psalms", "PSA"),
            ("wisdom", "WIS"),
            ("sirach", "SIR"),
            ("1maccabees", "1MA"),
            ("prayerofazariah", "S3Y"),
            ("revelation", "REV"),
            ("tobit", "TOB"),
        ] {
            assert_eq!(book_for(name).map(|b| b.info().code), Some(code), "{name}");
        }
        assert!(Fathers::open(Path::new("/nonexistent")).is_none());
    }
}
