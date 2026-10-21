//! The daily lectionary: the Scripture read at the Divine Liturgy and the other services on each
//! day of the church year, in Byzantine usage as either the Greek Orthodox Archdiocese of America
//! (the default) or the Antiochian Archdiocese of North America reads it ([`Usage`]).
//!
//! The tables in `data/lectionary/` (embedded, parsed once) give the readings of each kind of day,
//! with their sources; this module works out which apply and how they combine:
//!
//! - The **Triodion** (the Publican and the Pharisee to Holy Saturday) and the **Pentecostarion**
//!   (Pascha to the Monday of the Holy Spirit) are keyed by distance from Pascha.
//! - After Pentecost the **Epistle** follows the week after Pentecost, and weeks past the 32nd
//!   count back from the Publican and the Pharisee. The **Gospel** reads Matthew until the Sunday
//!   after the Elevation of the Cross and then Luke (the *Lukan jump*): forward until the Nativity,
//!   back from the Triodion after Theophany. Luke's autumn Sundays keep dated windows, and the
//!   Sundays between Theophany and the Triodion a fixed order (`Cx::winter_sunday`).
//! - The **Menaion** (fixed feasts and saints, on the church's calendar: the civil date on the New
//!   calendar, the Julian date on the Old) replaces or adds to those readings by each entry's
//!   rules: on a weekday, on a Sunday, on the Saturdays and Sundays around the great feasts, in
//!   Lent, in Holy Week and in Bright Week.
//!
//! Where the two usages differ the tables record both: a reading, an entry or a set of rules marked
//! for one usage (`"in"`, `"greek"`, `"antiochian"`) is read only there, and a few rules below
//! check [`Usage`] where the Archdioceses order the same days differently.
//!
//! References are stored in the grid's (Hebrew/KJV) numbering and read by a parser of their own,
//! so the Septuagint-numbering setting ([`crate::lxx::numbering`]) never changes which verses a
//! reading covers, only how [`Reading::label`] shows them.
//!
//! A day's readings vary from parish to parish, and the yearly ordo settles a few days that no
//! rule does (the free days between the Nativity and Theophany, long winters); those readings say
//! so in their [`Confidence`].

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::Deserialize;

use crate::books::{BOOKS, BookId, ChapterIdx, VerseRef};
use crate::calendar::{self, Day, Style};
use crate::reference::format_range;

/// What a reading is read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    OldTestament,
    /// The Apostle: Acts and the Epistles.
    Epistle,
    Gospel,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::OldTestament => "Old Testament",
            Kind::Epistle => "Epistle",
            Kind::Gospel => "Gospel",
        }
    }
}

/// How sure the tables are of a reading (see the data files' headers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    /// The yearly ordo decides, or no Antiochian or Greek source was found.
    Low,
    /// The sources differ, usually by a verse at the start or the end.
    Medium,
    /// An Antiochian source and an independent one agree.
    High,
}

/// One contiguous run of verses, in grid numbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Passage {
    pub book: BookId,
    /// First chapter and verse.
    pub from: (u8, u8),
    /// Last chapter and verse (inclusive).
    pub to: (u8, u8),
}

impl Passage {
    pub fn first(self) -> VerseRef {
        VerseRef::new(self.book, self.from.0, self.from.1)
    }

    pub fn last(self) -> VerseRef {
        VerseRef::new(self.book, self.to.0, self.to.1)
    }

    /// The chapter the passage starts in.
    pub fn chapter(self) -> ChapterIdx {
        self.first().chapter_idx()
    }

    pub fn contains(self, v: VerseRef) -> bool {
        v.book == self.book && (v.chapter, v.verse) >= self.from && (v.chapter, v.verse) <= self.to
    }

    /// Every verse of the passage, by the grid's verse counts.
    pub fn verses(self) -> impl Iterator<Item = VerseRef> {
        (self.from.0..=self.to.0).flat_map(move |c| {
            let last = ChapterIdx::new(self.book, c).map_or(0, ChapterIdx::kjv_verses);
            let a = if c == self.from.0 { self.from.1 } else { 1 };
            let b = if c == self.to.0 { self.to.1.min(last) } else { last };
            (a..=b).map(move |v| VerseRef::new(self.book, c, v))
        })
    }
}

/// One lesson: the passages read together at a service.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    /// "Liturgy", "Matins", "Vespers", "Sixth Hour", "Royal Hours", "Presanctified Liturgy"…
    pub service: &'static str,
    pub kind: Kind,
    pub passages: &'static [Passage],
    /// What tells this lesson from the others: "St Thekla", "for the departed", "1st",
    /// "Ninth Hour", "Eothinon 4".
    pub title: Option<&'static str>,
    /// When the service is held, if not at its usual time: "evening before".
    pub served: Option<&'static str>,
    /// The day or feast the reading belongs to: "Tuesday of the 21st week after Pentecost",
    /// "Great Martyr Artemius of Antioch…".
    pub occasion: &'static str,
    pub confidence: Confidence,
}

impl Reading {
    /// "Acts 1:12–17, 21–26", "Romans 14:19–23; 16:25–27", in the numbering shown.
    pub fn label(&self) -> String {
        label(self.passages)
    }

    /// Read at the Divine Liturgy (St John Chrysostom's, St Basil's or the Presanctified).
    pub fn at_liturgy(&self) -> bool {
        self.service.contains("Liturgy")
    }
}

/// How a run of passages is shown: the first in full, then only what changes.
pub fn label(passages: &[Passage]) -> String {
    let mut out = String::new();
    let mut prev: Option<Passage> = None;
    for &p in passages {
        match prev {
            Some(q) if q.book == p.book => {
                let (qc, _) = q.last().shown_numbers();
                let (c, v) = p.first().shown_numbers();
                let (ec, ev) = p.last().shown_numbers();
                if c == qc {
                    out.push_str(&format!(", {v}"));
                } else {
                    out.push_str(&format!("; {c}:{v}"));
                }
                if p.to != p.from {
                    if ec == c {
                        out.push_str(&format!("–{ev}"));
                    } else {
                        out.push_str(&format!("–{ec}:{ev}"));
                    }
                }
            }
            _ => {
                if prev.is_some() {
                    out.push_str("; ");
                }
                out.push_str(&format_range(p.first(), p.last()));
            }
        }
        prev = Some(p);
    }
    out
}

/// Whose service books the readings follow where the churches differ: which saints are kept,
/// a verse at the start or end of a lesson, a few rules of the Typikon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Usage {
    /// The Greek Orthodox Archdiocese of America (GOARCH): the readings goarch.org and its
    /// parishes' bulletins print.
    #[default]
    Greek,
    /// The Antiochian Orthodox Christian Archdiocese of North America: its Department of
    /// Liturgics' service texts and charts, with its own saints.
    Antiochian,
}

impl Usage {
    pub const ALL: [Usage; 2] = [Usage::Greek, Usage::Antiochian];

    /// The name settings store: "greek", "antiochian".
    pub fn as_str(self) -> &'static str {
        match self {
            Usage::Greek => "greek",
            Usage::Antiochian => "antiochian",
        }
    }

    /// The inverse of [`Usage::as_str`].
    pub fn parse(s: &str) -> Option<Usage> {
        Usage::ALL.into_iter().find(|u| u.as_str() == s)
    }
}

/// The readings of day `d` on the church's calendar `style`, in the default (Greek) usage.
pub fn readings(d: Day, style: Style) -> Vec<Reading> {
    readings_in(d, style, Usage::default())
}

/// The readings of day `d` on the church's calendar `style` in `usage`, in service order: the
/// Vespers of the evening before, Matins, the Hours, the Liturgy, then the services of the day's
/// evening.
pub fn readings_in(d: Day, style: Style, usage: Usage) -> Vec<Reading> {
    let t = tables(usage);
    let cx = Cx::new(d, style, usage);
    let mut plan = cx.moveable(t);
    cx.menaion(t, &mut plan);
    cx.matins(t, &mut plan);
    let mut out = plan.readings;
    out.sort_by_key(order);
    out
}

/// Service order within the day.
fn order(r: &Reading) -> (u8, u8) {
    let served = r.served.unwrap_or("");
    let s = r.service;
    let slot = if s.starts_with("Vespers")
        && (served.starts_with("evening before") || served.starts_with("Saturday evening"))
    {
        0
    } else if s.starts_with("Matins") {
        1
    } else if s == "Royal Hours" || s.ends_with("Hour") {
        2
    } else if (s.contains("Liturgy") && !s.starts_with("Presanctified")) || served.starts_with("joined") {
        3
    } else if s.starts_with("Presanctified") || s.starts_with("Vespers") {
        4
    } else {
        5
    };
    let kind = match r.kind {
        Kind::OldTestament => 0,
        Kind::Epistle => 1,
        Kind::Gospel => 2,
    };
    (slot, if slot == 3 { kind } else { 0 })
}

// ---- The tables ----------------------------------------------------------------------------------

macro_rules! table {
    ($name:literal) => {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/lectionary/", $name, ".json"))
    };
}

/// The data files, by name.
const FILES: [(&str, &str); 5] = [
    ("pentecostarion", table!("pentecostarion")),
    ("triodion", table!("triodion")),
    ("epistles", table!("epistles")),
    ("gospels", table!("gospels")),
    ("menaion", table!("menaion")),
];

#[derive(Deserialize)]
struct RawFile {
    entries: Vec<RawEntry>,
}

#[derive(Deserialize)]
struct RawEntry {
    pascha: Option<i64>,
    series: Option<String>,
    week: Option<u8>,
    day: Option<String>,
    id: Option<String>,
    date: Option<String>,
    range: Option<RawRange>,
    rank: Option<String>,
    weekday: Option<String>,
    sunday: Option<String>,
    saturday: Option<String>,
    bright: Option<String>,
    matins_on_sunday: Option<String>,
    name: String,
    readings: Vec<RawReading>,
    confidence: Option<String>,
    /// Kept only in this usage.
    #[serde(rename = "in")]
    only: Option<String>,
    /// What the entry changes in each usage.
    greek: Option<RawChanges>,
    antiochian: Option<RawChanges>,
}

/// An entry's name, rules or confidence in one usage.
#[derive(Deserialize)]
struct RawChanges {
    name: Option<String>,
    weekday: Option<String>,
    sunday: Option<String>,
    saturday: Option<String>,
    bright: Option<String>,
    confidence: Option<String>,
}

#[derive(Deserialize)]
struct RawRange {
    day: String,
    from: String,
    to: String,
    except: Option<String>,
}

#[derive(Deserialize)]
struct RawReading {
    service: String,
    kind: String,
    refs: Vec<String>,
    title: Option<String>,
    served: Option<String>,
    confidence: Option<String>,
    /// Read only in this usage.
    #[serde(rename = "in")]
    only: Option<String>,
}

/// A day's (or a feast's) readings.
struct Entry {
    name: &'static str,
    readings: Vec<Reading>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OnWeekday {
    /// Its Epistle and Gospel are read instead of the day's.
    Replaces,
    Epistle,
    Gospel,
    /// The entry is the day: a great feast, an eve, a Saturday or Sunday around a feast.
    Own,
    /// Read only where the saint is kept; not shown.
    Optional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OnSunday {
    Feast,
    Epistle,
    Sunday,
    Special,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum When {
    Date(u8, u8),
    /// `weekday` falling between `from` and `to` (inclusive; may cross the new year).
    Range {
        weekday: u8,
        from: (u8, u8),
        to: (u8, u8),
        except: Option<(u8, u8)>,
    },
}

impl When {
    fn matches(self, md: (u8, u8), wd: u8) -> bool {
        match self {
            When::Date(m, d) => md == (m, d),
            When::Range { weekday, from, to, except } => {
                let inside = if from <= to { md >= from && md <= to } else { md >= from || md <= to };
                wd == weekday && inside && except != Some(md)
            }
        }
    }
}

/// A Menaion entry.
struct Feast {
    id: &'static str,
    when: When,
    weekday: OnWeekday,
    sunday: OnSunday,
    /// On a Saturday with its own readings: which of the entry's readings it gives, if any.
    saturday: Option<Kind>,
    /// In Bright Week: the one reading the entry gives.
    bright: Option<Kind>,
    /// On a Sunday its Matins Gospel replaces the eothinon.
    feast_matins: bool,
    entry: Entry,
}

impl Feast {
    fn float(&self) -> bool {
        matches!(self.when, When::Range { .. })
    }

    /// The entry has a Matins Gospel of its own (in Greek usage, the mark of a saint who is moved
    /// rather than left unread).
    fn own_matins(&self) -> bool {
        self.entry.readings.iter().any(|r| r.service == "Matins" && r.kind == Kind::Gospel)
    }
}

struct Tables {
    /// Pascha 0..=50.
    pentecostarion: Vec<Entry>,
    /// Pascha -70..=-1, at index pascha + 70.
    triodion: Vec<Entry>,
    /// (week after Pentecost, weekday 0 = Sunday).
    epistles: HashMap<(u8, u8), Entry>,
    matthew: HashMap<(u8, u8), Entry>,
    luke: HashMap<(u8, u8), Entry>,
    /// The eleven Resurrection Gospels of Sunday Matins.
    eothina: Vec<Entry>,
    menaion: Vec<Feast>,
}

/// The tables as `usage` reads them.
fn tables(usage: Usage) -> &'static Tables {
    static TABLES: [OnceLock<Tables>; 2] = [OnceLock::new(), OnceLock::new()];
    TABLES[usage as usize].get_or_init(|| load(usage).unwrap_or_else(|e| panic!("lectionary data: {e}")))
}

/// Strings live as long as the program: the tables are parsed once.
fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn weekday_number(s: &str) -> Result<u8, String> {
    ["sun", "mon", "tue", "wed", "thu", "fri", "sat"]
        .iter()
        .position(|d| *d == s)
        .map(|i| i as u8)
        .ok_or_else(|| format!("bad weekday {s:?}"))
}

fn month_day(s: &str) -> Result<(u8, u8), String> {
    let (m, d) = s.split_once('-').ok_or_else(|| format!("bad date {s:?}"))?;
    Ok((m.parse().map_err(|_| format!("bad date {s:?}"))?, d.parse().map_err(|_| format!("bad date {s:?}"))?))
}

fn confidence(s: Option<&str>) -> Result<Confidence, String> {
    match s.unwrap_or("high") {
        "high" => Ok(Confidence::High),
        "medium" => Ok(Confidence::Medium),
        "low" => Ok(Confidence::Low),
        other => Err(format!("bad confidence {other:?}")),
    }
}

fn kind(s: &str) -> Result<Kind, String> {
    match s {
        "Old Testament" => Ok(Kind::OldTestament),
        "Epistle" => Ok(Kind::Epistle),
        "Gospel" => Ok(Kind::Gospel),
        other => Err(format!("bad kind {other:?}")),
    }
}

/// Reads one reference of the tables: "Matt 4:23-5:13", "Acts 1:12-17", "Jude 1:3", with the
/// grid's book abbreviations and numbering. Unlike [`crate::reference::parse`] it never looks at
/// the numbering setting.
pub fn parse_passage(s: &str) -> Result<Passage, String> {
    let (i, info) = BOOKS
        .iter()
        .enumerate()
        .filter(|(_, b)| s.strip_prefix(b.abbrev).is_some_and(|r| r.starts_with(' ')))
        .max_by_key(|(_, b)| b.abbrev.len())
        .ok_or_else(|| format!("unknown book in {s:?}"))?;
    let book = BookId(i as u8);
    let rest = s[info.abbrev.len()..].trim();
    let num = |t: &str| t.trim().parse::<u8>().map_err(|_| format!("bad number in {s:?}"));
    let point = |t: &str, chapter: Option<u8>| -> Result<(u8, u8), String> {
        match t.split_once(':') {
            Some((c, v)) => Ok((num(c)?, num(v)?)),
            None => Ok((chapter.ok_or_else(|| format!("no chapter in {s:?}"))?, num(t)?)),
        }
    };
    let (a, b) = rest.split_once('-').map_or((rest, None), |(a, b)| (a, Some(b)));
    let from = point(a, None)?;
    let to = match b {
        Some(b) => point(b, Some(from.0))?,
        None => from,
    };
    let inside = |(c, v): (u8, u8)| ChapterIdx::new(book, c).is_some_and(|ci| v >= 1 && v <= ci.kjv_verses());
    if to < from || !inside(from) || !inside(to) {
        return Err(format!("{s:?} is not a range of the grid"));
    }
    Ok(Passage { book, from, to })
}

fn entry(raw: &mut RawEntry) -> Result<Entry, String> {
    let name = leak(std::mem::take(&mut raw.name));
    let base = confidence(raw.confidence.as_deref())?;
    let mut readings = Vec::new();
    for r in raw.readings.drain(..) {
        let passages: Vec<Passage> = r.refs.iter().map(|s| parse_passage(s)).collect::<Result<_, _>>()?;
        if passages.is_empty() {
            return Err(format!("{name}: a reading without references"));
        }
        readings.push(Reading {
            service: leak(r.service),
            kind: kind(&r.kind)?,
            passages: Box::leak(passages.into_boxed_slice()),
            title: r.title.map(leak),
            served: r.served.map(leak),
            occasion: name,
            confidence: confidence(r.confidence.as_deref())?.min(base),
        });
    }
    Ok(Entry { name, readings })
}

/// Whether what is marked `in` (a usage's name, or nothing for every usage) belongs to `usage`.
fn kept(only: Option<&str>, usage: Usage) -> Result<bool, String> {
    only.map_or(Ok(true), |s| Usage::parse(s).map(|u| u == usage).ok_or_else(|| format!("bad usage {s:?}")))
}

/// The entries of table `name` as `usage` reads them: its own entries and readings, with its
/// changes to names and rules.
fn file(name: &str, usage: Usage) -> Result<Vec<RawEntry>, String> {
    let (_, text) = FILES.iter().find(|(n, _)| *n == name).ok_or_else(|| format!("no table {name}"))?;
    let entries =
        serde_json::from_str::<RawFile>(text).map(|f| f.entries).map_err(|e| format!("{name}.json: {e}"))?;
    let mut out = Vec::new();
    for mut raw in entries {
        if !kept(raw.only.as_deref(), usage)? {
            continue;
        }
        let mut readings = Vec::new();
        for r in std::mem::take(&mut raw.readings) {
            if kept(r.only.as_deref(), usage)? {
                readings.push(r);
            }
        }
        raw.readings = readings;
        let changes = match usage {
            Usage::Greek => raw.greek.take(),
            Usage::Antiochian => raw.antiochian.take(),
        };
        if let Some(c) = changes {
            raw.name = c.name.unwrap_or(raw.name);
            raw.weekday = c.weekday.or(raw.weekday);
            raw.sunday = c.sunday.or(raw.sunday);
            raw.saturday = c.saturday.or(raw.saturday);
            raw.bright = c.bright.or(raw.bright);
            raw.confidence = c.confidence.or(raw.confidence);
        }
        out.push(raw);
    }
    Ok(out)
}

fn load(usage: Usage) -> Result<Tables, String> {
    let file = |name: &str| file(name, usage);
    let keyed = |name: &str, offset: i64, len: usize| -> Result<Vec<Entry>, String> {
        let mut out = Vec::new();
        for (i, mut raw) in file(name)?.into_iter().enumerate() {
            if raw.pascha != Some(i as i64 + offset) {
                return Err(format!("{name}.json: entry {i} is not pascha {}", i as i64 + offset));
            }
            out.push(entry(&mut raw)?);
        }
        if out.len() != len {
            return Err(format!("{name}.json: {} entries, expected {len}", out.len()));
        }
        Ok(out)
    };
    let pentecostarion = keyed("pentecostarion", 0, 51)?;
    let triodion = keyed("triodion", -70, 70)?;

    let mut epistles = HashMap::new();
    for mut raw in file("epistles")? {
        let key =
            (raw.week.ok_or("epistle without week")?, weekday_number(raw.day.as_deref().unwrap_or(""))?);
        epistles.insert(key, entry(&mut raw)?);
    }
    let (mut matthew, mut luke, mut eothina) = (HashMap::new(), HashMap::new(), Vec::new());
    for mut raw in file("gospels")? {
        let key = (raw.week.ok_or("gospel without week")?, weekday_number(raw.day.as_deref().unwrap_or(""))?);
        match raw.series.as_deref() {
            Some("matthew") => {
                matthew.insert(key, entry(&mut raw)?);
            }
            Some("luke") => {
                luke.insert(key, entry(&mut raw)?);
            }
            Some("eothina") => eothina.push(entry(&mut raw)?),
            other => return Err(format!("gospels.json: bad series {other:?}")),
        }
    }
    if eothina.len() != 11 {
        return Err("gospels.json: there are eleven eothina".into());
    }

    let mut menaion = Vec::new();
    for mut raw in file("menaion")? {
        let id = leak(raw.id.take().ok_or("menaion entry without id")?);
        let when = match (&raw.date, &raw.range) {
            (Some(d), None) => {
                let (m, d) = month_day(d)?;
                When::Date(m, d)
            }
            (None, Some(r)) => When::Range {
                weekday: weekday_number(&r.day)?,
                from: month_day(&r.from)?,
                to: month_day(&r.to)?,
                except: r.except.as_deref().map(month_day).transpose()?,
            },
            _ => return Err(format!("{id}: needs a date or a range")),
        };
        let weekday = match raw.weekday.as_deref() {
            Some("replaces") => OnWeekday::Replaces,
            Some("epistle") => OnWeekday::Epistle,
            Some("gospel") => OnWeekday::Gospel,
            Some("own") => OnWeekday::Own,
            Some("optional") => OnWeekday::Optional,
            other => return Err(format!("{id}: bad weekday rule {other:?}")),
        };
        let sunday = match raw.sunday.as_deref() {
            Some("feast") => OnSunday::Feast,
            Some("epistle") => OnSunday::Epistle,
            Some("sunday") | None => OnSunday::Sunday,
            Some("special") => OnSunday::Special,
            Some(other) => return Err(format!("{id}: bad sunday rule {other:?}")),
        };
        let one = |s: Option<&str>| match s {
            Some("epistle") => Ok(Some(Kind::Epistle)),
            Some("gospel") => Ok(Some(Kind::Gospel)),
            None | Some("none") => Ok(None),
            Some(other) => Err(format!("{id}: bad reading {other:?}")),
        };
        let saturday = one(raw.saturday.as_deref())?;
        let bright = one(raw.bright.as_deref())?;
        let feast_matins = raw.matins_on_sunday.as_deref() == Some("feast");
        let _ = raw.rank.take();
        menaion.push(Feast {
            id,
            when,
            weekday,
            sunday,
            saturday,
            bright,
            feast_matins,
            entry: entry(&mut raw)?,
        });
    }
    Ok(Tables { pentecostarion, triodion, epistles, matthew, luke, eothina, menaion })
}

// ---- The rules -----------------------------------------------------------------------------------

/// How the day's moveable readings meet the Menaion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    /// Pascha, Holy Week, Palm Sunday, Lazarus Saturday, the Ascension, Pentecost: only the
    /// Annunciation is added.
    Paschal,
    /// Bright Monday to Saturday: only the `bright` entries, with one reading each.
    BrightWeek,
    /// Lenten weekdays and Cheesefare Wednesday and Friday: no Liturgy.
    LentWeekday,
    /// Mid-Pentecost, the leave-taking of Pascha, the Saturdays of Souls, the Monday of the Holy
    /// Spirit, the leave-taking of Pentecost, the Saturdays of Lent and Cheesefare: the Sunday
    /// rule (with the exceptions in `Cx::menaion`).
    Strong,
    Sunday,
    Saturday,
    Weekday,
}

struct Plan {
    readings: Vec<Reading>,
    class: Class,
    /// Readings come from the cycle after Pentecost (and so may give way to a float).
    cycle: bool,
    /// A Saturday or Sunday around a great feast governs the day.
    festal: bool,
    /// A feast's Matins Gospel replaced the Sunday's.
    feast_matins: bool,
}

impl Plan {
    /// Puts `with`'s Liturgy readings of `kind` in place of the day's.
    fn replace(&mut self, kind: Kind, with: &Entry) {
        let new: Vec<Reading> =
            with.readings.iter().filter(|r| r.service == "Liturgy" && r.kind == kind).cloned().collect();
        if new.is_empty() {
            return;
        }
        self.readings.retain(|r| !(r.service == "Liturgy" && r.kind == kind));
        self.readings.extend(new);
    }

    fn replace_both(&mut self, with: &Entry) {
        self.replace(Kind::Epistle, with);
        self.replace(Kind::Gospel, with);
    }

    /// Adds `with`'s readings other than the Liturgy's: Vespers, Matins, the Hours.
    fn extras(&mut self, with: &Entry, matins: bool, lent: bool) {
        for r in &with.readings {
            let lent_only = r.served.is_some_and(|s| s.contains("in Lent only"));
            let is_matins = r.service.starts_with("Matins");
            if r.service == "Liturgy" || (is_matins && !matins) || (lent_only && !lent) {
                continue;
            }
            if is_matins && r.kind == Kind::Gospel {
                self.readings.retain(|x| !(x.service.starts_with("Matins") && x.kind == Kind::Gospel));
            }
            self.readings.push(r.clone());
        }
    }

    fn add_all(&mut self, with: &Entry, lent: bool) {
        self.readings.extend(with.readings.iter().filter(|r| r.service == "Liturgy").cloned());
        self.extras(with, true, lent);
    }

    /// Lowers to `c` the confidence of the readings `with` gave.
    fn cap(&mut self, with: &Entry, c: Confidence) {
        for r in self.readings.iter_mut().filter(|r| r.occasion == with.name) {
            r.confidence = r.confidence.min(c);
        }
    }
}

/// What the cycle after Pentecost reads on a day.
struct Cycle {
    epistle: Option<&'static Entry>,
    gospel: Option<&'static Entry>,
    /// The yearly ordo settles the reading.
    epistle_low: bool,
    gospel_low: bool,
}

/// Day `d` and the dates around it that the rules need.
struct Cx {
    d: Day,
    style: Style,
    usage: Usage,
    wd: u8,
    /// The church's month and day.
    md: (u8, u8),
    /// The last Pascha on or before `d`, and its year.
    pascha: Day,
    year: i32,
    /// The next Pascha after `d`.
    next: Day,
}

/// A church month and day.
type MonthDay = (u8, u8);

/// The autumn Sundays that read a set Gospel of Luke: (first, last, number).
const WINDOWS: [(MonthDay, MonthDay, u8); 6] = [
    ((10, 11), (10, 17), 4),
    ((10, 30), (11, 5), 5),
    ((11, 24), (11, 30), 13),
    ((12, 1), (12, 3), 14),
    ((12, 4), (12, 10), 10),
    ((12, 11), (12, 17), 11),
];

impl Cx {
    fn new(d: Day, style: Style, usage: Usage) -> Self {
        let (y, _, _) = calendar::civil_date(d);
        let (pascha, year) = if calendar::pascha(y) <= d {
            (calendar::pascha(y), y)
        } else {
            (calendar::pascha(y - 1), y - 1)
        };
        let (_, m, dd) = calendar::church_date(d, style);
        Cx {
            d,
            style,
            usage,
            wd: calendar::weekday(d),
            md: (m, dd),
            pascha,
            year,
            next: calendar::pascha(year + 1),
        }
    }

    /// The day a church date of `year` falls on.
    fn fixed(&self, m: u8, d: u8, year: i32) -> Day {
        match self.style {
            Style::New => calendar::civil(year, m, d),
            Style::Old => calendar::julian(year, m, d),
        }
    }

    fn md_of(&self, d: Day) -> (u8, u8) {
        let (_, m, dd) = calendar::church_date(d, self.style);
        (m, dd)
    }

    /// The Sunday after the Elevation (the first Sunday after 14 September): the last day of
    /// Matthew.
    fn jump(&self) -> Day {
        let elevation = self.fixed(9, 14, self.year);
        elevation + 7 - calendar::weekday(elevation) as Day
    }

    fn moveable(&self, t: &'static Tables) -> Plan {
        let (p, q) = (self.d - self.pascha, self.d - self.next);
        let wd = self.wd;
        let weekly = |wd: u8| match wd {
            0 => Class::Sunday,
            6 => Class::Saturday,
            _ => Class::Weekday,
        };
        let (readings, class, cycle) = if q >= -70 {
            let class = match q {
                -8..=-1 => Class::Paschal,
                -53 | -51 => Class::LentWeekday,
                -48..=-9 if (1..=5).contains(&wd) => Class::LentWeekday,
                -57 | -50 | -43 | -36 | -29 | -22 | -15 => Class::Strong,
                _ => weekly(wd),
            };
            (t.triodion[(q + 70) as usize].readings.clone(), class, false)
        } else if p <= 50 {
            let class = match p {
                0 | 39 | 49 => Class::Paschal,
                1..=6 => Class::BrightWeek,
                24 | 38 | 48 | 50 => Class::Strong,
                _ => weekly(wd),
            };
            (t.pentecostarion[p as usize].readings.clone(), class, false)
        } else {
            let c = self.cycle(t);
            let mut readings: Vec<Reading> = Vec::new();
            for (e, low) in [(c.epistle, c.epistle_low), (c.gospel, c.gospel_low)] {
                let Some(e) = e else { continue };
                readings.extend(e.readings.iter().filter(|r| r.service == "Liturgy").cloned().map(
                    |mut r| {
                        if low {
                            r.confidence = Confidence::Low;
                        }
                        r
                    },
                ));
            }
            // The Saturday after Pentecost is its leave-taking.
            (readings, if p == 55 { Class::Strong } else { weekly(wd) }, true)
        };
        Plan { readings, class, cycle, festal: false, feast_matins: false }
    }

    /// The Epistle and Gospel of a day after the Monday of the Holy Spirit.
    fn cycle(&self, t: &'static Tables) -> Cycle {
        let pentecost = self.pascha + 49;
        let week = ((self.d - pentecost - 1) / 7 + 1) as u8;
        let (gospel, sunday_epistle, low) = self.gospel(t);
        let epistle = if self.wd == 0 {
            t.epistles.get(&(sunday_epistle.unwrap_or(week).min(32), 0))
        } else {
            // Weeks past the 32nd count back from the Publican: the six days before it read week 33.
            let publican_week = ((self.next - 70 - pentecost) / 7) as u8;
            let w = if week <= 32 { week } else { 33u8.saturating_sub(publican_week.saturating_sub(week)) };
            t.epistles.get(&(w, self.wd))
        };
        // The Epistle never jumps; only on 26 January does the ordo choose it too.
        Cycle { epistle, gospel, epistle_low: low && self.md == (1, 26), gospel_low: low }
    }

    /// The Gospel of a day after the Monday of the Holy Spirit, the Sunday Epistle that goes with
    /// it when the two are paired (Theophany to the Triodion), and whether the ordo decides.
    fn gospel(&self, t: &'static Tables) -> (Option<&'static Entry>, Option<u8>, bool) {
        let (d, wd) = (self.d, self.wd);
        let jump = self.jump();
        if d <= jump {
            let week = ((d - self.pascha - 49 + 6) / 7) as u8;
            if week > 17 {
                // Only on the Old calendar, whose Elevation comes 13 days later: the 18th and 19th
                // weeks read Luke's first, as the Slavic books number them, before the jump.
                return (t.luke.get(&(week - 17, wd)), None, true);
            }
            return (t.matthew.get(&(week, wd)), None, false);
        }
        let nativity = self.fixed(12, 25, self.year);
        let theophany = self.fixed(1, 6, self.year + 1);
        let publican = self.next - 70;
        if wd == 0 {
            if d < nativity {
                return (t.luke.get(&(self.luke_sunday(jump, d), 0)), None, false);
            }
            if d > theophany {
                return self.winter_sunday(t, jump, publican);
            }
            return (None, None, false);
        }
        if d < nativity {
            let week = (((d - jump + 6) / 7) as u8).min(16);
            (t.luke.get(&(week, wd)), None, false)
        } else if d > theophany {
            // Counted back from the Triodion: the week before it reads the 16th week, the one before
            // that the 15th, then the 14th. Earlier, longer winters keep counting back (the ordo
            // decides: goarch.org repeats weeks, antiochian.org has differed).
            let back = ((publican - d + 6) / 7) as u8;
            let week = 17u8.saturating_sub(back).max(1);
            // On 19 and 26 January (monastic saints), and in Greek usage on 24 January (St Xenia of
            // Rome), the ordo reads a Sunday Gospel of Matthew left unread in the summer; which one
            // follows no rule found (goarch.org 2018-2027; antiochian.org differed in 2021 and 2024).
            let ordo =
                matches!(self.md, (1, 19) | (1, 26)) || (self.usage == Usage::Greek && self.md == (1, 24));
            (t.luke.get(&(week, wd)), None, back > 3 || ordo)
        } else {
            // Between the Nativity and Theophany the yearly ordo decides. A free weekday reads here
            // the next pericope left unread before the feast, as antiochian.org and goarch.org did
            // in 2021, 2026 and 2027; other years kept the calendar week instead.
            let mut last = nativity - 2;
            while calendar::weekday(last) == 0 {
                last -= 1;
            }
            let (mut week, mut day) = ((((last - jump + 6) / 7) as u8).min(16), calendar::weekday(last));
            for x in nativity + 1..=d {
                if calendar::weekday(x) != 0 && self.free(t, x) {
                    day += 1;
                    if day == 7 {
                        (week, day) = ((week + 1).min(16), 1);
                    }
                }
            }
            (t.luke.get(&(week, day)), None, true)
        }
    }

    /// No Menaion entry reads on day `x`.
    fn free(&self, t: &Tables, x: Day) -> bool {
        let (md, wd) = (self.md_of(x), calendar::weekday(x));
        !t.menaion.iter().any(|f| f.weekday != OnWeekday::Optional && f.when.matches(md, wd))
    }

    /// The number of the Gospel of Luke read on Sunday `d` between the jump and the Nativity: the
    /// dated windows, and 1, 2, 3, 6, 7, 8, 9 in order on the other Sundays.
    fn luke_sunday(&self, jump: Day, d: Day) -> u8 {
        let mut seq = [1u8, 2, 3, 6, 7, 8, 9].into_iter();
        let (mut n, mut sunday) = (1, jump + 7);
        while sunday <= d {
            let md = self.md_of(sunday);
            n = WINDOWS
                .iter()
                .find(|(a, b, _)| md >= *a && md <= *b)
                .map(|w| w.2)
                .or_else(|| seq.next())
                .unwrap_or(n + 1);
            sunday += 7;
        }
        n
    }

    /// Sundays from Theophany to the Triodion. With n Sundays from the Sunday after Theophany to
    /// the Publican, the last before the Triodion reads the Canaanite Woman (17th of Matthew) when
    /// n >= 4 and Zacchaeus (15th of Luke) otherwise; the n-2 between take, in reading order (Luke
    /// first, ascending, then Matthew), the first n-2 of the 12th, 15th and 14th of Luke and the
    /// 16th and 15th of Matthew, leaving out a Luke Gospel read in the autumn (goarch.org every
    /// winter 2018-2027; antiochian.org in 8 of 9). The Epistle goes with the Gospel: the Nth of
    /// Luke takes the Epistle of the (N+17)th Sunday after Pentecost, the Nth of Matthew the Nth's.
    fn winter_sunday(
        &self,
        t: &'static Tables,
        jump: Day,
        publican: Day,
    ) -> (Option<&'static Entry>, Option<u8>, bool) {
        let j8 = self.fixed(1, 8, self.year + 1);
        let after = j8 + (7 - calendar::weekday(j8) as Day) % 7;
        if self.d <= after {
            return (None, None, false);
        }
        let n = ((publican - after) / 7) as usize;
        let slot = ((self.d - after) / 7) as usize;
        let luke = |k: u8| (t.luke.get(&(k, 0)), Some(k + 17), false);
        let matthew = |k: u8| (t.matthew.get(&(k, 0)), Some(k), false);
        if slot + 1 >= n {
            return if n >= 4 { matthew(17) } else { luke(15) };
        }
        let autumn = self.autumn_lukes(jump);
        let mut pool: Vec<(bool, u8)> = [(false, 12), (false, 15), (false, 14), (true, 16), (true, 15)]
            .into_iter()
            .filter(|&(matt, k)| matt || !autumn.contains(&k))
            .take(n - 2)
            .collect();
        pool.sort();
        match pool.get(slot - 1) {
            Some(&(false, k)) => luke(k),
            Some(&(true, k)) => matthew(k),
            // A seven-Sunday winter after an autumn that read the 14th: no rule found.
            None => (t.matthew.get(&(14, 0)), Some(14), true),
        }
    }

    /// The Gospels of Luke read on the autumn Sundays up to the Forefathers.
    fn autumn_lukes(&self, jump: Day) -> HashSet<u8> {
        let end = self.fixed(12, 17, self.year);
        let mut sunday = jump + 7;
        let mut out = HashSet::new();
        while sunday <= end {
            out.insert(self.luke_sunday(jump, sunday));
            sunday += 7;
        }
        out
    }

    /// The Menaion entries that fall today, floats (the Saturdays and Sundays around the feasts)
    /// first.
    fn feasts(&self, t: &'static Tables, plan: &Plan) -> Vec<&'static Feast> {
        let (md, wd) = (self.md, self.wd);
        let mut out: Vec<&'static Feast> = Vec::new();
        for f in &t.menaion {
            if f.weekday == OnWeekday::Optional {
                continue;
            }
            let today = match f.id {
                // Royal Hours on a weekday eve, otherwise on the Friday before.
                "1224.royal_hours" | "0105.royal_hours" => {
                    let When::Date(m, d) = f.when else { continue };
                    let eve = self.fixed(m, d, calendar::church_date(self.d, self.style).0);
                    let held = match calendar::weekday(eve) {
                        6 => eve - 1,
                        0 => eve - 2,
                        _ => eve,
                    };
                    held == self.d
                }
                // The Saturday and Sunday before Theophany move to 3 January when no day of their
                // week falls in 31 December-5 January except the Circumcision.
                "float.sat_before_theophany" | "float.sun_before_theophany" => {
                    f.when.matches(md, wd) || (md == (1, 3) && !self.has_week_day(f))
                }
                // In Antiochian usage St Ignatius on the Sunday before the Nativity is kept the day
                // before (the Archdiocese's service text, 2020).
                "1220.ignatius" if self.usage == Usage::Antiochian => {
                    f.when.matches(md, wd) || (wd == 6 && f.when.matches(self.md_of(self.d + 1), 0))
                }
                // A saint falling on a Saturday of Souls is read the day before (St Haralambos 2018,
                // Sts Bartholomew and Barnabas 2022, the Forty Martyrs 2024: goarch.org). A great
                // feast stays on its day. In Greek usage only a saint with a Matins Gospel of his
                // own moves; another yields (St Jude 2021, 2027).
                _ if !f.float() && f.weekday != OnWeekday::Own && self.souls(self.d + 1) => {
                    let moves = self.usage == Usage::Antiochian || f.own_matins();
                    f.when.matches(md, wd) || (moves && f.when.matches(self.md_of(self.d + 1), 6))
                }
                // In Greek usage such a saint falling on the Ascension is read the day after (the
                // Third Finding of the Forerunner's Head: goarch.org 2023, 2028); Sts Constantine
                // and Helen give their Epistle on the feast (see `menaion`).
                _ if self.usage == Usage::Greek
                    && self.d - self.pascha == 40
                    && f.own_matins()
                    && !f.float()
                    && f.weekday != OnWeekday::Own
                    && f.id != "0521.constantine_helen" =>
                {
                    f.when.matches(md, wd) || f.when.matches(self.md_of(self.d - 1), 4)
                }
                _ => f.when.matches(md, wd),
            };
            if today && !self.moved_away(f) {
                out.push(f);
            }
        }
        out.extend(self.moved_here(t, plan));
        // Floats first; then saints moved here, so that the day's own saint has the last word.
        out.sort_by_key(|f| (!f.float(), f.when.matches(md, wd)));
        out
    }

    /// Day `x` is a Saturday of Souls with a Liturgy of its own: Meatfare Saturday or the Saturday
    /// before Pentecost.
    fn souls(&self, x: Day) -> bool {
        let pascha = calendar::pascha(calendar::civil_date(x).0);
        x - pascha == -57 || x - pascha == 48
    }

    /// Whether a float's weekday falls in its range this church year.
    fn has_week_day(&self, f: &Feast) -> bool {
        let When::Range { weekday, from, .. } = f.when else { return true };
        let (y, _, _) = calendar::church_date(self.d, self.style);
        let start = self.fixed(from.0, from.1, if from.0 == 12 { y - 1 } else { y });
        (start..start + 7).any(|x| calendar::weekday(x) == weekday && f.when.matches(self.md_of(x), weekday))
    }

    /// Where this spring's `bright` entries are read: St George on Bright Monday when 23 April falls
    /// on or before Pascha, then those falling from Palm Sunday to Bright Saturday, in date order,
    /// on their own day or the next free one; `None` when Bright Week is full (goarch.org 2019-2030:
    /// St Mark on Bright Tuesday in 2019, 2021, 2022, 2027 and 2030, St Athanasius on Bright
    /// Wednesday in 2021 and 2027).
    fn bright_days(&self, t: &'static Tables) -> Vec<(&'static Feast, Option<Day>)> {
        let year = calendar::civil_date(self.d).0;
        let pascha = calendar::pascha(year);
        let mut wanted: Vec<(bool, Day, Day, &'static Feast)> = Vec::new();
        for f in t.menaion.iter().filter(|f| f.bright.is_some()) {
            let When::Date(m, d) = f.when else { continue };
            let day = self.fixed(m, d, year);
            let george = f.id == "0423.george" && day <= pascha;
            if george || (pascha - 7..=pascha + 6).contains(&day) {
                wanted.push((!george, day, if george { pascha + 1 } else { day.max(pascha + 1) }, f));
            }
        }
        wanted.sort_by_key(|&(later, day, _, _)| (later, day));
        let mut taken = Vec::new();
        wanted
            .into_iter()
            .map(|(_, _, mut to, f)| {
                while taken.contains(&to) {
                    to += 1;
                }
                taken.push(to);
                (f, (to <= pascha + 6).then_some(to))
            })
            .collect()
    }

    /// A `bright` entry read on another day than its own this year.
    fn moved_away(&self, f: &Feast) -> bool {
        f.bright.is_some()
            && self
                .bright_days(tables(self.usage))
                .iter()
                .any(|(g, to)| std::ptr::eq(*g, f) && *to != Some(self.d))
    }

    fn moved_here(&self, t: &'static Tables, plan: &Plan) -> Vec<&'static Feast> {
        if plan.class != Class::BrightWeek {
            return Vec::new();
        }
        self.bright_days(t)
            .into_iter()
            .filter(|(f, to)| *to == Some(self.d) && !f.when.matches(self.md, self.wd))
            .map(|(f, _)| f)
            .collect()
    }

    fn menaion(&self, t: &'static Tables, plan: &mut Plan) {
        for f in self.feasts(t, plan) {
            let e = &f.entry;
            if f.float() {
                // A Saturday or Sunday around a feast is the day, after Pentecost.
                if plan.cycle {
                    plan.readings.retain(|r| r.service != "Liturgy");
                    plan.add_all(e, false);
                    plan.festal = true;
                }
                continue;
            }
            match f.id {
                "1224.vesperal_liturgy" | "0105.vesperal_liturgy" if matches!(self.wd, 0 | 6) => {
                    // On a Saturday or Sunday the eve's Liturgy is St John Chrysostom's with the
                    // Saturday's or Sunday's readings, and Vespers is served in the evening alone.
                    for r in e.readings.iter().filter(|r| r.service != "Liturgy") {
                        let mut r = r.clone();
                        if r.service == "Vespers" {
                            r.served = Some("in the evening, without the Liturgy");
                        }
                        plan.readings.push(r);
                    }
                    continue;
                }
                "0908.nativity_theotokos" if self.wd == 0 => {
                    // It is then the Sunday before the Elevation, whose readings are read, with the
                    // feast's Matins Gospel (goarch.org 2019, 2024).
                    plan.extras(e, true, false);
                    plan.feast_matins = true;
                    continue;
                }
                "0521.constantine_helen" if self.d - self.pascha == 39 => {
                    // On the Ascension the saints' Epistle is read with the feast's Gospel (the
                    // Archdiocese's service text, GOA DCS and goarch.org, 2026).
                    plan.replace(Kind::Epistle, e);
                    continue;
                }
                "0326.synaxis_gabriel" if matches!(self.d - self.next, -36 | -29 | -22) => {
                    // The leave-taking of the Annunciation: on the 2nd to 4th Saturdays of Lent its
                    // readings are read (the Archdiocese's service text, goarch.org and
                    // antiochian.org, 2022).
                    plan.replace_both(e);
                    continue;
                }
                _ if f.weekday == OnWeekday::Own
                    && matches!(plan.class, Class::Paschal | Class::BrightWeek) =>
                {
                    // The Annunciation in Holy Week, on Pascha or in Bright Week, the Forerunner's
                    // Nativity on Pentecost: the Typikon's special orders apply; the feast's
                    // readings are shown beside the day's.
                    plan.readings.extend(e.readings.iter().filter(|r| r.service != "Vespers").cloned().map(
                        |mut r| {
                            r.confidence = Confidence::Low;
                            r
                        },
                    ));
                    continue;
                }
                _ => {}
            }
            let lent = plan.class == Class::LentWeekday;
            match plan.class {
                Class::Paschal => {}
                Class::BrightWeek => {
                    if let Some(k) = f.bright {
                        // On his own day the saint gives his `bright` reading, as the Archdiocese's
                        // 2025 text for St George and the published calendars did, though its 2020
                        // texts for St George and St Mark read both of his. Moved here, in
                        // Antiochian usage he gives his Epistle with the day's Gospel (the
                        // Archdiocese's texts for St George on Bright Monday and St Mark on Bright
                        // Tuesday); in Greek usage his `bright` reading still (goarch.org: St Mark
                        // on Bright Tuesday 2019, 2021, 2022, 2027).
                        let own = f.when.matches(self.md, self.wd);
                        let antiochian = self.usage == Usage::Antiochian;
                        plan.replace(if own || !antiochian { k } else { Kind::Epistle }, e);
                        plan.extras(e, true, false);
                        plan.readings.retain(|r| r.service != "Vespers" || r.occasion != e.name);
                        if own && antiochian {
                            plan.cap(e, Confidence::Medium);
                        }
                    }
                }
                // Only the great feasts and the saints the calendars print every year (antiochian.org
                // and goarch.org 2018-2027; St Mark in 2024).
                Class::LentWeekday => {
                    let printed =
                        matches!(f.id, "0309.forty_martyrs" | "0224.head_forerunner_1_2" | "0425.mark");
                    if f.weekday == OnWeekday::Own || printed {
                        plan.add_all(e, lent);
                    }
                }
                Class::Sunday | Class::Strong => {
                    // The Saturdays of Souls keep their own readings; their saints were read the day
                    // before (see `feasts`).
                    if self.souls(self.d) {
                        if f.weekday == OnWeekday::Own {
                            plan.replace_both(e);
                            plan.extras(e, true, false);
                        }
                        continue;
                    }
                    // All Saints and the Monday of the Holy Spirit keep their Epistle: a feast gives
                    // only its Gospel, and other saints yield (the Twelve Apostles on All Saints
                    // 2024: the Archdiocese's chart and service text, goarch.org; the Forerunner on
                    // the Monday of the Holy Spirit 2024: goarch.org only).
                    let p = self.d - self.pascha;
                    if matches!(p, 50 | 56) {
                        if f.sunday == OnSunday::Feast {
                            plan.replace(Kind::Gospel, e);
                            plan.extras(e, f.feast_matins || self.wd != 0, false);
                            plan.feast_matins |= f.feast_matins;
                            if p == 50 {
                                plan.cap(e, Confidence::Medium);
                            }
                        }
                        continue;
                    }
                    // St Theodore's and the Ascetics' Saturdays are commemorations themselves: in
                    // Antiochian usage a great saint gives its Gospel with the commemoration's
                    // Epistle (the Forty Martyrs on the Ascetics' Saturday 2019: the Archdiocese's
                    // service text). In Greek usage it gives both, as on another Saturday
                    // (goarch.org: the Forty Martyrs 2019, the Finding of the Head 2018).
                    let commemoration = matches!(self.d - self.next, -50 | -43);
                    // The leave-takings of Pascha and of Pentecost keep their readings against a
                    // saint's Epistle (the Typikon; no published example).
                    let leave_taking = matches!(p, 38 | 55);
                    match f.sunday {
                        OnSunday::Feast if f.weekday == OnWeekday::Own || plan.class == Class::Sunday => {
                            plan.replace_both(e);
                            plan.extras(e, f.feast_matins || self.wd != 0, false);
                            plan.feast_matins |= f.feast_matins;
                        }
                        OnSunday::Epistle if commemoration => {
                            if self.usage == Usage::Greek {
                                plan.replace_both(e);
                            } else {
                                plan.replace(Kind::Gospel, e);
                            }
                            plan.extras(e, true, false);
                        }
                        OnSunday::Epistle if !leave_taking => {
                            plan.replace(Kind::Epistle, e);
                            plan.extras(e, false, false);
                        }
                        _ => {}
                    }
                }
                // A Saturday around a feast, or its readings moved to 3 January.
                Class::Saturday | Class::Weekday if plan.festal => {
                    if f.weekday == OnWeekday::Own {
                        plan.readings.retain(|r| r.service != "Liturgy");
                        plan.add_all(e, false);
                    } else if let Some(k) = f.saturday {
                        plan.replace(k, e);
                        plan.extras(e, true, false);
                    }
                }
                // In Greek usage St Mark yields on a weekday of the Paschal season (goarch.org 2018,
                // 2023, 2028, 2029), though not on a Saturday (2026) or in Bright Week.
                Class::Weekday
                    if self.usage == Usage::Greek && f.id == "0425.mark" && self.d - self.pascha <= 55 => {}
                Class::Saturday | Class::Weekday => {
                    match f.weekday {
                        OnWeekday::Own | OnWeekday::Replaces => plan.replace_both(e),
                        OnWeekday::Epistle => plan.replace(Kind::Epistle, e),
                        // In Greek usage a saint who gives only his Gospel yields to the Gospels of
                        // the Paschal season and of the week after Pentecost (the Prophet Elisha:
                        // goarch.org 2019, 2021, 2022, 2024, 2027, 2030).
                        OnWeekday::Gospel if self.usage == Usage::Greek && self.d - self.pascha <= 55 => {
                            continue;
                        }
                        OnWeekday::Gospel => plan.replace(Kind::Gospel, e),
                        OnWeekday::Optional => continue,
                    }
                    plan.extras(e, true, false);
                }
            }
        }
    }

    /// The Resurrection Gospel at Sunday Matins, from All Saints to the fifth Sunday of Lent.
    fn matins(&self, t: &'static Tables, plan: &mut Plan) {
        let q = self.d - self.next;
        if self.wd != 0 || plan.feast_matins || !(plan.cycle || (-70..=-14).contains(&q)) {
            return;
        }
        let first = self.pascha + 56;
        if self.d < first {
            return;
        }
        let n = ((self.d - first) / 7) as usize % 11;
        plan.readings.extend(t.eothina[n].readings.iter().cloned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::civil;

    /// "Epistle Acts 1:1–8 | Gospel John 1:1–17": the day's Liturgy readings.
    fn liturgy(y: i32, m: u8, d: u8, style: Style, usage: Usage) -> String {
        readings_in(civil(y, m, d), style, usage)
            .iter()
            .filter(|r| r.service == "Liturgy")
            .map(|r| format!("{} {}", r.kind.label(), r.label()))
            .collect::<Vec<_>>()
            .join(" | ")
    }

    /// The default (Greek) usage on the New calendar.
    fn new(y: i32, m: u8, d: u8) -> String {
        liturgy(y, m, d, Style::New, Usage::Greek)
    }

    /// Antiochian usage on the New calendar.
    fn ant(y: i32, m: u8, d: u8) -> String {
        liturgy(y, m, d, Style::New, Usage::Antiochian)
    }

    /// The day's Matins Gospel.
    fn matins(y: i32, m: u8, d: u8, usage: Usage) -> String {
        let rs = readings_in(civil(y, m, d), Style::New, usage);
        rs.iter()
            .find(|r| r.service == "Matins" && r.kind == Kind::Gospel)
            .map_or(String::new(), Reading::label)
    }

    #[test]
    fn usage_names() {
        for u in Usage::ALL {
            assert_eq!(Usage::parse(u.as_str()), Some(u));
        }
        assert_eq!(Usage::default(), Usage::Greek);
        assert_eq!(Usage::parse("slavic"), None);
        assert_eq!(serde_json::to_string(&Usage::Antiochian).unwrap(), "\"antiochian\"");
        // `readings` is the Greek usage.
        let d = civil(2026, 11, 16);
        assert_eq!(readings(d, Style::New), readings_in(d, Style::New, Usage::Greek));
    }

    #[test]
    fn every_reference_is_inside_the_grid() {
        for usage in Usage::ALL {
            grid_and_keys(tables(usage));
        }
    }

    fn grid_and_keys(t: &Tables) {
        let all = t
            .pentecostarion
            .iter()
            .chain(&t.triodion)
            .chain(t.epistles.values())
            .chain(t.matthew.values())
            .chain(t.luke.values())
            .chain(&t.eothina)
            .chain(t.menaion.iter().map(|f| &f.entry));
        let mut n = 0;
        for e in all {
            for r in &e.readings {
                for p in r.passages {
                    assert!(p.verses().count() > 0, "{}: {p:?}", e.name);
                    assert!(ChapterIdx::new(p.book, p.to.0).unwrap().kjv_verses() >= p.to.1);
                    n += 1;
                }
            }
        }
        assert!(n > 1500, "{n} passages");
        // Every key the rules ask for is in the tables.
        for w in 1..=32 {
            for d in 0..7 {
                assert!(t.epistles.contains_key(&(w, d)) || (w, d) == (1, 1), "epistle week {w} day {d}");
            }
        }
        for d in 1..7 {
            assert!(t.epistles.contains_key(&(33, d)), "epistle week 33 day {d}");
        }
        for w in 1..=17 {
            for d in 0..7 {
                assert!(t.matthew.contains_key(&(w, d)) || (w, d) == (1, 1), "matthew week {w} day {d}");
            }
        }
        for w in 1..=16 {
            for d in 0..7 {
                assert!(t.luke.contains_key(&(w, d)) || (w, d) == (16, 0), "luke week {w} day {d}");
            }
        }
    }

    #[test]
    fn every_day_has_readings() {
        for (style, usage) in [Style::New, Style::Old].into_iter().flat_map(|s| Usage::ALL.map(|u| (s, u))) {
            for d in civil(2020, 1, 1)..civil(2040, 1, 1) {
                let rs = readings_in(d, style, usage);
                assert!(!rs.is_empty(), "{:?} {style:?} {usage:?}", calendar::civil_date(d));
                let liturgy = rs.iter().filter(|r| r.service == "Liturgy");
                let (e, g) = liturgy.fold((0, 0), |(e, g), r| match r.kind {
                    Kind::Epistle => (e + 1, g),
                    Kind::Gospel => (e, g + 1),
                    Kind::OldTestament => (e, g),
                });
                let p = calendar::day(d, style).from_pascha;
                let lenten = (-48..=-9).contains(&(d - calendar::pascha(calendar::civil_date(d).0)))
                    && (1..=5).contains(&calendar::weekday(d));
                if !lenten && !(-6..=-1).contains(&p) && !matches!(p, -53 | -51) {
                    assert!(
                        e >= 1 && g >= 1,
                        "{:?} {style:?} {usage:?}: {e} Epistles, {g} Gospels",
                        calendar::civil_date(d)
                    );
                }
            }
        }
    }

    #[test]
    fn pascha_and_its_season() {
        assert_eq!(new(2026, 4, 12), "Epistle Acts 1:1–8 | Gospel John 1:1–17");
        let pascha = readings(civil(2026, 4, 12), Style::New);
        assert_eq!(pascha[0].service, "Matins");
        assert_eq!(pascha[0].label(), "Mark 16:1–8");
        assert_eq!(pascha.last().unwrap().label(), "John 20:19–25", "Agape Vespers");
        // Thomas Sunday, with its eothinon.
        assert_eq!(new(2026, 4, 19), "Epistle Acts 5:12–20 | Gospel John 20:19–31");
        let thomas = readings(civil(2026, 4, 19), Style::New);
        assert_eq!(thomas[0].title, Some("Eothinon 1"));
        assert_eq!(new(2026, 4, 14), "Epistle Acts 2:14–21 | Gospel Luke 24:12–35", "Bright Tuesday");
        assert_eq!(new(2027, 6, 10), "Epistle Acts 1:1–12 | Gospel Luke 24:36–53", "Ascension");
        assert_eq!(new(2026, 5, 31), "Epistle Acts 2:1–11 | Gospel John 7:37–52; 8:12", "Pentecost");
        // St George on a weekday after Thomas Sunday.
        assert_eq!(new(2026, 4, 23), "Epistle Acts 12:1–11 | Gospel John 15:17–16:2");
    }

    /// The confidence of the day's Liturgy reading of `kind`.
    fn sure(y: i32, m: u8, d: u8, kind: Kind, usage: Usage) -> Confidence {
        let rs = readings_in(civil(y, m, d), Style::New, usage);
        rs.iter().find(|r| r.service == "Liturgy" && r.kind == kind).unwrap().confidence
    }

    #[test]
    fn bright_week_saints() {
        use Usage::Antiochian as A;
        // 2027: St George (23 April, in Lent) moves to Bright Monday and St Mark (25 April, Palm
        // Sunday) to Bright Tuesday. In Antiochian usage each gives his Epistle with the day's
        // Gospel (the Archdiocese's service texts).
        assert_eq!(ant(2027, 5, 3), "Epistle Acts 12:1–11 | Gospel John 1:18–28");
        assert_eq!(ant(2027, 5, 4), "Epistle 1 Peter 5:6–14 | Gospel Luke 24:12–35");
        assert_eq!(ant(2027, 4, 23), "", "Lenten Friday: no Liturgy");
        // In Greek usage each gives his own Bright Week reading: St Mark his Gospel
        // (goarch.org 2019, 2021, 2022, 2027).
        assert_eq!(new(2027, 5, 3), "Epistle Acts 12:1–11 | Gospel John 1:18–28");
        assert_eq!(new(2027, 5, 4), "Epistle Acts 2:14–21 | Gospel Luke 10:16–21");
        // 2025: St George on his own day, Bright Wednesday (service text 2025). In Antiochian usage
        // at medium confidence: the 2020 texts read both of a saint's readings on his own day.
        assert_eq!(ant(2025, 4, 23), "Epistle Acts 12:1–11 | Gospel John 1:35–51");
        assert_eq!(new(2025, 4, 23), "Epistle Acts 12:1–11 | Gospel John 1:35–51");
        assert_eq!(sure(2025, 4, 23, Kind::Epistle, A), Confidence::Medium);
        assert_eq!(sure(2025, 4, 23, Kind::Epistle, Usage::Greek), Confidence::High);
        assert_eq!(sure(2027, 5, 3, Kind::Epistle, A), Confidence::High);
        // The Life-giving Spring: in Antiochian usage the Theotokos's Epistle with the day's
        // Gospel; in Greek usage the day's readings (with St Mark's Gospel in 2025: goarch.org).
        assert_eq!(ant(2026, 4, 17), "Epistle Philippians 2:5–11 | Gospel John 2:12–22");
        assert_eq!(new(2026, 4, 17), "Epistle Acts 3:1–8 | Gospel John 2:12–22");
        assert_eq!(new(2025, 4, 25), "Epistle Acts 3:1–8 | Gospel Luke 10:16–21");
    }

    #[test]
    fn feasts_meeting_the_great_days() {
        // Sts Constantine and Helen on the Ascension give their Epistle (service text 2026).
        assert_eq!(new(2026, 5, 21), "Epistle Acts 26:1, 12–20 | Gospel Luke 24:36–53");
        // All Saints keeps its Epistle; the Twelve Apostles give their Gospel (chart 2024,
        // goarch.org 2024, each with its own verses), and a lesser saint yields (Sts Cosmas and
        // Damian, 2040).
        assert_eq!(ant(2024, 6, 30), "Epistle Hebrews 11:33–12:2 | Gospel Matthew 9:36–10:8");
        assert_eq!(new(2024, 6, 30), "Epistle Hebrews 11:33–12:2 | Gospel Matthew 9:36; 10:1–8");
        assert_eq!(new(2040, 7, 1), "Epistle Hebrews 11:33–12:2 | Gospel Matthew 10:32–33, 37–38; 19:27–30");
        // So does the Monday of the Holy Spirit, with the Forerunner's Gospel (goarch.org 2024).
        assert_eq!(new(2024, 6, 24), "Epistle Ephesians 5:8–19 | Gospel Luke 1:1–25, 57–68, 76, 80");
        assert_eq!(sure(2024, 6, 24, Kind::Gospel, Usage::Greek), Confidence::Medium);
        assert_eq!(new(2037, 5, 25), "Epistle Ephesians 5:8–19 | Gospel Matthew 18:10–20");
        // The leave-takings of Pascha and of Pentecost keep their readings.
        assert_eq!(new(2031, 5, 21), "Epistle Acts 18:22–28 | Gospel John 12:36–47");
        assert_eq!(new(2051, 7, 1), "Epistle Romans 1:7–12 | Gospel Matthew 5:42–48");
        // A great feast on the Saturday of Souls stays on its day: not read on the Friday too.
        let forerunner = |d: u8| {
            readings(civil(2051, 6, d), Style::New)
                .iter()
                .filter(|r| r.occasion.contains("Forerunner"))
                .count()
        };
        assert_eq!((forerunner(23), forerunner(24)), (0, 6));
        // The Forerunner's Nativity on Pentecost (2040) is shown beside it, at low confidence.
        let pentecost = readings(civil(2040, 6, 24), Style::New);
        let feast: Vec<_> = pentecost.iter().filter(|r| r.occasion.contains("Forerunner")).collect();
        assert_eq!(feast.len(), 3);
        assert!(feast.iter().all(|r| r.confidence == Confidence::Low));
        assert!(pentecost.iter().any(|r| r.label() == "Acts 2:1–11"));
    }

    #[test]
    fn saints_in_lent_and_the_triodion() {
        // The Synaxis of Gabriel (the Annunciation's leave-taking) on a Saturday of Lent (service
        // text 2022), but not on a Sunday of Lent (2023).
        assert_eq!(new(2022, 3, 26), "Epistle Hebrews 2:2–10 | Gospel Luke 10:16–21");
        assert_eq!(new(2023, 3, 26), "Epistle Hebrews 6:13–20 | Gospel Mark 9:17–31");
        // The Forty Martyrs on the Ascetics' Saturday: in Antiochian usage they give their Gospel
        // (service text 2019); in Greek usage both their readings (goarch.org 2019), as the
        // Finding of the Head did on St Theodore's Saturday (2018).
        assert_eq!(ant(2019, 3, 9), "Epistle Galatians 5:22–6:2 | Gospel Matthew 20:1–16");
        assert_eq!(new(2019, 3, 9), "Epistle Hebrews 12:1–10 | Gospel Matthew 20:1–16");
        assert_eq!(new(2018, 2, 24), "Epistle 2 Corinthians 4:6–15 | Gospel Matthew 11:2–15");
        // St Raphael on 27 February before Lent, an Antiochian saint; in Greek usage the day's.
        assert_eq!(ant(2027, 2, 27), "Epistle Hebrews 7:26–8:2 | Gospel John 10:9–16");
        assert_eq!(new(2027, 2, 27), "Epistle 1 Timothy 6:11–16 | Gospel Luke 20:46–21:4");
        assert_eq!(ant(2026, 2, 27), "", "Lenten Friday: no Liturgy");
        // The Akathist Saturday's Gospel.
        assert_eq!(ant(2026, 3, 28), "Epistle Hebrews 9:1–7 | Gospel Luke 10:38–42; 11:27–28");
        assert_eq!(new(2026, 3, 28), "Epistle Hebrews 9:1–7 | Gospel Luke 1:39–49, 56");
    }

    #[test]
    fn antiochian_saints() {
        use Usage::Antiochian as A;
        // The Archdiocese's service texts.
        assert_eq!(
            ant(2026, 7, 10),
            "Epistle 2 Timothy 2:1–10 | Gospel John 15:17–16:2",
            "Joseph of Damascus"
        );
        assert_eq!(ant(2022, 7, 10), "Epistle 2 Timothy 2:1–10 | Gospel Matthew 8:5–13", "on a Sunday");
        assert_eq!(ant(2026, 7, 16), "Epistle Acts 12:1–11 | Gospel John 15:17–16:2", "Nicholas and Habib");
        assert_eq!(ant(2026, 7, 18), "Epistle Hebrews 7:26–8:2 | Gospel John 10:9–16", "Raphael");
        assert_eq!(
            ant(2026, 11, 7),
            "Epistle Hebrews 7:26–8:2 | Gospel John 10:9–16",
            "first Saturday of November"
        );
        assert_eq!(ant(2026, 2, 6), "Epistle 2 Timothy 2:1–10 | Gospel John 15:17–16:2", "Julian of Homs");
        assert_eq!(
            ant(2026, 1, 24),
            "Epistle Galatians 3:23–4:5 | Gospel Mark 5:24–34",
            "Xenia of St Petersburg"
        );
        assert_eq!(ant(2026, 9, 24), "Epistle 2 Timothy 3:10–15 | Gospel Matthew 25:1–13", "Thekla");
        assert_eq!(ant(2026, 11, 16), "Epistle 1 Corinthians 4:9–16 | Gospel Matthew 9:9–13", "Matthew");
        assert_eq!(ant(2026, 6, 30), "Epistle 1 Corinthians 4:9–16 | Gospel Matthew 9:36–10:8");
        assert_eq!(ant(2026, 5, 8), "Epistle 1 John 1:1–7 | Gospel John 19:25–27; 21:24–25");
        assert_eq!(
            ant(2026, 9, 14),
            "Epistle 1 Corinthians 1:18–24 | Gospel John 19:6–11, 13–20, 25–28, 30–35",
            "Elevation"
        );
        // St Nektarios on a Sunday gives his Epistle (service text 2025).
        assert_eq!(ant(2025, 11, 9), "Epistle Ephesians 5:8–19 | Gospel Luke 8:41–56");
        // St Ignatius on the Sunday before the Nativity is kept the Saturday before, with the
        // Saturday's Gospel (service text 2020).
        assert_eq!(ant(2026, 12, 19), "Epistle Hebrews 10:32–38 | Gospel Luke 13:19–29");
        assert_eq!(ant(2020, 12, 19), "Epistle Hebrews 10:32–38 | Gospel Luke 13:19–29");
        // The Protection on 28 October is not kept: the day's readings.
        assert_eq!(ant(2026, 10, 28), "Epistle Colossians 3:17–4:1 | Gospel Luke 11:9–13");
        assert_eq!(ant(2018, 10, 28), "Epistle Galatians 6:11–18 | Gospel Luke 8:41–56");
        // Matins: Eothinon 11 (as goarch.org), the Apostles' Gospel on 11 June and St John's.
        assert_eq!(matins(2026, 1, 25, A), "John 21:14–25");
        assert_eq!(matins(2026, 1, 25, Usage::Greek), "John 21:14–25");
        assert_eq!(matins(2026, 6, 11, A), "Matthew 9:36–10:8");
        assert_eq!(matins(2026, 5, 8, A), "John 21:14–25");
        // Holy Wednesday's Exodus with the naming of Eliezer, as GOA DCS reads it too.
        for usage in Usage::ALL {
            let wednesday = readings_in(civil(2026, 4, 8), Style::New, usage);
            assert!(wednesday.iter().any(|r| r.label() == "Exodus 2:11–22; 18:4"));
        }
    }

    #[test]
    fn greek_saints() {
        use Usage::Greek as G;
        // goarch.org reads the day's readings where the Archdiocese keeps its own saints: Joseph of
        // Damascus, Nicholas and Habib Khasha, Raphael of Brooklyn.
        assert_eq!(new(2026, 7, 10), "Epistle 1 Corinthians 4:5–8 | Gospel Matthew 13:44–54");
        assert_eq!(new(2026, 7, 16), "Epistle 1 Corinthians 7:24–35 | Gospel Matthew 15:12–21");
        assert_eq!(new(2026, 7, 18), "Epistle Romans 12:1–3 | Gospel Matthew 10:37–11:1");
        assert_eq!(new(2026, 11, 7), "Epistle 2 Corinthians 8:1–5 | Gospel Luke 9:1–6");
        // St Photios on 6 February, and on a Sunday his Epistle (2022).
        assert_eq!(new(2026, 2, 6), "Epistle Hebrews 7:26–8:2 | Gospel John 10:9–16");
        assert_eq!(matins(2026, 2, 6, G), "John 10:1–9");
        assert_eq!(new(2022, 2, 6), "Epistle Hebrews 7:26–8:2 | Gospel Matthew 15:21–28");
        // St Xenia of Rome's Epistle; the ordo chooses the Gospel.
        assert_eq!(new(2026, 1, 24).split(" | ").next(), Some("Epistle Galatians 5:22–6:2"));
        assert_eq!(sure(2026, 1, 24, Kind::Gospel, G), Confidence::Low);
        assert_eq!(matins(2026, 1, 24, G), "");
        // St Thekla's Epistle with the Gospel and Matins Gospel of the Myrtidiotissa.
        assert_eq!(new(2026, 9, 24), "Epistle 2 Timothy 3:10–15 | Gospel Luke 10:38–42; 11:27–28");
        assert_eq!(matins(2026, 9, 24, G), "Luke 1:39–49, 56");
        // The Apostle Matthew, the Twelve Apostles, St John, the Elevation: goarch.org's verses.
        assert_eq!(new(2026, 11, 16), "Epistle Romans 10:11–11:2 | Gospel Matthew 9:9–13");
        assert_eq!(new(2026, 6, 30), "Epistle 1 Corinthians 4:9–16 | Gospel Matthew 9:36; 10:1–8");
        assert_eq!(new(2026, 5, 8), "Epistle 1 John 1:1–7 | Gospel John 19:25–28; 21:24–25");
        assert_eq!(matins(2026, 5, 8, G), "John 21:15–25");
        assert_eq!(new(2026, 9, 14), "Epistle 1 Corinthians 1:18–24 | Gospel John 19:6–11, 13–20, 25–28, 30");
        assert_eq!(matins(2026, 6, 11, G), "Luke 10:16–21", "Bartholomew and Barnabas");
        // The Protection on 28 October, on a Sunday its Epistle with the Sunday Gospel (2018).
        assert_eq!(new(2026, 10, 28), "Epistle Hebrews 9:1–7 | Gospel Luke 10:38–42; 11:27–28");
        assert_eq!(matins(2026, 10, 28, G), "Luke 1:39–49, 56");
        assert_eq!(new(2018, 10, 28), "Epistle Hebrews 9:1–7 | Gospel Luke 8:41–56");
        // On a Sunday St Nektarios yields (2025), the Three Hierarchs give both readings (2022), St
        // Tryphon his Epistle, even on the Publican's Sunday (2026).
        assert_eq!(new(2025, 11, 9), "Epistle Galatians 6:11–18 | Gospel Luke 8:41–56");
        assert_eq!(new(2022, 1, 30), "Epistle Hebrews 13:7–16 | Gospel Matthew 5:14–19");
        assert_eq!(matins(2022, 1, 30, G), "John 21:1–14");
        assert_eq!(new(2026, 2, 1), "Epistle Romans 8:28–39 | Gospel Luke 18:10–14");
        assert_eq!(ant(2026, 2, 1), "Epistle 2 Timothy 3:10–15 | Gospel Luke 18:10–14");
        // St Ignatius stays on his day: the Saturday before the Nativity reads its own (2025, 2026).
        assert_eq!(new(2025, 12, 20), "Epistle Galatians 3:8–12 | Gospel Luke 13:19–29");
        assert_eq!(new(2026, 12, 19), "Epistle Galatians 3:8–12 | Gospel Luke 13:19–29");
        // The Prophet Elisha's Gospel yields in the Paschal season (2024, 2027), and St Mark on a
        // weekday of it (2028), though not on a Saturday (2026).
        assert_eq!(new(2024, 6, 14), "Epistle Acts 19:1–8 | Gospel John 14:1–11");
        assert_eq!(ant(2024, 6, 14), "Epistle Acts 19:1–8 | Gospel Luke 4:22–30");
        assert_eq!(new(2028, 4, 25), "Epistle Acts 4:1–10 | Gospel John 3:16–21");
        assert_eq!(new(2026, 4, 25), "Epistle 1 Peter 5:6–14 | Gospel Luke 10:16–21");
        // The Third Finding of the Head on the Ascension is read the day after (2028); Sts
        // Constantine and Helen are not (2026).
        assert_eq!(new(2028, 5, 26), "Epistle 2 Corinthians 4:6–15 | Gospel Matthew 11:2–15");
        assert_eq!(ant(2028, 5, 26), "Epistle Acts 19:1–8 | Gospel John 14:1–11");
        assert_eq!(new(2026, 5, 22), "Epistle Acts 19:1–8 | Gospel John 14:1–11");
        // St Jude on the Saturday of Souls yields (2027); Sts Bartholomew and Barnabas, with a Matins
        // Gospel of their own, are read the day before (2022), in both usages.
        let jude = |usage| {
            readings_in(civil(2027, 6, 18), Style::New, usage)
                .iter()
                .filter(|r| r.occasion.contains("Jude"))
                .count()
        };
        assert_eq!((jude(G), jude(Usage::Antiochian) > 0), (0, true));
        assert_eq!(new(2027, 6, 18), "Epistle Acts 27:1–28:1 | Gospel John 17:18–26");
        assert_eq!(new(2022, 6, 10), "Epistle Acts 11:19–30 | Gospel Luke 10:16–21");
        assert_eq!(ant(2022, 6, 10), "Epistle Acts 11:19–30 | Gospel Luke 10:16–21");
    }

    #[test]
    fn greek_moveable_days() {
        // The 2nd Sunday after Pentecost: the cycle's Epistle, not All Saints of Antioch.
        assert_eq!(new(2026, 6, 14), "Epistle Romans 2:10–16 | Gospel Matthew 4:18–23");
        assert_eq!(ant(2026, 6, 14), "Epistle Acts 11:19–30 | Gospel Matthew 4:18–23");
        // The Saturday of Souls before Pentecost: the Epistle of the departed.
        assert_eq!(new(2026, 5, 30), "Epistle 1 Thessalonians 4:13–17 | Gospel John 21:14–25");
        assert_eq!(ant(2026, 5, 30), "Epistle Acts 28:1–31 | Gospel John 21:14–25");
        // Holy Thursday's Gospel.
        let thursday = |usage| {
            let rs = readings_in(civil(2026, 4, 9), Style::New, usage);
            rs.iter().find(|r| r.service.starts_with("Vesperal") && r.kind == Kind::Gospel).unwrap().label()
        };
        assert_eq!(
            thursday(Usage::Greek),
            "Matthew 26:1–20; John 13:3–17; Matthew 26:21–39; Luke 22:43–44; Matthew 26:40–27:2"
        );
        assert_eq!(
            thursday(Usage::Antiochian),
            "Matthew 26:2–20; John 13:3–17; Matthew 26:21–39; Luke 22:43–45; Matthew 26:40–27:2"
        );
        // Weekday boundaries as goarch.org prints them.
        assert_eq!(new(2026, 6, 13), "Epistle Romans 3:19–24 | Gospel Matthew 7:1–8");
        assert_eq!(new(2026, 6, 15).split(" | ").next(), Some("Epistle Romans 7:1–14"));
        assert_eq!(new(2026, 10, 12).split(" | ").next(), Some("Epistle Philippians 2:12–15"));
    }

    #[test]
    fn parish_sundays() {
        // Holy Trinity Greek Orthodox Church, Augusta (GOARCH Bulletin Builder, 2026).
        let sunday = |m, d| format!("{} | Matins {}", new(2026, m, d), matins(2026, m, d, Usage::Greek));
        assert_eq!(
            sunday(8, 9),
            "Epistle 1 Corinthians 4:9–16 | Gospel Matthew 17:14–23 | Matins John 21:1–14"
        );
        assert_eq!(
            sunday(8, 23),
            "Epistle 1 Corinthians 15:1–11 | Gospel Matthew 19:16–26 | Matins Matthew 28:16–20"
        );
        assert_eq!(
            sunday(8, 30),
            "Epistle 1 Corinthians 16:13–24 | Gospel Matthew 21:33–42 | Matins Mark 16:1–8"
        );
        assert_eq!(sunday(9, 13), "Epistle Galatians 6:11–18 | Gospel John 3:13–17 | Matins Luke 24:1–12");
        assert_eq!(sunday(9, 20), "Epistle Galatians 2:16–20 | Gospel Mark 8:34–9:1 | Matins Luke 24:12–35");
        assert_eq!(
            sunday(9, 27),
            "Epistle 2 Corinthians 6:16–7:1 | Gospel Luke 5:1–11 | Matins Luke 24:36–53"
        );
        assert_eq!(sunday(10, 11), "Epistle Titus 3:8–15 | Gospel Luke 8:5–15 | Matins John 20:11–18");
    }

    #[test]
    fn matthew_weeks_and_the_lukan_jump() {
        // 2026: Pentecost 31 May. Thursday 9 July is the Thursday of the 6th week (antiochian.org).
        assert_eq!(new(2026, 7, 9), "Epistle 1 Corinthians 3:18–23 | Gospel Matthew 13:36–43");
        // The Sunday after the Elevation (20 September) ends Matthew; Monday 21 September reads
        // Luke week 1 Monday, while the Epistle stays in the 17th week after Pentecost.
        assert_eq!(new(2026, 9, 20), "Epistle Galatians 2:16–20 | Gospel Mark 8:34–9:1");
        assert_eq!(new(2026, 9, 21), "Epistle Ephesians 1:22–2:3 | Gospel Luke 3:19–22");
        // 20 October 2026: Tuesday of the 21st week and of the 5th week of Luke, with St Artemius's
        // Epistle (antiochian.org).
        assert_eq!(new(2026, 10, 20), "Epistle 2 Timothy 2:1–10 | Gospel Luke 9:23–27");
        assert_eq!(new(2026, 10, 21), "Epistle 2 Corinthians 9:6–11 | Gospel Luke 9:44–50");
        // The 1st Sunday of Luke, with the 17th Sunday's Epistle.
        assert_eq!(new(2026, 9, 27), "Epistle 2 Corinthians 6:16–7:1 | Gospel Luke 5:1–11");
        // St Luke on a Sunday takes it.
        assert_eq!(new(2026, 10, 18), "Epistle Colossians 4:5–11, 14–18 | Gospel Luke 10:16–21");
    }

    #[test]
    fn autumn_sundays_of_luke() {
        // Antiochian chart 2026.
        assert_eq!(new(2026, 10, 4), "Epistle 2 Corinthians 9:6–11 | Gospel Luke 6:31–36");
        assert_eq!(new(2026, 10, 11), "Epistle Titus 3:8–15 | Gospel Luke 8:5–15");
        assert_eq!(new(2026, 10, 25), "Epistle Galatians 2:16–20 | Gospel Luke 8:26–39");
        assert_eq!(new(2026, 11, 1), "Epistle 1 Corinthians 12:27–13:8 | Gospel Luke 16:19–31");
        assert_eq!(new(2026, 11, 8), "Epistle Hebrews 2:2–10 | Gospel Luke 8:41–56");
        assert_eq!(new(2026, 11, 29), "Epistle Ephesians 5:8–19 | Gospel Luke 18:18–27");
        assert_eq!(new(2026, 12, 6), "Epistle Hebrews 13:17–21 | Gospel Luke 13:10–17");
        assert_eq!(new(2026, 12, 13), "Epistle Colossians 3:4–11 | Gospel Luke 14:16–24");
    }

    #[test]
    fn nativity_and_theophany() {
        assert_eq!(new(2026, 12, 25), "Epistle Galatians 4:4–7 | Gospel Matthew 2:1–12");
        assert_eq!(new(2026, 12, 20), "Epistle Hebrews 11:9–10, 32–40 | Gospel Matthew 1:1–25");
        // St Stephen's Epistle on the Sunday after the Nativity (chart 2026).
        assert_eq!(new(2026, 12, 27), "Epistle Acts 6:8–7:5, 47–60 | Gospel Matthew 2:13–23");
        let eve = readings(civil(2026, 12, 24), Style::New);
        assert!(eve.iter().any(|r| r.service == "Royal Hours" && r.title == Some("Ninth Hour")));
        assert_eq!(new(2026, 12, 24), "Epistle Hebrews 1:1–12 | Gospel Luke 2:1–20");
        assert_eq!(new(2026, 1, 6), "Epistle Titus 2:11–14; 3:4–7 | Gospel Matthew 3:13–17");
        assert_eq!(new(2026, 1, 4), "Epistle 2 Timothy 4:5–8 | Gospel Mark 1:1–8");
        // St Theodosius's Epistle on the Sunday after Theophany (chart 2026).
        assert_eq!(new(2026, 1, 11), "Epistle Hebrews 13:7–16 | Gospel Matthew 4:12–17");
        // On the Old calendar the Nativity is on 7 January.
        assert_eq!(
            liturgy(2027, 1, 7, Style::Old, Usage::Greek),
            "Epistle Galatians 4:4–7 | Gospel Matthew 2:1–12"
        );
    }

    #[test]
    fn winter_sundays() {
        // 2026 (n = 3): the 12th of Luke with St Athanasius's Epistle, then Zacchaeus.
        assert_eq!(new(2026, 1, 18), "Epistle Hebrews 13:7–16 | Gospel Luke 17:12–19");
        assert_eq!(new(2026, 1, 25), "Epistle Hebrews 7:26–8:2 | Gospel Luke 19:1–10");
        // 2027 (n = 6): 12th, 14th, 15th of Luke, 16th of Matthew, then the Canaanite Woman.
        assert_eq!(new(2027, 1, 24), "Epistle 1 Timothy 1:15–17 | Gospel Luke 18:35–43");
        assert_eq!(new(2027, 2, 7), "Epistle 2 Corinthians 6:1–10 | Gospel Matthew 25:14–30");
        assert_eq!(new(2027, 2, 14), "Epistle 2 Corinthians 6:16–7:1 | Gospel Matthew 15:21–28");
    }

    #[test]
    fn triodion_and_lent() {
        // The Publican and the Pharisee (in Greek usage St Tryphon gave his Epistle in 2026).
        assert_eq!(ant(2026, 2, 1), "Epistle 2 Timothy 3:10–15 | Gospel Luke 18:10–14");
        assert_eq!(new(2027, 2, 21), "Epistle 2 Timothy 3:10–15 | Gospel Luke 18:10–14");
        // A Lenten weekday has no Liturgy: Isaiah at the Sixth Hour, Genesis and Proverbs at Vespers.
        let clean = readings(civil(2026, 2, 23), Style::New);
        let shown: Vec<_> = clean.iter().map(|r| (r.service, r.label())).collect();
        assert_eq!(
            shown,
            [
                ("Sixth Hour", "Isaiah 1:1–20".into()),
                ("Vespers", "Genesis 1:1–13".into()),
                ("Vespers", "Proverbs 1:1–20".into())
            ]
        );
        // The second Saturday of Lent (a Saturday of Souls).
        assert_eq!(new(2026, 3, 7), "Epistle Hebrews 3:12–16 | Gospel Mark 1:35–44");
        // The Annunciation on a Lenten Wednesday.
        assert_eq!(new(2026, 3, 25), "Epistle Hebrews 2:11–18 | Gospel Luke 1:24–38");
        assert_eq!(new(2026, 4, 5), "Epistle Philippians 4:4–9 | Gospel John 12:1–18", "Palm Sunday");
    }

    #[test]
    fn holy_week() {
        let friday = readings(civil(2026, 4, 10), Style::New);
        let passion: Vec<_> = friday.iter().filter(|r| r.service.contains("Twelve")).collect();
        assert_eq!(passion.len(), 12);
        assert_eq!(passion[0].label(), "John 13:31–18:1");
        assert_eq!(friday.iter().filter(|r| r.service == "Royal Hours").count(), 12);
        let saturday = readings(civil(2026, 4, 11), Style::New);
        assert_eq!(saturday.iter().filter(|r| r.kind == Kind::OldTestament && r.at_liturgy()).count(), 15);
        assert_eq!(saturday.last().unwrap().label(), "Matthew 28:1–20");
    }

    #[test]
    fn labels() {
        let p = |s| parse_passage(s).unwrap();
        assert_eq!(label(&[p("Acts 1:12-17"), p("Acts 1:21-26")]), "Acts 1:12–17, 21–26");
        assert_eq!(label(&[p("Rom 14:19-23"), p("Rom 16:25-27")]), "Romans 14:19–23; 16:25–27");
        assert_eq!(label(&[p("John 7:37-52"), p("John 8:12")]), "John 7:37–52; 8:12");
        assert_eq!(label(&[p("Matt 27:1-38"), p("Luke 23:39-43")]), "Matthew 27:1–38; Luke 23:39–43");
        assert_eq!(label(&[p("Jude 1:1-10")]), "Jude 1:1–10");
        assert!(parse_passage("Matt 29:1").is_err());
        assert!(parse_passage("Mark 16:21").is_err());
        assert!(parse_passage("Gen 2:1-1:3").is_err());
        assert_eq!(p("Pr Azar 1:1-66").book, BookId::from_code("S3Y").unwrap());
    }
}
