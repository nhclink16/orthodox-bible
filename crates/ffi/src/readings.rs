//! The day's readings (Orthodox fork, `bible_core::lectionary`): each lesson with the passages the
//! reader opens, and the occasion it's read for when that isn't the day's own.

use std::collections::HashSet;

use bible_core::calendar::{self, Style};
use bible_core::lectionary::{self, Confidence, Kind};

use crate::VerseRef;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ReadingKind {
    OldTestament,
    /// The Apostle: Acts and the Epistles.
    Epistle,
    Gospel,
}

/// How sure the lectionary is of a reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ReadingConfidence {
    /// An Antiochian source and an independent one agree.
    High,
    /// The sources differ, usually by a verse at the start or the end.
    Medium,
    /// The yearly ordo decides, or no Antiochian or Greek source was found.
    Low,
}

/// A run of verses in the grid's numbering, from `from` to `to` (inclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ReadingPassage {
    pub from: VerseRef,
    pub to: VerseRef,
    /// The chapter it starts in, to open.
    pub chapter_index: u16,
}

/// One lesson: the passages read together at a service.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Reading {
    /// "Liturgy", "Matins", "Sixth Hour", "Royal Hours", "Presanctified Liturgy"…
    pub service: String,
    /// Read at the Divine Liturgy (St John Chrysostom's, St Basil's or the Presanctified).
    pub at_liturgy: bool,
    pub kind: ReadingKind,
    /// "Acts 1:12–17, 21–26", in the psalm numbering shown.
    pub label: String,
    pub passages: Vec<ReadingPassage>,
    /// What tells it from the day's other lessons: "1st", "Eothinon 4", "First Hour".
    pub title: Option<String>,
    /// When the service is held, if not at its usual time: "evening before".
    pub served: Option<String>,
    /// The saint or feast it's read for, when that isn't the day's own: St Artemius's Epistle
    /// beside a weekday's Gospel.
    pub occasion: Option<String>,
    pub confidence: ReadingConfidence,
}

/// The readings of a civil date in service order: the Vespers of the evening before, Matins, the
/// Hours, the Liturgy, then the day's evening services. On the New or Old calendar.
#[uniffi::export]
pub fn day_readings(year: i32, month: u8, day: u8, old_calendar: bool) -> Vec<Reading> {
    let style = crate::rule::style(old_calendar);
    let rs = lectionary::readings(calendar::civil(year, month, day), style);
    let fixed = fixed(year, month, day, style);
    // An occasion is the day's own unless it's a saint's or a fixed feast's that doesn't take
    // the whole day: one the Liturgy's Epistle and Gospel (without them, all the day's readings)
    // don't all share.
    let main: Vec<&lectionary::Reading> =
        rs.iter().filter(|r| r.at_liturgy() && r.kind != Kind::OldTestament).collect();
    let anchor = if main.is_empty() { rs.iter().collect() } else { main };
    let own = |o: &str| !fixed.contains(o) || anchor.iter().all(|r| r.occasion == o);
    rs.iter()
        .map(|r| Reading {
            service: r.service.into(),
            at_liturgy: r.at_liturgy(),
            kind: match r.kind {
                Kind::OldTestament => ReadingKind::OldTestament,
                Kind::Epistle => ReadingKind::Epistle,
                Kind::Gospel => ReadingKind::Gospel,
            },
            label: r.label(),
            passages: r
                .passages
                .iter()
                .map(|p| ReadingPassage {
                    from: p.first().into(),
                    to: p.last().into(),
                    chapter_index: p.chapter().0,
                })
                .collect(),
            title: r.title.map(Into::into),
            served: r.served.map(Into::into),
            occasion: (!own(r.occasion)).then(|| r.occasion.into()),
            confidence: match r.confidence {
                Confidence::High => ReadingConfidence::High,
                Confidence::Medium => ReadingConfidence::Medium,
                Confidence::Low => ReadingConfidence::Low,
            },
        })
        .collect()
}

/// The occasions read on this civil date a year before and a year after: the saints' and the
/// fixed feasts' (the Menaion's), which keep their date. The weekly cycles' and Pascha's never fall
/// on the same date two years running, since the date's weekday moves on.
fn fixed(year: i32, month: u8, day: u8, style: Style) -> HashSet<&'static str> {
    // 29 February comes round again four years on.
    let step = if (month, day) == (2, 29) { 4 } else { 1 };
    [year - step, year + step]
        .into_iter()
        .flat_map(|y| lectionary::readings(calendar::civil(y, month, day), style))
        .map(|r| r.occasion)
        .collect()
}

/// What the rule's Readings and Fathers items open: the Liturgy's Gospel, else the day's first
/// Gospel, else its first reading; with the line that names it ("Luke 9:23–27", "No Gospel today
/// · Isaiah 5:16–25 at the Sixth Hour").
pub(crate) fn gospel(rs: &[lectionary::Reading]) -> Option<(&lectionary::Reading, String)> {
    if let Some(g) = rs.iter().find(|r| r.at_liturgy() && r.kind == Kind::Gospel) {
        return Some((g, g.label()));
    }
    if let Some(g) = rs.iter().find(|r| r.kind == Kind::Gospel) {
        return Some((g, format!("{} {}", g.label(), at(g.service))));
    }
    let first = rs.first()?;
    Some((first, format!("No Gospel today · {} {}", first.label(), at(first.service))))
}

/// "at Matins", "at the Sixth Hour".
fn at(service: &str) -> String {
    if service.starts_with("Matins") || service.starts_with("Vespers") {
        format!("at {service}")
    } else {
        format!("at the {service}")
    }
}
