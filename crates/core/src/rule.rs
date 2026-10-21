//! A prayer rule (Orthodox fork): the daily discipline someone keeps, in blocks through the day
//! (morning, meals, midday, evening), each a list of items to pray or read.
//!
//! The rule is a JSON document in the study store's settings ([`StudyStore::rule`]); what was
//! done each day is a row per (day, item) ([`StudyStore::rule_done`]). Nothing scores or counts
//! streaks: Orthodox writers warn against making prayer an achievement, so the app only shows
//! which days were kept, quietly, on the calendar.
//!
//! Praying through a block on Today is worked out here, so the Mac and the iPhone agree: which
//! block to offer ([`Rule::now`]), the item Begin and Done open ([`Rule::next_item`]) and the
//! Scripture each item reads ([`parts`]).
//!
//! [`StudyStore::rule`]: crate::study::StudyStore::rule
//! [`StudyStore::rule_done`]: crate::study::StudyStore::rule_done

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::calendar::{self, Day, Style};
use crate::lectionary::{self, Passage, Reading};
use crate::plans;

/// The rule: its blocks in the order of the day.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub blocks: Vec<Block>,
}

/// A part of the day: "Morning", "Before meals", "Midday", "Evening".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub id: String,
    pub title: String,
    /// When to be reminded, "HH:MM" local time; `None` for a block without a set time (meals).
    #[serde(default)]
    pub time: Option<String>,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    /// Stable within the rule: the day's check marks are kept by it.
    pub id: String,
    pub title: String,
    pub kind: Kind,
    /// A line under the title: "From the Pocket Prayer Book", "10–15 minutes".
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub season: Season,
    /// Done or not, it doesn't decide whether the day's rule was kept.
    #[serde(default)]
    pub optional: bool,
}

/// What an item opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Kind {
    /// Prayers: a built-in text (`crate::prayers`), the person's own text, or neither (prayed from
    /// their own prayer book).
    Prayers {
        #[serde(default)]
        prayer: Option<String>,
        #[serde(default)]
        text: Option<String>,
    },
    /// The day's kathismata (`calendar::kathismata`), at Matins in the morning and Vespers in the
    /// evening, by the block's `id` ("morning" or anything else).
    Kathisma,
    /// One psalm, in Septuagint numbering.
    Psalm { lxx: u8 },
    /// The day's Epistle and Gospel.
    Readings,
    /// The next day of a reading plan.
    Plan { plan: String },
    /// The Church Fathers on the day's Gospel.
    Fathers,
    /// The Jesus Prayer, with a count to pray.
    JesusPrayer { count: u16 },
    /// A short look back over the day before sleep.
    Reflection,
    /// Anything else, described by its title and note.
    Other,
}

/// When an item belongs to the rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Season {
    #[default]
    Always,
    /// Great Lent and Holy Week only.
    GreatLent,
    /// Every day but those of Great Lent and Holy Week.
    OutsideLent,
}

impl Season {
    fn applies(self, lent: bool) -> bool {
        match self {
            Season::Always => true,
            Season::GreatLent => lent,
            Season::OutsideLent => !lent,
        }
    }
}

/// Great Lent or Holy Week on day `d`.
pub fn is_lent(d: Day, style: Style) -> bool {
    matches!(calendar::day(d, style).season, Some("Great Lent") | Some("Holy Week"))
}

/// A rule's time ("HH:MM") as minutes after midnight.
pub fn minutes(time: &str) -> Option<i64> {
    let (h, m) = time.split_once(':')?;
    let (h, m) = (h.trim().parse::<i64>().ok()?, m.trim().parse::<i64>().ok()?);
    ((0..24).contains(&h) && (0..60).contains(&m)).then_some(h * 60 + m)
}

/// The day's kathismata for a kathisma item in `block`: at Matins in the morning block (or any
/// block before noon), at Vespers in the others.
pub fn kathismata(block: &Block, d: Day) -> &'static [u8] {
    let (matins, vespers) = calendar::kathismata(calendar::weekday(d));
    let before_noon = block.time.as_deref().and_then(minutes).is_some_and(|m| m < 12 * 60);
    if block.id == "morning" || before_noon { matins } else { vespers }
}

/// Nothing to do on day `d`: a kathisma where the day has none (Sunday evening).
fn idle(item: &Item, block: &Block, d: Day) -> bool {
    item.kind == Kind::Kathisma && kathismata(block, d).is_empty()
}

/// The part of the day to offer now ([`Rule::now`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Now<'a> {
    /// Its time has come and something it asks for is still to do.
    Due(&'a Block),
    /// Nothing is due; this one comes later in the day.
    Next(&'a Block),
}

/// A stretch of Scripture an item reads, as it is shown: the Epistle or the Gospel, a kathisma, a
/// psalm, a plan's passage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    /// What it is: "Epistle", "Gospel", "Kathisma 12", "Psalm 90", "Day 3".
    pub label: String,
    /// Its verses, in the numbering shown: "2 Corinthians 9:6–11", "Psalms 85–90".
    pub reference: String,
    /// The verses in grid numbering, in order: several for a reading in pieces ("Romans
    /// 14:19–23; 16:25–27") and for a kathisma, one per psalm.
    pub passages: Vec<Passage>,
    /// The line after it: "End of the Gospel", "End of kathisma 12".
    pub end: String,
}

impl Part {
    /// One of the day's readings: the Liturgy's, named by kind, or one read `instead` on a day
    /// without a Liturgy Gospel.
    fn reading(r: &Reading, instead: bool) -> Part {
        let label = r.kind.label();
        let end = if instead { "End of the reading".into() } else { format!("End of the {label}") };
        Part { label: label.into(), reference: r.label(), passages: r.passages.to_vec(), end }
    }
}

/// What the rule's Scripture and the Fathers read on a day with these `lessons`: its Liturgy's
/// Gospel or, on a day without one (a Lenten weekday, Holy Friday), its first reading; and
/// whether it is that Gospel.
fn day_gospel(lessons: &[Reading]) -> Option<(&Reading, bool)> {
    let gospel = lessons.iter().find(|r| r.at_liturgy() && r.kind == lectionary::Kind::Gospel);
    gospel.map(|g| (g, true)).or_else(|| lessons.first().map(|r| (r, false)))
}

/// The Scripture `item` (in `block`) reads on day `d`, whose readings are `lessons`: the
/// Liturgy's Epistle then its Gospel (or the day's first reading when it has no Liturgy Gospel),
/// a psalm, the day's kathismata, or the Gospel the Fathers are read on. A plan's item reads
/// `plan`: the plan's day that falls on `d` and which of its passages (from 0). Empty for an item
/// that reads no Scripture, or has none to read that day.
pub fn parts(
    item: &Item,
    block: &Block,
    d: Day,
    lessons: &[Reading],
    plan: Option<(usize, usize)>,
) -> Vec<Part> {
    match &item.kind {
        Kind::Readings => match day_gospel(lessons) {
            Some((g, true)) => {
                let epistle = lessons.iter().find(|r| r.at_liturgy() && r.kind == lectionary::Kind::Epistle);
                epistle.into_iter().chain([g]).map(|r| Part::reading(r, false)).collect()
            }
            Some((first, false)) => vec![Part::reading(first, true)],
            None => Vec::new(),
        },
        Kind::Fathers => {
            day_gospel(lessons).map(|(r, gospel)| Part::reading(r, !gospel)).into_iter().collect()
        }
        Kind::Psalm { lxx } => {
            let name = format!("Psalm {}", calendar::shown_psalm(*lxx));
            let end = format!("End of {name}");
            let psalm = calendar::psalm_passage(*lxx);
            psalm
                .map(|p| Part { label: name.clone(), reference: name, passages: vec![p], end })
                .into_iter()
                .collect()
        }
        Kind::Kathisma => kathismata(block, d)
            .iter()
            .map(|&k| Part {
                label: format!("Kathisma {k}"),
                reference: calendar::kathisma_psalms(k),
                passages: calendar::kathisma_passages(k),
                end: format!("End of kathisma {k}"),
            })
            .collect(),
        Kind::Plan { plan: id } => {
            let Some((day, i)) = plan else { return Vec::new() };
            let Some(p) = plans::get(id).and_then(|p| p.day(day).get(i)) else { return Vec::new() };
            let (c, verses) = (p.chapter, p.verses);
            let (a, b) = verses.unwrap_or((1, c.kjv_verses()));
            let reference = match verses {
                Some((a, b)) => format!("{}:{a}–{b}", c.display()),
                None => c.display(),
            };
            let passage = Passage { book: c.book(), from: (c.chapter(), a), to: (c.chapter(), b) };
            vec![Part {
                label: format!("Day {day}"),
                end: format!("End of {reference}"),
                reference,
                passages: vec![passage],
            }]
        }
        Kind::Prayers { .. } | Kind::JesusPrayer { .. } | Kind::Reflection | Kind::Other => Vec::new(),
    }
}

impl Rule {
    /// The items of each block that belong to day `d` (its season), blocks with none left out.
    pub fn for_day(&self, d: Day, style: Style) -> Vec<(&Block, Vec<&Item>)> {
        let lent = is_lent(d, style);
        self.blocks
            .iter()
            .map(|b| (b, b.items.iter().filter(|i| i.season.applies(lent)).collect::<Vec<_>>()))
            .filter(|(_, items)| !items.is_empty())
            .collect()
    }

    /// How the day went: (required items done, required items), from the ids checked off.
    pub fn kept(&self, d: Day, style: Style, done: &HashSet<String>) -> (usize, usize) {
        let required: Vec<&Item> =
            self.for_day(d, style).into_iter().flat_map(|(_, items)| items).filter(|i| !i.optional).collect();
        (required.iter().filter(|i| done.contains(&i.id)).count(), required.len())
    }

    /// The ids of day `d`'s items with nothing to do then (the kathisma on Sunday evening): they
    /// count as done for [`Rule::kept`] and the reminders.
    pub fn idle(&self, d: Day, style: Style) -> Vec<String> {
        let items =
            self.for_day(d, style).into_iter().flat_map(|(b, items)| items.into_iter().map(move |i| (b, i)));
        items.filter(|(b, i)| idle(i, b, d)).map(|(_, i)| i.id.clone()).collect()
    }

    /// The part of day `d` to offer at `minute` (after midnight): the latest whose time has come
    /// and that still has a required item to do, or else the next such part later in the day.
    /// One without a time (before meals) is never offered, nor one `ended` today (by id). `done`
    /// holds the ids to pass over, as for [`Rule::next_item`].
    pub fn now(
        &self,
        d: Day,
        style: Style,
        minute: i64,
        done: &HashSet<String>,
        ended: &HashSet<String>,
    ) -> Option<Now<'_>> {
        let open: Vec<(&Block, i64)> = self
            .for_day(d, style)
            .into_iter()
            .filter(|(b, items)| {
                !ended.contains(&b.id)
                    && items.iter().any(|i| !i.optional && !done.contains(&i.id) && !idle(i, b, d))
            })
            .filter_map(|(b, _)| Some((b, b.time.as_deref().and_then(minutes)?)))
            .collect();
        let due = open.iter().filter(|(_, at)| *at <= minute).max_by_key(|(_, at)| *at);
        let next = open.iter().filter(|(_, at)| *at > minute).min_by_key(|(_, at)| *at);
        due.map(|(b, _)| Now::Due(b)).or_else(|| next.map(|(b, _)| Now::Next(b)))
    }

    /// The item of block `block` to open on day `d` after the item `after` (from the top when
    /// `None`, or when `after` is no longer in it): the first below it that isn't in `done` and
    /// has something to do that day. `done` holds what's checked off, a plan's item whose reading
    /// for the day is read, and one with nothing to read that day (its plan finished, or no
    /// reading on the day); a kathisma on a day without one is passed over by itself. Optional
    /// items are offered too. `None` at the end of the block.
    pub fn next_item(
        &self,
        d: Day,
        style: Style,
        block: &str,
        after: Option<&str>,
        done: &HashSet<String>,
    ) -> Option<&Item> {
        let (b, items) = self.for_day(d, style).into_iter().find(|(b, _)| b.id == block)?;
        let from = after.and_then(|a| items.iter().position(|i| i.id == a)).map_or(0, |p| p + 1);
        items.into_iter().skip(from).find(|i| !done.contains(&i.id) && !idle(i, b, d))
    }

    /// The rule Nicholas set out (2026-10-20): prayer and 10–15 minutes of reading on waking,
    /// the Lord's Prayer before meals, a midday pause, and prayer, the kathisma and the Fathers
    /// in the evening. Everything can be changed.
    pub fn starter() -> Rule {
        let item = |id: &str, title: &str, kind: Kind, note: &str| Item {
            id: id.into(),
            title: title.into(),
            kind,
            note: note.into(),
            season: Season::Always,
            optional: false,
        };
        let own_book = || Kind::Prayers { prayer: None, text: None };
        Rule {
            blocks: vec![
                Block {
                    id: "morning".into(),
                    title: "Morning".into(),
                    time: Some("07:00".into()),
                    items: vec![
                        item("morning-prayers", "Morning Prayers", own_book(), "From your prayer book"),
                        item(
                            "morning-reading",
                            "Scripture",
                            Kind::Readings,
                            "The day's Gospel, 10–15 minutes",
                        ),
                    ],
                },
                Block {
                    id: "meals".into(),
                    title: "Before meals".into(),
                    time: None,
                    items: vec![item(
                        "meal-prayer",
                        "The Lord's Prayer",
                        Kind::Prayers { prayer: Some("before-meals".into()), text: None },
                        "Before each meal",
                    )],
                },
                Block {
                    id: "midday".into(),
                    title: "Midday".into(),
                    time: Some("12:00".into()),
                    items: vec![
                        item(
                            "midday-psalm",
                            "Psalm 90",
                            Kind::Psalm { lxx: 90 },
                            "Read at the Sixth Hour, at noon",
                        ),
                        Item {
                            optional: true,
                            ..item(
                                "midday-jesus-prayer",
                                "The Jesus Prayer",
                                Kind::JesusPrayer { count: 33 },
                                "",
                            )
                        },
                    ],
                },
                Block {
                    id: "evening".into(),
                    title: "Evening".into(),
                    time: Some("20:30".into()),
                    items: vec![
                        item("evening-prayers", "Evening Prayers", own_book(), "From your prayer book"),
                        Item {
                            season: Season::GreatLent,
                            ..item(
                                "evening-ephrem",
                                "Prayer of St Ephrem",
                                Kind::Prayers { prayer: Some("st-ephrem".into()), text: None },
                                "With prostrations, in Great Lent",
                            )
                        },
                        item("evening-kathisma", "The Psalter", Kind::Kathisma, "The day's kathisma"),
                        item("evening-fathers", "The Fathers", Kind::Fathers, "On the day's Gospel"),
                        item(
                            "evening-reflection",
                            "Examination of conscience",
                            Kind::Reflection,
                            "Before sleep",
                        ),
                    ],
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn starter_rule_round_trips_and_follows_the_seasons() {
        let r = Rule::starter();
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(serde_json::from_str::<Rule>(&json).unwrap(), r);
        let ordinary = calendar::civil(2026, 10, 20);
        let lent = calendar::civil(2026, 3, 4);
        let titles = |d| -> Vec<String> {
            r.for_day(d, Style::New).into_iter().flat_map(|(_, i)| i).map(|i| i.id.clone()).collect()
        };
        assert!(!titles(ordinary).contains(&"evening-ephrem".to_string()));
        assert!(titles(lent).contains(&"evening-ephrem".to_string()));
        let mut done = HashSet::new();
        assert_eq!(r.kept(ordinary, Style::New, &done), (0, 8));
        done.insert("morning-prayers".to_string());
        done.insert("midday-jesus-prayer".to_string());
        assert_eq!(r.kept(ordinary, Style::New, &done), (1, 8), "optional items don't count");
    }

    /// The rule Nicholas keeps (the routine design's journeys): the starter's, with the morning
    /// at 5:00 and Psalm 90 alone at midday.
    fn his_rule() -> Rule {
        let mut r = Rule::starter();
        r.blocks[0].time = Some("05:00".into());
        r.blocks[2].items.truncate(1);
        r
    }

    fn ids(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    const MORNING: [&str; 2] = ["morning-prayers", "morning-reading"];
    const EVENING: [&str; 4] =
        ["evening-prayers", "evening-kathisma", "evening-fathers", "evening-reflection"];

    /// The Now card at `minute` of `d`: the block's id, and whether it's due.
    fn now<'a>(r: &'a Rule, d: Day, minute: i64, done: &[&str], ended: &[&str]) -> Option<(&'a str, bool)> {
        r.now(d, Style::New, minute, &ids(done), &ids(ended)).map(|n| match n {
            Now::Due(b) => (b.id.as_str(), true),
            Now::Next(b) => (b.id.as_str(), false),
        })
    }

    #[test]
    fn rule_times_read_as_minutes() {
        assert_eq!(minutes("20:30"), Some(20 * 60 + 30));
        assert_eq!(minutes(" 5:00"), Some(300));
        assert_eq!((minutes("24:00"), minutes("7"), minutes("noon")), (None, None, None));
    }

    #[test]
    fn the_now_card_offers_the_block_due_or_the_next() {
        let wed = calendar::civil(2026, 10, 21);
        let at = |h: i64, m: i64| h * 60 + m;
        let his = his_rule();
        assert_eq!(now(&his, wed, at(4, 30), &[], &[]), Some(("morning", false)));
        assert_eq!(now(&his, wed, at(5, 10), &[], &[]), Some(("morning", true)));
        assert_eq!(now(&his, wed, at(5, 10), &MORNING, &[]), Some(("midday", false)));
        // The latest block whose time has come, even with the morning still to do.
        assert_eq!(now(&his, wed, at(12, 30), &[], &[]), Some(("midday", true)));
        let day: Vec<&str> = MORNING.into_iter().chain(["midday-psalm"]).collect();
        assert_eq!(now(&his, wed, at(12, 30), &day, &[]), Some(("evening", false)));
        assert_eq!(now(&his, wed, at(20, 45), &day, &[]), Some(("evening", true)));
        assert_eq!(now(&his, wed, at(23, 59), &day, &[]), Some(("evening", true)));
        // With the evening done, an earlier block still to do is offered again.
        assert_eq!(now(&his, wed, at(23, 59), &EVENING, &[]), Some(("midday", true)));
        // Nothing when everything is done or ended, and never the meals, which have no time.
        let all: Vec<&str> = day.iter().copied().chain(EVENING).collect();
        assert_eq!(now(&his, wed, at(23, 59), &all, &[]), None);
        assert_eq!(now(&his, wed, at(20, 45), &day, &["evening"]), None);
        assert_eq!(now(&his, wed, at(12, 30), &MORNING, &["midday"]), Some(("evening", false)));

        // The starter rule: the morning at 7:00, and an optional Jesus Prayer at midday.
        let starter = Rule::starter();
        assert_eq!(now(&starter, wed, at(4, 30), &[], &[]), Some(("morning", false)));
        assert_eq!(now(&starter, wed, at(5, 10), &[], &[]), Some(("morning", false)));
        assert_eq!(now(&starter, wed, at(12, 30), &[], &[]), Some(("midday", true)));
        // Only the optional item left: the midday is finished.
        assert_eq!(now(&starter, wed, at(12, 30), &day, &[]), Some(("evening", false)));
        assert_eq!(now(&starter, wed, at(20, 45), &day, &[]), Some(("evening", true)));
        assert_eq!(now(&starter, wed, at(23, 59), &all, &[]), None);
        // On Sunday evening there's no kathisma to keep the evening open.
        let sun = calendar::civil(2026, 10, 25);
        let sunday: Vec<&str> = all.iter().copied().filter(|&i| i != "evening-kathisma").collect();
        assert_eq!(now(&starter, sun, at(23, 59), &sunday, &[]), None);
        assert_eq!(now(&starter, wed, at(23, 59), &sunday, &[]), Some(("evening", true)));
    }

    /// The next item of `block` on `d` after `after`, by id.
    fn next(r: &Rule, d: Day, block: &str, after: Option<&str>, done: &[&str]) -> Option<String> {
        r.next_item(d, Style::New, block, after, &ids(done)).map(|i| i.id.clone())
    }

    #[test]
    fn done_moves_on_through_the_block() {
        let wed = calendar::civil(2026, 10, 21);
        let mut r = Rule::starter();
        assert_eq!(next(&r, wed, "evening", None, &[]).as_deref(), Some("evening-prayers"));
        assert_eq!(
            next(&r, wed, "evening", Some("evening-prayers"), &[]).as_deref(),
            Some("evening-kathisma")
        );
        // Lent adds St Ephrem; Sunday evening drops the kathisma.
        let lent = calendar::civil(2026, 3, 4);
        assert_eq!(
            next(&r, lent, "evening", Some("evening-prayers"), &[]).as_deref(),
            Some("evening-ephrem")
        );
        let sun = calendar::civil(2026, 10, 25);
        assert_eq!(
            next(&r, sun, "evening", Some("evening-prayers"), &[]).as_deref(),
            Some("evening-fathers")
        );
        // What's done is passed over; Begin finds the first item still to do.
        let done = ["evening-prayers", "evening-kathisma"];
        assert_eq!(next(&r, wed, "evening", None, &done).as_deref(), Some("evening-fathers"));
        // It never goes back up, nor into the next block: the end is the end.
        assert_eq!(
            next(&r, wed, "evening", Some("evening-fathers"), &[]).as_deref(),
            Some("evening-reflection")
        );
        assert_eq!(next(&r, wed, "evening", Some("evening-reflection"), &[]), None);
        assert_eq!(next(&r, wed, "morning", Some("morning-reading"), &[]), None);
        assert_eq!(next(&r, wed, "evening", None, &EVENING), None);
        // Optional items are offered.
        assert_eq!(
            next(&r, wed, "midday", Some("midday-psalm"), &[]).as_deref(),
            Some("midday-jesus-prayer")
        );
        // An item no longer in the block (edited away) starts from the top; no block, no item.
        assert_eq!(
            next(&r, wed, "evening", Some("gone"), &["evening-prayers"]).as_deref(),
            Some("evening-kathisma")
        );
        assert_eq!(next(&r, wed, "night", None, &[]), None);
        // A plan with nothing to read that day is passed over: the apps hand it in with what's done.
        let plan = Item {
            id: "evening-plan".into(),
            title: "Gospels in 30 Days".into(),
            kind: Kind::Plan { plan: "gospels-30".into() },
            note: String::new(),
            season: Season::Always,
            optional: false,
        };
        r.blocks[3].items.insert(3, plan);
        assert_eq!(next(&r, wed, "evening", Some("evening-kathisma"), &[]).as_deref(), Some("evening-plan"));
        let idle = ["evening-plan"];
        assert_eq!(
            next(&r, wed, "evening", Some("evening-kathisma"), &idle).as_deref(),
            Some("evening-fathers")
        );
    }

    /// What item `item_id` of `r` reads on `d` (in the Greek usage): (label, reference, end) for
    /// each part.
    fn read(r: &Rule, item_id: &str, d: Day, plan: Option<(usize, usize)>) -> Vec<(String, String, String)> {
        let lessons = lectionary::readings(d, Style::New);
        let (block, items) = r
            .for_day(d, Style::New)
            .into_iter()
            .find(|(_, items)| items.iter().any(|i| i.id == item_id))
            .unwrap();
        let item = items.into_iter().find(|i| i.id == item_id).unwrap();
        parts(item, block, d, &lessons, plan).into_iter().map(|p| (p.label, p.reference, p.end)).collect()
    }

    fn part(label: &str, reference: &str, end: &str) -> (String, String, String) {
        (label.into(), reference.into(), end.into())
    }

    #[test]
    fn items_read_their_parts_of_scripture() {
        let r = Rule::starter();
        // Wednesday 21 October 2026: the Epistle, then the Gospel; the Fathers on the Gospel.
        let wed = calendar::civil(2026, 10, 21);
        let epistle = part("Epistle", "2 Corinthians 9:6–11", "End of the Epistle");
        let gospel = part("Gospel", "Luke 9:44–50", "End of the Gospel");
        assert_eq!(read(&r, "morning-reading", wed, None), [epistle, gospel.clone()]);
        assert_eq!(read(&r, "evening-fathers", wed, None), [gospel]);
        // A Lenten weekday has no Liturgy: its first reading, for both.
        let lent = calendar::civil(2026, 3, 4);
        let isaiah = part("Old Testament", "Isaiah 5:16–25", "End of the reading");
        assert_eq!(read(&r, "morning-reading", lent, None), read(&r, "evening-fathers", lent, None));
        assert_eq!(read(&r, "morning-reading", lent, None), [isaiah]);
        // Holy Friday: the first Passion Gospel, over six chapters.
        let friday = calendar::civil(2026, 4, 10);
        assert_eq!(
            read(&r, "morning-reading", friday, None),
            [part("Gospel", "John 13:31–18:1", "End of the reading")]
        );
        let item = &r.blocks[0].items[1];
        let ps = parts(item, &r.blocks[0], friday, &lectionary::readings(friday, Style::New), None);
        let chapters: HashSet<u8> =
            ps[0].passages.iter().flat_map(|p| p.verses()).map(|v| v.chapter).collect();
        assert_eq!(chapters, (13..=18).collect());

        // A reading in two pieces is one part.
        let piece = |s| lectionary::parse_passage(s).unwrap();
        let romans: &'static [Passage] = vec![piece("Rom 14:19-23"), piece("Rom 16:25-27")].leak();
        let lesson = |kind, passages| Reading {
            service: "Liturgy",
            kind,
            passages,
            title: None,
            served: None,
            occasion: "",
            confidence: lectionary::Confidence::High,
        };
        let matthew: &'static [Passage] = vec![piece("Matt 6:1-13")].leak();
        let lessons = [lesson(lectionary::Kind::Epistle, romans), lesson(lectionary::Kind::Gospel, matthew)];
        let ps = parts(item, &r.blocks[0], friday, &lessons, None);
        assert_eq!(
            (ps[0].reference.as_str(), ps[0].passages.as_slice()),
            ("Romans 14:19–23; 16:25–27", romans)
        );
        assert_eq!(ps[1].reference, "Matthew 6:1–13");

        // Psalm 90 and the day's kathisma, in the numbering shown (here the Hebrew).
        let ps = parts(&r.blocks[2].items[0], &r.blocks[2], wed, &[], None);
        assert_eq!(ps[0].passages, [Passage { book: crate::BookId::PSALMS, from: (91, 1), to: (91, 16) }]);
        assert_eq!(read(&r, "midday-psalm", wed, None), [part("Psalm 91", "Psalm 91", "End of Psalm 91")]);
        assert_eq!(
            read(&r, "evening-kathisma", wed, None),
            [part("Kathisma 12", "Psalms 86–91", "End of kathisma 12")]
        );
        let kathisma = parts(&r.blocks[3].items[2], &r.blocks[3], wed, &[], None);
        assert_eq!(kathisma[0].passages, calendar::kathisma_passages(12));
        assert!(read(&r, "evening-kathisma", calendar::civil(2026, 10, 25), None).is_empty(), "Sunday");
        let mut morning = r.clone();
        morning.blocks[0].items[1].kind = Kind::Kathisma;
        let tue = calendar::civil(2026, 10, 20);
        let labels: Vec<String> =
            read(&morning, "morning-reading", tue, None).into_iter().map(|p| p.0).collect();
        assert_eq!(labels, ["Kathisma 7", "Kathisma 8"], "Matins");

        // A plan's passage: the one asked for, of the plan's day.
        let mut planned = r.clone();
        planned.blocks[0].items[1].kind = Kind::Plan { plan: "gospels-30".into() };
        assert_eq!(read(&planned, "morning-reading", wed, None), []);
        assert_eq!(
            read(&planned, "morning-reading", wed, Some((1, 1))),
            [part("Day 1", "Matthew 2", "End of Matthew 2")]
        );
        let item = &planned.blocks[0].items[1];
        let ps = parts(item, &planned.blocks[0], wed, &[], Some((1, 1)));
        let matthew = crate::BookId::from_code("MAT").unwrap();
        assert_eq!(ps[0].passages, [Passage { book: matthew, from: (2, 1), to: (2, 23) }]);
        planned.blocks[0].items[1].kind = Kind::Plan { plan: "sermon-mount".into() };
        let sermon = read(&planned, "morning-reading", wed, Some((1, 0)));
        assert_eq!(sermon, [part("Day 1", "Matthew 5:1–16", "End of Matthew 5:1–16")]);
        assert_eq!(read(&planned, "morning-reading", wed, Some((1, 9))), [], "no such passage");

        // Prayers and the rest read no Scripture.
        for id in ["morning-prayers", "meal-prayer", "midday-jesus-prayer", "evening-reflection"] {
            assert!(read(&r, id, wed, None).is_empty(), "{id}");
        }
    }
}
