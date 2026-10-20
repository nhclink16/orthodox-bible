//! `cargo run -p xtask -- lectionary <from> [to] [--old]`: prints the module's readings.
//! `cargo run -p xtask -- lectionary-check <from> <to> [--old]`: compares the Liturgy's Epistle
//! and Gospel with orthocal.info's Greek tradition (Antiochian and GOARCH usage), day by day.
//!
//! orthocal is a check, not a source: where it and the tables disagree, the tables follow the
//! Antiochian books and the evidence in `data/lectionary/`. Responses are cached in
//! `.sources/orthocal/` (gitignored), so a rerun makes no requests.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use bible_core::books::{BOOKS, BookId, ChapterIdx, VerseRef};
use bible_core::calendar::{self, Day, Style};
use bible_core::lectionary::{self, Kind, Reading};

fn parse_date(s: &str) -> Result<Day> {
    let p: Vec<&str> = s.split('-').collect();
    let [y, m, d] = p[..] else { bail!("dates are YYYY-MM-DD, not {s:?}") };
    Ok(calendar::civil(y.parse()?, m.parse()?, d.parse()?))
}

fn date(d: Day) -> String {
    let (y, m, dd) = calendar::civil_date(d);
    format!("{y:04}-{m:02}-{dd:02}")
}

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// Dates and the calendar: `<from> [to] [--old]`.
fn range(args: &[String], need_to: bool) -> Result<(Day, Day, Style)> {
    let old = args.iter().any(|a| a == "--old");
    let dates: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let from = parse_date(dates.first().context("a start date is needed")?)?;
    let to = match dates.get(1) {
        Some(s) => parse_date(s)?,
        None if need_to => bail!("an end date is needed"),
        None => from,
    };
    Ok((from, to, if old { Style::Old } else { Style::New }))
}

pub fn show(args: &[String]) -> Result<()> {
    let (from, to, style) = range(args, false)?;
    for d in from..=to {
        println!("{} {}", date(d), WEEKDAYS[calendar::weekday(d) as usize]);
        for r in lectionary::readings(d, style) {
            let extra: Vec<&str> = [r.title, r.served].into_iter().flatten().collect();
            println!(
                "  {:<28} {:<13} {}{}  ({}; {:?})",
                r.service,
                r.kind.label(),
                r.label(),
                if extra.is_empty() { String::new() } else { format!(" [{}]", extra.join("; ")) },
                r.occasion,
                r.confidence
            );
        }
    }
    Ok(())
}

/// The verses of one lesson, for comparing.
type Verses = BTreeSet<VerseRef>;

/// The day's Epistles or Gospels: the Liturgy's, or on a day without one (Holy Friday) the
/// Vespers'.
fn ours(readings: &[Reading], kind: Kind) -> Vec<(String, Verses)> {
    let of = |service: fn(&Reading) -> bool| -> Vec<(String, Verses)> {
        readings
            .iter()
            .filter(|r| r.kind == kind && service(r))
            .map(|r| (r.label(), r.passages.iter().flat_map(|p| p.verses()).collect()))
            .collect()
    };
    let liturgy = of(Reading::at_liturgy);
    if liturgy.is_empty() { of(|r| r.service.starts_with("Vespers")) } else { liturgy }
}

fn book_named(name: &str) -> Option<BookId> {
    let key: String = name.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
    let norm = |s: &str| s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
    BOOKS
        .iter()
        .position(|b| norm(b.name) == key || norm(b.abbrev) == key || b.aliases.contains(&key.as_str()))
        .map(|i| BookId(i as u8))
}

/// orthocal's display form: "Colossians 1.1-3, 7-11", "Titus 2.11-14, 3.4-7", "Jude 1-10",
/// "Matthew 27.1-38; Luke 23:39-43; Matt 27:39-54".
fn parse_display(s: &str) -> Option<Verses> {
    let mut out = Verses::new();
    let (mut book, mut chapter): (Option<BookId>, Option<u8>) = (None, None);
    for token in s.split([';', ',']).map(str::trim).filter(|t| !t.is_empty()) {
        let (name, nums) = match token.rfind(' ') {
            Some(i) if token[i + 1..].starts_with(|c: char| c.is_ascii_digit()) => {
                (&token[..i], &token[i + 1..])
            }
            _ => ("", token),
        };
        if !name.is_empty() {
            book = Some(book_named(name)?);
            chapter = None;
        }
        let b = book?;
        let nums = nums.replace(':', ".");
        let point = |t: &str, chapter: Option<u8>| -> Option<(u8, u8)> {
            match t.split_once('.') {
                Some((c, v)) => Some((c.parse().ok()?, v.parse().ok()?)),
                None if b.chapters() == 1 => Some((1, t.parse().ok()?)),
                None => Some((chapter?, t.parse().ok()?)),
            }
        };
        let (a, z) = nums.split_once('-').map_or((nums.as_str(), None), |(a, z)| (a, Some(z)));
        let from = point(a, chapter)?;
        let to = match z {
            Some(z) => point(z, Some(from.0))?,
            None => from,
        };
        chapter = Some(to.0);
        for c in from.0..=to.0 {
            let last = ChapterIdx::new(b, c)?.kjv_verses();
            let (v0, v1) =
                (if c == from.0 { from.1 } else { 1 }, if c == to.0 { to.1.min(last) } else { last });
            out.extend((v0..=v1).map(|v| VerseRef::new(b, c, v)));
        }
    }
    (!out.is_empty()).then_some(out)
}

/// One of orthocal's lessons.
struct Theirs {
    display: String,
    description: String,
    verses: Option<Verses>,
}

fn fetch(root: &Path, d: Day, cal: &str) -> Result<serde_json::Value> {
    let dir = root.join(".sources/orthocal");
    std::fs::create_dir_all(&dir)?;
    let (y, m, dd) = calendar::civil_date(d);
    let path = dir.join(format!("greek-{cal}-{}.json", date(d)));
    if !path.exists() {
        let url = format!("https://orthocal.info/api/greek/{cal}/{y}/{m}/{dd}/");
        let tmp = path.with_extension("tmp");
        let status = Command::new("curl").args(["-fsS", "-m", "60", "-o"]).arg(&tmp).arg(&url).status()?;
        if !status.success() {
            bail!("download failed: {url}");
        }
        std::fs::rename(&tmp, &path)?;
        // Be gentle with a free service.
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    serde_json::from_slice(&std::fs::read(&path)?).with_context(|| path.display().to_string())
}

/// orthocal's Epistles or Gospels of the day, and the one its abbreviated list (the pair the
/// Antiochian and GOARCH calendars print) picks.
fn theirs(day: &serde_json::Value, kind: Kind) -> (Vec<Theirs>, Option<usize>) {
    let source = match kind {
        Kind::Epistle => "Epistle",
        _ => "Gospel",
    };
    let readings = day["readings"].as_array().cloned().unwrap_or_default();
    let abbreviated: Vec<usize> = day["abbreviated_reading_indices"]
        .as_array()
        .map(|a| a.iter().filter_map(|i| i.as_u64()).map(|i| i as usize).collect())
        .unwrap_or_default();
    let mut out = Vec::new();
    let mut main = None;
    for (i, r) in readings.iter().enumerate() {
        if r["source"].as_str() != Some(source) {
            continue;
        }
        let display = r["display"].as_str().unwrap_or("").to_string();
        if abbreviated.contains(&i) && main.is_none() {
            main = Some(out.len());
        }
        out.push(Theirs {
            verses: parse_display(&display),
            display,
            description: r["description"].as_str().unwrap_or("").to_string(),
        });
    }
    if main.is_none() && !out.is_empty() {
        main = Some(0);
    }
    (out, main)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// The same verses (or neither reads one).
    Same,
    /// The same pericope, starting or ending a verse or more apart.
    Boundary,
    /// orthocal lists ours too, but prints another as the day's.
    Listed,
    Different,
}

fn overlap(a: &Verses, b: &Verses) -> f64 {
    let common = a.intersection(b).count() as f64;
    common / a.len().max(b.len()).max(1) as f64
}

fn verdict(ours: &[(String, Verses)], theirs: &[Theirs], main: Option<usize>) -> Verdict {
    let main = main.and_then(|i| theirs[i].verses.as_ref());
    match (ours.first(), main) {
        (None, None) => Verdict::Same,
        (Some((_, a)), Some(b)) if a == b => Verdict::Same,
        (Some((_, a)), Some(b)) => {
            if theirs.iter().any(|t| t.verses.as_ref() == Some(a)) {
                Verdict::Listed
            } else if overlap(a, b) >= 0.5 {
                Verdict::Boundary
            } else {
                Verdict::Different
            }
        }
        _ => Verdict::Different,
    }
}

pub fn check(root: &Path, args: &[String]) -> Result<()> {
    let (from, to, style) = range(args, true)?;
    let cal = if style == Style::Old { "julian" } else { "gregorian" };
    let kinds = [Kind::Epistle, Kind::Gospel];
    let mut counts = [[0usize; 4]; 2];
    for d in from..=to {
        let day = fetch(root, d, cal)?;
        let mine = lectionary::readings(d, style);
        for (k, kind) in kinds.into_iter().enumerate() {
            let ours = ours(&mine, kind);
            let (theirs, main) = theirs(&day, kind);
            let v = verdict(&ours, &theirs, main);
            counts[k][v as usize] += 1;
            if v == Verdict::Same {
                continue;
            }
            let shown = |t: &Theirs| {
                if t.description.is_empty() {
                    t.display.clone()
                } else {
                    format!("{} [{}]", t.display, t.description)
                }
            };
            let mut line = format!(
                "{} {} {:<7} {:<10} ours: {:<40} orthocal: {}",
                date(d),
                WEEKDAYS[calendar::weekday(d) as usize],
                kind.label(),
                format!("{v:?}"),
                if ours.is_empty() {
                    "-".to_string()
                } else {
                    ours.iter().map(|o| o.0.clone()).collect::<Vec<_>>().join(" + ")
                },
                main.map_or("-".to_string(), |i| shown(&theirs[i])),
            );
            let others: Vec<String> =
                theirs.iter().enumerate().filter(|(i, _)| Some(*i) != main).map(|(_, t)| shown(t)).collect();
            if !others.is_empty() {
                line.push_str(&format!("  (also {})", others.join("; ")));
            }
            if let Some(o) = mine.iter().find(|r| r.at_liturgy() && r.kind == kind) {
                line.push_str(&format!("  — {}", o.occasion));
            }
            println!("{line}");
        }
    }
    let days = (to - from + 1) as f64;
    println!(
        "\n{} days, {} to {} ({style:?} calendar), against orthocal.info greek/{cal}",
        days,
        date(from),
        date(to)
    );
    for (k, kind) in kinds.into_iter().enumerate() {
        let [same, boundary, listed, different] = counts[k];
        println!(
            "{:<8} same {same:>4} ({:.1}%)  boundary {boundary:>3}  listed {listed:>3}  different {different:>3}  \
             (same or listed {:.1}%)",
            kind.label(),
            same as f64 * 100.0 / days,
            (same + listed) as f64 * 100.0 / days
        );
    }
    println!(
        "same: the verses orthocal prints as the day's; boundary: the same lesson a verse or more apart; \
         listed: orthocal reads ours too but prints another as the day's."
    );
    Ok(())
}
