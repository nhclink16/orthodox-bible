//! Reading plans (`bible_core::plans`) for the phone: the catalogue, the plans under way and each
//! day's passages with what's been read.
//!
//! A plan's start is the Unix time of the local midnight it began on, as the Mac keeps it; the
//! phone passes its own local midnight for "today", so the bridge needs no time zone.

use std::collections::HashSet;

use bible_core::BookId;
use bible_core::plans::{self, Passage, Plan};

use crate::{Bible, BibleError, fail};

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PlanInfo {
    pub id: String,
    pub title: String,
    /// "20 days · a kathisma a day".
    pub meta: String,
    pub description: String,
    pub days: u32,
}

/// A plan under way.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ActivePlan {
    pub plan: PlanInfo,
    /// Unix seconds: the start of the day it began.
    pub started: i64,
    /// The plan's day on the day asked about: 1 on the day it began, past `plan.days` once over.
    pub day: i64,
    /// The first day not read yet; `None` once every day is.
    pub next: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PlanDay {
    /// From 1.
    pub day: u32,
    /// "Psalms 1–8", "Luke 5–6 · Psalm 3".
    pub label: String,
    pub passages: Vec<PlanPassage>,
    /// Every passage read.
    pub read: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PlanPassage {
    /// "Psalm 17", "Matthew 5:1–16".
    pub label: String,
    pub chapter_index: u16,
    /// Where it begins.
    pub verse: u8,
    pub read: bool,
}

fn info(p: &Plan) -> PlanInfo {
    PlanInfo {
        id: p.id.into(),
        title: p.title.into(),
        meta: p.meta.into(),
        description: p.description.into(),
        days: p.len() as u32,
    }
}

fn plan(id: &str) -> Result<&'static Plan, BibleError> {
    plans::get(id).ok_or_else(|| fail(format!("no plan {id}")))
}

/// A day's passages as one line: "Psalms 1–8 · Psalm 151".
pub(crate) fn day_label(passages: &[Passage]) -> String {
    let psalm_151 = BookId::from_code("PS2");
    let (extra, rest): (Vec<Passage>, Vec<Passage>) =
        passages.iter().partition(|p| Some(p.chapter.book()) == psalm_151);
    let psalms = rest.iter().filter(|p| p.chapter.book() == BookId::PSALMS).count();
    let name =
        |b: BookId| if b == BookId::PSALMS && psalms > 1 { "Psalms" } else { b.ref_name() }.to_string();
    let mut label = plans::label(&rest, name);
    if !extra.is_empty() {
        // `plans::label` would number Psalm 151's one chapter.
        if !label.is_empty() {
            label.push_str(" · ");
        }
        label.push_str("Psalm 151");
    }
    label
}

/// The first day of a plan with a passage not read.
pub(crate) fn next_unread(plan: &Plan, reads: &HashSet<(u16, u8)>) -> Option<usize> {
    (1..=plan.len()).find(|&d| (0..plan.day(d).len()).any(|i| !reads.contains(&(d as u16, i as u8))))
}

fn day(plan: &Plan, n: usize, reads: &HashSet<(u16, u8)>) -> PlanDay {
    let passages: Vec<PlanPassage> = plan
        .day(n)
        .iter()
        .enumerate()
        .map(|(i, p)| PlanPassage {
            label: match p.verses {
                Some((a, b)) => format!("{}:{a}–{b}", p.chapter.display()),
                None => p.chapter.display(),
            },
            chapter_index: p.chapter.0,
            verse: p.verses.map_or(1, |(a, _)| a),
            read: reads.contains(&(n as u16, i as u8)),
        })
        .collect();
    PlanDay {
        day: n as u32,
        label: day_label(plan.day(n)),
        read: !passages.is_empty() && passages.iter().all(|p| p.read),
        passages,
    }
}

/// Every reading plan, in the order they're offered.
#[uniffi::export]
pub fn reading_plans() -> Vec<PlanInfo> {
    plans::catalogue().iter().map(info).collect()
}

#[uniffi::export]
impl Bible {
    /// The plans under way, newest first, on the day that starts at `today` (Unix seconds of a
    /// local midnight).
    pub fn active_plans(&self, today: i64) -> Vec<ActivePlan> {
        let study = self.study();
        study
            .plans()
            .into_iter()
            .filter(|s| s.finished.is_none())
            .filter_map(|s| {
                let p = plans::get(&s.id)?;
                // Rounded, so a day of 23 or 25 hours (summer time) still counts as one.
                let day = (today - s.started + 43_200).div_euclid(86_400) + 1;
                let next = next_unread(p, &study.plan_reads(&s.id)).map(|d| d as u32);
                Some(ActivePlan { plan: info(p), started: s.started, day, next })
            })
            .collect()
    }

    /// Day `day` (from 1) of a plan, with what's been read; `None` outside the plan.
    pub fn plan_day(&self, plan: String, day: u32) -> Option<PlanDay> {
        let p = plans::get(&plan)?;
        (1..=p.len())
            .contains(&(day as usize))
            .then(|| self::day(p, day as usize, &self.study().plan_reads(&plan)))
    }

    /// Every day of a plan, with what's been read.
    pub fn plan_days(&self, plan: String) -> Vec<PlanDay> {
        let Some(p) = plans::get(&plan) else { return Vec::new() };
        let reads = self.study().plan_reads(&plan);
        (1..=p.len()).map(|n| day(p, n, &reads)).collect()
    }

    /// Starts (or restarts, forgetting what was read) a plan on the day that starts at `today`.
    pub fn start_plan(&self, plan: String, today: i64) -> Result<(), BibleError> {
        self::plan(&plan)?;
        Ok(self.study().start_plan(&plan, today)?)
    }

    pub fn stop_plan(&self, plan: String) -> Result<(), BibleError> {
        Ok(self.study().stop_plan(&plan)?)
    }

    pub fn set_plan_read(&self, plan: String, day: u32, passage: u8, read: bool) -> Result<(), BibleError> {
        let day = u16::try_from(day).map_err(|_| fail(format!("no day {day}")))?;
        Ok(self.study().set_plan_read(&plan, day, passage, read)?)
    }
}
