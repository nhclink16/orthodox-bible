//! `cargo run -p xtask -- build-data`
//! `cargo run -p xtask -- build-orthodox` (after build-data, or with the committed `kjv.bbl`)
//! `cargo run -p xtask -- lectionary-check 2026-01-01 2027-12-31` (see `lectionary.rs`)
//!
//! Downloads the public-domain USFM sources pinned below from eBible.org (into `.sources/`),
//! verifies their SHA-256, converts them with the same USFM importer users get, and writes
//! `data/translations/<id>.bbl`. Set `XTASK_ACCEPT_UPDATED_SOURCES=1` to accept a newer upstream
//! file (its new hash is printed so the pin can be updated).

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use bible_core::format;
use bible_core::model::{Origin, TranslationBuilder, TranslationMeta};
use bible_core::usfm;
use sha2::{Digest, Sha256};

mod lectionary;

struct Source {
    id: &'static str,
    abbrev: &'static str,
    name: &'static str,
    ebible_id: &'static str,
    sha256: &'static str,
    license: &'static str,
    copyright: &'static str,
}

const SOURCES: &[Source] = &[
    Source {
        id: "kjv",
        abbrev: "KJV",
        name: "King James Version",
        ebible_id: "eng-kjv2006",
        sha256: "77bbe9c78f4f588b1465e49ddad986293a09e7f4b140abe7248232c8c5ae2a5b",
        license: "Public Domain (outside the United Kingdom)",
        copyright: "King James Version (1769 standardized text). Public domain outside the United Kingdom, where \
                    the Crown's letters patent apply.",
    },
    Source {
        id: "web",
        abbrev: "WEB",
        name: "World English Bible",
        ebible_id: "engwebp",
        sha256: "6b47e6553ce0bd71902fe4b4ba71a5d7d862928e2ce47936acdda345bc6c5dd0",
        license: "Public Domain",
        copyright: "The World English Bible is in the public domain. \u{201c}World English Bible\u{201d} is a \
                    trademark of eBible.org.",
    },
    Source {
        id: "asv",
        abbrev: "ASV",
        name: "American Standard Version",
        ebible_id: "eng-asv",
        sha256: "161ac2da2b0668cd7c6d3a2bffbf13aeb5c58b9a0ba3bfab0f576b232cb4adc8",
        license: "Public Domain",
        copyright: "American Standard Version (1901). Public domain.",
    },
    Source {
        id: "bsb",
        abbrev: "BSB",
        name: "Berean Standard Bible",
        ebible_id: "engbsb",
        sha256: "872c04668a29fe674c0013b6ebcbbcc81bc30dd0d9ae9f6e85bc53489749b37d",
        license: "Public Domain (dedicated April 30, 2023)",
        copyright: "The Holy Bible, Berean Standard Bible, BSB is produced in cooperation with Bible Hub, \
                    Discovery Bible, OpenBible.com, and the Berean Bible Translation Committee. This text of \
                    God's Word has been dedicated to the public domain.",
    },
    Source {
        id: "ylt",
        abbrev: "YLT",
        name: "Young's Literal Translation",
        ebible_id: "engylt",
        sha256: "987b38d903546e16af66caa69cab40c5abc1959db9ea64168531a497abc59d5c",
        license: "Public Domain",
        copyright: "Young's Literal Translation (Robert Young, 1898). Public domain.",
    },
    Source {
        id: "dby",
        abbrev: "DBY",
        name: "Darby Translation",
        ebible_id: "engDBY",
        sha256: "ebb1952e7ee657e6e45065178257009185ba89230de3c59d88e740ac60fd1a70",
        license: "Public Domain",
        copyright: "The Holy Scriptures, a New Translation from the Original Languages by J. N. Darby (1890). \
                    Public domain.",
    },
    Source {
        id: "wbs",
        abbrev: "WBS",
        name: "Webster Bible",
        ebible_id: "engwebster",
        sha256: "16748b10c224e0ec0acdf183b76741ba618e3b8f1c3a15f1a6464c04331a6b78",
        license: "Public Domain",
        copyright: "The Holy Bible, Containing the Old and New Testaments, in the Common Version, with \
                    Amendments of the Language by Noah Webster, LL.D. (1833). Public domain.",
    },
];

/// The Orthodox fork's Old Testament: Brenton's Septuagint as eBible.org's LXX2012 (updated
/// spelling, the deuterocanonical books, Psalm 151). Its New Testament is the bundled KJV's.
const LXX2012: Source = Source {
    id: "lxx",
    abbrev: "LXX",
    name: "Septuagint and KJV New Testament",
    ebible_id: "eng-lxx2012",
    sha256: "dc9a055d0b4e82fbcdc576f6330cf7138f987285d7ff01d0b29e061d49df87ef",
    license: "Public Domain",
    copyright: "Old Testament: Sir Lancelot C. L. Brenton's English Septuagint (1851), public domain, with \
                spelling and wording updated by Michael Paul Johnson (LXX2012), dedicated to the public \
                domain. New Testament: King James Version (1769), public domain outside the United Kingdom.",
};

/// Verses of the grid that LXX2012 keeps inside another verse: across a chapter break, at a
/// lectionary reading's first verse, or marked with `\vp` (1 Kings). Each (book, text,
/// replacement) starts the grid's verse there, numbered as LXX2012 numbers its chapter so that
/// `lxx::remap_lxx2012` moves it with the rest; they apply in order through the book.
const LXX2012_SPLITS: &[(&str, &str, &str)] = &[
    // Genesis 2:25 and 5:32 begin LXX2012's 3:1 and 6:1; 8:4 ends its 8:3, and its 8:4–5 are 8:5.
    ("GEN", "\\v 1 And the two were naked", "\\c 2 \\v 25 And the two were naked"),
    ("GEN", "ashamed. Now the serpent", "ashamed. \\c 3 \\p \\v 1 Now the serpent"),
    ("GEN", "\\v 1 And Noe was", "\\c 5 \\v 32 And Noe was"),
    ("GEN", "Japheth. And it came", "Japheth. \\c 6 \\p \\v 1 And it came"),
    ("GEN", "diminished, and the ark rested", "diminished, \\v 4 and the ark rested"),
    ("GEN", "\\v 4 And the water continued", "\\v 5 And the water continued"),
    // 1 Kings 2:36–46 are inside 3:1; 7:21, 7:46 and 7:1–12 inside 7:18, 7:47 and 7:51.
    ("1KI", "\\p \\vp 36) \\vp*", "\\c 2 \\p \\v 36"),
    ("1KI", "\\vp 37) \\vp*", "\\v 37"),
    ("1KI", "\\vp 38) \\vp*", "\\v 38"),
    ("1KI", "\\vp 39) \\vp*", "\\v 39"),
    ("1KI", "\\vp 40) \\vp*", "\\v 40"),
    ("1KI", "\\vp 41) \\vp*", "\\v 41"),
    ("1KI", "\\vp 42) \\vp*", "\\v 42"),
    ("1KI", "\\vp 43) \\vp*", "\\v 43"),
    ("1KI", "\\vp 44) \\vp*", "\\v 44"),
    ("1KI", "\\vp 45) \\vp*", "\\v 45"),
    ("1KI", "\\vp 46) \\vp*", "\\v 46"),
    ("1KI", "\\v 2 Nevertheless", "\\c 3 \\v 2 Nevertheless"),
    // LXX2012 holds 7:1–12's place with an empty verse; they come out of 7:51 below.
    ("1KI", "\\v 1-12 ", ""),
    ("1KI", "\\vp 21\\vp*", "\\v 21"),
    ("1KI", "\\vp 46 \\vp*", "\\v 46"),
    ("1KI", "\\vp 1) \\vp*", "\\v 1"),
    ("1KI", "\\vp 2) \\vp*", "\\v 2"),
    ("1KI", "\\vp 3) \\vp*", "\\v 3"),
    ("1KI", "\\vp 4) \\vp*", "\\v 4"),
    ("1KI", "\\vp 5) \\vp*", "\\v 5"),
    ("1KI", "\\vp 6) \\vp*", "\\v 6"),
    ("1KI", "\\vp 7) \\vp*", "\\v 7"),
    ("1KI", "\\vp 8) \\vp*", "\\v 8"),
    ("1KI", "\\vp 9) \\vp*", "\\v 9"),
    ("1KI", "\\vp 10) \\vp*", "\\v 10"),
    ("1KI", "\\vp 11) \\vp*", "\\v 11"),
    ("1KI", "\\vp 12) \\vp*", "\\v 12"),
    // Edom's oracle (Jeremiah 49:7) begins inside 29:7; it reads on as 30, the rest of the grid's 49.
    ("JER", "! CONCERNING IDUMEA", "! \\c 30 \\v 7 CONCERNING IDUMEA"),
    // Zephaniah 2:15 begins LXX2012's 3:1.
    ("ZEP", "\\v 1 This is the scornful", "\\c 2 \\v 15 This is the scornful"),
    ("ZEP", "hands. Alas the glorious", "hands. \\c 3 \\p \\v 1 Alas the glorious"),
];

/// A book of LXX2012 with its [`LXX2012_SPLITS`] made.
fn split_lxx2012(book: &str, mut text: String) -> Result<String> {
    let mut at = 0;
    for (_, find, with) in LXX2012_SPLITS.iter().filter(|s| s.0 == book) {
        let i =
            at + text[at..].find(find).with_context(|| format!("LXX2012 {book}: no {find:?} to split"))?;
        text.replace_range(i..i + find.len(), with);
        at = i + with.len();
    }
    Ok(text)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
    match args.first().map(String::as_str) {
        Some("build-data") => build_data(&root, &args[1..]),
        Some("build-orthodox") => build_orthodox(&root),
        Some("missing") => {
            // Lists verses present in the first translation but absent from the others.
            let load = |id: &str| -> Result<bible_core::Translation> {
                format::decode(&std::fs::read(root.join(format!("data/translations/{id}.bbl")))?)
            };
            let base = load(&args[1])?;
            for other in &args[2..] {
                let t = load(other)?;
                let mut missing = Vec::new();
                for c in bible_core::ChapterIdx::all() {
                    for v in base.verses_in(c) {
                        if t.find(v.reference()).is_none() {
                            missing.push(v.reference().to_string());
                        }
                    }
                }
                println!("{other}: {} missing: {}", missing.len(), missing.join(", "));
            }
            Ok(())
        }
        Some("dump") => {
            let t = format::decode(&std::fs::read(root.join(format!("data/translations/{}.bbl", args[1])))?)?;
            let r = bible_core::reference::parse(&args[2]).context("bad reference")?;
            let c = r.chapter_idx();
            println!("{} — {:?}", c.display(), t.meta.books[c.book().index()]);
            for v in t.verses_in(c) {
                let marks: Vec<String> = v
                    .marks
                    .iter()
                    .map(|m| {
                        format!(
                            "{:?}{}@{}{}",
                            m.kind,
                            m.level,
                            m.offset,
                            if m.kind.has_text() {
                                format!("“{}”", t.mark_text(m))
                            } else {
                                String::new()
                            }
                        )
                    })
                    .collect();
                let spans: Vec<String> =
                    v.spans.iter().map(|s| format!("{:?}[{}..{}]", s.style, s.start, s.end)).collect();
                println!("{:>3} {}\n      marks {:?} spans {:?}", v.number, v.text, marks, spans);
            }
            Ok(())
        }
        Some("verse-table") => {
            let t = format::decode(&std::fs::read(root.join("data/translations/kjv.bbl"))?)?;
            let counts: Vec<String> =
                bible_core::ChapterIdx::all().map(|c| t.last_verse(c).to_string()).collect();
            for line in counts.chunks(30) {
                println!("    {},", line.join(", "));
            }
            Ok(())
        }
        Some("import") => {
            // Imports a file and compares it verse by verse with a bundled translation.
            let started = std::time::Instant::now();
            let imp = bible_core::import::import_path(Path::new(&args[1]), &Default::default())?;
            let t = imp.translation;
            println!(
                "{} — {} ({}) — {} verses in {:?}; {} warnings",
                imp.format.name(),
                t.meta.name,
                t.meta.abbrev,
                t.verse_count(),
                started.elapsed(),
                imp.warnings.len()
            );
            if let Some(base) = args.get(2) {
                let b = format::decode(&std::fs::read(root.join(format!("data/translations/{base}.bbl")))?)?;
                let (mut same, mut diff, mut missing) = (0, 0, 0);
                for c in bible_core::ChapterIdx::all() {
                    for v in b.verses_in(c) {
                        match t.find(v.reference()) {
                            Some(o) if o.plain_text() == v.plain_text() => same += 1,
                            Some(o) => {
                                if diff < 5 {
                                    println!(
                                        "  {}\n    {}\n    {}",
                                        v.reference(),
                                        v.plain_text(),
                                        o.plain_text()
                                    );
                                }
                                diff += 1
                            }
                            None => missing += 1,
                        }
                    }
                }
                println!("vs {base}: {same} identical, {diff} different, {missing} missing");
            }
            for r in ["ps 23:1", "jn 3:16", "gen 1:1", "rev 22:21"] {
                let r = bible_core::reference::parse(r).unwrap();
                if let Some(v) = t.find(r.verse_ref()) {
                    let marks: Vec<String> =
                        v.marks.iter().map(|m| format!("{:?}{}", m.kind, m.level)).collect();
                    let spans: Vec<String> = v.spans.iter().map(|s| format!("{:?}", s.style)).collect();
                    println!("{}: {} {marks:?} {spans:?}", r.display(), v.text);
                }
            }
            Ok(())
        }
        Some("fetch") => {
            for s in selected(&args[1..]) {
                fetch(&root, s)?;
            }
            Ok(())
        }
        Some("lectionary") => lectionary::show(&args[1..]),
        Some("lectionary-check") => lectionary::check(&root, &args[1..]),
        _ => {
            eprintln!(
                "usage: cargo run -p xtask -- build-data [ids…] | fetch [ids…] | lectionary <from> [to] [--old] \
                 [--usage greek|antiochian] | lectionary-check <from> <to> [--old] [--usage greek|antiochian] \
                 [--against orthocal|goarch|antiochian] [--data <path>]"
            );
            std::process::exit(2)
        }
    }
}

fn selected(ids: &[String]) -> Vec<&'static Source> {
    SOURCES.iter().filter(|s| ids.is_empty() || ids.iter().any(|i| i == s.id)).collect()
}

fn fetch(root: &Path, s: &Source) -> Result<PathBuf> {
    let dir = root.join(".sources");
    std::fs::create_dir_all(&dir)?;
    let file = dir.join(format!("{}_usfm.zip", s.ebible_id));
    if !file.exists() {
        let url = format!("https://ebible.org/Scriptures/{}_usfm.zip", s.ebible_id);
        eprintln!("downloading {url}");
        let status = Command::new("curl").args(["-fsSL", "-o"]).arg(&file).arg(&url).status()?;
        if !status.success() {
            bail!("download failed: {url}");
        }
    }
    let digest = hex(&Sha256::digest(std::fs::read(&file)?));
    if digest != s.sha256 {
        if std::env::var_os("XTASK_ACCEPT_UPDATED_SOURCES").is_some() {
            eprintln!("warning: {} changed upstream; new sha256 = {digest}", s.ebible_id);
        } else {
            bail!(
                "{} sha256 mismatch: expected {}, got {digest}. The upstream file changed; review it, then \
                 update the pin (or set XTASK_ACCEPT_UPDATED_SOURCES=1).",
                file.display(),
                s.sha256
            );
        }
    }
    Ok(file)
}

fn build_data(root: &Path, ids: &[String]) -> Result<()> {
    let out_dir = root.join("data/translations");
    std::fs::create_dir_all(&out_dir)?;
    for s in selected(ids) {
        let zip_path = fetch(root, s)?;
        let digest = hex(&Sha256::digest(std::fs::read(&zip_path)?));
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&zip_path)?)?;
        let mut names: Vec<String> =
            archive.file_names().filter(|n| n.ends_with(".usfm")).map(String::from).collect();
        names.sort();
        let meta = TranslationMeta {
            id: s.id.into(),
            abbrev: s.abbrev.into(),
            name: s.name.into(),
            language: "en".into(),
            license: s.license.into(),
            copyright: s.copyright.into(),
            source: format!(
                "https://ebible.org/Scriptures/{}_usfm.zip (sha256 {digest}); details: https://ebible.org/{}/",
                s.ebible_id, s.ebible_id
            ),
            origin: Origin::Bundled,
            ..Default::default()
        };
        let mut b = TranslationBuilder::new(meta);
        for name in &names {
            let mut f = archive.by_name(name)?;
            let mut text = String::new();
            std::io::Read::read_to_string(&mut f, &mut text).with_context(|| name.clone())?;
            usfm::UsfmParser::new(&mut b).parse(&text)?;
        }
        let warnings = std::mem::take(&mut b.warnings);
        let t = b.finish();
        let bytes = format::encode(&t, 19)?;
        let path = out_dir.join(format!("{}.bbl", s.id));
        std::fs::write(&path, &bytes)?;
        println!(
            "{:<4} {:>6} verses  {:>5.2} MB text → {:>5.2} MB  ({} warnings){}",
            s.abbrev,
            t.verse_count(),
            t.total_text_bytes() as f64 / 1e6,
            bytes.len() as f64 / 1e6,
            warnings.len(),
            if t.meta.red_letters { "  red letters" } else { "" }
        );
        for w in warnings.iter().take(5) {
            println!("      {w}");
        }
    }
    Ok(())
}

fn build_orthodox(root: &Path) -> Result<()> {
    let s = &LXX2012;
    let zip_path = fetch(root, s)?;
    let digest = hex(&Sha256::digest(std::fs::read(&zip_path)?));
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&zip_path)?)?;
    let mut names: Vec<String> =
        archive.file_names().filter(|n| n.ends_with(".usfm")).map(String::from).collect();
    names.sort();
    let meta = TranslationMeta {
        id: s.id.into(),
        abbrev: s.abbrev.into(),
        name: s.name.into(),
        language: "en".into(),
        license: s.license.into(),
        copyright: s.copyright.into(),
        source: format!(
            "https://ebible.org/Scriptures/{}_usfm.zip (sha256 {digest}); details: https://ebible.org/{}/. \
             New Testament from the bundled KJV.",
            s.ebible_id, s.ebible_id
        ),
        origin: Origin::Bundled,
        ..Default::default()
    };
    let mut b = TranslationBuilder::new(meta);
    // LXX2012 numbers the Psalms as the Septuagint does; the grid numbers them as the Hebrew.
    b.remap = Some(bible_core::lxx::remap_lxx2012);
    for name in &names {
        let mut f = archive.by_name(name)?;
        let mut text = String::new();
        std::io::Read::read_to_string(&mut f, &mut text).with_context(|| name.clone())?;
        // LXX2012 marks a plural "you" as "you⌃"; read aloud or at a glance it's a stray symbol.
        let text = split_lxx2012(name.get(3..6).unwrap_or(""), text.replace('⌃', ""))?;
        usfm::UsfmParser::new(&mut b).parse(&text)?;
    }
    let warnings = std::mem::take(&mut b.warnings);
    let skipped = std::mem::take(&mut b.skipped_books);
    let mut lxx = b.finish();
    // The grid's book names (Isaiah, 1 Kings) rather than LXX2012's (Esias, Kings III).
    for book in bible_core::BookId::all() {
        lxx.meta.books[book.index()] =
            bible_core::model::BookTitle { short: book.name().into(), long: String::new() };
    }
    let kjv = format::decode(&std::fs::read(root.join("data/translations/kjv.bbl"))?)?;
    let t = lxx.with_chapters_from(&kjv, |c| c.book().testament() == bible_core::books::Testament::New);
    let bytes = format::encode(&t, 19)?;
    std::fs::write(root.join(format!("data/translations/{}.bbl", s.id)), &bytes)?;
    println!(
        "{:<4} {:>6} verses  {:>5.2} MB text → {:>5.2} MB  ({} warnings; skipped {:?})",
        s.abbrev,
        t.verse_count(),
        t.total_text_bytes() as f64 / 1e6,
        bytes.len() as f64 / 1e6,
        warnings.len(),
        skipped
    );
    for w in &warnings {
        println!("      {w}");
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
