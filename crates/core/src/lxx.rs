//! Septuagint psalm numbering.
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

/// [`TranslationBuilder::remap`](crate::model::TranslationBuilder::remap) for LXX2012: its
/// psalms move to grid numbers; other books keep theirs.
pub fn remap_lxx2012(book: BookId, chapter: u8, verse: u8) -> Option<(BookId, u8, u8)> {
    if book == BookId::PSALMS { from_lxx2012(chapter, verse) } else { Some((book, chapter, verse)) }
}

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
