//! The prayer rule (Orthodox fork, `bible_core::rule`): the rule to edit, a day's rule with what
//! was checked off and what each item opens, and a month of fasts, feasts and kept days.

use std::collections::{HashMap, HashSet};

use bible_core::ChapterIdx;
use bible_core::calendar::{self, Day, Fast, Style};
use bible_core::lectionary::{self, Kind as Lesson};
use bible_core::plans::Plan;
use bible_core::rule::{self as core, Kind, Season};
use bible_core::study::StudyStore;

use crate::{Bible, BibleError, VerseRef};

/// The rule: its parts of the day in order.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Rule {
    pub blocks: Vec<RuleBlock>,
}

/// A part of the day: "Morning", "Before meals".
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RuleBlock {
    /// Stable. A kathisma reads at Matins in "morning" or a part of the day before noon, at
    /// Vespers in the others.
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
    /// The plan's day that falls on the day; its check box ticks the day's passages, and it's
    /// done once they're read.
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
    /// A passage in the reader at its first verse: the day's Gospel, or its first reading.
    Passage {
        chapter_index: u16,
        verse: u8,
    },
    /// The Church Fathers on a verse: the first of the day's Gospel.
    Fathers {
        verse: VerseRef,
    },
    /// Nothing to do on this day (no kathisma on Sunday evening): no check box, and it doesn't
    /// count against the day.
    NotToday,
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

pub(crate) fn style(old_calendar: bool) -> Style {
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
    /// `midnight` (Unix seconds of the local midnight that starts the date) places the reading
    /// plans' days.
    pub fn rule_day(&self, year: i32, month: u8, day: u8, old_calendar: bool, midnight: i64) -> RuleDay {
        let d = calendar::civil(year, month, day);
        let lessons = lectionary::readings(d, style(old_calendar));
        let study = self.study();
        let rule = study.rule();
        let done = study.rule_done(d);
        let under = under_way(&rule, &study);
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
                        let (open, detail) = opens(i, b, (d, midnight), &lessons, &study, &under);
                        // As the Mac counts it: ticked, or a plan's reading for the day read.
                        let done = done.contains(&i.id) || plan_read(i, midnight, &under);
                        RuleDayItem {
                            id: i.id.clone(),
                            title: i.title.clone(),
                            note: i.note.clone(),
                            optional: i.optional,
                            done,
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
    /// was kept: what was ticked, a plan's item once that day's reading was read, and what had
    /// nothing to do that day counting as kept. `midnight` starts the 1st, as for
    /// [`Bible::rule_day`].
    pub fn church_month(&self, year: i32, month: u8, old_calendar: bool, midnight: i64) -> Vec<MonthDay> {
        let style = style(old_calendar);
        let first = calendar::civil(year, month, 1);
        let next =
            if month >= 12 { calendar::civil(year + 1, 1, 1) } else { calendar::civil(year, month + 1, 1) };
        let study = self.study();
        let rule = study.rule();
        let mut done = study.rule_done_between(first, next - 1);
        let under = under_way(&rule, &study);
        (first..next)
            .map(|d| {
                let info = calendar::day(d, style);
                let midnight = midnight + (d - first) * 86_400;
                let items: Vec<(&core::Block, &core::Item)> = rule
                    .for_day(d, style)
                    .into_iter()
                    .flat_map(|(b, items)| items.into_iter().map(move |i| (b, i)))
                    .collect();
                let mut ticked = done.remove(&d).unwrap_or_default();
                ticked.extend(
                    items.iter().filter(|(_, i)| plan_read(i, midnight, &under)).map(|(_, i)| i.id.clone()),
                );
                let some = items.iter().any(|(_, i)| ticked.contains(&i.id));
                ticked.extend(items.iter().filter(|(b, i)| idle(i, b, d)).map(|(_, i)| i.id.clone()));
                let (n, required) = rule.kept(d, style, &ticked);
                MonthDay {
                    day: d,
                    date: calendar::civil_date(d).2,
                    weekday: info.weekday,
                    fast: info.fast.into(),
                    great_feast: info.feasts.iter().any(|f| GREAT_FEASTS.iter().any(|g| f.starts_with(g))),
                    kept: match (some, required > 0 && n == required) {
                        (false, _) => RuleKept::Nothing,
                        (true, true) => RuleKept::Fully,
                        (true, false) => RuleKept::Partly,
                    },
                }
            })
            .collect()
    }
}

/// The plans under way that the rule's items follow, by id: when each began and what's read.
type UnderWay = HashMap<String, (i64, HashSet<(u16, u8)>)>;

fn under_way(rule: &core::Rule, study: &StudyStore) -> UnderWay {
    let followed: HashSet<&str> = rule
        .blocks
        .iter()
        .flat_map(|b| &b.items)
        .filter_map(|i| match &i.kind {
            Kind::Plan { plan } => Some(plan.as_str()),
            _ => None,
        })
        .collect();
    study
        .plans()
        .into_iter()
        .filter(|s| s.finished.is_none() && followed.contains(s.id.as_str()))
        .map(|s| {
            let reads = study.plan_reads(&s.id);
            (s.id, (s.started, reads))
        })
        .collect()
}

/// A plan's day on the day that starts at `midnight`, if it has one then.
fn plan_day(plan: &Plan, started: i64, midnight: i64) -> Option<usize> {
    let day = crate::plans::day_of(started, midnight);
    (1..=plan.len() as i64).contains(&day).then_some(day as usize)
}

/// Whether `item` follows a plan under way whose reading for the day starting at `midnight` is
/// read.
fn plan_read(item: &core::Item, midnight: i64, under: &UnderWay) -> bool {
    let Kind::Plan { plan } = &item.kind else { return false };
    let (Some(p), Some((started, reads))) = (bible_core::plans::get(plan), under.get(plan)) else {
        return false;
    };
    plan_day(p, *started, midnight).is_some_and(|day| crate::plans::day_read(p, day, reads))
}

/// The day's kathismata for a kathisma item, as the Mac reads them: at Matins in "morning" or a
/// part of the day before noon, at Vespers in the others.
fn kathismata(block: &core::Block, d: Day) -> &'static [u8] {
    let (matins, vespers) = calendar::kathismata(calendar::weekday(d));
    let hour = block.time.as_deref().and_then(|t| t.split_once(':')?.0.trim().parse::<u8>().ok());
    if block.id == "morning" || hour.is_some_and(|h| h < 12) { matins } else { vespers }
}

/// Nothing to do on day `d`: a kathisma where the day has none (Sunday evening).
fn idle(item: &core::Item, block: &core::Block, d: Day) -> bool {
    item.kind == Kind::Kathisma && kathismata(block, d).is_empty()
}

/// What an item opens on day `d` (starting at `midnight`, with the readings `lessons`), and the
/// line that says so.
fn opens(
    item: &core::Item,
    block: &core::Block,
    (d, midnight): (Day, i64),
    lessons: &[lectionary::Reading],
    study: &StudyStore,
    under: &UnderWay,
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
            let ks = kathismata(block, d);
            if ks.is_empty() {
                let none = if calendar::weekday(d) == 0 {
                    "No kathisma on Sunday evening"
                } else {
                    "No kathisma today"
                };
                return (RuleOpen::NotToday, Some(none.into()));
            }
            let Some(chapter_index) = ks
                .first()
                .and_then(|&n| calendar::kathisma_start(n))
                .and_then(|(b, c)| ChapterIdx::new(b, c))
                .map(|c| c.0)
            else {
                return (RuleOpen::Nothing, None);
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
            let Some((started, _)) = under.get(plan) else {
                // Finished (on the Mac): ticked like any other item.
                if study.plans().iter().any(|s| s.id == *plan) {
                    return (RuleOpen::Nothing, Some("Finished".into()));
                }
                let open = RuleOpen::StartPlan { plan: plan.clone(), title: p.title.into() };
                return (open, Some("Not started".into()));
            };
            match plan_day(p, *started, midnight) {
                Some(day) => {
                    let detail = format!("Day {day} · {}", crate::plans::day_label(p.day(day)));
                    (RuleOpen::Plan { plan: plan.clone(), day: day as u32 }, Some(detail))
                }
                None => (RuleOpen::Nothing, Some("No reading on this day".into())),
            }
        }
        Kind::Readings | Kind::Fathers => {
            let Some((gospel, line)) = crate::readings::gospel(lessons) else {
                return (RuleOpen::Nothing, None);
            };
            let Some(first) = gospel.passages.first().map(|p| p.first()) else {
                return (RuleOpen::Nothing, None);
            };
            if item.kind == Kind::Fathers {
                return (RuleOpen::Fathers { verse: first.into() }, Some(line));
            }
            // The Liturgy's Epistle, then its Gospel.
            let epistle = lessons.iter().find(|r| r.at_liturgy() && r.kind == Lesson::Epistle);
            let detail = match epistle {
                Some(e) if gospel.at_liturgy() => format!("{} · {line}", e.label()),
                _ => line,
            };
            (RuleOpen::Passage { chapter_index: first.chapter_idx().0, verse: first.verse }, Some(detail))
        }
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
