//! `cargo run -p xtask -- lectionary <from> [to] [--old] [--usage greek|antiochian]`: prints the
//! module's readings.
//! `cargo run -p xtask -- lectionary-check <from> <to> [--old] [--usage …]
//! [--against orthocal|goarch|antiochian] [--data <path>]`: compares the Liturgy's Epistle and
//! Gospel with another calendar, day by day, in the usage given (Greek by default):
//!
//! - `orthocal` (the default): orthocal.info's Greek tradition (Antiochian and GOARCH usage).
//!   Responses are cached in `.sources/orthocal/` (gitignored), so a rerun makes no requests.
//! - `goarch`, `antiochian`: a harvest of goarch.org's daily readings, or of the Antiochian
//!   Archdiocese's service texts and charts, read from `--data` (default `.sources/<name>/`) and
//!   never fetched: every `.json` under it in the shape of goarch.org's monthly grid
//!   (`{"YYYY-MM-DD": [label, [[type, citation], …]]}`) and every `.xml` of the Online Chapel's
//!   daily feed, where type is E, G, MG (Matins Gospel) or OT. Days the harvest lacks are skipped.
//!   Where it has Matins Gospels, those are compared too.
//!
//! The other calendar is a check, not a source: where it and the tables disagree, the tables
//! follow the evidence in `data/lectionary/`.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use bible_core::books::{BOOKS, BookId, ChapterIdx, VerseRef};
use bible_core::calendar::{self, Day, Style};
use bible_core::lectionary::{self, Kind, Reading, Usage};

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

/// The options that take a value.
const VALUED: [&str; 3] = ["--usage", "--against", "--data"];

/// The value of option `name`.
fn option<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

/// Dates, calendar and usage: `<from> [to] [--old] [--usage greek|antiochian]`.
fn range(args: &[String], need_to: bool) -> Result<(Day, Day, Style, Usage)> {
    let old = args.iter().any(|a| a == "--old");
    let usage = match option(args, "--usage") {
        Some(u) => Usage::parse(u).with_context(|| format!("unknown usage {u:?}"))?,
        None => Usage::default(),
    };
    let dates: Vec<&String> = args
        .iter()
        .enumerate()
        .filter(|&(i, a)| !a.starts_with("--") && !(i > 0 && VALUED.contains(&args[i - 1].as_str())))
        .map(|(_, a)| a)
        .collect();
    let from = parse_date(dates.first().context("a start date is needed")?)?;
    let to = match dates.get(1) {
        Some(s) => parse_date(s)?,
        None if need_to => bail!("an end date is needed"),
        None => from,
    };
    Ok((from, to, if old { Style::Old } else { Style::New }, usage))
}

pub fn show(args: &[String]) -> Result<()> {
    let (from, to, style, usage) = range(args, false)?;
    for d in from..=to {
        println!("{} {}", date(d), WEEKDAYS[calendar::weekday(d) as usize]);
        for r in lectionary::readings_in(d, style, usage) {
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

/// What is compared: the Liturgy's Epistle and Gospel, and Matins' Gospel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lesson {
    Epistle,
    Gospel,
    Matins,
}

impl Lesson {
    fn label(self) -> &'static str {
        match self {
            Lesson::Epistle => "Epistle",
            Lesson::Gospel => "Gospel",
            Lesson::Matins => "Matins",
        }
    }

    /// goarch.org's type.
    fn code(self) -> &'static str {
        match self {
            Lesson::Epistle => "E",
            Lesson::Gospel => "G",
            Lesson::Matins => "MG",
        }
    }
}

/// The day's lessons: the Liturgy's Epistles or Gospels (on a day without one, as Holy Friday,
/// the Vespers'), or the Matins Gospels.
fn ours(readings: &[Reading], lesson: Lesson) -> Vec<(String, Verses)> {
    let of = |service: &dyn Fn(&Reading) -> bool, kind: Kind| -> Vec<(String, Verses)> {
        readings
            .iter()
            .filter(|r| r.kind == kind && service(r))
            .map(|r| (r.label(), r.passages.iter().flat_map(|p| p.verses()).collect()))
            .collect()
    };
    let kind = if lesson == Lesson::Epistle { Kind::Epistle } else { Kind::Gospel };
    if lesson == Lesson::Matins {
        return of(&|r| r.service.starts_with("Matins"), kind);
    }
    let liturgy = of(&Reading::at_liturgy, kind);
    if liturgy.is_empty() { of(&|r| r.service.starts_with("Vespers"), kind) } else { liturgy }
}

fn book_named(name: &str) -> Option<BookId> {
    let norm = |s: &str| s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
    let key = norm(name);
    BOOKS
        .iter()
        .position(|b| norm(b.name) == key || norm(b.abbrev) == key || b.aliases.contains(&key.as_str()))
        .map(|i| BookId(i as u8))
}

/// A book by any of the names the calendars use: "Colossians", "Matt", "I Corinthians", "Acts of
/// the Apostles", "St. Paul's First Letter to the Corinthians", "St. Jude's First Universal Letter".
fn book_titled(name: &str) -> Option<BookId> {
    if let Some(b) = book_named(name) {
        return Some(b);
    }
    let name = name.replace("Acts of the Apostles", "Acts");
    let words: Vec<&str> = name.split_whitespace().collect();
    let number = words.iter().find_map(|w| match *w {
        "I" | "First" => Some("1"),
        "II" | "Second" => Some("2"),
        "III" | "Third" => Some("3"),
        _ => None,
    });
    // "… Letter to the Romans" names its readers; "St. John's … Letter" its writer.
    let book = if words.contains(&"to") {
        words.last().copied()
    } else if words.contains(&"Letter") {
        words
            .iter()
            .find(|w| w.ends_with('\'') || w.ends_with("'s"))
            .map(|w| w.trim_end_matches("'s").trim_end_matches('\''))
    } else {
        words.last().copied()
    }?;
    number.and_then(|n| book_named(&format!("{n} {book}"))).or_else(|| book_named(book))
}

/// A citation as the calendars print it: "Colossians 1.1-3, 7-11", "Titus 2:11-14; 3:4-7",
/// "Jude 1-10", "Matthew 27.1-38; Luke 23:39-43; Matt 27:39-54", "I Timothy 3:13-16; 4:1-5".
fn parse_display(s: &str) -> Option<Verses> {
    let mut out = Verses::new();
    let (mut book, mut chapter): (Option<BookId>, Option<u8>) = (None, None);
    // goarch.org's "Mark 13:31-37: 14:1-2" means "…; 14:1-2".
    let s = s.replace(": ", "; ");
    for token in s.split([';', ',']).map(str::trim).filter(|t| !t.is_empty()) {
        let (name, nums) = match token.rfind(' ') {
            Some(i) if token[i + 1..].starts_with(|c: char| c.is_ascii_digit()) => {
                (&token[..i], &token[i + 1..])
            }
            _ => ("", token),
        };
        if !name.is_empty() {
            book = Some(book_titled(name)?);
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

/// One of the other calendar's lessons.
struct Theirs {
    display: String,
    description: String,
    verses: Option<Verses>,
}

impl Theirs {
    fn new(display: &str, description: &str) -> Self {
        Theirs { verses: parse_display(display), display: display.into(), description: description.into() }
    }
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
fn orthocal(day: &serde_json::Value, lesson: Lesson) -> (Vec<Theirs>, Option<usize>) {
    let source = match lesson {
        Lesson::Epistle => "Epistle",
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
        if abbreviated.contains(&i) && main.is_none() {
            main = Some(out.len());
        }
        out.push(Theirs::new(r["display"].as_str().unwrap_or(""), r["description"].as_str().unwrap_or("")));
    }
    if main.is_none() && !out.is_empty() {
        main = Some(0);
    }
    (out, main)
}

/// A harvest's readings by date ("YYYY-MM-DD"): (type, citation). The feed's files win over the
/// grid's where both give a day.
type Harvest = HashMap<String, Vec<(String, String)>>;

fn files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if dir.is_file() {
        out.push(dir.to_path_buf());
        return Ok(());
    }
    for e in std::fs::read_dir(dir).with_context(|| dir.display().to_string())? {
        files(&e?.path(), out)?;
    }
    Ok(())
}

/// The text between `<tag>` and `</tag>` in `s`.
fn element<'a>(s: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let start = s.find(&open)? + open.len();
    Some(s[start..start + s[start..].find(&format!("</{tag}>"))?].trim())
}

fn harvest(path: &Path) -> Result<Harvest> {
    let mut paths = Vec::new();
    files(path, &mut paths)?;
    paths.sort_by_key(|p| p.extension().is_some_and(|e| e == "xml"));
    let mut out = Harvest::new();
    for p in paths {
        let text = std::fs::read_to_string(&p).with_context(|| p.display().to_string())?;
        match p.extension().and_then(|e| e.to_str()) {
            Some("json") => {
                let grid: serde_json::Value =
                    serde_json::from_str(&text).with_context(|| p.display().to_string())?;
                for (day, v) in grid.as_object().into_iter().flatten().filter(|(k, _)| !k.starts_with('_')) {
                    let rows =
                        v[1].as_array().into_iter().flatten().filter_map(|r| {
                            Some((r[0].as_str()?.to_string(), r.get(1)?.as_str()?.to_string()))
                        });
                    out.insert(day.clone(), rows.collect());
                }
            }
            Some("xml") => {
                let Some(us) = element(&text, "date") else { continue };
                let [m, d, y] = us.split('/').collect::<Vec<_>>()[..] else { continue };
                let rows = text.split("<reading ").skip(1).filter_map(|r| {
                    Some((element(r, "type")?.to_string(), element(r, "shorttitle")?.to_string()))
                });
                out.insert(format!("{y}-{m}-{d}"), rows.collect());
            }
            _ => {}
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// The same verses (or neither reads one).
    Same,
    /// The same pericope, starting or ending a verse or more apart.
    Boundary,
    /// The other calendar lists ours too, but prints another as the day's.
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
    let (from, to, style, usage) = range(args, true)?;
    let cal = if style == Style::Old { "julian" } else { "gregorian" };
    let (source, harvested) = match option(args, "--against").unwrap_or("orthocal") {
        "orthocal" => (format!("orthocal.info greek/{cal}"), None),
        name @ ("goarch" | "antiochian") => {
            let path = option(args, "--data").map_or_else(|| root.join(".sources").join(name), PathBuf::from);
            (format!("the {name} harvest in {}", path.display()), Some(harvest(&path)?))
        }
        other => bail!("unknown calendar {other:?}: orthocal, goarch or antiochian"),
    };
    let matins = harvested.as_ref().is_some_and(|h| h.values().flatten().any(|(t, _)| t == "MG"));
    let lessons: &[Lesson] = if matins {
        &[Lesson::Epistle, Lesson::Gospel, Lesson::Matins]
    } else {
        &[Lesson::Epistle, Lesson::Gospel]
    };
    let mut counts = [[0usize; 4]; 3];
    let mut missing = 0;
    for d in from..=to {
        let day: Vec<(Lesson, Vec<Theirs>, Option<usize>)> = match &harvested {
            None => {
                let day = fetch(root, d, cal)?;
                lessons.iter().map(|&l| (l, orthocal(&day, l))).map(|(l, (t, m))| (l, t, m)).collect()
            }
            Some(harvested) => {
                let Some(rows) = harvested.get(&date(d)) else {
                    missing += 1;
                    continue;
                };
                lessons
                    .iter()
                    .map(|&l| {
                        let t: Vec<Theirs> = rows
                            .iter()
                            .filter(|(t, _)| t == l.code())
                            .map(|(_, c)| Theirs::new(c, ""))
                            .collect();
                        let main = (!t.is_empty()).then_some(0);
                        (l, t, main)
                    })
                    .collect()
            }
        };
        let mine = lectionary::readings_in(d, style, usage);
        for (k, (lesson, theirs, main)) in day.into_iter().enumerate() {
            let ours = ours(&mine, lesson);
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
                "{} {} {:<7} {:<10} ours: {:<40} theirs: {}",
                date(d),
                WEEKDAYS[calendar::weekday(d) as usize],
                lesson.label(),
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
            let kind = if lesson == Lesson::Epistle { Kind::Epistle } else { Kind::Gospel };
            let at = |r: &Reading| {
                if lesson == Lesson::Matins { r.service.starts_with("Matins") } else { r.at_liturgy() }
            };
            if let Some(o) = mine.iter().find(|r| r.kind == kind && at(r)) {
                line.push_str(&format!("  — {}", o.occasion));
            }
            println!("{line}");
        }
    }
    let days = (to - from + 1 - missing) as f64;
    println!(
        "\n{days} days, {} to {} ({style:?} calendar, {} usage), against {source}",
        date(from),
        date(to),
        usage.as_str()
    );
    if missing > 0 {
        println!("{missing} days missing from the harvest were skipped");
    }
    for (k, lesson) in lessons.iter().enumerate() {
        let [same, boundary, listed, different] = counts[k];
        println!(
            "{:<8} same {same:>4} ({:.1}%)  boundary {boundary:>3}  listed {listed:>3}  different {different:>3}  \
             (same or listed {:.1}%)",
            lesson.label(),
            same as f64 * 100.0 / days,
            (same + listed) as f64 * 100.0 / days
        );
    }
    println!(
        "same: the verses the other calendar prints as the day's (Matins: or neither has one); boundary: the \
         same lesson a verse or more apart; listed: it reads ours too but prints another as the day's."
    );
    Ok(())
}
