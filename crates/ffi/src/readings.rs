//! The day's readings (Orthodox fork, `bible_core::lectionary`): each lesson with the passages the
//! reader opens, and the occasion it's read for when the day's great feast doesn't name it.

use std::collections::HashSet;
use std::sync::OnceLock;

use bible_core::calendar::{self, Style};
use bible_core::lectionary::{self, Confidence, Kind, Usage};

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

/// Whose calendar of readings to follow where the churches differ (`lectionary::Usage`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ReadingUsage {
    /// The Greek Orthodox Archdiocese of America, as goarch.org prints them.
    Greek,
    /// The Antiochian Archdiocese, with its own saints.
    Antiochian,
}

impl From<ReadingUsage> for Usage {
    fn from(u: ReadingUsage) -> Self {
        match u {
            ReadingUsage::Greek => Usage::Greek,
            ReadingUsage::Antiochian => Usage::Antiochian,
        }
    }
}

/// A run of verses in the grid's numbering, from `from` to `to` (inclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ReadingPassage {
    pub from: VerseRef,
    pub to: VerseRef,
    /// The chapter it starts in, to open.
    pub chapter_index: u16,
}

impl From<lectionary::Passage> for ReadingPassage {
    fn from(p: lectionary::Passage) -> Self {
        ReadingPassage { from: p.first().into(), to: p.last().into(), chapter_index: p.chapter().0 }
    }
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
    /// The saint or feast it's read for, when the day's great feast doesn't name it: St
    /// Artemius's Epistle beside a weekday's Gospel, or St Demetrius's whole day.
    pub occasion: Option<String>,
    pub confidence: ReadingConfidence,
}

/// The readings of a civil date in service order: the Vespers of the evening before, Matins, the
/// Hours, the Liturgy, then the day's evening services. On the New or Old calendar, in the Greek
/// or Antiochian usage.
#[uniffi::export]
pub fn day_readings(year: i32, month: u8, day: u8, old_calendar: bool, usage: ReadingUsage) -> Vec<Reading> {
    let style = crate::rule::style(old_calendar);
    let usage = Usage::from(usage);
    let d = calendar::civil(year, month, day);
    let rs = lectionary::readings_in(d, style, usage);
    let feast = !calendar::day(d, style).feasts.is_empty();
    // An occasion is the day's own unless it's a saint's or a fixed feast's that the day's great
    // feast doesn't name: on a day with none, or one the Liturgy's Epistle and Gospel (without
    // them, all the day's readings) don't all share. The Royal Hours are named by their service.
    let main: Vec<&lectionary::Reading> =
        rs.iter().filter(|r| r.at_liturgy() && r.kind != Kind::OldTestament).collect();
    let anchor = if main.is_empty() { rs.iter().collect() } else { main };
    let own = |r: &lectionary::Reading| {
        !fixed(usage).contains(r.occasion)
            || r.service == "Royal Hours"
            || (feast && anchor.iter().all(|a| a.occasion == r.occasion))
    };
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
            passages: r.passages.iter().map(|&p| p.into()).collect(),
            title: r.title.map(Into::into),
            served: r.served.map(Into::into),
            occasion: (!own(r)).then(|| r.occasion.into()),
            confidence: match r.confidence {
                Confidence::High => ReadingConfidence::High,
                Confidence::Medium => ReadingConfidence::Medium,
                Confidence::Low => ReadingConfidence::Low,
            },
        })
        .collect()
}

/// The saints' and the fixed feasts' occasions (the Menaion's) in `usage`, which keep their date:
/// the ones read on the same date two years running, in 2024–2032. The weekly cycles' and
/// Pascha's never are, since the date's weekday moves on. Known by name, so a feast moved off its
/// date (St George to Bright Monday) is known there too. Found once per usage, in a few
/// milliseconds.
fn fixed(usage: Usage) -> &'static HashSet<&'static str> {
    static FIXED: [OnceLock<HashSet<&'static str>>; Usage::ALL.len()] = [OnceLock::new(), OnceLock::new()];
    FIXED[usage as usize].get_or_init(|| {
        let mut fixed = HashSet::new();
        // Each date of a leap year; 29 February comes round again four years on.
        for date in calendar::civil(2024, 1, 1)..calendar::civil(2025, 1, 1) {
            let (_, month, day) = calendar::civil_date(date);
            let step = if (month, day) == (2, 29) { 4 } else { 1 };
            let years: Vec<HashSet<&'static str>> = (2024..=2032)
                .step_by(step)
                .map(|y| {
                    let rs = lectionary::readings_in(calendar::civil(y, month, day), Style::New, usage);
                    rs.into_iter().map(|r| r.occasion).collect()
                })
                .collect();
            for pair in years.windows(2) {
                fixed.extend(pair[0].intersection(&pair[1]));
            }
        }
        fixed
    })
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
