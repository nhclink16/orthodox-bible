use std::path::PathBuf;
use std::sync::Arc;

use bible_ffi::*;

/// A fresh data folder per test.
fn open(name: &str) -> (Arc<Bible>, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bible-ffi-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    (Bible::new(dir.to_string_lossy().into()).unwrap(), dir)
}

fn index(reference: &str) -> u16 {
    parse_reference(reference.into()).unwrap().chapter_index
}

fn text(b: &Block) -> String {
    b.runs.iter().map(|r| r.text.as_str()).collect()
}

const JOHN_3_16: VerseRef = VerseRef { book: 58, chapter: 3, verse: 16 };

#[test]
fn books_and_references() {
    let books = books();
    assert_eq!(books.len(), 82);
    assert_eq!((books[58].name.as_str(), books[58].chapters), ("John", 21));
    assert!(!books[54].new_testament && books[55].new_testament);
    assert_eq!((books[18].name.as_str(), books[18].new_testament), ("Tobit", false));
    assert_eq!(books[1].first_chapter, 50);

    let r = parse_reference("jn 3:16".into()).unwrap();
    assert_eq!((r.book, r.chapter, r.verse, r.label.as_str()), (58, 3, Some(16), "John 3:16"));
    assert_eq!(chapter_label(r.chapter_index), "John 3");
    assert_eq!(parse_reference("1co13:4-7".into()).unwrap().label, "1 Corinthians 13:4–7");
    assert_eq!(parse_reference("revalation 21".into()).unwrap().label, "Revelation 21");
    assert!(parse_reference("xyzzy".into()).is_none());
    assert_eq!(suggest_references("jo 3".into(), 3)[0].label, "John 3");
    assert_eq!(range_label(VerseRef { verse: 18, ..JOHN_3_16 }, JOHN_3_16), "John 3:16–18");
}

#[test]
fn translations_list_the_bundled_texts() {
    let (bible, dir) = open("translations");
    let list = bible.translations();
    let ids: Vec<&str> = list.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["lxx", "kjv", "web", "bsb", "asv", "ylt", "dby", "wbs"]);
    assert!(
        list.iter().all(|t| t.origin == TranslationOrigin::Bundled && t.license.starts_with("Public Domain"))
    );
    assert_eq!((list[0].abbrev.as_str(), list[0].year), ("LXX", Some(1851)));
    assert_eq!((list[1].abbrev.as_str(), list[1].year), ("KJV", Some(1769)));
    // The Septuagint's New Testament is the KJV's, red letters and all.
    assert!(list[0].red_letters && list[1].red_letters && list[3].headings);
    bible.prepare("kjv".into()).unwrap();
    assert!(bible.prepare("nlt".into()).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn prose_has_inline_numbers_and_red_letters() {
    let (bible, dir) = open("prose");
    let c = bible.chapter("kjv".into(), index("jn 3"), ContentOptions::default());
    assert_eq!(c.status, ChapterStatus::Ready);
    assert_eq!((c.book, c.chapter, c.book_name.as_str()), (58, 3, "John"));
    assert_eq!(c.book_title, "The Gospel According to St. John");
    let first = &c.blocks[0];
    assert_eq!(first.kind, BlockKind::Prose { indent: false });
    assert_eq!(first.space_before, 16.0);
    assert_eq!(first.runs[0], Run { text: "1\u{a0}".into(), verse: 1, number: true, ..plain() });
    // Later paragraphs are indented and 16 pt apart.
    assert!(
        c.blocks[1..].iter().all(|b| b.kind == BlockKind::Prose { indent: true } && b.space_before == 16.0)
    );
    let v16: Vec<&Run> = c.blocks.iter().flat_map(|b| &b.runs).filter(|r| r.verse == 16).collect();
    assert_eq!(v16[0].text, "16\u{a0}");
    assert!(v16[1].red && v16[1].text.starts_with("For God so loved the world"));
    // Translator additions are italic.
    let added: Vec<&str> =
        c.blocks.iter().flat_map(|b| &b.runs).filter(|r| r.italic).map(|r| r.text.as_str()).collect();
    assert!(added.contains(&"of") && added.contains(&"must"), "{added:?}");
    let _ = std::fs::remove_dir_all(dir);
}

fn plain() -> Run {
    Run {
        text: String::new(),
        verse: 0,
        number: false,
        red: false,
        italic: false,
        bold: false,
        small_caps: false,
    }
}

#[test]
fn psalms_hang_numbers_and_set_lord_in_small_caps() {
    let (bible, dir) = open("psalm");
    // Psalms are typed in Septuagint numbers (the bridge turns them on): 22 is the Shepherd psalm.
    let c = bible.chapter("kjv".into(), index("ps 22"), ContentOptions::default());
    assert_eq!(c.blocks[0].kind, BlockKind::PsalmTitle);
    assert_eq!((text(&c.blocks[0]).as_str(), c.blocks[0].space_before), ("A Psalm of David.", 0.0));
    let line = &c.blocks[1];
    assert_eq!((line.kind, line.space_before), (BlockKind::Poetry { level: 1 }, 16.0));
    assert_eq!((line.gutter.as_deref(), line.gutter_verse), (Some("1"), 1));
    assert!(line.runs.iter().all(|r| !r.number));
    assert_eq!(text(line), "The LORD is my shepherd; I shall not want.");
    let lord: Vec<(&str, bool)> = line.runs.iter().take(2).map(|r| (r.text.as_str(), r.small_caps)).collect();
    assert_eq!(lord, [("The L", false), ("ORD", true)]);

    // The BSB opens a psalm with a heading over its title; second lines are indented a level.
    let c = bible.chapter("bsb".into(), index("ps 3"), ContentOptions::default());
    let kinds: Vec<BlockKind> = c.blocks.iter().take(5).map(|b| b.kind).collect();
    assert_eq!(
        kinds,
        [
            BlockKind::PsalmHeading,
            BlockKind::PsalmTitle,
            BlockKind::Poetry { level: 1 },
            BlockKind::Poetry { level: 2 },
            BlockKind::Poetry { level: 1 },
        ]
    );
    assert_eq!(c.blocks[3].gutter, None);
    assert!(c.blocks.iter().any(|b| b.kind == BlockKind::Right && text(b) == "Selah"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headings_numbers_and_verse_per_line_follow_the_options() {
    let (bible, dir) = open("options");
    let gen1 = index("gen 1");
    let c = bible.chapter("bsb".into(), gen1, ContentOptions::default());
    assert_eq!(c.blocks[0].kind, BlockKind::Heading { level: 1 });
    assert_eq!((text(&c.blocks[0]).as_str(), c.blocks[0].space_before), ("The Creation", 24.0));
    // The paragraph after a heading: the heading's 8 pt plus the paragraph gap, and no indent.
    assert_eq!((c.blocks[1].kind, c.blocks[1].space_before), (BlockKind::Prose { indent: false }, 24.0));

    let bare = bible.chapter("bsb".into(), gen1, ContentOptions { headings: false, ..Default::default() });
    assert!(bare.blocks.iter().all(|b| !matches!(b.kind, BlockKind::Heading { .. })));
    assert_eq!(bare.blocks[0].space_before, 16.0);

    let per_line = ContentOptions { verse_per_line: true, ..Default::default() };
    let c = bible.chapter("web".into(), index("jn 3"), per_line);
    assert_eq!(c.blocks.len(), 36);
    for (i, b) in c.blocks.iter().enumerate() {
        assert_eq!(b.gutter.as_deref(), Some((i + 1).to_string().as_str()));
        assert_eq!(b.space_before, if i == 0 { 16.0 } else { 8.0 });
    }

    let no_numbers = ContentOptions { verse_numbers: false, ..per_line };
    let c = bible.chapter("web".into(), index("jn 3"), no_numbers);
    assert!(c.blocks.iter().all(|b| b.gutter.is_none() && b.runs.iter().all(|r| !r.number)));
    let _ = std::fs::remove_dir_all(dir);
}

/// Every verse's words arrive, in order, in runs marked with its number.
#[test]
fn every_chapter_keeps_all_its_text() {
    let (bible, dir) = open("complete");
    let squash =
        |s: &str| s.chars().filter(|c| !c.is_whitespace()).flat_map(char::to_lowercase).collect::<String>();
    for id in ["kjv", "bsb"] {
        let t = bible_core::bundled::load(id).unwrap();
        for c in bible_core::ChapterIdx::all() {
            for options in
                [ContentOptions::default(), ContentOptions { verse_per_line: true, ..Default::default() }]
            {
                let content = bible.chapter(id.into(), c.0, options);
                for v in t.verses_in(c) {
                    let got: String = content
                        .blocks
                        .iter()
                        .flat_map(|b| &b.runs)
                        .filter(|r| r.verse == v.number && !r.number)
                        .map(|r| r.text.as_str())
                        .collect();
                    assert_eq!(squash(&got), squash(&v.plain_text()), "{id} {}:{}", c.display(), v.number);
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn search_pages_and_utf16_match_ranges() {
    let (bible, dir) = open("search");
    let page = bible.search("kjv".into(), "love one another".into(), SearchScope::All, false, 0, 5).unwrap();
    assert!(page.total > 5 && page.hits.len() == 5, "{}", page.total);
    assert!(page.elapsed_ms < 1_000.0);
    let rest = bible
        .search("kjv".into(), "love one another".into(), SearchScope::All, false, page.total - 2, 5)
        .unwrap();
    assert_eq!(rest.hits.len(), 2);

    // The WEB's curly quotes are three bytes in UTF-8 and one unit in UTF-16.
    let john = SearchScope::Book { book: 58 };
    let page = bible.search("web".into(), "kingdom".into(), john, false, 0, 100).unwrap();
    let hit = page.hits.iter().find(|h| h.reference == "John 3:3").unwrap();
    assert!(hit.text.contains('“'));
    let units: Vec<u16> = hit.text.encode_utf16().collect();
    let m = hit.matches[0];
    let matched = String::from_utf16(&units[m.start as usize..(m.start + m.length) as usize]).unwrap();
    assert_eq!(matched, "Kingdom");
    assert_eq!((hit.verse, hit.chapter_index), (VerseRef { book: 58, chapter: 3, verse: 3 }, index("jn 3")));

    let nt = bible.search("kjv".into(), "faith".into(), SearchScope::NewTestament, false, 0, 1).unwrap();
    let ot = bible.search("kjv".into(), "faith".into(), SearchScope::OldTestament, false, 0, 1).unwrap();
    assert!(nt.total > ot.total && nt.hits[0].verse.book >= 55);
    assert!(bible.search("nlt".into(), "love".into(), SearchScope::All, false, 0, 1).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_search_that_finds_nothing_suggests_a_spelling() {
    let (bible, dir) = open("suggest");
    let page = bible.search("kjv".into(), "bethsaida pool".into(), SearchScope::All, true, 0, 10).unwrap();
    assert_eq!(page.total, 0);
    let s = page.suggestion.unwrap();
    assert_eq!((s.query.as_str(), s.total), ("bethesda pool", 1));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn study_marks_follow_the_verse_and_persist() {
    let (bible, dir) = open("study");
    let jn3 = index("jn 3");
    assert!(bible.marks("kjv".into(), jn3).is_empty());
    bible.set_highlight(JOHN_3_16, Some(HighlightColor::Green)).unwrap();
    bible.set_note(JOHN_3_16, "God's love".into()).unwrap();
    bible.set_bookmark(VerseRef { verse: 1, ..JOHN_3_16 }, true).unwrap();
    bible.set_setting("ios".into(), "{\"theme\":\"sepia\"}".into()).unwrap();
    assert!(bible.set_highlight(VerseRef { book: 82, ..JOHN_3_16 }, None).is_err());
    drop(bible);

    let bible = Bible::new(dir.to_string_lossy().into()).unwrap();
    let marks = bible.marks("web".into(), jn3);
    assert_eq!(
        marks,
        [
            VerseMarks { verse: 1, highlight: None, note: false, bookmark: true },
            VerseMarks { verse: 16, highlight: Some(HighlightColor::Green), note: true, bookmark: false },
        ]
    );
    assert_eq!(bible.note(JOHN_3_16).unwrap().as_deref(), Some("God's love"));
    assert_eq!(bible.setting("ios".into()).as_deref(), Some("{\"theme\":\"sepia\"}"));

    let notes = bible.study_items(StudyKind::Notes, "web".into());
    assert_eq!((notes[0].reference.as_str(), notes[0].body.as_str()), ("John 3:16", "God's love"));
    assert!(notes[0].text.starts_with("For God so loved the world"));
    assert_eq!(notes[0].highlight, Some(HighlightColor::Green));
    assert_eq!(bible.study_items(StudyKind::Bookmarks, "kjv".into())[0].verse.verse, 1);
    assert_eq!(bible.study_items(StudyKind::Highlights, "kjv".into())[0].chapter_index, jn3);

    let md = bible.export(ExportFormat::Markdown, "kjv".into()).unwrap();
    assert!(md.contains("### John 3:16") && md.contains("God's love"), "{md}");
    assert!(bible.export(ExportFormat::Json, "kjv".into()).unwrap().contains("\"color\": \"green\""));

    bible.set_note(JOHN_3_16, "  ".into()).unwrap();
    bible.set_highlight(JOHN_3_16, None).unwrap();
    bible.set_bookmark(VerseRef { verse: 1, ..JOHN_3_16 }, false).unwrap();
    assert!(bible.marks("kjv".into(), jn3).is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn copy_text_carries_the_reference() {
    let (bible, dir) = open("copy");
    let one = bible.copy_text("kjv".into(), vec![JOHN_3_16]).unwrap();
    assert!(one.starts_with("“For God so loved the world") && one.ends_with("” — John 3:16 (KJV)"), "{one}");
    let two = bible.copy_text("web".into(), vec![JOHN_3_16, VerseRef { verse: 17, ..JOHN_3_16 }]).unwrap();
    assert!(two.starts_with("John 3:16–17 (WEB)\n16 For God so loved"), "{two}");
    assert!(bible.copy_text("kjv".into(), vec![VerseRef { verse: 99, ..JOHN_3_16 }]).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unknown_translations_and_chapters_fail_softly() {
    let (bible, dir) = open("missing");
    let c = bible.chapter("nlt".into(), index("jn 3"), ContentOptions::default());
    assert!(matches!(c.status, ChapterStatus::Failed { .. }) && c.blocks.is_empty());
    assert_eq!(c.book_name, "John");
    let last = bible.chapter("kjv".into(), u16::MAX, ContentOptions::default());
    assert_eq!((last.book, last.chapter), (81, 22));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn church_days_and_the_fathers() {
    let (bible, dir) = open("church");
    let pascha = church_day(2026, 4, 12, false);
    assert!(pascha.feasts[0].starts_with("Pascha"));
    assert_eq!((pascha.fast.as_str(), pascha.days_to_pascha), ("No fast", 0));
    let nativity_old = church_day(2027, 1, 7, true);
    assert_eq!(nativity_old.feasts, ["Nativity of Our Lord"]);
    let tuesday = church_day(2026, 10, 20, false);
    assert_eq!(tuesday.matins.iter().map(|k| k.number).collect::<Vec<_>>(), [7, 8]);
    assert_eq!(tuesday.vespers[0].number, 9);
    assert_eq!(tuesday.matins[0].chapter_index, index("ps 46"));
    assert!(!bible.fathers_installed());
    assert!(bible.fathers(JOHN_3_16).unwrap().is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

/// The rule on a date, its plans' days counted from UTC midnights.
fn rule_on(bible: &Bible, year: i32, month: u8, day: u8) -> RuleDay {
    bible.rule_day(year, month, day, false, civil_day(year, month, day) * 86_400)
}

fn month(bible: &Bible, year: i32, month: u8, old_calendar: bool) -> Vec<MonthDay> {
    bible.church_month(year, month, old_calendar, civil_day(year, month, 1) * 86_400)
}

fn rule_item(day: &RuleDay, id: &str) -> Option<RuleDayItem> {
    day.blocks.iter().flat_map(|b| &b.items).find(|i| i.id == id).cloned()
}

fn new_item(id: &str, kind: RuleKind) -> RuleItem {
    RuleItem {
        id: id.into(),
        title: id.into(),
        kind,
        note: String::new(),
        season: RuleSeason::Always,
        optional: false,
    }
}

#[test]
fn a_days_rule_opens_what_each_item_needs() {
    let (bible, dir) = open("rule");
    assert_eq!(bible.rule(), starter_rule());
    // Tuesday 20 October 2026, outside Lent.
    let day = rule_on(&bible, 2026, 10, 20);
    assert_eq!(day.day, civil_day(2026, 10, 20));
    let blocks: Vec<(&str, Option<&str>)> =
        day.blocks.iter().map(|b| (b.id.as_str(), b.time.as_deref())).collect();
    assert_eq!(
        blocks,
        [("morning", Some("07:00")), ("meals", None), ("midday", Some("12:00")), ("evening", Some("20:30"))]
    );
    assert!(rule_item(&day, "evening-ephrem").is_none(), "Great Lent only");
    let meal = rule_item(&day, "meal-prayer").unwrap();
    assert!(
        matches!(&meal.open, RuleOpen::Text { text, source: Some(_), .. } if text.starts_with("Our Father")),
        "{meal:?}"
    );
    let own_book = rule_item(&day, "morning-prayers").unwrap();
    assert_eq!((own_book.open, own_book.note.as_str()), (RuleOpen::Nothing, "From your prayer book"));
    let psalter = rule_item(&day, "evening-kathisma").unwrap();
    assert_eq!(psalter.detail.as_deref(), Some("Kathisma 9 · Psalms 64–69"));
    assert_eq!(psalter.open, RuleOpen::Chapter { chapter_index: index("ps 64") });
    assert_eq!(
        rule_item(&day, "midday-psalm").unwrap().open,
        RuleOpen::Chapter { chapter_index: index("ps 90") }
    );
    // The day's readings: the Liturgy's Epistle and Gospel, opening at the Gospel, and the
    // Fathers on its first verse.
    let readings = rule_item(&day, "morning-reading").unwrap();
    assert_eq!(readings.detail.as_deref(), Some("2 Timothy 2:1–10 · Luke 9:23–27"));
    assert_eq!(readings.open, RuleOpen::Passage { chapter_index: index("lk 9"), verse: 23 });
    let fathers = rule_item(&day, "evening-fathers").unwrap();
    assert_eq!(fathers.detail.as_deref(), Some("Luke 9:23–27"));
    assert_eq!(fathers.open, RuleOpen::Fathers { verse: VerseRef { book: 57, chapter: 9, verse: 23 } });
    // A Lenten weekday has no Gospel: the first reading, and it says so.
    let lent = rule_item(&rule_on(&bible, 2026, 3, 4), "morning-reading").unwrap();
    assert_eq!(lent.detail.as_deref(), Some("No Gospel today · Isaiah 5:16–25 at the Sixth Hour"));
    assert_eq!(lent.open, RuleOpen::Passage { chapter_index: index("is 5"), verse: 16 });
    // Holy Friday has no Liturgy: the first of its Gospels.
    let friday = rule_item(&rule_on(&bible, 2026, 4, 10), "evening-fathers").unwrap();
    assert_eq!(friday.detail.as_deref(), Some("John 13:31–18:1 at Matins of the Twelve Passion Gospels"));
    assert_eq!(friday.open, RuleOpen::Fathers { verse: VerseRef { book: 58, chapter: 13, verse: 31 } });
    let jesus = rule_item(&day, "midday-jesus-prayer").unwrap();
    assert_eq!((jesus.open, jesus.optional), (RuleOpen::JesusPrayer { count: 33 }, true));
    assert!(matches!(
        rule_item(&day, "evening-reflection").unwrap().open,
        RuleOpen::Text { source: None, .. }
    ));

    let sunday = rule_on(&bible, 2026, 10, 25);
    let none = rule_item(&sunday, "evening-kathisma").unwrap();
    assert_eq!(
        (none.open, none.detail.as_deref()),
        (RuleOpen::NotToday, Some("No kathisma on Sunday evening"))
    );
    assert!(rule_item(&rule_on(&bible, 2026, 3, 4), "evening-ephrem").is_some(), "in Great Lent");

    // A kathisma in the morning reads the Matins ones; the person's own words are a text.
    let mut rule = bible.rule();
    rule.blocks[0].items.push(new_item("matins", RuleKind::Kathisma));
    let own = RuleKind::Prayers {
        prayer: None,
        text: Some("O Lord, grant me to greet the coming day in peace.".into()),
    };
    rule.blocks[0].items.push(new_item("optina", own));
    // So does one in a part of the day before noon, whatever it's called.
    let early = RuleBlock {
        id: "part-1".into(),
        title: "Before work".into(),
        time: Some("06:30".into()),
        items: vec![new_item("early", RuleKind::Kathisma)],
    };
    rule.blocks.push(early);
    bible.set_rule(rule.clone()).unwrap();
    assert_eq!(bible.rule(), rule);
    let day = rule_on(&bible, 2026, 10, 20);
    let matins = rule_item(&day, "matins").unwrap();
    assert_eq!(matins.detail.as_deref(), Some("Kathismata 7 and 8 · Psalms 46–54, 55–63"));
    assert_eq!(matins.open, RuleOpen::Chapter { chapter_index: index("ps 46") });
    assert_eq!(rule_item(&day, "early").unwrap().detail, matins.detail);
    assert!(
        matches!(rule_item(&day, "optina").unwrap().open, RuleOpen::Text { title, .. } if title == "optina")
    );

    // Check marks belong to their day.
    bible.set_rule_done(day.day, "morning-prayers".into(), true).unwrap();
    assert!(rule_item(&rule_on(&bible, 2026, 10, 20), "morning-prayers").unwrap().done);
    assert!(!rule_item(&rule_on(&bible, 2026, 10, 21), "morning-prayers").unwrap().done);
    bible.set_rule_done(day.day, "morning-prayers".into(), false).unwrap();
    assert!(!rule_item(&rule_on(&bible, 2026, 10, 20), "morning-prayers").unwrap().done);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_month_shows_fasts_feasts_and_the_days_kept() {
    let (bible, dir) = open("month");
    let oct = month(&bible, 2026, 10, false);
    assert_eq!(oct.len(), 31);
    assert_eq!((oct[0].date, oct[0].weekday, oct[0].day), (1, 4, civil_day(2026, 10, 1)));
    assert_eq!((oct[19].fast, oct[20].fast), (FastLevel::None, FastLevel::Strict), "Tuesday, Wednesday");
    assert!(oct.iter().all(|d| d.kept == RuleKept::Nothing && !d.great_feast));

    let required = [
        "morning-prayers",
        "morning-reading",
        "meal-prayer",
        "midday-psalm",
        "evening-prayers",
        "evening-kathisma",
        "evening-fathers",
        "evening-reflection",
    ];
    for id in required {
        bible.set_rule_done(oct[19].day, id.into(), true).unwrap();
    }
    bible.set_rule_done(oct[18].day, "midday-jesus-prayer".into(), true).unwrap();
    bible.set_rule_done(oct[17].day, "no-longer-in-the-rule".into(), true).unwrap();
    let oct = month(&bible, 2026, 10, false);
    let kept: Vec<RuleKept> = oct[17..20].iter().map(|d| d.kept).collect();
    assert_eq!(kept, [RuleKept::Nothing, RuleKept::Partly, RuleKept::Fully]);

    // On a Sunday there's no kathisma in the evening: the rest keeps the day.
    for id in required.iter().filter(|&&id| id != "evening-kathisma") {
        bible.set_rule_done(oct[24].day, (*id).into(), true).unwrap();
    }
    assert_eq!(month(&bible, 2026, 10, false)[24].kept, RuleKept::Fully, "Sunday 25 October");

    let aug = month(&bible, 2026, 8, false);
    assert!(aug[5].great_feast && aug[14].great_feast, "Transfiguration, Dormition");
    assert!(!aug[28].great_feast, "the Beheading is a great day but not one of the Twelve");
    assert_eq!((aug[9].fast, aug[5].fast), (FastLevel::Strict, FastLevel::Fish));
    let april = month(&bible, 2026, 4, false);
    assert!(april[11].great_feast && april[4].great_feast, "Pascha, Palm Sunday");
    assert_eq!(april[10].fast, FastLevel::Wine, "Holy Saturday");
    let jan = month(&bible, 2027, 1, true);
    assert!(jan[6].great_feast && !jan[0].great_feast, "the Nativity on the Old calendar");
    assert_eq!(month(&bible, 2026, 12, false).len(), 31);
    assert_eq!(month(&bible, 2028, 2, false).len(), 29);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn plans_start_and_read_day_by_day() {
    let (bible, dir) = open("plans");
    let all = reading_plans();
    for id in
        ["psalter-20", "orthodox-bible-180", "orthodox-bible-365", "orthodox-bible-730", "nt-psalter-180"]
    {
        assert!(all.iter().any(|p| p.id == id), "{id}");
    }
    assert_eq!(all.iter().find(|p| p.id == "psalter-20").unwrap().days, 20);
    assert!(bible.active_plans(0).is_empty());
    assert!(bible.start_plan("nonexistent".into(), 0).is_err());

    // A local midnight.
    let start = 1_792_368_000;
    bible.start_plan("psalter-20".into(), start).unwrap();
    let active = bible.active_plans(start + 2 * 86_400);
    assert_eq!((active[0].plan.id.as_str(), active[0].day, active[0].next), ("psalter-20", 3, Some(1)));
    assert_eq!(bible.active_plans(start + 86_400 - 3_600)[0].day, 2, "a 23-hour day is a day");

    let first = bible.plan_day("psalter-20".into(), 1).unwrap();
    assert_eq!((first.label.as_str(), first.passages[0].label.as_str()), ("Psalms 1–8", "Psalm 1"));
    assert_eq!(first.passages[0].chapter_index, index("ps 1"));
    for i in 0..first.passages.len() {
        bible.set_plan_read("psalter-20".into(), 1, i as u8, true).unwrap();
    }
    assert!(bible.plan_day("psalter-20".into(), 1).unwrap().read);
    assert_eq!(bible.active_plans(start)[0].next, Some(2));
    let days = bible.plan_days("psalter-20".into());
    assert_eq!(days.len(), 20);
    assert_eq!(days[1].label, "Psalms 9–16");
    assert_eq!(days[19].label, "Psalms 143–150 · Psalm 151");
    assert!(bible.plan_day("psalter-20".into(), 21).is_none());

    // In the rule: the plan's day that falls on the date, or an offer to start.
    let mut rule = bible.rule();
    rule.blocks[0].items.push(new_item("psalter", RuleKind::Plan { plan: "psalter-20".into() }));
    rule.blocks[0].items.push(new_item("nt", RuleKind::Plan { plan: "nt-psalter-180".into() }));
    bible.set_rule(rule).unwrap();
    // The plan began on Monday 19 October: Tuesday is its second day, not read yet.
    let day = rule_on(&bible, 2026, 10, 20);
    let psalter = rule_item(&day, "psalter").unwrap();
    assert_eq!(psalter.open, RuleOpen::Plan { plan: "psalter-20".into(), day: 2 });
    assert_eq!((psalter.detail.as_deref(), psalter.done), (Some("Day 2 · Psalms 9–16"), false));
    let nt = rule_item(&day, "nt").unwrap();
    assert!(matches!(nt.open, RuleOpen::StartPlan { ref plan, .. } if plan == "nt-psalter-180"));
    // Done once that day's reading is read, which keeps the day in the month too.
    let monday = rule_item(&rule_on(&bible, 2026, 10, 19), "psalter").unwrap();
    assert_eq!((monday.open, monday.done), (RuleOpen::Plan { plan: "psalter-20".into(), day: 1 }, true));
    // A tick of its own still counts, as on the Mac.
    bible.set_rule_done(day.day, "psalter".into(), true).unwrap();
    assert!(rule_item(&rule_on(&bible, 2026, 10, 20), "psalter").unwrap().done);
    bible.set_rule_done(day.day, "psalter".into(), false).unwrap();
    let oct = month(&bible, 2026, 10, false);
    assert_eq!((oct[18].kept, oct[19].kept), (RuleKept::Partly, RuleKept::Nothing));
    let before = rule_item(&rule_on(&bible, 2026, 10, 18), "psalter").unwrap();
    assert_eq!((before.open, before.detail.as_deref()), (RuleOpen::Nothing, Some("No reading on this day")));

    bible.stop_plan("psalter-20".into()).unwrap();
    assert!(bible.active_plans(start).is_empty());
    assert!(prayers().iter().any(|p| p.id == "before-meals"));
    let _ = std::fs::remove_dir_all(dir);
}

/// "Liturgy Epistle 2 Timothy 2:1–10 (Great Martyr …)": a day's readings, one line each.
fn lessons(year: i32, month: u8, day: u8) -> Vec<String> {
    day_readings(year, month, day, false)
        .iter()
        .map(|r| {
            let occasion = r.occasion.as_ref().map(|o| format!(" ({o})")).unwrap_or_default();
            format!("{} {:?} {}{occasion}", r.service, r.kind, r.label)
        })
        .collect()
}

#[test]
fn the_days_readings_open_in_the_reader() {
    // Tuesday 20 October 2026: St Artemius's Epistle with the weekday's Gospel.
    let day = day_readings(2026, 10, 20, false);
    assert_eq!(
        lessons(2026, 10, 20),
        [
            "Liturgy Epistle 2 Timothy 2:1–10 (Great Martyr Artemius of Antioch; St Gerasimos of Kefalonia)",
            "Liturgy Gospel Luke 9:23–27",
        ]
    );
    let gospel = &day[1];
    assert!(gospel.at_liturgy && gospel.title.is_none() && gospel.confidence == ReadingConfidence::High);
    assert_eq!(
        gospel.passages,
        [ReadingPassage {
            from: VerseRef { book: 57, chapter: 9, verse: 23 },
            to: VerseRef { book: 57, chapter: 9, verse: 27 },
            chapter_index: index("lk 9"),
        }]
    );
    // A Sunday: the Resurrection Gospel at Matins; both cycles are the day's own.
    let sunday = day_readings(2026, 10, 25, false);
    assert_eq!(sunday[0].title.as_deref(), Some("Eothinon 10"));
    assert!(sunday.iter().all(|r| r.occasion.is_none()));
    // A feast that takes the whole day is the day's own too.
    assert!(day_readings(2026, 10, 26, false).iter().all(|r| r.occasion.is_none()), "St Demetrius");
    assert_eq!(
        lessons(2024, 6, 30)[1..],
        [
            "Liturgy Epistle Hebrews 11:33–12:2",
            "Liturgy Gospel Matthew 9:36–10:8 (Synaxis of the Holy, Glorious and All-praised Twelve Apostles)",
        ],
        "All Saints with the Apostles' Gospel"
    );
    // A Lenten weekday has no Liturgy of its own: the Hours' and the Presanctified's readings.
    assert_eq!(
        lessons(2026, 3, 4),
        [
            "Sixth Hour OldTestament Isaiah 5:16–25",
            "Presanctified Liturgy OldTestament Genesis 4:16–26",
            "Presanctified Liturgy OldTestament Proverbs 5:15–6:3",
        ]
    );
    // Holy Friday: the Passion Gospels and the Royal Hours, with no Liturgy.
    let friday = day_readings(2026, 4, 10, false);
    assert!(friday.iter().all(|r| !r.at_liturgy && r.occasion.is_none()));
    assert_eq!((friday.len(), friday[0].title.as_deref()), (29, Some("1st")));
    assert_eq!(friday[0].served.as_deref(), Some("Thursday evening"));
    // A feast that meets Pentecost is shown beside it, to check with the parish.
    let pentecost = day_readings(2040, 6, 24, false);
    let forerunner: Vec<&Reading> = pentecost.iter().filter(|r| r.occasion.is_some()).collect();
    assert_eq!(forerunner.len(), 3);
    assert!(forerunner.iter().all(|r| r.confidence == ReadingConfidence::Low));
    // On the Old calendar the Nativity is on 7 January.
    let nativity = day_readings(2027, 1, 7, true);
    assert!(nativity.iter().any(|r| r.at_liturgy && r.label == "Matthew 2:1–12"));
}

#[test]
fn sync_carries_study_and_the_rule_between_devices() {
    let (phone, phone_dir) = open("sync-phone");
    let (mac, mac_dir) = open("sync-mac");
    let folder = std::env::temp_dir().join(format!("bible-ffi-sync-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let shared = folder.to_string_lossy().to_string();

    mac.set_note(JOHN_3_16, "From the Mac".into()).unwrap();
    mac.set_highlight(JOHN_3_16, Some(HighlightColor::Blue)).unwrap();
    mac.set_rule_done(civil_day(2026, 10, 20), "morning-prayers".into(), true).unwrap();
    let first = mac.sync(shared.clone()).unwrap();
    assert_eq!(first, SyncReport { applied: 0, devices: 0, skipped: vec![] }, "the folder is made");

    // The phone gets all three, and a file it can't read is reported, not fatal.
    std::fs::write(folder.join("broken.json"), "{").unwrap();
    let report = phone.sync(shared.clone()).unwrap();
    assert_eq!((report.applied, report.devices), (3, 1));
    assert_eq!(report.skipped.len(), 1);
    assert!(report.skipped[0].starts_with("broken.json: "), "{:?}", report.skipped);
    assert_eq!(phone.note(JOHN_3_16).unwrap().as_deref(), Some("From the Mac"));
    assert!(rule_on(&phone, 2026, 10, 20).blocks[0].items[0].done);

    // And back: the phone's edits reach the Mac; syncing again applies nothing.
    phone.set_note(JOHN_3_16, "".into()).unwrap();
    phone.sync(shared.clone()).unwrap();
    assert_eq!(mac.sync(shared.clone()).unwrap().applied, 1);
    assert_eq!(mac.note(JOHN_3_16).unwrap(), None);
    assert_eq!(mac.marks("kjv".into(), index("jn 3"))[0].highlight, Some(HighlightColor::Blue));
    assert_eq!(phone.sync(shared.clone()).unwrap().applied, 0);
    for dir in [phone_dir, mac_dir, folder] {
        let _ = std::fs::remove_dir_all(dir);
    }
}
