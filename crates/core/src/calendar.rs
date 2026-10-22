//! The Orthodox Church year: Pascha, the great feasts, the fasts and the Psalter's weekly reading.
//!
//! Every Orthodox church keeps Pascha by the Julian computus. Fixed feasts follow the church's
//! calendar: the *New* (Revised Julian) calendar keeps them on the civil date, the *Old* (Julian)
//! calendar 13 days later (Nativity on 7 January) between 1900 and 2099.
//!
//! The fasting rules are the general ones most English-speaking Orthodox parishes publish. Local
//! practice varies, and a priest's guidance comes first; the app says so where it shows them.

use crate::books::{BookId, ChapterIdx};
use crate::lectionary::Passage;

/// A day, counted from 1970-01-01 in the proleptic Gregorian (civil) calendar.
pub type Day = i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Style {
    /// Revised Julian: fixed feasts on their civil dates (Greek, Antiochian, OCA…).
    #[default]
    New,
    /// Julian: fixed feasts 13 days later (Russian, Serbian, Jerusalem, Athos…).
    Old,
}

/// Days since 1970-01-01 of a Gregorian date (Howard Hinnant's algorithm).
pub fn civil(year: i32, month: u8, day: u8) -> Day {
    let (y, m) =
        if month <= 2 { (year as i64 - 1, month as i64 + 9) } else { (year as i64, month as i64 - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The Gregorian date of a day: (year, month, day).
pub fn civil_date(d: Day) -> (i32, u8, u8) {
    let z = d + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    ((yoe + era * 400 + i64::from(month <= 2)) as i32, month, day)
}

/// Days since 1970-01-01 of a Julian-calendar date.
pub fn julian(year: i32, month: u8, day: u8) -> Day {
    let (y, m) =
        if month <= 2 { (year as i64 - 1, month as i64 + 9) } else { (year as i64, month as i64 - 3) };
    let doy = (153 * m + 2) / 5 + day as i64 - 1;
    // Days from Julian 0000-03-01, then shifted so that Julian 1969-12-19 is day 0.
    y * 365 + y.div_euclid(4) + doy - 719_470
}

/// The Julian date of a day.
pub fn julian_date(d: Day) -> (i32, u8, u8) {
    let z = d + 719_470;
    let y = (4 * z + 3).div_euclid(1461);
    let doy = z - (365 * y + y.div_euclid(4));
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    ((y + i64::from(month <= 2)) as i32, month, day)
}

/// 0 = Sunday … 6 = Saturday.
pub fn weekday(d: Day) -> u8 {
    (d + 4).rem_euclid(7) as u8
}

/// Orthodox Pascha of `year` (Meeus's Julian computus), as a day.
pub fn pascha(year: i32) -> Day {
    let (a, b, c) = (year.rem_euclid(4), year.rem_euclid(7), year.rem_euclid(19));
    let d = (19 * c + 15) % 30;
    let e = (2 * a + 4 * b - d + 34) % 7;
    let month = (d + e + 114) / 31;
    let day = (d + e + 114) % 31 + 1;
    julian(year, month as u8, day as u8)
}

/// A day's date in the church's calendar: the civil date (New) or the Julian one (Old).
pub fn church_date(d: Day, style: Style) -> (i32, u8, u8) {
    match style {
        Style::New => civil_date(d),
        Style::Old => julian_date(d),
    }
}

/// What may be eaten on a day, from most to least.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Fast {
    /// No fast; on fast-free weeks even Wednesday and Friday.
    None,
    /// No meat; dairy, eggs and fish allowed (Cheesefare week).
    NoMeat,
    /// Fish, wine and oil allowed.
    Fish,
    /// Wine and oil allowed; no meat, dairy or fish.
    WineOil,
    /// Wine allowed, but no oil (Holy Saturday).
    Wine,
    /// No meat, dairy, fish, wine or oil.
    Strict,
}

impl Fast {
    pub fn label(self) -> &'static str {
        match self {
            Fast::None => "No fast",
            Fast::NoMeat => "No meat",
            Fast::Fish => "Fish, wine and oil allowed",
            Fast::WineOil => "Wine and oil allowed",
            Fast::Wine => "Wine allowed",
            Fast::Strict => "Strict fast",
        }
    }

    /// What the level means at the table.
    pub fn detail(self) -> &'static str {
        match self {
            Fast::None => "Any food.",
            Fast::NoMeat => "Dairy, eggs and fish, but no meat.",
            Fast::Fish => "No meat or dairy.",
            Fast::WineOil => "No meat, dairy or fish.",
            Fast::Wine => "No meat, dairy, fish or oil.",
            Fast::Strict => "No meat, dairy, fish, wine or oil.",
        }
    }
}

/// One day of the church year.
#[derive(Debug, Clone, PartialEq)]
pub struct DayInfo {
    /// The day in the church's calendar.
    pub date: (i32, u8, u8),
    pub weekday: u8,
    /// Days from this year's Pascha (negative before it).
    pub from_pascha: i64,
    /// Feasts and named days: "Pascha", "Transfiguration of Our Lord".
    pub feasts: Vec<&'static str>,
    /// The fasting season, if any: "Great Lent", "Nativity Fast".
    pub season: Option<&'static str>,
    pub fast: Fast,
}

/// Fixed feasts by church date: the Twelve Great Feasts and other great days.
const FIXED: &[((u8, u8), &str)] = &[
    ((9, 1), "Church New Year"),
    ((9, 8), "Nativity of the Theotokos"),
    ((9, 14), "Elevation of the Cross"),
    ((10, 1), "Protection of the Theotokos"),
    ((11, 21), "Entry of the Theotokos into the Temple"),
    ((12, 24), "Eve of the Nativity"),
    ((12, 25), "Nativity of Our Lord"),
    ((1, 1), "Circumcision of Our Lord · St Basil the Great"),
    ((1, 5), "Eve of Theophany"),
    ((1, 6), "Theophany"),
    ((1, 30), "Three Holy Hierarchs"),
    ((2, 2), "Meeting of Our Lord in the Temple"),
    ((3, 25), "Annunciation"),
    ((6, 24), "Nativity of St John the Baptist"),
    ((6, 29), "Sts Peter and Paul"),
    ((8, 6), "Transfiguration of Our Lord"),
    ((8, 15), "Dormition of the Theotokos"),
    ((8, 29), "Beheading of St John the Baptist"),
];

/// Movable days by their distance from Pascha.
const MOVABLE: &[(i64, &str)] = &[
    (-70, "Sunday of the Publican and the Pharisee"),
    (-63, "Sunday of the Prodigal Son"),
    (-56, "Sunday of the Last Judgment (Meatfare)"),
    (-49, "Forgiveness Sunday (Cheesefare)"),
    (-48, "Clean Monday"),
    (-42, "Sunday of Orthodoxy"),
    (-35, "Sunday of St Gregory Palamas"),
    (-28, "Sunday of the Holy Cross"),
    (-21, "Sunday of St John Climacus"),
    (-14, "Sunday of St Mary of Egypt"),
    (-8, "Lazarus Saturday"),
    (-7, "Palm Sunday"),
    (-6, "Holy Monday"),
    (-5, "Holy Tuesday"),
    (-4, "Holy Wednesday"),
    (-3, "Holy Thursday"),
    (-2, "Holy Friday"),
    (-1, "Holy Saturday"),
    (0, "Pascha · The Resurrection of Our Lord"),
    (7, "Thomas Sunday"),
    (14, "Sunday of the Myrrhbearers"),
    (21, "Sunday of the Paralytic"),
    (24, "Mid-Pentecost"),
    (28, "Sunday of the Samaritan Woman"),
    (35, "Sunday of the Blind Man"),
    (39, "Ascension of Our Lord"),
    (42, "Sunday of the Fathers of the First Council"),
    (49, "Pentecost"),
    (50, "Monday of the Holy Spirit"),
    (56, "All Saints"),
];

/// Feasts of the Lord and the Theotokos on which a Wednesday or Friday fast is eased to fish.
const FISH_FEASTS: &[(u8, u8)] = &[(9, 8), (11, 21), (2, 2), (3, 25), (6, 24), (6, 29), (8, 6), (8, 15)];

/// The church year's facts for day `d`.
pub fn day(d: Day, style: Style) -> DayInfo {
    let date = church_date(d, style);
    let (year, month, dom) = date;
    let md = (month, dom);
    let wd = weekday(d);
    let p = d - pascha(year);
    let weekend = wd == 0 || wd == 6;
    let wed_fri = wd == 3 || wd == 5;
    let mut feasts: Vec<&'static str> = MOVABLE.iter().filter(|(o, _)| *o == p).map(|(_, n)| *n).collect();
    feasts.extend(FIXED.iter().filter(|(m, _)| *m == md).map(|(_, n)| *n));

    // The church date this year's fixed day falls on, as a distance from Pascha.
    let fixed_from_pascha = |m: u8, dd: u8| match style {
        Style::New => civil(year, m, dd),
        Style::Old => julian(year, m, dd),
    } - pascha(year);
    let in_range = |a: (u8, u8), b: (u8, u8)| md >= a && md <= b;
    // The Apostles' Fast runs from the Monday after All Saints to 28 June.
    let apostles = p >= 57 && p <= fixed_from_pascha(6, 28);

    let (season, fast) = if in_range((12, 25), (12, 31)) || in_range((1, 1), (1, 4)) {
        (Some("Fast-free: Nativity to Theophany"), Fast::None)
    } else if (-69..=-64).contains(&p) {
        (Some("Fast-free week"), Fast::None)
    } else if (1..=6).contains(&p) {
        (Some("Bright Week: fast-free"), Fast::None)
    } else if (50..=55).contains(&p) {
        (Some("Trinity Week: fast-free"), Fast::None)
    } else if (-55..=-50).contains(&p) {
        (Some("Cheesefare Week"), Fast::NoMeat)
    } else if (-48..=-1).contains(&p) {
        let fast = match p {
            -3 => Fast::WineOil,
            -1 => Fast::Wine,
            -6..=-2 => Fast::Strict,
            -7 => Fast::Fish,
            _ if md == (3, 25) => Fast::Fish,
            -8 => Fast::WineOil,
            _ if weekend => Fast::WineOil,
            _ => Fast::Strict,
        };
        (Some(if p >= -6 { "Holy Week" } else { "Great Lent" }), fast)
    } else if apostles {
        (
            Some("Apostles' Fast"),
            if FISH_FEASTS.contains(&md) || !wed_fri { Fast::Fish } else { Fast::Strict },
        )
    } else if in_range((8, 1), (8, 14)) {
        let fast = if md == (8, 6) {
            Fast::Fish
        } else if weekend {
            Fast::WineOil
        } else {
            Fast::Strict
        };
        (Some("Dormition Fast"), fast)
    } else if in_range((11, 15), (12, 24)) {
        let fast = if md == (11, 21) {
            Fast::Fish
        } else if md == (12, 24) {
            if weekend { Fast::WineOil } else { Fast::Strict }
        } else if wed_fri {
            Fast::Strict
        } else if md >= (12, 20) {
            Fast::WineOil
        } else {
            Fast::Fish
        };
        (Some("Nativity Fast"), fast)
    } else if [(1, 5), (8, 29), (9, 14)].contains(&md) {
        (Some("Fast day"), if weekend { Fast::WineOil } else { Fast::Strict })
    } else if wed_fri {
        (None, if FISH_FEASTS.contains(&md) { Fast::Fish } else { Fast::Strict })
    } else {
        (None, Fast::None)
    };
    DayInfo { date, weekday: wd, from_pascha: p, feasts, season, fast }
}

/// The Psalter's twenty kathismata, as Septuagint psalm ranges.
pub const KATHISMATA: [(u8, u8); 20] = [
    (1, 8),
    (9, 16),
    (17, 23),
    (24, 31),
    (32, 36),
    (37, 45),
    (46, 54),
    (55, 63),
    (64, 69),
    (70, 76),
    (77, 84),
    (85, 90),
    (91, 100),
    (101, 104),
    (105, 108),
    (109, 117),
    (118, 118),
    (119, 133),
    (134, 142),
    (143, 150),
];

/// The kathismata read at Matins and at Vespers on a weekday (0 = Sunday), in ordinary weeks of
/// the year: the whole Psalter once a week. (In winter a third is added at Matins, and in Great
/// Lent the Psalter is read twice a week.)
pub fn kathismata(weekday: u8) -> (&'static [u8], &'static [u8]) {
    match weekday {
        0 => (&[2, 3], &[]),
        1 => (&[4, 5], &[6]),
        2 => (&[7, 8], &[9]),
        3 => (&[10, 11], &[12]),
        4 => (&[13, 14], &[15]),
        5 => (&[19, 20], &[18]),
        _ => (&[16, 17], &[1]),
    }
}

/// A Septuagint psalm's number in the numbering shown ([`crate::lxx::numbering`]): the grid
/// chapter it starts in when that numbering is off.
pub(crate) fn shown_psalm(p: u8) -> u8 {
    if crate::lxx::numbering() { p } else { crate::lxx::from_lxx(p, 1).map_or(p, |(_, c, _)| c) }
}

/// "Psalms 24–31": kathisma `n`'s psalms in the numbering shown ([`crate::lxx::numbering`]).
pub fn kathisma_psalms(n: u8) -> String {
    let Some(&(a, b)) = KATHISMATA.get(n.saturating_sub(1) as usize) else { return String::new() };
    let shown = shown_psalm;
    if a == b { format!("Psalm {}", shown(a)) } else { format!("Psalms {}–{}", shown(a), shown(b)) }
}

/// Kathisma `n`'s psalms in the grid, one passage per Septuagint psalm: its Psalm 9 runs over the
/// grid's 9 and 10, and its 114 and 115 share the grid's 116.
pub fn kathisma_passages(n: u8) -> Vec<Passage> {
    let Some(&(a, b)) = KATHISMATA.get(n.wrapping_sub(1) as usize) else { return Vec::new() };
    (a..=b).filter_map(psalm_passage).collect()
}

/// Septuagint psalm `n` in the grid: from its first verse to the verse before the next psalm.
pub(crate) fn psalm_passage(n: u8) -> Option<Passage> {
    let (book, c, v) = crate::lxx::from_lxx(n, 1)?;
    let end_of = |c: u8| Some((c, ChapterIdx::new(book, c)?.kjv_verses()));
    let to = match crate::lxx::from_lxx(n.saturating_add(1), 1) {
        Some((b, next, 1)) if b == book => end_of(next - 1)?,
        Some((b, next, v)) if b == book => (next, v - 1),
        // Psalm 150, and 151 (a book of one psalm).
        _ => end_of(c)?,
    };
    Some(Passage { book, from: (c, v), to })
}

/// Where kathisma `n` (1–20) begins in the grid: its first psalm.
pub fn kathisma_start(n: u8) -> Option<(BookId, u8)> {
    let (first, _) = *KATHISMATA.get(n.checked_sub(1)? as usize)?;
    crate::lxx::from_lxx(first, 1).map(|(b, c, _)| (b, c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_round_trip() {
        assert_eq!(civil(1970, 1, 1), 0);
        assert_eq!(civil_date(civil(2026, 10, 20)), (2026, 10, 20));
        assert_eq!(weekday(civil(2026, 10, 20)), 2); // a Tuesday
        // Julian 25 December is civil 7 January this century.
        assert_eq!(julian(2025, 12, 25), civil(2026, 1, 7));
        for d in (-50_000..50_000).step_by(37) {
            let (y, m, dd) = julian_date(d);
            assert_eq!(julian(y, m, dd), d);
            let (y, m, dd) = civil_date(d);
            assert_eq!(civil(y, m, dd), d);
        }
    }

    #[test]
    fn known_paschas() {
        for (year, month, day) in [(2024, 5, 5), (2025, 4, 20), (2026, 4, 12), (2027, 5, 2), (2030, 4, 28)] {
            assert_eq!(civil_date(pascha(year)), (year, month, day), "{year}");
            assert_eq!(weekday(pascha(year)), 0);
        }
    }

    #[test]
    fn feasts_and_fasts() {
        let at = |y, m, d, s| day(civil(y, m, d), s);
        let p = at(2026, 4, 12, Style::New);
        assert!(p.feasts[0].starts_with("Pascha"));
        assert_eq!(p.fast, Fast::None);
        assert_eq!(at(2026, 4, 15, Style::New).fast, Fast::None, "Bright Wednesday");
        assert_eq!(at(2026, 4, 10, Style::New).fast, Fast::Strict, "Holy Friday");
        assert_eq!(at(2026, 4, 11, Style::New).fast, Fast::Wine, "Holy Saturday");
        assert_eq!(at(2026, 4, 5, Style::New).fast, Fast::Fish, "Palm Sunday");
        assert_eq!(at(2026, 2, 23, Style::New).feasts, ["Clean Monday"]);
        assert_eq!(at(2026, 2, 23, Style::New).fast, Fast::Strict);
        assert_eq!(at(2026, 2, 28, Style::New).fast, Fast::WineOil, "a Saturday in Lent");
        assert_eq!(at(2026, 3, 25, Style::New).fast, Fast::Fish, "Annunciation in Lent");
        assert_eq!(at(2026, 2, 18, Style::New).fast, Fast::NoMeat, "Cheesefare Wednesday");
        assert_eq!(at(2026, 12, 25, Style::New).feasts, ["Nativity of Our Lord"]);
        assert_eq!(at(2026, 12, 25, Style::New).fast, Fast::None);
        // On the Old calendar, Nativity is on 7 January.
        assert_eq!(at(2027, 1, 7, Style::Old).feasts, ["Nativity of Our Lord"]);
        assert_eq!(at(2026, 12, 25, Style::Old).season, Some("Nativity Fast"));
        assert_eq!(at(2026, 8, 6, Style::New).fast, Fast::Fish, "Transfiguration");
        assert_eq!(at(2026, 8, 10, Style::New).fast, Fast::Strict, "a Monday in the Dormition Fast");
        assert_eq!(at(2026, 11, 17, Style::New).fast, Fast::Fish, "a Tuesday in the Nativity Fast");
        assert_eq!(at(2026, 11, 18, Style::New).fast, Fast::Strict, "a Wednesday in the Nativity Fast");
        assert_eq!(at(2026, 10, 21, Style::New).fast, Fast::Strict, "an ordinary Wednesday");
        assert_eq!(at(2026, 10, 20, Style::New).fast, Fast::None, "an ordinary Tuesday");
        // 2026: Pentecost 31 May, All Saints 7 June, so the Apostles' Fast is 8–28 June.
        assert_eq!(at(2026, 6, 7, Style::New).feasts, ["All Saints"]);
        assert_eq!(at(2026, 6, 8, Style::New).season, Some("Apostles' Fast"));
        assert_eq!(at(2026, 6, 29, Style::New).season, None);
        assert_eq!(at(2026, 6, 3, Style::New).fast, Fast::None, "Trinity Week Wednesday");
    }

    #[test]
    fn kathismata_cover_the_psalter_once_a_week() {
        let mut read: Vec<u8> = (0..7)
            .flat_map(|d| {
                let (m, v) = kathismata(d);
                m.iter().chain(v).copied().collect::<Vec<_>>()
            })
            .collect();
        read.sort_unstable();
        assert_eq!(read, (1..=20).collect::<Vec<_>>());
        let ps: u32 = KATHISMATA.iter().map(|(a, b)| (b - a + 1) as u32).sum();
        assert_eq!(ps, 150);
        assert_eq!(kathisma_start(17), Some((BookId::PSALMS, 119)));
        assert_eq!(kathisma_start(1), Some((BookId::PSALMS, 1)));
    }

    /// Kathisma `n`'s passages as (first chapter, first verse, last chapter, last verse).
    fn psalms(n: u8) -> Vec<(u8, u8, u8, u8)> {
        let ps = kathisma_passages(n);
        assert!(ps.iter().all(|p| p.book == BookId::PSALMS));
        ps.iter().map(|p| (p.from.0, p.from.1, p.to.0, p.to.1)).collect()
    }

    #[test]
    fn kathismata_open_the_septuagint_psalms_in_the_grid() {
        // Psalms 85–90 are the grid's 86–91, whole.
        assert_eq!(
            psalms(12),
            [
                (86, 1, 86, 17),
                (87, 1, 87, 7),
                (88, 1, 88, 18),
                (89, 1, 89, 52),
                (90, 1, 90, 17),
                (91, 1, 91, 16)
            ]
        );
        // The psalms the two numberings join or split.
        let second = psalms(2);
        assert_eq!((second.len(), second[0], second[7]), (8, (9, 1, 10, 18), (17, 1, 17, 15)));
        assert_eq!(
            psalms(16)[3..7],
            [(113, 1, 113, 9), (114, 1, 115, 18), (116, 1, 116, 9), (116, 10, 116, 19)]
        );
        assert_eq!(psalms(20)[2..5], [(146, 1, 146, 10), (147, 1, 147, 11), (147, 12, 147, 20)]);
        assert_eq!(psalms(20).last(), Some(&(150, 1, 150, 6)));
        assert!(kathisma_passages(0).is_empty() && kathisma_passages(21).is_empty());
        // Together they read every verse of the Psalter once, in order.
        let read: Vec<crate::VerseRef> =
            (1..=20).flat_map(kathisma_passages).flat_map(|p| p.verses().collect::<Vec<_>>()).collect();
        let psalter: Vec<crate::VerseRef> = (1..=150)
            .flat_map(|c| (1..=ChapterIdx::new(BookId::PSALMS, c).unwrap().kjv_verses()).map(move |v| (c, v)))
            .map(|(c, v)| crate::VerseRef::new(BookId::PSALMS, c, v))
            .collect();
        assert_eq!(read, psalter);
    }
}
