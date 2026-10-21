//! Septuagint numbering: the psalms in every translation, and the bundled Septuagint's own.
//!
//! The grid numbers the Psalms as the Hebrew text and the KJV do. The Septuagint, which the
//! Orthodox Church reads, joins and splits some of them, so most of its psalm numbers are one
//! lower (Psalm 51, "Have mercy on me, O God", is its Psalm 50). Text is always stored by grid
//! numbers, so notes, parallel columns and commentary line up whatever the translation; this
//! module converts for display and for references typed in Septuagint numbers, when that
//! numbering is turned on.
//!
//! Verses are numbered as in English Bibles (psalm titles aren't counted), so only the Psalms
//! that are joined or split change their verse numbers:
//!
//! | Septuagint | Grid (Hebrew) |
//! |---|---|
//! | 9:1–20, 9:21–38 | 9:1–20, 10:1–18 |
//! | 10–112 | 11–113 |
//! | 113:1–8, 113:9–26 | 114, 115 |
//! | 114, 115 | 116:1–9, 116:10–19 |
//! | 116–145 | 117–146 |
//! | 146, 147 | 147:1–11, 147:12–20 |
//! | 151 | Psalm 151 (its own book in the grid) |
//!
//! The bundled Septuagint (eBible.org's LXX2012) also numbers some chapters and verses of other
//! books its own way: Jeremiah 25–51 in the Septuagint's order, Exodus 35–40 and 1 Kings 20–21
//! rearranged, verses joined or split where Brenton's sentences run differently. [`MOVED`] puts
//! them back on the grid when it is built, so that a grid reference opens the same text in every
//! column, and a verse the Septuagint lacks is simply missing.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::books::{BookId, ChapterIdx};

static ON: AtomicBool = AtomicBool::new(false);

/// Shows and reads psalm numbers in Septuagint numbering everywhere (labels, references).
pub fn set_numbering(on: bool) {
    ON.store(on, Ordering::Relaxed);
}

pub fn numbering() -> bool {
    ON.load(Ordering::Relaxed)
}

fn psalm_151() -> BookId {
    BookId::from_code("PS2").expect("Psalm 151 is in the grid")
}

/// The Septuagint number of a grid psalm chapter, or `None` outside the Psalms. A grid psalm
/// that the Septuagint joins to the one before keeps that one's number (Psalm 10 is 9).
pub fn psalm_number(c: ChapterIdx) -> Option<u8> {
    if c.book() == psalm_151() {
        return Some(151);
    }
    (c.book() == BookId::PSALMS).then(|| to_lxx(c.chapter(), 1).0)
}

/// A grid psalm verse in Septuagint numbering.
pub fn to_lxx(chapter: u8, verse: u8) -> (u8, u8) {
    match chapter {
        10 => (9, verse.saturating_add(20)),
        11..=113 | 117..=146 => (chapter - 1, verse),
        114 => (113, verse),
        115 => (113, verse.saturating_add(8)),
        116 if verse <= 9 => (114, verse),
        116 => (115, verse - 9),
        147 if verse <= 11 => (146, verse),
        147 => (147, verse - 11),
        _ => (chapter, verse),
    }
}

/// Where a psalm verse given in Septuagint numbering sits in the grid: the book (Psalms, or
/// Psalm 151), chapter and verse. Verse 1 stands for a whole chapter.
pub fn from_lxx(chapter: u8, verse: u8) -> Option<(BookId, u8, u8)> {
    let (c, v) = match chapter {
        0 | 152.. => return None,
        151 => return Some((psalm_151(), 1, verse)),
        9 if verse > 20 => (10, verse - 20),
        10..=112 | 116..=145 => (chapter + 1, verse),
        113 if verse > 8 => (115, verse - 8),
        113 => (114, verse),
        114 => (116, verse),
        115 => (116, verse.saturating_add(9)),
        146 => (147, verse),
        147 => (147, verse.saturating_add(11)),
        _ => (chapter, verse),
    };
    Some((BookId::PSALMS, c, v))
}

/// Where a psalm verse of eBible.org's LXX2012 sits in the grid. LXX2012 follows the Septuagint,
/// except that it splits Psalm 113 in two (113 = Hebrew 114, 114 = 115) and so its 115 is all
/// of Hebrew 116.
pub fn from_lxx2012(chapter: u8, verse: u8) -> Option<(BookId, u8, u8)> {
    match chapter {
        113 => Some((BookId::PSALMS, 114, verse)),
        114 => Some((BookId::PSALMS, 115, verse)),
        115 => Some((BookId::PSALMS, 116, verse)),
        _ => from_lxx(chapter, verse),
    }
}

/// How a grid reference is shown: name, chapter (none for Psalm 151, a book of one psalm) and
/// verse. With Septuagint numbering on, the Psalms are numbered as the Septuagint numbers them.
pub fn shown(book: BookId, chapter: u8, verse: u8) -> (&'static str, Option<u8>, u8) {
    shown_in(numbering(), book, chapter, verse)
}

/// [`shown`] with Septuagint numbering `lxx`.
pub fn shown_in(lxx: bool, book: BookId, chapter: u8, verse: u8) -> (&'static str, Option<u8>, u8) {
    if book == psalm_151() {
        ("Psalm 151", None, verse)
    } else if book == BookId::PSALMS && lxx {
        let (c, v) = to_lxx(chapter, verse);
        ("Psalm", Some(c), v)
    } else {
        (book.ref_name(), Some(chapter), verse)
    }
}

/// [`TranslationBuilder::remap`](crate::model::TranslationBuilder::remap) for LXX2012: the
/// verses in [`MOVED`] and its psalms move to grid numbers; other verses keep theirs.
pub fn remap_lxx2012(book: BookId, chapter: u8, verse: u8) -> Option<(BookId, u8, u8, u8)> {
    let code = book.info().code;
    let moved = MOVED.iter().find(|m| m.0 == code && m.1 == chapter && (m.2.0..=m.2.1).contains(&verse));
    Some(match moved {
        Some(&(_, _, (first, last), c, (to, end))) if last - first == end - to => {
            let v = to + (verse - first);
            (book, c, v, v)
        }
        // One verse holding several of the grid's, or several the grid counts as one.
        Some(&(_, _, _, c, (to, end))) => (book, c, to, end),
        None if book == BookId::PSALMS => return from_lxx2012(chapter, verse).map(|(b, c, v)| (b, c, v, v)),
        None => (book, chapter, verse, verse),
    })
}

/// A run of [`MOVED`]: book, chapter and verses in LXX2012, chapter and verses in the grid.
pub type Moved = (&'static str, u8, (u8, u8), u8, (u8, u8));

/// LXX2012 verses that the grid numbers otherwise, besides the psalm numbers above: (book,
/// chapter, first and last verse) in LXX2012 are the grid's (chapter, first and last verse). Runs
/// of the same length move verse for verse; one verse onto several holds them all, and several
/// onto one are joined. Found by comparing each book with the KJV (names, numbers, wording).
/// Verses the Septuagint lacks stay missing (Jeremiah 33:14–26, most of Exodus 36:10–33): where
/// LXX2012 numbers a verse as a range for them (1 Kings 9:14–25), it is only its first verse
/// unless listed here. Its additions stay in the verse LXX2012 puts them in (Esther's, 1 Kings
/// 2:46), apart from 1 Kings 2:35's, which the build splits out of LXX2012's 3:1.
pub const MOVED: &[Moved] = &[
    ("GEN", 22, (15, 15), 22, (15, 16)),
    ("GEN", 22, (16, 23), 22, (17, 24)),
    ("GEN", 35, (21, 22), 35, (22, 22)),
    ("GEN", 36, (44, 44), 37, (1, 1)),
    ("GEN", 37, (1, 2), 37, (2, 2)),
    ("GEN", 37, (32, 32), 37, (32, 33)),
    ("GEN", 37, (33, 35), 37, (34, 36)),
    ("EXO", 20, (13, 14), 20, (14, 15)),
    ("EXO", 20, (15, 15), 20, (13, 13)),
    ("EXO", 21, (16, 16), 21, (17, 17)),
    ("EXO", 21, (17, 17), 21, (16, 16)),
    ("EXO", 28, (23, 23), 28, (29, 29)),
    ("EXO", 28, (26, 26), 28, (30, 30)),
    ("EXO", 28, (27, 39), 28, (31, 43)),
    ("EXO", 32, (9, 9), 32, (8, 8)),
    ("EXO", 35, (8, 11), 35, (9, 12)),
    ("EXO", 35, (12, 13), 35, (17, 17)),
    ("EXO", 35, (14, 14), 35, (15, 15)),
    ("EXO", 35, (15, 15), 35, (13, 13)),
    ("EXO", 35, (16, 16), 35, (14, 14)),
    ("EXO", 35, (17, 17), 35, (16, 16)),
    ("EXO", 35, (18, 18), 35, (19, 19)),
    ("EXO", 36, (8, 23), 39, (1, 16)),
    ("EXO", 36, (24, 24), 39, (16, 16)),
    ("EXO", 36, (25, 37), 39, (17, 29)),
    ("EXO", 36, (38, 39), 39, (30, 30)),
    ("EXO", 36, (40, 40), 39, (31, 31)),
    ("EXO", 37, (1, 2), 36, (8, 9)),
    ("EXO", 37, (3, 6), 36, (35, 38)),
    ("EXO", 37, (7, 21), 38, (9, 23)),
    ("EXO", 38, (1, 3), 37, (1, 3)),
    ("EXO", 38, (4, 9), 37, (5, 10)),
    ("EXO", 38, (10, 10), 37, (13, 14)),
    ("EXO", 38, (11, 15), 37, (15, 19)),
    ("EXO", 38, (16, 16), 37, (20, 22)),
    ("EXO", 38, (17, 17), 37, (23, 23)),
    ("EXO", 38, (18, 21), 36, (34, 34)),
    ("EXO", 38, (22, 22), 38, (1, 1)),
    ("EXO", 38, (23, 23), 38, (3, 3)),
    ("EXO", 38, (24, 24), 38, (4, 5)),
    ("EXO", 38, (25, 25), 37, (29, 29)),
    ("EXO", 38, (26, 27), 38, (8, 8)),
    ("EXO", 39, (1, 4), 38, (24, 27)),
    ("EXO", 39, (5, 5), 38, (27, 27)),
    ("EXO", 39, (6, 9), 38, (28, 31)),
    ("EXO", 39, (10, 10), 38, (31, 31)),
    ("EXO", 39, (11, 13), 39, (32, 32)),
    ("EXO", 39, (14, 14), 39, (33, 33)),
    ("EXO", 39, (15, 15), 39, (35, 35)),
    ("EXO", 39, (16, 16), 39, (38, 38)),
    ("EXO", 39, (17, 17), 39, (37, 37)),
    ("EXO", 39, (18, 18), 39, (36, 36)),
    ("EXO", 39, (19, 19), 39, (41, 41)),
    ("EXO", 39, (20, 20), 39, (40, 40)),
    ("EXO", 39, (21, 21), 39, (34, 34)),
    ("EXO", 39, (22, 23), 39, (42, 43)),
    ("EXO", 40, (6, 6), 40, (6, 8)),
    ("EXO", 40, (31, 31), 40, (33, 33)),
    ("LEV", 6, (4, 4), 6, (4, 5)),
    ("LEV", 6, (5, 7), 6, (6, 8)),
    ("LEV", 6, (8, 8), 6, (9, 9)),
    ("LEV", 6, (31, 40), 7, (1, 10)),
    ("LEV", 7, (1, 28), 7, (11, 38)),
    ("LEV", 8, (18, 18), 8, (18, 19)),
    ("LEV", 8, (19, 29), 8, (20, 30)),
    ("NUM", 1, (24, 35), 1, (26, 37)),
    ("NUM", 1, (36, 37), 1, (24, 25)),
    ("NUM", 9, (18, 18), 9, (18, 19)),
    ("NUM", 9, (19, 22), 9, (20, 23)),
    ("NUM", 13, (1, 1), 12, (16, 16)),
    ("NUM", 13, (2, 34), 13, (1, 33)),
    ("NUM", 21, (19, 19), 21, (19, 20)),
    ("NUM", 21, (20, 21), 21, (21, 22)),
    ("NUM", 26, (15, 23), 26, (19, 27)),
    ("NUM", 26, (24, 27), 26, (15, 18)),
    ("NUM", 26, (28, 31), 26, (44, 47)),
    ("NUM", 26, (32, 47), 26, (28, 43)),
    ("NUM", 27, (3, 3), 27, (3, 4)),
    ("NUM", 27, (4, 7), 27, (5, 8)),
    ("NUM", 30, (1, 1), 29, (40, 40)),
    ("NUM", 30, (2, 17), 30, (1, 16)),
    ("DEU", 24, (1, 1), 23, (25, 25)),
    ("DEU", 24, (2, 2), 23, (24, 24)),
    ("DEU", 24, (3, 24), 24, (1, 22)),
    ("DEU", 31, (29, 29), 31, (29, 30)),
    ("JOS", 5, (15, 15), 5, (14, 14)),
    ("JOS", 5, (16, 16), 5, (15, 15)),
    ("JOS", 6, (4, 4), 6, (5, 5)),
    ("JOS", 6, (7, 7), 6, (8, 8)),
    ("JOS", 6, (8, 8), 6, (7, 7)),
    ("JOS", 8, (12, 12), 8, (12, 13)),
    ("JOS", 8, (25, 25), 8, (25, 26)),
    ("JOS", 19, (47, 47), 19, (48, 48)),
    ("JOS", 19, (48, 48), 19, (47, 47)),
    ("JOS", 20, (4, 6), 20, (7, 9)),
    ("JOS", 21, (36, 36), 21, (36, 37)),
    ("JOS", 21, (37, 37), 21, (38, 39)),
    ("JOS", 21, (38, 43), 21, (40, 45)),
    ("JDG", 5, (32, 32), 5, (31, 31)),
    ("1SA", 13, (1, 1), 13, (2, 2)),
    ("1SA", 24, (1, 1), 23, (29, 29)),
    ("1SA", 24, (2, 23), 24, (1, 22)),
    ("1KI", 4, (20, 21), 4, (27, 28)),
    ("1KI", 6, (21, 21), 6, (21, 22)),
    ("1KI", 11, (6, 6), 11, (6, 8)),
    ("1KI", 11, (23, 23), 11, (25, 25)),
    ("1KI", 20, (1, 29), 21, (1, 29)),
    ("1KI", 21, (1, 43), 20, (1, 43)),
    ("1KI", 22, (44, 44), 22, (43, 43)),
    ("1KI", 22, (45, 45), 22, (44, 44)),
    ("1KI", 22, (46, 50), 22, (45, 45)),
    ("1KI", 22, (51, 54), 22, (50, 53)),
    ("1CH", 1, (18, 18), 1, (24, 24)),
    ("2CH", 3, (11, 11), 3, (11, 12)),
    ("JOB", 16, (5, 23), 16, (4, 22)),
    ("JOB", 35, (3, 5), 35, (4, 6)),
    ("JOB", 39, (31, 35), 40, (1, 5)),
    ("JOB", 40, (1, 19), 40, (6, 24)),
    ("JOB", 40, (20, 27), 41, (1, 8)),
    ("JOB", 41, (1, 1), 41, (9, 10)),
    ("JOB", 41, (2, 25), 41, (11, 34)),
    ("PSA", 10, (2, 2), 11, (1, 1)),
    ("PSA", 10, (3, 8), 11, (2, 7)),
    ("PSA", 149, (8, 8), 149, (8, 9)),
    ("PRO", 7, (6, 6), 7, (6, 7)),
    ("PRO", 7, (7, 8), 7, (8, 9)),
    ("PRO", 15, (29, 29), 15, (29, 30)),
    ("ECC", 4, (17, 17), 5, (1, 1)),
    ("ECC", 5, (1, 19), 5, (2, 20)),
    ("ECC", 7, (1, 1), 6, (12, 12)),
    ("ECC", 7, (2, 30), 7, (1, 29)),
    ("SNG", 5, (17, 17), 6, (1, 1)),
    ("SNG", 6, (1, 12), 6, (2, 13)),
    ("ISA", 3, (21, 21), 3, (21, 23)),
    ("ISA", 3, (22, 24), 3, (24, 26)),
    ("ISA", 38, (14, 14), 38, (14, 15)),
    ("JER", 16, (1, 2), 16, (2, 2)),
    ("JER", 17, (1, 1), 17, (5, 5)),
    ("JER", 25, (34, 39), 49, (34, 39)),
    ("JER", 26, (1, 1), 49, (39, 39)),
    ("JER", 26, (2, 28), 46, (2, 28)),
    ("JER", 27, (1, 46), 50, (1, 46)),
    ("JER", 28, (1, 64), 51, (1, 64)),
    ("JER", 29, (1, 7), 47, (1, 7)),
    ("JER", 30, (1, 33), 49, (1, 33)),
    ("JER", 31, (1, 44), 48, (1, 44)),
    ("JER", 32, (15, 38), 25, (15, 38)),
    ("JER", 33, (1, 24), 26, (1, 24)),
    ("JER", 34, (1, 1), 27, (2, 2)),
    ("JER", 34, (3, 22), 27, (3, 22)),
    ("JER", 35, (1, 17), 28, (1, 17)),
    ("JER", 36, (1, 32), 29, (1, 32)),
    ("JER", 37, (1, 24), 30, (1, 24)),
    ("JER", 38, (1, 40), 31, (1, 40)),
    ("JER", 39, (1, 44), 32, (1, 44)),
    ("JER", 40, (1, 13), 33, (1, 13)),
    ("JER", 41, (1, 22), 34, (1, 22)),
    ("JER", 42, (1, 19), 35, (1, 19)),
    ("JER", 43, (1, 32), 36, (1, 32)),
    ("JER", 44, (1, 21), 37, (1, 21)),
    ("JER", 45, (1, 28), 38, (1, 28)),
    ("JER", 46, (1, 18), 39, (1, 18)),
    ("JER", 47, (1, 16), 40, (1, 16)),
    ("JER", 48, (1, 18), 41, (1, 18)),
    ("JER", 49, (1, 22), 42, (1, 22)),
    ("JER", 50, (1, 13), 43, (1, 13)),
    ("JER", 51, (1, 30), 44, (1, 30)),
    ("JER", 51, (31, 35), 45, (1, 5)),
    ("EZK", 7, (3, 3), 7, (6, 6)),
    ("EZK", 7, (4, 6), 7, (7, 9)),
    ("EZK", 7, (7, 9), 7, (3, 5)),
    ("EZK", 11, (11, 11), 11, (13, 13)),
    ("DAN", 3, (31, 33), 4, (1, 3)),
    ("DAN", 4, (1, 34), 4, (4, 37)),
    ("HOS", 6, (2, 2), 6, (1, 1)),
    ("HOS", 6, (3, 12), 6, (2, 11)),
    ("HOS", 14, (1, 1), 13, (16, 16)),
    ("HOS", 14, (2, 10), 14, (1, 9)),
    ("AMO", 6, (11, 15), 6, (10, 14)),
    ("OBA", 1, (2, 2), 1, (1, 1)),
    ("OBA", 1, (3, 3), 1, (2, 3)),
    ("JON", 2, (1, 1), 1, (17, 17)),
    ("JON", 2, (2, 11), 2, (1, 10)),
    ("HAG", 2, (2, 24), 2, (1, 23)),
    ("ZEC", 3, (3, 11), 3, (2, 10)),
    ("MAL", 4, (4, 5), 4, (5, 6)),
    ("MAL", 4, (6, 6), 4, (4, 4)),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::books::VerseRef;
    use crate::model::{TranslationBuilder, TranslationMeta};
    use crate::usfm::UsfmParser;

    #[test]
    fn lxx2012_psalms_land_on_grid_numbers() {
        let mut b = TranslationBuilder::new(TranslationMeta::default());
        b.remap = Some(remap_lxx2012);
        let src = "\\id PSA\n\\c 50\n\\d (51) For the end.\n\\v 1 Have mercy upon me, O God\n\\c 151\n\\v 1 I was small\n";
        UsfmParser::new(&mut b).parse(src).unwrap();
        UsfmParser::new(&mut b).parse("\\id GEN\n\\c 1\n\\v 1 In the beginning").unwrap();
        let t = b.finish();
        assert_eq!(t.find(VerseRef::new(BookId::PSALMS, 51, 1)).unwrap().text, "Have mercy upon me, O God");
        assert!(t.find(VerseRef::new(BookId::PSALMS, 50, 1)).is_none());
        assert_eq!(t.find(VerseRef::new(psalm_151(), 1, 1)).unwrap().text, "I was small");
        assert_eq!(t.find(VerseRef::new(BookId::GENESIS, 1, 1)).unwrap().text, "In the beginning");
    }

    #[test]
    fn lxx2012_chapters_land_on_grid_numbers() {
        let mut b = TranslationBuilder::new(TranslationMeta::default());
        b.remap = Some(remap_lxx2012);
        for src in [
            // The Septuagint's order of Malachi's last verses.
            "\\id MAL\n\\c 4\n\\v 4 And, behold, I will send to you Elias\n\\v 5 who shall turn\n\\v 6 Remember the law\n",
            // LXX2012 splits Proverbs 7:9 in two, and its Job 41:1 is the grid's 41:9–10.
            "\\id PRO\n\\c 7\n\\v 8 and speaking,\n\\v 9 when there happens\n",
            "\\id JOB\n\\c 41\n\\v 1 Have you not seen him?\n",
            // Its 1 Kings 9:14–25 stands for 9:14 alone: the Septuagint has the rest elsewhere.
            "\\id 1KI\n\\c 9\n\\v 14-25 And Chiram brought\n",
        ] {
            UsfmParser::new(&mut b).parse(src).unwrap();
        }
        let t = b.finish();
        let at = |code, c, v| t.find(VerseRef::new(BookId::from_code(code).unwrap(), c, v));
        let text = |code, c, v| at(code, c, v).map(|v| v.plain_text());
        assert_eq!(text("MAL", 4, 4).unwrap(), "Remember the law");
        assert_eq!(text("MAL", 4, 5).unwrap(), "And, behold, I will send to you Elias");
        assert_eq!(text("PRO", 7, 9).unwrap(), "and speaking, when there happens");
        assert!(at("PRO", 7, 8).is_none());
        let job = at("JOB", 41, 10).unwrap();
        assert_eq!((job.number, job.number_end), (9, 10));
        assert!(at("1KI", 9, 14).is_some());
        assert!(at("1KI", 9, 15).is_none());
    }

    #[test]
    fn moved_verses_fit_the_grid() {
        for &(code, c, (first, last), to_c, (to, end)) in MOVED {
            let book = BookId::from_code(code).unwrap();
            let shape = last - first == end - to || first == last || to == end;
            assert!(first <= last && to <= end && shape, "{code} {c}:{first}-{last}");
            let grid = ChapterIdx::new(book, to_c).unwrap_or_else(|| panic!("{code} {to_c}"));
            assert!(to >= 1 && end <= grid.kjv_verses(), "{code} {to_c}:{to}-{end}");
            let overlaps =
                MOVED.iter().filter(|m| m.0 == code && m.1 == c && m.2.0 <= last && first <= m.2.1);
            assert_eq!(overlaps.count(), 1, "{code} {c}:{first}-{last} overlaps another");
        }
    }

    /// Each grid reference opens its own text in the bundled Septuagint, and a verse it lacks
    /// is missing rather than standing in for another.
    #[cfg(feature = "bundled")]
    #[test]
    fn bundled_septuagint_follows_the_grid() {
        let t = crate::bundled::load("lxx").unwrap();
        let text = |code, c, v| {
            t.find(VerseRef::new(BookId::from_code(code).unwrap(), c, v))
                .map_or(String::new(), |v| v.plain_text())
        };
        let has = |code, c, v, words: &str| {
            let found = text(code, c, v);
            assert!(found.contains(words), "{code} {c}:{v} is {found:?}, not {words:?}");
        };
        // The new covenant, not Moab (LXX2012's 38:31).
        has("JER", 31, 31, "I will make a new covenant with the house of Israel");
        has("JER", 31, 32, "the covenant which I made with their fathers");
        has("JER", 31, 33, "I will surely put my laws into their mind");
        has("JER", 31, 34, "Know the Lord");
        has("JER", 48, 1, "concerning MOAB");
        has("JER", 25, 15, "cup of this unmixed wine");
        assert_eq!(text("JER", 33, 14), "");
        // The Septuagint has no 46:1: LXX2012's 26:1 closes Elam's oracle.
        assert_eq!(text("JER", 46, 1), "");
        has("JER", 46, 2, "FOR EGYPT");
        assert!(text("JER", 49, 39).ends_with("there came this word concerning Aelam."));
        // Genesis 2:25, 5:32 and 8:4 begin LXX2012's 3:1, 6:1 and 8:3's second half.
        has("GEN", 2, 25, "And the two were naked");
        assert!(text("GEN", 3, 1).starts_with("Now the serpent"));
        has("GEN", 5, 32, "And Noe was five hundred years old");
        assert!(text("GEN", 6, 1).starts_with("And it came to pass when men began"));
        has("GEN", 8, 4, "the ark rested");
        assert!(!text("GEN", 8, 3).contains("ark"));
        has("JOL", 2, 28, "I will pour out of my Spirit upon all flesh");
        has("MAL", 4, 4, "Remember the law of my servant Moses");
        has("MAL", 4, 5, "Elias the Thesbite");
        has("MAL", 4, 6, "turn again the heart of the father");
        // LXX2012 splits Obadiah 1:1, and its 1:3 is the grid's 1:2–3.
        has("OBA", 1, 1, "Arise you, and let us rise up against her to war");
        assert!(text("OBA", 1, 3).starts_with("Behold, I have made you small"));
        // The Ten Commandments and 21:16–17 in the grid's order, 32:8 whole (the Septuagint has no
        // 32:9), and Exodus 35–40 rearranged.
        assert_eq!(text("EXO", 20, 13), "You shall not kill.");
        assert_eq!(text("EXO", 20, 14), "You shall not commit adultery.");
        has("EXO", 21, 16, "steal one of the children of Israel");
        has("EXO", 21, 17, "reviles his father");
        has("EXO", 32, 8, "and said, These are your gods, O Israel");
        assert_eq!(text("EXO", 32, 9), "");
        has("EXO", 36, 8, "ten curtains for the tabernacle");
        has("EXO", 37, 1, "Beseleel made the ark");
        has("EXO", 38, 24, "twenty-nine talents");
        has("EXO", 39, 1, "the robes of the holy places, which belong to Aaron");
        has("EXO", 40, 34, "the cloud covered the tabernacle");
        assert_eq!(text("EXO", 36, 15), "");
        // 1 Kings 20 and 21 change places, 2:35's additions and 2:36–46 come out of 3:1 in the
        // Septuagint's order, and 7:1–12 out of 7:51.
        has("1KI", 20, 1, "son of Ader gathered all his forces");
        has("1KI", 21, 1, "Nabuthai the Jezraelite had a vineyard");
        has("1KI", 21, 19, "you have slain and taken possession");
        has("1KI", 2, 36, "the king called Semei");
        has("1KI", 7, 1, "Solomon built a house for himself in thirteen years");
        has("1KI", 2, 35, "the wisdom of Solomon abounded");
        assert_eq!(text("1KI", 3, 1), "");
        // Proverbs 24–31 are already in the grid's order in LXX2012.
        has("PRO", 24, 23, "respect of persons in judgment");
        has("PRO", 30, 15, "The horse-leech");
        has("PRO", 31, 10, "Who shall find a virtuous woman");
        has("PSA", 51, 1, "Have mercy upon me, O God");
        has("PSA", 11, 2, "the sinners have bent their bow");
        has("PSA", 34, 17, "The righteous cried, and the Lord listened to them, and delivered them");
    }

    /// Every Old Testament reading of the lectionary opens its first and last verses in the
    /// bundled Septuagint, and starts and ends where the reading does.
    #[cfg(feature = "bundled")]
    #[test]
    fn lectionary_readings_open_in_the_septuagint() {
        use crate::calendar::{Style, civil};
        use crate::lectionary::{Kind, Usage, readings_in};
        let t = crate::bundled::load("lxx").unwrap();
        let mut n = 0;
        for (style, usage) in [Style::New, Style::Old].into_iter().flat_map(|s| Usage::ALL.map(|u| (s, u))) {
            for d in civil(2026, 1, 1)..civil(2028, 1, 1) {
                let lessons = readings_in(d, style, usage);
                for p in lessons.iter().filter(|r| r.kind == Kind::OldTestament).flat_map(|r| r.passages) {
                    let first = t.find(p.first()).unwrap_or_else(|| panic!("{p:?} opens nothing"));
                    assert!(first.number >= p.from.1, "{p:?} starts at {}", first.number);
                    // The Septuagint has no Proverbs 16:9 (it reads it after 15:29).
                    if p.last() != VerseRef::new(BookId::from_code("PRO").unwrap(), 16, 9) {
                        let last = t.find(p.last()).unwrap_or_else(|| panic!("{p:?} has no last verse"));
                        assert!(last.number_end <= p.to.1, "{p:?} ends at {}", last.number_end);
                    }
                    n += 1;
                }
            }
        }
        assert!(n > 1000, "{n} passages");
        // Lent's Isaiah, Genesis and Proverbs (in 2026 Clean Monday is 23 February).
        let lesson = |m, d, code| {
            let lessons = crate::lectionary::readings(civil(2026, m, d), Style::New);
            let p = lessons.iter().flat_map(|r| r.passages).find(|p| p.book.info().code == code).copied();
            p.unwrap_or_else(|| panic!("no {code} on {m}/{d}"))
        };
        let opens = |p: crate::lectionary::Passage, words: &str| {
            let first = t.find(p.first()).unwrap().plain_text();
            assert!(first.starts_with(words), "{p:?} opens {first:?}");
        };
        opens(lesson(2, 23, "ISA"), "The vision which Esaias the son of Amos saw");
        opens(lesson(2, 23, "PRO"), "The Proverbs of Solomon son of David");
        opens(lesson(3, 6, "GEN"), "And Noe was five hundred years old");
        opens(lesson(3, 13, "GEN"), "and the ark rested");
        let flood = lesson(3, 12, "GEN");
        assert_eq!(flood.to, (8, 3));
        assert!(!t.find(flood.last()).unwrap().text.contains("ark"));
    }

    #[test]
    fn known_psalms() {
        assert_eq!(to_lxx(51, 1), (50, 1));
        assert_eq!(to_lxx(23, 1), (22, 1));
        assert_eq!(to_lxx(10, 1), (9, 21));
        assert_eq!(to_lxx(115, 1), (113, 9));
        assert_eq!(to_lxx(116, 10), (115, 1));
        assert_eq!(to_lxx(147, 12), (147, 1));
        assert_eq!(to_lxx(150, 6), (150, 6));
        assert_eq!(from_lxx(50, 3), Some((BookId::PSALMS, 51, 3)));
        assert_eq!(from_lxx(151, 4), Some((psalm_151(), 1, 4)));
        assert_eq!(from_lxx(152, 1), None);
        assert_eq!(from_lxx2012(114, 1), Some((BookId::PSALMS, 115, 1)));
        assert_eq!(from_lxx2012(115, 10), Some((BookId::PSALMS, 116, 10)));
        let ps = |n| ChapterIdx::new(BookId::PSALMS, n).unwrap();
        assert_eq!(psalm_number(ps(51)), Some(50));
        assert_eq!(psalm_number(ps(10)), Some(9));
        assert_eq!(psalm_number(ChapterIdx::new(psalm_151(), 1).unwrap()), Some(151));
        assert_eq!(psalm_number(ChapterIdx::new(BookId::GENESIS, 1).unwrap()), None);
        assert_eq!(shown_in(true, BookId::PSALMS, 51, 3), ("Psalm", Some(50), 3));
        assert_eq!(shown_in(false, BookId::PSALMS, 51, 3), ("Psalm", Some(51), 3));
        assert_eq!(shown_in(false, psalm_151(), 1, 3), ("Psalm 151", None, 3));
        assert_eq!(shown_in(true, BookId::GENESIS, 1, 1), ("Genesis", Some(1), 1));
    }

    #[test]
    fn every_grid_psalm_verse_round_trips() {
        let mut seen = std::collections::HashSet::new();
        for chapter in 1..=150u8 {
            let c = ChapterIdx::new(BookId::PSALMS, chapter).unwrap();
            for verse in 1..=c.kjv_verses() {
                let lxx = to_lxx(chapter, verse);
                assert!(seen.insert(lxx), "Psalm {chapter}:{verse} and another verse are both {lxx:?}");
                assert_eq!(from_lxx(lxx.0, lxx.1), Some((BookId::PSALMS, chapter, verse)), "{lxx:?}");
            }
        }
        // The Septuagint's 150 psalms, plus 151.
        let chapters: std::collections::HashSet<u8> = seen.iter().map(|(c, _)| *c).collect();
        assert_eq!(chapters, (1..=150).collect());
    }
}
