//! Praying through a block of the rule on Today (Orthodox fork), a step at a time, without
//! leaving Today: Begin opens the block's first item not done, Done ticks it and opens the next
//! below it, and a block of two or more items ends on a short page. The day's readings, a
//! kathisma of the Psalter and a plan's passages open the same way. Steps go by name and nothing
//! counts them. The block's list stays beside the text in a card, or in a strip under the
//! toolbar when the window is narrow. Back always means Today, and the reader is left alone.

use std::collections::{HashMap, HashSet};

use bible_core::calendar::{self, Day};
use bible_core::lectionary;
use bible_core::rule::{Block, Item, Kind as ItemKind, Now, Part, Rule, Season, minutes};
use bible_core::{VerseRef, plans};
use eframe::egui::{self, FontId, Key, Modifiers, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::passage::{Goto, PassageView};
use super::rule::{self, Open, Step, Tick, clock, crumb};
use super::{PageEnv, text};
use crate::actions::Action;
use crate::fathers_panel::{FathersPanel, Outcome};
use crate::theme::{self, Palette};
use crate::widgets::{self, BadgeKind, Button, Kind};

/// The side card's slot: the reader's inspector's width on every step, so nothing jumps.
const CARD_W: f32 = crate::panel::INSPECTOR_WIDTH;
const CARD_GAP: f32 = 40.0;
/// A text column narrower than this beside the card puts the block's list in a strip instead
/// (the month's test on Today).
const NARROW: f32 = 440.0;
/// From the crumb to a step's title.
const TITLE_TOP: f32 = 48.0;

/// What a session walks through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Walk {
    /// A block of the rule, by id.
    Block(String),
    /// The day's readings (the Readings section).
    Readings,
    /// One kathisma (the Psalter section).
    Kathisma(u8),
    /// A plan's day (the Reading plans section).
    Plan(&'static str, usize),
}

/// A button pressed on a step, or its key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    /// Tick it and go on.
    Done,
    /// Go on without ticking.
    Skip,
    /// Go on from a step already done (or one not ticked here).
    Next,
    /// Mark it as not done.
    Undo,
    /// Start the plan it follows, today.
    Start,
    /// Back to Today.
    Back,
}

/// Where Done or Skip goes from a step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Move {
    /// The step with this id.
    To(String),
    /// The same item, on to its plan's next passage for the day.
    Stay,
    /// The block's end page.
    End,
    /// Back to Today.
    Close,
}

/// A block (or readings) begun on Today: open in the page's place, or paused for Continue.
pub struct Session {
    /// The day it ticks, fixed at Begin (a block begun at 23:50 ticks that day).
    pub day: Day,
    pub walk: Walk,
    /// The step open, by id; `None` on the end page.
    pub item: Option<String>,
    /// Steps passed by with Skip, only to work out Continue: never shown or saved.
    pub skipped: HashSet<String>,
    /// The verse each step was left at, for Continue.
    places: HashMap<String, VerseRef>,
    /// The Jesus Prayer's count for each counter, kept for the day.
    pub counts: HashMap<String, u16>,
    /// The Fathers step's panel, on its verse.
    fathers: Option<FathersPanel>,
    /// The step's Scripture, typeset.
    passage: PassageView,
    /// Where to scroll on the next frame: the top, or into the text.
    top: bool,
    goto: Option<Goto>,
    /// A press from outside the footer (the card, a demo step), for the next frame.
    pub press: Option<Press>,
    /// The footer was in view last frame (else the card offers Done).
    footer_seen: bool,
    /// Which of the text's stops (psalms, parts) was in view last frame.
    stop: usize,
    /// The Fathers' verse moved by a key or a button: bring it into view.
    show_focus: bool,
    /// The step open is the Jesus Prayer's counter (Space counts).
    pub counting: bool,
    /// Demo captures: the Fathers' verse to go to, and Open in reader.
    demo_verse: Option<u8>,
    pub demo_reader: bool,
}

impl Session {
    pub fn new(day: Day, walk: Walk, item: Option<String>) -> Self {
        Self {
            day,
            walk,
            item,
            skipped: HashSet::new(),
            places: HashMap::new(),
            counts: HashMap::new(),
            fathers: None,
            passage: PassageView::default(),
            top: true,
            goto: None,
            press: None,
            footer_seen: true,
            stop: 0,
            show_focus: false,
            counting: false,
            demo_verse: None,
            demo_reader: false,
        }
    }

    /// Opens step `id` (or the end page), at the verse it was left at.
    pub fn open(&mut self, id: Option<String>) {
        if id != self.item {
            self.passage = PassageView::default();
            self.fathers = None;
        }
        self.goto = id.as_ref().and_then(|i| self.places.get(i)).copied().map(Goto::Verse);
        self.top = self.goto.is_none();
        self.item = id;
        self.footer_seen = true;
    }

    /// Back on a paused step: where it was left.
    pub fn resume(&mut self) {
        let id = self.item.clone();
        self.goto = id.as_ref().and_then(|i| self.places.get(i)).copied().map(Goto::Verse);
        self.top = self.goto.is_none();
    }

    /// Puts the Fathers' step on verse `n` of its reading's chapter (demo captures).
    pub fn demo_verse(&mut self, n: u8) {
        self.demo_verse = Some(n);
    }

    /// The verse at the top of the step's text (Open in reader starts there), or the Fathers'.
    pub fn verse(&self) -> Option<VerseRef> {
        self.fathers.as_ref().map(|f| f.verse).or_else(|| self.passage.top())
    }

    /// Whether this is a block of `rule` begun on day `d`, and where it is paused: the item.
    pub fn paused_in(&self, d: Day) -> Option<(&str, &str)> {
        match (&self.walk, &self.item) {
            (Walk::Block(b), Some(i)) if self.day == d => Some((b.as_str(), i.as_str())),
            _ => None,
        }
    }
}

/// An item and block that stand for a plan's day outside the rule, so the core works out its
/// passage's part as it does for a rule's item.
fn plan_part(plan: &'static str, d: Day, day: usize, passage: usize) -> Vec<Part> {
    let item = Item {
        id: String::new(),
        title: String::new(),
        kind: ItemKind::Plan { plan: plan.into() },
        note: String::new(),
        season: Season::Always,
        optional: false,
    };
    let block = Block { id: String::new(), title: String::new(), time: None, items: Vec::new() };
    bible_core::rule::parts(&item, &block, d, &[], Some((day, passage)))
}

/// A walk's steps on day `d`, with its name and time ("Evening", "20:30").
pub fn steps(walk: &Walk, d: Day, env: &PageEnv<'_>, rule: &Rule) -> (String, Option<String>, Vec<Step>) {
    let step = |id: String, title: String, detail: String, parts: Vec<Part>| Step {
        id,
        title,
        detail,
        open: Open::Reading(parts),
        done: false,
        optional: false,
        idle: false,
        tick: Tick::Nothing,
        link: None,
        group: None,
    };
    match walk {
        Walk::Block(id) => {
            let mut blocks = rule::day_steps(rule, d, env);
            match blocks.iter().position(|(b, _)| b.id == *id) {
                Some(i) => {
                    let (b, steps) = blocks.swap_remove(i);
                    (b.title.clone(), b.time.as_deref().and_then(minutes).map(clock), steps)
                }
                None => (String::new(), None, Vec::new()),
            }
        }
        Walk::Readings => {
            let lessons = lectionary::readings_in(d, env.settings.calendar, env.settings.usage);
            let steps = lessons.iter().enumerate().map(|(i, r)| {
                let kind = r.kind.label();
                let end = match r.kind {
                    lectionary::Kind::OldTestament => "End of the reading".to_string(),
                    _ => format!("End of the {kind}"),
                };
                let part =
                    Part { label: kind.into(), reference: r.label(), passages: r.passages.to_vec(), end };
                let detail = format!("{kind} · {}", r.service);
                Step { group: Some(r.service), ..step(i.to_string(), r.label(), detail, vec![part]) }
            });
            ("Readings".into(), None, steps.collect())
        }
        Walk::Kathisma(k) => {
            let psalms = calendar::kathisma_psalms(*k);
            let part = Part {
                label: format!("Kathisma {k}"),
                reference: psalms.clone(),
                passages: calendar::kathisma_passages(*k),
                end: format!("End of kathisma {k}"),
            };
            ("Psalter".into(), None, vec![step(k.to_string(), format!("Kathisma {k}"), psalms, vec![part])])
        }
        Walk::Plan(id, day) => {
            let Some(plan) = plans::get(id) else { return (String::new(), None, Vec::new()) };
            let started = env.study.plans();
            let under =
                started.iter().find(|s| s.id == *id).map(|s| super::plans::Under::new(plan, s, env.study));
            let t = super::reading_text(env);
            let passages = plan.day(*day);
            let steps = passages.iter().enumerate().map(|(i, ps)| {
                let title = super::plans::day_label(std::slice::from_ref(ps), t.as_deref());
                let detail = format!("{} · Day {day}", plan.title);
                Step {
                    done: under.as_ref().is_some_and(|u| u.passage_done(*day, i)),
                    tick: Tick::Plan {
                        plan: plan.id,
                        day: *day,
                        passage: i,
                        passages: passages.len(),
                        next: None,
                    },
                    ..step(i.to_string(), title, detail, plan_part(plan.id, d, *day, i))
                }
            });
            (plan.title.to_string(), None, steps.collect())
        }
    }
}

/// Where Done (`ticked`: it ticks `at` now) or Skip goes from step `at` of `walk` on day `d`:
/// the next step below it that isn't done and has something to do, as `Rule::next_item` works
/// it out for a block, never back up the list nor into another block. After a block's last, its
/// end page when it has two or more items that day; otherwise back to Today.
pub fn next(
    rule: &Rule,
    env: &PageEnv<'_>,
    walk: &Walk,
    d: Day,
    steps: &[Step],
    at: &str,
    ticked: bool,
) -> Move {
    let step = steps.iter().find(|s| s.id == at);
    if ticked
        && step.is_some_and(|s| matches!(&s.tick, Tick::Plan { next: Some(_), .. }))
        && walk_is_block(walk)
    {
        return Move::Stay;
    }
    let mut passed = rule::passed(steps);
    if ticked {
        passed.insert(at.to_string());
    }
    let to = match walk {
        Walk::Block(b) => {
            rule.next_item(d, env.settings.calendar, b, Some(at), &passed).map(|i| i.id.clone())
        }
        _ => {
            let from = steps.iter().position(|s| s.id == at).map_or(0, |p| p + 1);
            steps[from..].iter().find(|s| !passed.contains(&s.id)).map(|s| s.id.clone())
        }
    };
    match to {
        Some(id) => Move::To(id),
        None if walk_is_block(walk) && steps.len() >= 2 => Move::End,
        None => Move::Close,
    }
}

fn walk_is_block(walk: &Walk) -> bool {
    matches!(walk, Walk::Block(_))
}

/// The step Begin opens: the first that isn't done and has something to do, passing over those
/// skipped unless only they are left.
pub fn first(
    rule: &Rule,
    env: &PageEnv<'_>,
    walk: &Walk,
    d: Day,
    steps: &[Step],
    skipped: &HashSet<String>,
) -> Option<String> {
    let passed = rule::passed(steps);
    [true, false].into_iter().find_map(|pass_skipped| {
        let mut p = passed.clone();
        if pass_skipped {
            p.extend(skipped.iter().cloned());
        }
        match walk {
            Walk::Block(b) => rule.next_item(d, env.settings.calendar, b, None, &p).map(|i| i.id.clone()),
            _ => steps.iter().find(|s| !p.contains(&s.id)).map(|s| s.id.clone()),
        }
    })
}

/// The step Continue opens: the paused one, or the next not done after it once it has been
/// ticked since, else the first still to do.
pub fn resume_at(rule: &Rule, env: &PageEnv<'_>, s: &Session, steps: &[Step]) -> Option<String> {
    let passed = rule::passed(steps);
    match s.item.as_deref() {
        Some(id) if steps.iter().any(|st| st.id == id) && !passed.contains(id) => Some(id.to_string()),
        Some(id) if steps.iter().any(|st| st.id == id) => {
            match next(rule, env, &s.walk, s.day, steps, id, false) {
                Move::To(n) => Some(n),
                _ => first(rule, env, &s.walk, s.day, steps, &s.skipped),
            }
        }
        _ => first(rule, env, &s.walk, s.day, steps, &s.skipped),
    }
}

/// What Done ticks on day `d`.
fn tick(step: &Step, d: Day, actions: &mut Vec<Action>) {
    match &step.tick {
        Tick::Item => actions.push(Action::SetRuleDone { day: d, item: step.id.clone(), done: true }),
        Tick::Plan { plan, day, passage, .. } => actions.push(Action::SetPlanRead {
            plan: plan.to_string(),
            day: *day as u16,
            passage: *passage as u8,
            read: true,
        }),
        Tick::Nothing => {}
    }
}

/// What the step shell asks of the Today page.
#[derive(Default)]
pub struct Shown {
    pub bottom: f32,
    /// Back to Today.
    pub back: bool,
    /// The end page was reached: the block is finished for today.
    pub ended: bool,
    /// The page scrolled itself (to a verse), so it isn't put at the top.
    pub scrolled: bool,
}

/// What the footer offers.
struct Footer {
    primary: &'static str,
    press: Press,
    /// After the primary button: "Next: The Psalter".
    next: Option<String>,
    secondary: Option<(&'static str, Press)>,
    skip: bool,
    note: Option<&'static str>,
}

/// Draws the step open in `s` in the page's `content` (in the page's scrolling coordinates).
pub fn show(
    s: &mut Session,
    ui: &mut Ui,
    content: Rect,
    env: &PageEnv<'_>,
    rule: &Rule,
    actions: &mut Vec<Action>,
) -> Shown {
    let p = env.settings.theme.palette();
    let today = super::today::today();
    let (name, time, steps) = steps(&s.walk, s.day, env, rule);
    let mut out = Shown::default();
    // An item the rule no longer has on this day (edited here or on the iPhone): the next one.
    if let Some(id) = s.item.clone()
        && !steps.iter().any(|st| st.id == id)
    {
        match next(rule, env, &s.walk, s.day, &steps, &id, false) {
            Move::To(n) => s.open(Some(n)),
            Move::End => s.open(None),
            _ => {
                out.back = true;
                return out;
            }
        }
    }
    // Presses from the keyboard and the card are handled with the footer's.
    let press = s.press.take().or_else(|| {
        let enter = ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter));
        let escape = !ui.ctx().egui_wants_keyboard_input()
            && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        if escape {
            Some(Press::Back)
        } else if enter {
            Some(Press::Done)
        } else {
            None
        }
    });
    let home = if s.day == today { "Today".to_string() } else { super::today::short_date(s.day) };
    let Some(id) = s.item.clone() else {
        // The end page.
        out.ended = true;
        let back = end_page(ui, env, content, (&home, &name), rule, s.day);
        out.bottom = back.0;
        out.back = back.1 || press == Some(Press::Back) || press == Some(Press::Done);
        return out;
    };
    let Some(step) = steps.iter().find(|st| st.id == id).cloned() else { return out };
    s.counting = matches!(step.open, Open::Count(_));
    let mut scroll_to = None;
    let future = s.day > today;
    let counted = matches!(step.open, Open::Count(n) if s.counts.get(&id).is_some_and(|c| *c >= n));
    let done = step.done && !counted;
    let then = next(rule, env, &s.walk, s.day, &steps, &id, true);
    let ticks = !future && step.tick != Tick::Nothing;

    // The footer's buttons.
    let next_name = |m: &Move| match m {
        Move::To(n) => steps.iter().find(|st| st.id == *n).map(|st| st.title.clone()),
        Move::Stay => match &step.tick {
            Tick::Plan { next, .. } => next.clone(),
            _ => None,
        },
        _ => None,
    };
    let footer = if !ticks {
        let last = matches!(then, Move::Close | Move::End);
        let reading = matches!(s.walk, Walk::Readings);
        Footer {
            primary: match (last, reading) {
                (true, _) => "Back to Today",
                (false, true) => "Next reading",
                (false, false) => "Next",
            },
            press: Press::Next,
            next: next_name(&then),
            secondary: None,
            skip: false,
            note: future.then_some("Items are checked off on the day itself."),
        }
    } else if done {
        Footer {
            primary: "Next",
            press: Press::Next,
            next: None,
            secondary: Some(("Mark as not done", Press::Undo)),
            skip: false,
            note: None,
        }
    } else if matches!(step.open, Open::Start(_)) {
        Footer {
            primary: "Start today",
            press: Press::Start,
            next: None,
            secondary: None,
            skip: true,
            note: None,
        }
    } else {
        Footer {
            primary: "Done",
            press: Press::Done,
            next: next_name(&then),
            secondary: None,
            skip: true,
            note: None,
        }
    };
    // ⌘↩ presses the primary button.
    let mut press = press.map(|pr| if pr == Press::Done { footer.press } else { pr });

    // Where things go: the text, and the card or the panel beside it when there's room.
    let typo = &env.settings.typo;
    let measure = (typo.size * typo.measure).floor();
    // Room for verse numbers in the left margin (a verse a line), and on the right for the note
    // markers, between the text and the card.
    let lead = 2.0 * typo.size;
    let markers = crate::reader::layout::MARKER_GAP * typo.size / crate::reader::layout::DESIGN_EM + 64.0;
    let beside = content.width() - CARD_W - lead - markers;
    let side_col = beside.min(measure);
    let room = side_col >= NARROW;
    let listed = steps.len() >= 2;
    let fathers_step = matches!(step.open, Open::Fathers(ref parts) if !parts.is_empty());
    let side = room && (listed || fathers_step);
    let strip = listed && (!room || fathers_step);
    let (x, col) = if side {
        (content.left() + lead + ((beside - side_col) / 2.0).max(0.0), side_col)
    } else {
        let col = (content.width() - 2.0 * lead.max(CARD_GAP)).min(measure).max(200.0);
        (content.center().x - col / 2.0, col)
    };
    let clip = ui.clip_rect();
    let below_toolbar = clip.top() + env.toolbar;

    // The crumb, and Open in reader for a step with text.
    let mut y = content.top();
    let path = [name.as_str()];
    if crumb(ui, p, (content.left(), y, content.width() - 160.0), &home, &path) {
        press = Some(Press::Back);
    }
    let text_parts = match &step.open {
        Open::Reading(parts) | Open::Fathers(parts) if !parts.is_empty() => Some(parts.clone()),
        _ => None,
    };
    let open_reader = text_parts.is_some()
        && (std::mem::take(&mut s.demo_reader)
            || ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::O))
            || {
                let b = Button::new(Kind::Ghost, "Open in reader ⌘O").small();
                let at = content.right() - 12.0 + b.rest_inset();
                b.show_right(ui, p, at, y + 8.0, ui.id().with("open-reader")).1.clicked()
            });
    if open_reader && let (Some(parts), Some(at)) = (&text_parts, s.verse()) {
        s.places.insert(id.clone(), at);
        let passages = parts.iter().flat_map(|p| p.passages.iter().copied()).collect();
        actions.push(Action::OpenInReader { passages, at, from: format!("{name} › {}", step.title) });
    }
    y += 28.0;
    // The strip's place, under the crumb.
    let strip_top = y;
    let strip_h = if strip { strip_height(ui, p, &steps, &id, content.width()) } else { 0.0 };
    if strip {
        y += strip_h + 8.0;
    }

    // The title, the line under it, and the step's state.
    y += TITLE_TOP - 28.0;
    let painter = ui.painter().clone();
    let heading = FontId::new(28.0, theme::family(theme::SERIF_HEADING));
    let lines = widgets::flow(&painter, &step.title, &heading, 28.0, col, 4);
    let mut tail = x;
    for mut runs in lines {
        runs.iter_mut().for_each(|r| r.tracking -= 0.01);
        widgets::paint_runs(&painter, x, widgets::css_baseline(y, 36.0, 28.0, true), &runs, p.ink);
        tail = x + widgets::runs_width(&painter, &runs);
        y += 36.0;
    }
    if step.optional {
        widgets::badge(&painter, p, tail + 12.0, y - 18.0, BadgeKind::Neutral, "Optional");
    }
    // "Optional" shows as the tag by the title.
    let mut detail = step.detail.trim_end_matches("Optional").trim_end_matches(" · ").to_string();
    let stops = s.passage.stops();
    if strip && !stops.is_empty() {
        // The psalm or chapter in view, since no card shows it.
        let stop = stops.get(s.stop).cloned().unwrap_or_default();
        let stop = if stop.chars().all(|c| c.is_ascii_digit()) { psalm_name(&step, &stop) } else { stop };
        detail = format!("{detail} · {stop}");
    }
    if !detail.is_empty() {
        y += 2.0
            + super::today::paragraph(
                &painter,
                &detail,
                &theme::ui_font(13.0),
                20.0,
                x,
                y + 2.0,
                col,
                p.ink_2,
            );
    }
    if done {
        text(&painter, x, y + 6.0, 20.0, "Done today ✓", theme::ui_semibold(13.0), p.accent, false);
        y += 26.0;
    }
    // "Go to the Gospel" over the Epistle.
    let gospel = text_parts.as_ref().and_then(|parts| {
        (parts.len() > 1).then(|| parts.iter().position(|p| p.label == "Gospel")).flatten()
    });
    let mut goto = s.goto.take();
    if let (Some(i), false) = (gospel, fathers_step) {
        let g = painter.layout_no_wrap("Go to the Gospel ↓".into(), theme::ui_medium(13.0), p.accent);
        let r = Rect::from_min_size(pos2(x, y + 8.0), g.size());
        let resp = ui.interact(r.expand(4.0), ui.id().with("to-gospel"), Sense::click());
        painter.galley(r.min, g, p.accent);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            goto = Some(Goto::Part(i));
        }
        y += 28.0;
    }
    y += 24.0;

    // The step's body.
    let view_top = below_toolbar + if strip { strip_h + 8.0 } else { 0.0 };
    let mut panel_slot: Option<Rect> = None;
    match &step.open {
        Open::Nothing => {
            let line = if matches!(rule_kind(rule, &s.walk, &id), Some(ItemKind::Prayers { .. })) {
                "Pray from your prayer book. When you've finished, press Done."
            } else {
                "When you've finished, press Done."
            };
            y += super::today::paragraph(
                &painter,
                line,
                &theme::ui_font(15.0),
                24.0,
                x,
                y,
                col.min(560.0),
                p.ink_2,
            );
        }
        Open::Text { text, source } => y = rule::reading(ui, env, x, y, col.min(640.0), text, source),
        Open::Count(target) => {
            let n = s.counts.entry(id.clone()).or_insert(0);
            y = rule::counter(ui, env, (x + col / 2.0, y), col, (s.day, &id), (*target).max(1), n, actions);
        }
        Open::Start(plan) => {
            let title = plans::get(plan).map_or("this plan", |p| p.title);
            let line = format!("You haven't started {title}.");
            text(&painter, x, y, 28.0, &line, FontId::new(20.0, theme::family(theme::SERIF)), p.ink, false);
            y += 28.0;
        }
        Open::Reading(parts) | Open::Fathers(parts) if parts.is_empty() => {
            text(
                &painter,
                x,
                y,
                24.0,
                "No readings are listed for this day.",
                theme::ui_font(14.0),
                p.ink_2,
                false,
            );
            y += 24.0;
        }
        Open::Reading(parts) => {
            let shown = s.passage.show(ui, env, parts, (x, y, col), view_top, None, false, goto.take());
            scroll_to = shown.scroll_to;
            s.stop = shown.stop;
            if let Some(v) = shown.top {
                s.places.insert(id.clone(), v);
            }
            y = shown.bottom;
        }
        Open::Fathers(parts) => {
            (y, scroll_to) =
                fathers_body(s, ui, env, parts, (x, y, col), (view_top, side), goto.take(), actions);
            let slot = Rect::from_min_max(pos2(content.right() - CARD_W, y), pos2(content.right(), y));
            panel_slot = side.then_some(slot);
        }
        Open::NotToday => {}
    }

    // The footer: Done, what comes next, Skip.
    y += 40.0;
    let footer_rect = Rect::from_min_size(pos2(x, y), vec2(col, 32.0));
    s.footer_seen = footer_rect.bottom() > view_top && footer_rect.top() < clip.bottom();
    if let Some(note) = footer.note {
        text(&painter, x, y - 4.0, 20.0, note, theme::ui_font(13.0), p.ink_3, false);
        y += 28.0;
    }
    let cy = y + 16.0;
    let (r, resp) = done_button(ui, p, x, cy, footer.primary, ui.id().with("step-primary"));
    if resp.clicked() {
        press = Some(footer.press);
    }
    let mut after = r.right() + 16.0;
    if let Some((label, pr)) = footer.secondary {
        let b = Button::new(Kind::Plain, label);
        let (r2, resp) = b.show_left(ui, p, r.right() + 8.0, cy, ui.id().with("step-secondary"));
        if resp.clicked() {
            press = Some(pr);
        }
        after = r2.right() + 16.0;
    }
    if footer.skip {
        let b = Button::new(Kind::Ghost, "Skip");
        let (_, resp) = b.show_right(ui, p, x + col + b.rest_inset(), cy, ui.id().with("step-skip"));
        if resp.clicked() {
            press = Some(Press::Skip);
        }
    }
    if let Some(next) = &footer.next {
        let label = if matches!(s.walk, Walk::Readings) { next.clone() } else { format!("Next: {next}") };
        let room = x + col - after - if footer.skip { 60.0 } else { 0.0 };
        let label = super::elide(&painter, &label, &theme::ui_font(13.0), room);
        text(&painter, after, cy - 10.0, 20.0, &label, theme::ui_font(13.0), p.ink_3, false);
    }
    y += 32.0;
    out.bottom = y;

    // The card or the Fathers' panel beside the text, and the strip, in view while the text
    // scrolls; drawn last, so they take the clicks.
    let offer_done = !s.footer_seen && ticks && !done;
    let mut picked = None;
    if let Some(slot) = panel_slot
        && let Some(panel) = s.fathers.as_mut()
    {
        let top = (content.top() + 28.0 + strip_h + 8.0 + TITLE_TOP - 28.0).max(view_top + 16.0);
        let r = Rect::from_min_max(pos2(slot.left(), top), pos2(slot.right(), clip.bottom() - 16.0));
        fathers_panel(ui, env, p, panel, r, actions);
    } else if side {
        let top = (content.top() + TITLE_TOP).max(below_toolbar + 16.0);
        let card = Rect::from_min_max(
            pos2(content.right() - CARD_W, top),
            pos2(content.right(), clip.bottom() - 16.0),
        );
        let c = card_list(ui, env, card, (&name, time.as_deref()), &steps, &id, &stops, s.stop, offer_done);
        picked = c.0;
        if c.1 {
            press = Some(Press::Done);
        }
        if c.2 {
            press = Some(Press::Back);
        }
    }
    if strip {
        let top = strip_top.max(below_toolbar);
        let r = Rect::from_min_size(pos2(content.left(), top), vec2(content.width(), strip_h));
        let st = strip_list(ui, p, r, &steps, &id, offer_done, (strip_top - top).abs() > 0.5);
        picked = st.0.or(picked);
        if st.1 {
            press = Some(Press::Done);
        }
    }
    if let Some(n) = picked.filter(|n| *n != id) {
        s.open(Some(n));
    }

    // What the press does.
    match press {
        Some(Press::Back) => out.back = true,
        Some(Press::Done) => {
            if ticks {
                tick(&step, s.day, actions);
            }
            go(s, then, &mut out);
        }
        Some(Press::Next) => go(s, next(rule, env, &s.walk, s.day, &steps, &id, false), &mut out),
        Some(Press::Skip) => {
            s.skipped.insert(id.clone());
            go(s, next(rule, env, &s.walk, s.day, &steps, &id, false), &mut out);
        }
        Some(Press::Undo) => rule::toggle(&step, s.day, actions),
        // Then the step shows the plan's first day in place.
        Some(Press::Start) => {
            if let Open::Start(plan) = step.open {
                actions.push(Action::StartPlan(plan.to_string()));
            }
        }
        None => {}
    }
    // Scrolling last: the card's and the panel's own scroll areas would take the request.
    let top = s.top.then(|| content.top() - env.toolbar - super::TOP);
    s.top = false;
    if let Some(y) = top.or(scroll_to) {
        let at = Rect::from_min_size(pos2(content.left(), y), vec2(1.0, 1.0));
        ui.scroll_to_rect_animation(at, Some(egui::Align::TOP), egui::style::ScrollAnimation::none());
        ui.ctx().request_repaint();
        out.scrolled = true;
    }
    if goto.is_some() {
        // A verse not laid out yet this frame: try again on the next.
        s.goto = goto;
    }
    out
}

/// The Fathers' step: the reading with the verse the panel is on tinted, the panel beside it
/// (`side`), or in a narrow window that verse alone under ‹ ›, with the panel under it. Returns
/// the bottom and where to scroll.
#[allow(clippy::too_many_arguments)]
fn fathers_body(
    s: &mut Session,
    ui: &mut Ui,
    env: &PageEnv<'_>,
    parts: &[Part],
    (x, mut y, col): (f32, f32, f32),
    (view_top, side): (f32, bool),
    goto: Option<Goto>,
    actions: &mut Vec<Action>,
) -> (f32, Option<f32>) {
    let p = env.settings.theme.palette();
    let painter = ui.painter().clone();
    let clip = ui.clip_rect();
    let (mut scroll_to, mut goto) = (None, goto);
    let verses: Vec<VerseRef> =
        parts.iter().flat_map(|p| p.passages.iter().flat_map(|q| q.verses())).collect();
    let panel = s.fathers.get_or_insert_with(|| {
        let mut f = FathersPanel::new(verses.first().copied().unwrap_or(parts[0].passages[0].first()));
        f.step = true;
        f
    });
    if let Some(n) = s.demo_verse.take() {
        panel.verse = VerseRef::new(panel.verse.book, panel.verse.chapter, n);
        s.show_focus = true;
    }
    if !verses.contains(&panel.verse) {
        panel.verse = verses[0];
    }
    // J and K, as ‹ and ›.
    let typing = ui.ctx().egui_wants_keyboard_input();
    let step_by = ui.input_mut(|i| {
        if typing {
            0
        } else if i.consume_key(Modifiers::NONE, Key::J) {
            1
        } else if i.consume_key(Modifiers::NONE, Key::K) {
            -1
        } else {
            0
        }
    });
    move_verse(panel, &verses, step_by, &mut s.show_focus);
    let focus = panel.verse;
    if side {
        panel.within = verses.clone();
        let shown_goto = goto.take().or(s.show_focus.then_some(Goto::Show(focus)));
        s.show_focus = false;
        let shown = s.passage.show(ui, env, parts, (x, y, col), view_top, Some(focus), true, shown_goto);
        scroll_to = shown.scroll_to;
        if let Some(v) = shown.clicked.filter(|v| verses.contains(v)) {
            panel.verse = v;
        }
        y = shown.bottom;
    } else {
        // Narrow: the verse alone, ‹ › above it, and the panel under it.
        panel.within.clear();
        let at = panel.verse;
        let prev = Rect::from_min_size(pos2(x, y), vec2(28.0, 28.0));
        let i = verses.iter().position(|v| *v == at).unwrap_or(0);
        if i > 0
            && widgets::round_button(ui, p, prev, crate::icons::chevron_left, ui.id().with("prev-verse"))
                .clicked()
        {
            move_verse(panel, &verses, -1, &mut s.show_focus);
        }
        let label = at.to_string();
        let w =
            text(&painter, prev.right() + 8.0, y + 4.0, 20.0, &label, theme::ui_semibold(13.0), p.ink, false);
        let next_r = Rect::from_min_size(pos2(prev.right() + 16.0 + w, y), vec2(28.0, 28.0));
        if i + 1 < verses.len()
            && widgets::round_button(ui, p, next_r, crate::icons::chevron_right, ui.id().with("next-verse"))
                .clicked()
        {
            move_verse(panel, &verses, 1, &mut s.show_focus);
        }
        y += 40.0;
        let one = Part {
            label: String::new(),
            reference: String::new(),
            passages: vec![lectionary::Passage {
                book: at.book,
                from: (at.chapter, at.verse),
                to: (at.chapter, at.verse),
            }],
            end: String::new(),
        };
        let shown =
            s.passage.show(ui, env, std::slice::from_ref(&one), (x, y, col), view_top, None, false, None);
        y = shown.bottom + 24.0;
        let h = (clip.bottom() - view_top - 120.0).max(360.0);
        let r = Rect::from_min_size(pos2(x, y), vec2(col, h));
        fathers_panel(ui, env, p, panel, r, actions);
        y = r.bottom() + 8.0;
    }
    (y, scroll_to)
}

/// "Psalm 87", from a stop's number.
fn psalm_name(step: &Step, n: &str) -> String {
    match &step.open {
        Open::Reading(parts)
            if parts
                .iter()
                .any(|p| p.passages.first().is_some_and(|q| q.book == bible_core::BookId::PSALMS)) =>
        {
            format!("Psalm {n}")
        }
        _ => n.to_string(),
    }
}

/// The kind of the rule's item `id`, for a block's walk.
fn rule_kind<'r>(rule: &'r Rule, walk: &Walk, id: &str) -> Option<&'r ItemKind> {
    let Walk::Block(b) = walk else { return None };
    rule.blocks.iter().find(|bl| bl.id == *b)?.items.iter().find(|i| i.id == id).map(|i| &i.kind)
}

/// Moves the Fathers' verse by `by` within `verses`.
fn move_verse(panel: &mut FathersPanel, verses: &[VerseRef], by: i32, show: &mut bool) {
    if by == 0 {
        return;
    }
    let i = verses.iter().position(|v| *v == panel.verse).unwrap_or(0) as i32 + by;
    if let Some(v) = usize::try_from(i).ok().and_then(|i| verses.get(i)) {
        panel.verse = *v;
        *show = true;
    }
}

/// Goes where `m` says.
fn go(s: &mut Session, m: Move, out: &mut Shown) {
    match m {
        Move::To(n) => s.open(Some(n)),
        Move::Stay => {
            let id = s.item.clone();
            if let Some(id) = &id {
                s.places.remove(id);
            }
            s.passage = PassageView::default();
            s.open(id);
            s.top = true;
        }
        Move::End => s.open(None),
        Move::Close => {
            s.open(None);
            out.back = true;
            out.ended = true;
        }
    }
}

/// The Church Fathers' panel in `r`, as beside the reader.
fn fathers_panel(
    ui: &mut Ui,
    env: &PageEnv<'_>,
    p: &Palette,
    panel: &mut FathersPanel,
    r: Rect,
    actions: &mut Vec<Action>,
) {
    p.shadows.panel.paint(ui.painter(), r, theme::radius::LG);
    ui.painter().rect_filled(r, theme::radius::LG, p.sidebar);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r).id_salt("step-fathers"));
    child.set_clip_rect(r.intersect(ui.clip_rect()));
    match panel.show(&mut child, p, env.fathers) {
        Outcome::Stay | Outcome::Close => {}
        Outcome::Step(by) => {
            let within = panel.within.clone();
            let mut show = false;
            move_verse(panel, &within, by, &mut show);
        }
        Outcome::AddToNote(note) => actions.push(Action::AddToNote(panel.verse, note)),
        Outcome::Install(path) => actions.push(Action::InstallFathers(path)),
    }
}

/// The width of [`done_button`] for `label`.
fn done_width(painter: &egui::Painter, label: &str) -> f32 {
    let w = |s: &str, font| painter.layout_no_wrap(s.into(), font, egui::Color32::PLACEHOLDER).size().x;
    (14.0 + w(label, theme::ui_semibold(theme::text::UI.size)) + 8.0 + w("⌘↩", theme::ui_medium(12.0)) + 14.0)
        .round()
}

/// The primary button: `label` with ⌘↩ after it.
fn done_button(
    ui: &mut Ui,
    p: &Palette,
    left: f32,
    cy: f32,
    label: &str,
    id: egui::Id,
) -> (Rect, egui::Response) {
    let painter = ui.painter().clone();
    let font = theme::ui_semibold(theme::text::UI.size);
    let g = painter.layout_no_wrap(label.into(), font, p.on_accent);
    let k = painter.layout_no_wrap("⌘↩".into(), theme::ui_medium(12.0), p.on_accent.gamma_multiply(0.7));
    let w = done_width(&painter, label);
    let rect = Rect::from_min_size(pos2(left, cy - 16.0), vec2(w, 32.0));
    let resp = Button::new(Kind::Primary, "").show(ui, p, rect, id);
    painter.galley(pos2(rect.left() + 14.0, cy - g.size().y / 2.0), g.clone(), p.on_accent);
    painter.galley(pos2(rect.left() + 14.0 + g.size().x + 8.0, cy - k.size().y / 2.0), k, p.on_accent);
    (rect, resp)
}

/// How tall the strip of the block's names is, `width` wide.
fn strip_height(ui: &Ui, p: &Palette, steps: &[Step], at: &str, width: f32) -> f32 {
    let rows = strip_rows(ui, p, steps, at, width - 24.0 - 96.0);
    16.0 + rows.iter().map(|r| r.0).max().map_or(1, |n| n + 1) as f32 * 22.0
}

/// The strip's names placed in rows: (row, x, step index, galley).
fn strip_rows(
    ui: &Ui,
    p: &Palette,
    steps: &[Step],
    at: &str,
    width: f32,
) -> Vec<(usize, f32, usize, std::sync::Arc<egui::Galley>)> {
    let painter = ui.painter();
    let sep = painter.layout_no_wrap(" · ".into(), theme::ui_font(13.0), p.ink_3).size().x;
    let (mut row, mut x) = (0, 0.0);
    let mut out = Vec::new();
    for (i, st) in steps.iter().enumerate() {
        let current = st.id == at;
        let (label, font, ink) = if current {
            (st.title.clone(), theme::ui_semibold(13.0), p.ink)
        } else if st.done {
            (format!("✓ {}", st.title), theme::ui_font(13.0), p.ink_3)
        } else if st.idle {
            (st.title.clone(), theme::ui_font(13.0), p.ink_3.gamma_multiply(0.7))
        } else {
            (st.title.clone(), theme::ui_font(13.0), p.ink_2)
        };
        let g = painter.layout_no_wrap(label, font, ink);
        let w = g.size().x;
        if i > 0 && x + sep + w > width {
            row += 1;
            x = 0.0;
        } else if i > 0 {
            x += sep;
        }
        out.push((row, x, i, g));
        x += w;
    }
    out
}

/// The block's names in a strip at `r`: done ones with ✓, the current one in ink, and Done ⌘↩
/// at the end while the footer is out of view. Returns the step clicked and whether Done was.
fn strip_list(
    ui: &mut Ui,
    p: &Palette,
    r: Rect,
    steps: &[Step],
    at: &str,
    offer_done: bool,
    stuck: bool,
) -> (Option<String>, bool) {
    let painter = ui.painter().clone();
    if stuck {
        painter.rect_filled(r.expand2(vec2(super::SIDE, 0.0)), 0.0, p.paper);
    }
    painter.hline(r.left()..=r.right(), r.bottom() - 0.5, Stroke::new(1.0, p.separator));
    let sep = painter.layout_no_wrap(" · ".into(), theme::ui_font(13.0), p.ink_3);
    let mut picked = None;
    for (row, x, i, g) in strip_rows(ui, p, steps, at, r.width() - 24.0 - 96.0) {
        let at_pos = pos2(r.left() + x, r.top() + 8.0 + row as f32 * 22.0);
        if x > 0.0 {
            painter.galley(pos2(at_pos.x - sep.size().x, at_pos.y), sep.clone(), p.ink_3);
        }
        let hit = Rect::from_min_size(at_pos, g.size());
        let st = &steps[i];
        if !st.idle {
            let resp = ui.interact(hit, ui.id().with(("strip", i)), Sense::click());
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                picked = Some(st.id.clone());
            }
        }
        painter.galley(at_pos, g, p.ink);
    }
    let mut done = false;
    if offer_done {
        let b = Button::new(Kind::Primary, "Done ⌘↩").small();
        done = b.show_right(ui, p, r.right(), r.bottom() - 19.0, ui.id().with("strip-done")).1.clicked();
    }
    (picked, done)
}

/// The block's card at `card` (its height capped there): its name and time, a row for each
/// step (done with ✓, the current one marked, with the stops of its text and, while the footer
/// is out of view, Done ⌘↩), then Back to Today. The day's readings are grouped by service.
/// Returns the step clicked, whether Done was, and whether Back to Today was.
#[allow(clippy::too_many_arguments)]
fn card_list(
    ui: &mut Ui,
    env: &PageEnv<'_>,
    card: Rect,
    (name, time): (&str, Option<&str>),
    steps: &[Step],
    at: &str,
    stops: &[String],
    stop: usize,
    offer_done: bool,
) -> (Option<String>, bool, bool) {
    let p = env.settings.theme.palette();
    let painter = ui.painter().clone();
    let w = card.width();
    // The rows, measured first: (step index or a group's name, height).
    let mut rows: Vec<(Result<usize, &str>, f32)> = Vec::new();
    let mut group = None;
    for (i, st) in steps.iter().enumerate() {
        if st.group.is_some() && st.group != group {
            group = st.group;
            rows.push((Err(st.group.unwrap_or_default()), 28.0));
        }
        let current = st.id == at;
        let extra = match current {
            true => (if stops.is_empty() { 0.0 } else { 20.0 }) + if offer_done { 32.0 } else { 0.0 },
            false => 0.0,
        };
        let idle = if st.idle { 18.0 } else { 0.0 };
        rows.push((Ok(i), 36.0 + extra + idle));
    }
    let head = 52.0;
    let foot = 1.0 + 44.0;
    let body_h: f32 = rows.iter().map(|r| r.1).sum();
    let h = (head + body_h + foot + 8.0).min(card.height());
    let rect = Rect::from_min_size(card.min, vec2(w, h));
    p.shadows.panel.paint(&painter, rect, 14.0);
    painter.add(egui::epaint::RectShape::new(
        rect,
        14.0,
        p.surface,
        Stroke::new(0.5, p.border),
        StrokeKind::Outside,
    ));
    let title = match time {
        Some(t) => format!("{name} · {t}"),
        None => name.to_string(),
    };
    let title = super::elide(&painter, &title, &theme::ui_semibold(15.0), w - 32.0);
    text(
        &painter,
        rect.left() + 16.0,
        rect.top() + 16.0,
        24.0,
        &title,
        theme::ui_semibold(15.0),
        p.ink,
        false,
    );
    let (mut picked, mut done) = (None, false);
    let body =
        Rect::from_min_max(pos2(rect.left(), rect.top() + head), pos2(rect.right(), rect.bottom() - foot));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body).id_salt("step-card"));
    child.set_clip_rect(body.intersect(ui.clip_rect()));
    egui::ScrollArea::vertical()
        .id_salt("step-card-rows")
        .auto_shrink([false, true])
        .max_height(body.height())
        .show(&mut child, |ui| {
            let top = ui.max_rect().top();
            let painter = ui.painter().clone();
            let mut y = top;
            for (row, rh) in &rows {
                let r = Rect::from_min_size(pos2(body.left() + 8.0, y), vec2(w - 16.0, *rh));
                y += rh;
                let i = match row {
                    Err(g) => {
                        text(
                            &painter,
                            r.left() + 8.0,
                            r.top() + 8.0,
                            16.0,
                            g,
                            theme::ui_semibold(12.0),
                            p.ink_2,
                            false,
                        );
                        continue;
                    }
                    Ok(i) => *i,
                };
                let st = &steps[i];
                let current = st.id == at;
                let resp = ui.interact(
                    r,
                    ui.id().with(("card-step", i)),
                    if st.idle || current { Sense::hover() } else { Sense::click() },
                );
                if current {
                    painter.rect_filled(r, 8.0, p.accent_soft);
                    painter.rect_filled(
                        Rect::from_min_size(r.min + vec2(0.0, 6.0), vec2(3.0, 24.0)),
                        1.5,
                        p.accent,
                    );
                } else if let Some(fill) = widgets::hover_fill(ui, &resp, p, false).filter(|_| !st.idle) {
                    painter.rect_filled(r, 8.0, fill);
                }
                let (label, font, ink) = if st.done {
                    (format!("✓ {}", st.title), theme::ui_medium(14.0), p.ink_3)
                } else if st.idle {
                    (st.title.clone(), theme::ui_medium(14.0), p.ink_3.gamma_multiply(0.7))
                } else {
                    (
                        st.title.clone(),
                        if current { theme::ui_semibold(14.0) } else { theme::ui_medium(14.0) },
                        p.ink,
                    )
                };
                let label = super::elide(&painter, &label, &font, r.width() - 24.0);
                text(&painter, r.left() + 12.0, r.top() + 8.0, 20.0, &label, font, ink, false);
                let mut ly = r.top() + 30.0;
                if st.idle {
                    let why = super::elide(&painter, &st.detail, &theme::ui_font(12.0), r.width() - 24.0);
                    text(
                        &painter,
                        r.left() + 12.0,
                        ly - 2.0,
                        18.0,
                        &why,
                        theme::ui_font(12.0),
                        p.ink_3,
                        false,
                    );
                }
                if current && !stops.is_empty() {
                    // The stops, the one in view in ink: names, not a count.
                    let mut x = r.left() + 12.0;
                    for (k, name) in stops.iter().enumerate() {
                        if k > 0 {
                            x += text(&painter, x, ly, 18.0, " · ", theme::ui_font(12.0), p.ink_3, false);
                        }
                        let ink = if k == stop { p.ink } else { p.ink_3 };
                        if x > r.right() - 24.0 {
                            break;
                        }
                        x += text(&painter, x, ly, 18.0, name, theme::ui_font(12.0), ink, false);
                    }
                    ly += 20.0;
                }
                if current && offer_done {
                    let b = Button::new(Kind::Primary, "Done ⌘↩").small();
                    done |=
                        b.show_left(ui, p, r.left() + 12.0, ly + 14.0, ui.id().with("card-done")).1.clicked();
                }
                if resp
                    .on_hover_cursor(if st.idle || current {
                        egui::CursorIcon::Default
                    } else {
                        egui::CursorIcon::PointingHand
                    })
                    .clicked()
                {
                    picked = Some(st.id.clone());
                }
            }
            ui.allocate_rect(
                Rect::from_min_max(pos2(body.left(), top), pos2(body.right(), y)),
                Sense::hover(),
            );
        });
    // Back to Today, under a hairline.
    let fy = rect.bottom() - foot;
    painter.hline(rect.left() + 16.0..=rect.right() - 16.0, fy, Stroke::new(1.0, p.separator));
    let back = Rect::from_min_size(pos2(rect.left() + 8.0, fy + 6.0), vec2(w - 16.0, 32.0));
    let resp = ui.interact(back, ui.id().with("card-back"), Sense::click());
    if let Some(fill) = widgets::hover_fill(ui, &resp, p, false) {
        painter.rect_filled(back, 8.0, fill);
    }
    text(
        &painter,
        back.left() + 12.0,
        back.top() + 6.0,
        20.0,
        "Back to Today",
        theme::ui_medium(13.0),
        p.accent,
        false,
    );
    let back = resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
    (picked, done, back)
}

/// The end of a block: the cross, its name, "Glory to God for all things.", what comes next in
/// the rule, and Back to Today. Returns the bottom and whether Back to Today was pressed.
fn end_page(
    ui: &mut Ui,
    env: &PageEnv<'_>,
    content: Rect,
    (home, name): (&str, &str),
    rule: &Rule,
    d: Day,
) -> (f32, bool) {
    let p = env.settings.theme.palette();
    let painter = ui.painter().clone();
    let back = crumb(ui, p, (content.left(), content.top(), content.width()), home, &[name]);
    let cx = content.center().x;
    let mut y = content.top() + 96.0;
    let big = |painter: &egui::Painter, c: egui::Pos2| {
        let s = Stroke::new(1.75, p.rubric);
        painter.line_segment([c - vec2(0.0, 12.0), c + vec2(0.0, 12.0)], s);
        painter.line_segment([c + vec2(-8.0, -4.5), c + vec2(8.0, -4.5)], s);
    };
    big(&painter, pos2(cx, y + 12.0));
    y += 48.0;
    let centred = |s: &str, font: FontId, lh: f32, ink, y: f32| {
        let g = painter.layout_no_wrap(s.to_string(), font.clone(), ink);
        let w = g.size().x;
        text(&painter, cx - w / 2.0, y, lh, s, font, ink, false);
    };
    centred(name, FontId::new(28.0, theme::family(theme::SERIF_HEADING)), 36.0, p.ink, y);
    y += 44.0;
    centred("Glory to God for all things.", FontId::new(20.0, theme::family(theme::SERIF)), 28.0, p.ink_2, y);
    y += 56.0;
    if let Some(line) = next_in_rule(env, rule, d) {
        centred(&line, theme::ui_font(13.0), 20.0, p.ink_2, y);
        y += 44.0;
    }
    let w = done_width(&painter, "Back to Today");
    let (_, resp) = done_button(ui, p, cx - w / 2.0, y + 16.0, "Back to Today", ui.id().with("end-today"));
    (y + 32.0, back || resp.clicked())
}

/// What comes next in the rule after a block ended on day `d`: "Next in your rule: Midday at
/// 12:00" later today, else "Tomorrow: Morning at 5:00". Nothing on another day.
fn next_in_rule(env: &PageEnv<'_>, rule: &Rule, d: Day) -> Option<String> {
    let today = super::today::today();
    if d != today {
        return None;
    }
    let t = crate::time::local(bible_core::study::now());
    let minute = i64::from(t.hour) * 60 + i64::from(t.minute);
    let style = env.settings.calendar;
    let later = rule
        .for_day(d, style)
        .into_iter()
        .filter_map(|(b, _)| Some((b, b.time.as_deref().and_then(minutes)?)))
        .filter(|(_, at)| *at > minute)
        .min_by_key(|(_, at)| *at);
    if let Some((b, at)) = later {
        return Some(format!("Next in your rule: {} at {}", b.title, clock(at)));
    }
    let first = rule
        .for_day(d + 1, style)
        .into_iter()
        .filter_map(|(b, _)| Some((b, b.time.as_deref().and_then(minutes)?)))
        .min_by_key(|(_, at)| *at)?;
    Some(format!("Tomorrow: {} at {}", first.0.title, clock(first.1)))
}

/// The Now card's block on day `d` at `minute`: the one due, or the next later today, passing
/// over blocks `ended` today.
pub fn now<'r>(
    rule: &'r Rule,
    env: &PageEnv<'_>,
    d: Day,
    minute: i64,
    ended: &HashSet<String>,
) -> Option<(Now<'r>, Vec<Step>)> {
    let blocks = rule::day_steps(rule, d, env);
    let passed: HashSet<String> = blocks.iter().flat_map(|(_, steps)| rule::passed(steps)).collect();
    let n = rule.now(d, env.settings.calendar, minute, &passed, ended)?;
    let id = match n {
        Now::Due(b) | Now::Next(b) => &b.id,
    };
    let steps = blocks.into_iter().find(|(b, _)| b.id == *id)?.1;
    Some((n, steps))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bible_core::library::Library;
    use bible_core::study::StudyStore;

    /// The starter rule's steps on Wednesday 21 October 2026, with Evening Prayers and Psalm 90
    /// ticked, and a check of where Done and Skip go from them.
    #[test]
    fn done_and_skip_go_down_the_block_and_end_it() {
        let dir = std::env::temp_dir().join(format!("bible-rs-steps-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let library = Library::open(&dir, std::sync::Arc::new(|| {}));
        let mut study = StudyStore::open_in_memory().unwrap();
        let d = calendar::civil(2026, 10, 21);
        study.set_rule_done(d, "evening-prayers", true).unwrap();
        let settings = crate::app::Settings::default();
        let env = PageEnv {
            library: &library,
            study: &study,
            settings: &settings,
            query: "",
            toolbar: 52.0,
            fathers: None,
        };
        let rule = study.rule();
        let block = |id: &str| Walk::Block(id.to_string());
        let ids = |w: &Walk| steps(w, d, &env, &rule).2.into_iter().map(|s| s.id).collect::<Vec<_>>();
        let evening = block("evening");
        let st = steps(&evening, d, &env, &rule).2;
        assert_eq!(
            ids(&evening),
            ["evening-prayers", "evening-kathisma", "evening-fathers", "evening-reflection"]
        );
        let to = |id: &str| Move::To(id.to_string());

        // Begin opens the first item not done; Done ticks and goes to the next below.
        assert_eq!(first(&rule, &env, &evening, d, &st, &HashSet::new()), Some("evening-kathisma".into()));
        assert_eq!(next(&rule, &env, &evening, d, &st, "evening-kathisma", true), to("evening-fathers"));
        // Skip goes on the same way; Begin then passes over what was skipped, unless only it is left.
        let skipped: HashSet<String> = ["evening-kathisma".to_string()].into();
        assert_eq!(next(&rule, &env, &evening, d, &st, "evening-kathisma", false), to("evening-fathers"));
        assert_eq!(first(&rule, &env, &evening, d, &st, &skipped), Some("evening-fathers".into()));
        let all: HashSet<String> =
            ["evening-kathisma", "evening-fathers", "evening-reflection"].map(String::from).into();
        assert_eq!(first(&rule, &env, &evening, d, &st, &all), Some("evening-kathisma".into()));
        // Never back up the list: after the last, the end page of a block of two or more items.
        assert_eq!(next(&rule, &env, &evening, d, &st, "evening-fathers", true), to("evening-reflection"));
        assert_eq!(next(&rule, &env, &evening, d, &st, "evening-reflection", true), Move::End);
        // A step done already goes on with Next, its tick left as it is.
        assert_eq!(next(&rule, &env, &evening, d, &st, "evening-prayers", false), to("evening-kathisma"));
        // An item no longer in the rule: the next to do, from the top.
        assert_eq!(next(&rule, &env, &evening, d, &st, "gone", false), to("evening-kathisma"));
        // Optional items are offered too; a block of one item goes straight back to Today.
        let midday = block("midday");
        let st = steps(&midday, d, &env, &rule).2;
        assert_eq!(next(&rule, &env, &midday, d, &st, "midday-psalm", true), to("midday-jesus-prayer"));
        let meals = block("meals");
        let st = steps(&meals, d, &env, &rule).2;
        assert_eq!(next(&rule, &env, &meals, d, &st, "meal-prayer", true), Move::Close);

        // The day is fixed at Begin: Done ticks that day, whatever day it is now.
        let st = steps(&evening, d, &env, &rule).2;
        let mut actions = Vec::new();
        tick(&st[1], d, &mut actions);
        assert_eq!(actions, [Action::SetRuleDone { day: d, item: "evening-kathisma".into(), done: true }]);

        // Continue opens the paused step, or the next once it has been ticked since.
        let mut s = Session::new(d, evening.clone(), Some("evening-fathers".into()));
        assert_eq!(resume_at(&rule, &env, &s, &st), Some("evening-fathers".into()));
        s.item = Some("evening-prayers".into());
        assert_eq!(resume_at(&rule, &env, &s, &st), Some("evening-kathisma".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn readings_walk_one_reading_after_another_and_tick_nothing() {
        let dir = std::env::temp_dir().join(format!("bible-rs-readings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let library = Library::open(&dir, std::sync::Arc::new(|| {}));
        let study = StudyStore::open_in_memory().unwrap();
        let settings = crate::app::Settings::default();
        let env = PageEnv {
            library: &library,
            study: &study,
            settings: &settings,
            query: "",
            toolbar: 52.0,
            fathers: None,
        };
        let rule = study.rule();
        let d = calendar::civil(2026, 10, 21);
        let (_, _, st) = steps(&Walk::Readings, d, &env, &rule);
        let titles: Vec<_> = st.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["2 Corinthians 9:6–11", "Luke 9:44–50"]);
        assert!(st.iter().all(|s| s.tick == Tick::Nothing && s.group == Some("Liturgy")));
        assert_eq!(next(&rule, &env, &Walk::Readings, d, &st, "0", false), Move::To("1".into()));
        assert_eq!(next(&rule, &env, &Walk::Readings, d, &st, "1", false), Move::Close);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
