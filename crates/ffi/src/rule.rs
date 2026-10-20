//! The prayer rule (Orthodox fork, `bible_core::rule`): the rule to edit, a day's rule with what
//! was checked off and what each item opens, and a month of fasts, feasts and kept days.

use std::collections::HashSet;

use bible_core::ChapterIdx;
use bible_core::calendar::{self, Fast, Style};
use bible_core::rule::{self as core, Kind, Season};
use bible_core::study::StudyStore;

use crate::{Bible, BibleError};

/// The rule: its parts of the day in order.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Rule {
    pub blocks: Vec<RuleBlock>,
}

/// A part of the day: "Morning", "Before meals".
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RuleBlock {
    /// Stable; "morning" reads the Matins kathismata, any other the Vespers ones.
    pub id: String,
    pub title: String,
    /// "HH:MM" local time, for the reminder; `None` for a part without a set time.
    pub time: Option<String>,
    pub items: Vec<RuleItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RuleItem {
    /// Stable within the rule: a day's check marks are kept by it.
    pub id: String,
    pub title: String,
    pub kind: RuleKind,
    pub note: String,
    pub season: RuleSeason,
    pub optional: bool,
}

/// What an item opens (`bible_core::rule::Kind`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum RuleKind {
    /// A built-in prayer (`prayer`), the person's own words (`text`), or neither: their prayer book.
    Prayers {
        prayer: Option<String>,
        text: Option<String>,
    },
    Kathisma,
    /// A psalm in Septuagint numbering.
    Psalm {
        lxx: u8,
    },
    Readings,
    Plan {
        plan: String,
    },
    Fathers,
    JesusPrayer {
        count: u16,
    },
    Reflection,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum RuleSeason {
    Always,
    GreatLent,
    OutsideLent,
}

impl From<core::Rule> for Rule {
    fn from(r: core::Rule) -> Self {
        Rule {
            blocks: r
                .blocks
                .into_iter()
                .map(|b| RuleBlock {
                    id: b.id,
                    title: b.title,
                    time: b.time,
                    items: b.items.into_iter().map(Into::into).collect(),
                })
                .collect(),
        }
    }
}

impl From<Rule> for core::Rule {
    fn from(r: Rule) -> Self {
        core::Rule {
            blocks: r
                .blocks
                .into_iter()
                .map(|b| core::Block {
                    id: b.id,
                    title: b.title,
                    time: b.time,
                    items: b.items.into_iter().map(Into::into).collect(),
                })
                .collect(),
        }
    }
}

impl From<core::Item> for RuleItem {
    fn from(i: core::Item) -> Self {
        let kind = match i.kind {
            Kind::Prayers { prayer, text } => RuleKind::Prayers { prayer, text },
            Kind::Kathisma => RuleKind::Kathisma,
            Kind::Psalm { lxx } => RuleKind::Psalm { lxx },
            Kind::Readings => RuleKind::Readings,
            Kind::Plan { plan } => RuleKind::Plan { plan },
            Kind::Fathers => RuleKind::Fathers,
            Kind::JesusPrayer { count } => RuleKind::JesusPrayer { count },
            Kind::Reflection => RuleKind::Reflection,
            Kind::Other => RuleKind::Other,
        };
        let season = match i.season {
            Season::Always => RuleSeason::Always,
            Season::GreatLent => RuleSeason::GreatLent,
            Season::OutsideLent => RuleSeason::OutsideLent,
        };
        RuleItem { id: i.id, title: i.title, kind, note: i.note, season, optional: i.optional }
    }
}

impl From<RuleItem> for core::Item {
    fn from(i: RuleItem) -> Self {
        let kind = match i.kind {
            RuleKind::Prayers { prayer, text } => Kind::Prayers { prayer, text },
            RuleKind::Kathisma => Kind::Kathisma,
            RuleKind::Psalm { lxx } => Kind::Psalm { lxx },
            RuleKind::Readings => Kind::Readings,
            RuleKind::Plan { plan } => Kind::Plan { plan },
            RuleKind::Fathers => Kind::Fathers,
            RuleKind::JesusPrayer { count } => Kind::JesusPrayer { count },
            RuleKind::Reflection => Kind::Reflection,
            RuleKind::Other => Kind::Other,
        };
        let season = match i.season {
            RuleSeason::Always => Season::Always,
            RuleSeason::GreatLent => Season::GreatLent,
            RuleSeason::OutsideLent => Season::OutsideLent,
        };
        core::Item { id: i.id, title: i.title, kind, note: i.note, season, optional: i.optional }
    }
}

/// A built-in prayer text (`bible_core::prayers`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Prayer {
    pub id: String,
    pub title: String,
    pub text: String,
    /// Where the wording comes from.
    pub source: String,
}

/// The rule on one day: the parts of the day with their items in season.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RuleDay {
    /// Days since 1970-01-01, for [`Bible::set_rule_done`].
    pub day: i64,
    pub blocks: Vec<RuleDayBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RuleDayBlock {
    pub id: String,
    pub title: String,
    pub time: Option<String>,
    pub items: Vec<RuleDayItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RuleDayItem {
    pub id: String,
    pub title: String,
    pub note: String,
    pub optional: bool,
    pub done: bool,
    /// What it opens on this day: "Kathisma 9 · Psalms 64–69", "Day 3 · Psalms 17–23".
    pub detail: Option<String>,
    pub open: RuleOpen,
}

/// What tapping an item of the day opens.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum RuleOpen {
    /// Nothing: prayed from the person's own book, or described by its title.
    Nothing,
    /// A prayer or the examination of conscience, to read.
    Text {
        title: String,
        text: String,
        source: Option<String>,
    },
    /// A chapter in the reader: a psalm, or the first psalm of the day's kathisma.
    Chapter {
        chapter_index: u16,
    },
    /// The next day of a plan under way that isn't read yet.
    Plan {
        plan: String,
        day: u32,
    },
    /// A plan not started yet.
    StartPlan {
        plan: String,
        title: String,
    },
    JesusPrayer {
        count: u16,
    },
    /// The day's readings and the Fathers on the Gospel wait for the lectionary.
    ComingSoon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FastLevel {
    None,
    NoMeat,
    Fish,
    WineOil,
    Wine,
    Strict,
}

impl From<Fast> for FastLevel {
    fn from(f: Fast) -> Self {
        match f {
            Fast::None => FastLevel::None,
            Fast::NoMeat => FastLevel::NoMeat,
            Fast::Fish => FastLevel::Fish,
            Fast::WineOil => FastLevel::WineOil,
            Fast::Wine => FastLevel::Wine,
            Fast::Strict => FastLevel::Strict,
        }
    }
}

/// How much of the day's rule was checked off. No counts: only kept, partly kept or not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum RuleKept {
    Nothing,
    Partly,
    /// Every required item.
    Fully,
}

/// A day in the month calendar.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MonthDay {
    /// Days since 1970-01-01.
    pub day: i64,
    /// The civil day of the month.
    pub date: u8,
    /// 0 = Sunday.
    pub weekday: u8,
    pub fast: FastLevel,
    pub great_feast: bool,
    pub kept: RuleKept,
}

/// The Twelve Great Feasts and Pascha, as `calendar` names them.
const GREAT_FEASTS: &[&str] = &[
    "Pascha",
    "Nativity of the Theotokos",
    "Elevation of the Cross",
    "Entry of the Theotokos",
    "Nativity of Our Lord",
    "Theophany",
    "Meeting of Our Lord",
    "Annunciation",
    "Palm Sunday",
    "Ascension of Our Lord",
    "Pentecost",
    "Transfiguration of Our Lord",
    "Dormition of the Theotokos",
];

fn style(old_calendar: bool) -> Style {
    if old_calendar { Style::Old } else { Style::New }
}

/// The rule Nicholas set out, to start again from.
#[uniffi::export]
pub fn starter_rule() -> Rule {
    core::Rule::starter().into()
}

/// The built-in prayer texts, for the rule's editor.
#[uniffi::export]
pub fn prayers() -> Vec<Prayer> {
    bible_core::prayers::all()
        .iter()
        .map(|p| Prayer {
            id: p.id.into(),
            title: p.title.into(),
            text: p.text.into(),
            source: p.source.into(),
        })
        .collect()
}

/// Days since 1970-01-01 of a civil date.
#[uniffi::export]
pub fn civil_day(year: i32, month: u8, day: u8) -> i64 {
    calendar::civil(year, month, day)
}

#[uniffi::export]
impl Bible {
    /// The prayer rule; the starter rule until one is saved.
    pub fn rule(&self) -> Rule {
        self.study().rule().into()
    }

    pub fn set_rule(&self, rule: Rule) -> Result<(), BibleError> {
        Ok(self.study().set_rule(&rule.into())?)
    }

    /// The rule on a civil date: its items in season, what was checked off and what each opens.
    pub fn rule_day(&self, year: i32, month: u8, day: u8, old_calendar: bool) -> RuleDay {
        let d = calendar::civil(year, month, day);
        let study = self.study();
        let rule = study.rule();
        let done = study.rule_done(d);
        let blocks = rule
            .for_day(d, style(old_calendar))
            .into_iter()
            .map(|(b, items)| RuleDayBlock {
                id: b.id.clone(),
                title: b.title.clone(),
                time: b.time.clone(),
                items: items
                    .into_iter()
                    .map(|i| {
                        let (open, detail) = opens(i, b, calendar::weekday(d), &study);
                        RuleDayItem {
                            id: i.id.clone(),
                            title: i.title.clone(),
                            note: i.note.clone(),
                            optional: i.optional,
                            done: done.contains(&i.id),
                            detail,
                            open,
                        }
                    })
                    .collect(),
            })
            .collect();
        RuleDay { day: d, blocks }
    }

    /// Checks an item off on `day` (as [`RuleDay::day`] gives it), or clears it.
    pub fn set_rule_done(&self, day: i64, item: String, done: bool) -> Result<(), BibleError> {
        Ok(self.study().set_rule_done(day, &item, done)?)
    }

    /// Every day of a civil month: its fast, whether it's a great feast, and how much of the rule
    /// was kept.
    pub fn church_month(&self, year: i32, month: u8, old_calendar: bool) -> Vec<MonthDay> {
        let style = style(old_calendar);
        let first = calendar::civil(year, month, 1);
        let next =
            if month >= 12 { calendar::civil(year + 1, 1, 1) } else { calendar::civil(year, month + 1, 1) };
        let study = self.study();
        let rule = study.rule();
        let done = study.rule_done_between(first, next - 1);
        let none = HashSet::new();
        (first..next)
            .map(|d| {
                let info = calendar::day(d, style);
                let done = done.get(&d).unwrap_or(&none);
                let any =
                    rule.for_day(d, style).iter().flat_map(|(_, items)| items).any(|i| done.contains(&i.id));
                let (n, required) = rule.kept(d, style, done);
                MonthDay {
                    day: d,
                    date: calendar::civil_date(d).2,
                    weekday: info.weekday,
                    fast: info.fast.into(),
                    great_feast: info.feasts.iter().any(|f| GREAT_FEASTS.iter().any(|g| f.starts_with(g))),
                    kept: match (any, n == required) {
                        (false, _) => RuleKept::Nothing,
                        (true, true) => RuleKept::Fully,
                        (true, false) => RuleKept::Partly,
                    },
                }
            })
            .collect()
    }
}

/// What an item opens on a day of the week, and the line that says so.
fn opens(
    item: &core::Item,
    block: &core::Block,
    weekday: u8,
    study: &StudyStore,
) -> (RuleOpen, Option<String>) {
    match &item.kind {
        Kind::Prayers { prayer, text } => {
            if let Some(p) = prayer.as_deref().and_then(bible_core::prayers::get) {
                let text = p.text.into();
                (RuleOpen::Text { title: p.title.into(), text, source: Some(p.source.into()) }, None)
            } else if let Some(text) = text.as_ref().filter(|t| !t.trim().is_empty()) {
                (RuleOpen::Text { title: item.title.clone(), text: text.clone(), source: None }, None)
            } else {
                (RuleOpen::Nothing, None)
            }
        }
        Kind::Kathisma => {
            let (matins, vespers) = calendar::kathismata(weekday);
            let ks = if block.id == "morning" { matins } else { vespers };
            let Some(chapter_index) = ks
                .first()
                .and_then(|&n| calendar::kathisma_start(n))
                .and_then(|(b, c)| ChapterIdx::new(b, c))
                .map(|c| c.0)
            else {
                let none = if weekday == 0 { "No kathisma on Sunday evening" } else { "No kathisma today" };
                return (RuleOpen::Nothing, Some(none.into()));
            };
            let psalms: Vec<String> = ks.iter().map(|&n| calendar::kathisma_psalms(n)).collect();
            let detail = match ks {
                [n] => format!("Kathisma {n} · {}", psalms[0]),
                _ => {
                    let numbers: Vec<String> = ks.iter().map(u8::to_string).collect();
                    let rest: Vec<&str> = psalms[1..]
                        .iter()
                        .map(|p| p.trim_start_matches("Psalms ").trim_start_matches("Psalm "))
                        .collect();
                    format!("Kathismata {} · {}, {}", numbers.join(" and "), psalms[0], rest.join(", "))
                }
            };
            (RuleOpen::Chapter { chapter_index }, Some(detail))
        }
        Kind::Psalm { lxx } => {
            match bible_core::lxx::from_lxx(*lxx, 1).and_then(|(b, c, _)| ChapterIdx::new(b, c)) {
                Some(c) => (RuleOpen::Chapter { chapter_index: c.0 }, None),
                None => (RuleOpen::Nothing, None),
            }
        }
        Kind::Plan { plan } => {
            let Some(p) = bible_core::plans::get(plan) else { return (RuleOpen::Nothing, None) };
            if !study.plans().iter().any(|s| s.id == *plan && s.finished.is_none()) {
                let open = RuleOpen::StartPlan { plan: plan.clone(), title: p.title.into() };
                return (open, Some("Not started".into()));
            }
            match crate::plans::next_unread(p, &study.plan_reads(plan)) {
                Some(day) => {
                    let detail = format!("Day {day} · {}", crate::plans::day_label(p.day(day)));
                    (RuleOpen::Plan { plan: plan.clone(), day: day as u32 }, Some(detail))
                }
                None => (RuleOpen::Nothing, Some("Every day read".into())),
            }
        }
        Kind::Readings | Kind::Fathers => (RuleOpen::ComingSoon, Some("Coming soon".into())),
        Kind::JesusPrayer { count } => {
            (RuleOpen::JesusPrayer { count: *count }, Some(format!("{count} times")))
        }
        Kind::Reflection => (
            RuleOpen::Text {
                title: item.title.clone(),
                text: bible_core::prayers::REFLECTION.into(),
                source: None,
            },
            None,
        ),
        Kind::Other => (RuleOpen::Nothing, None),
    }
}
