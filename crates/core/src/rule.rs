//! A prayer rule (Orthodox fork): the daily discipline someone keeps, in blocks through the day
//! (morning, meals, midday, evening), each a list of items to pray or read.
//!
//! The rule is a JSON document in the study store's settings ([`StudyStore::rule`]); what was
//! done each day is a row per (day, item) ([`StudyStore::rule_done`]). Nothing scores or counts
//! streaks: Orthodox writers warn against making prayer an achievement, so the app only shows
//! which days were kept, quietly, on the calendar.
//!
//! [`StudyStore::rule`]: crate::study::StudyStore::rule
//! [`StudyStore::rule_done`]: crate::study::StudyStore::rule_done

use serde::{Deserialize, Serialize};

use crate::calendar::{self, Day, Style};

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
    pub fn kept(&self, d: Day, style: Style, done: &std::collections::HashSet<String>) -> (usize, usize) {
        let required: Vec<&Item> =
            self.for_day(d, style).into_iter().flat_map(|(_, items)| items).filter(|i| !i.optional).collect();
        (required.iter().filter(|i| done.contains(&i.id)).count(), required.len())
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
}
