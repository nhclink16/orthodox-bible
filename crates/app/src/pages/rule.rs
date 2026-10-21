//! The prayer rule on the Today page (Orthodox fork): the day's blocks with a check box for each
//! item and what the item opens (a prayer's text, the reader, the Fathers beside it, the Jesus
//! Prayer's counter) in the page's place. Nothing counts streaks or scores: an item is done that
//! day or it isn't.

use std::collections::HashSet;

use eframe::egui::{self, Align2, FontId, Key, Modifiers, Rect, Sense, Ui, pos2, vec2};

use bible_core::calendar::{self, Day, Style};
use bible_core::lectionary::{self, Passage, Reading};
use bible_core::rule::{Block, Item, Kind, Rule};
use bible_core::study::{PlanProgress, StudyStore};
use bible_core::{BookId, ChapterIdx, Translation, VerseRef, lxx, plans, prayers};

use super::plans::Under;
use super::{PageEnv, group_label, text};
use crate::actions::Action;
use crate::icons;
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind as ButtonKind};

/// The Jesus Prayer, as the counter shows it.
const JESUS_PRAYER: &str = "Lord Jesus Christ, Son of God, have mercy on me, a sinner.";

/// A rule's time ("HH:MM") as minutes after midnight.
pub fn minutes(time: &str) -> Option<i64> {
    let (h, m) = time.split_once(':')?;
    let (h, m) = (h.trim().parse::<i64>().ok()?, m.trim().parse::<i64>().ok()?);
    ((0..24).contains(&h) && (0..60).contains(&m)).then_some(h * 60 + m)
}

/// Minutes after midnight as the app shows a time: "7:00", "20:30".
pub fn clock(minutes: i64) -> String {
    format!("{}:{:02}", minutes / 60, minutes % 60)
}

/// A psalm's Septuagint number in the numbering the app shows.
pub(super) fn shown_psalm(n: u8) -> u8 {
    match lxx::from_lxx(n, 1) {
        _ if lxx::numbering() => n,
        Some((b, c, _)) if b == BookId::PSALMS => c,
        _ => n,
    }
}

/// The rule's reminders while the app is open: a block's title and time when it comes and
/// something in it is still to do. Up to 30 minutes late still reminds; later doesn't.
#[derive(Default)]
pub struct Reminders {
    /// The minute last looked at, the blocks reminded of (day, block id), and when the next
    /// block's time comes (Unix seconds).
    checked: i64,
    reminded: Vec<(Day, String)>,
    wake: Option<i64>,
}

impl Reminders {
    /// Looks at the rule again on the next call (it, or the setting, changed).
    pub fn reset(&mut self) {
        self.checked = 0;
        self.wake = None;
    }

    /// At `now`, what to remind of ("Evening — 20:30"), and how many seconds egui may sleep
    /// before asking again: until the next block's time (midnight after the day's last), and no
    /// more than an hour.
    pub fn due(&mut self, now: i64, study: &StudyStore, style: Style) -> (Vec<String>, u64) {
        let mut due = Vec::new();
        if self.checked != now.div_euclid(60) {
            self.checked = now.div_euclid(60);
            let t = crate::time::local(now);
            let day = calendar::civil(t.year, t.month, t.day);
            let minute = i64::from(t.hour) * 60 + i64::from(t.minute);
            let at_minute = |m: i64| now - now.rem_euclid(60) + (m - minute) * 60;
            let rule = study.rule();
            let started = study.plans();
            let mut done = study.rule_done(day);
            done.extend(plans_read(&rule, day, style, day, &following(&rule, &started, study)));
            done.extend(idle(&rule, day, style));
            self.wake = Some(at_minute(24 * 60));
            for (block, items) in rule.for_day(day, style) {
                let Some(at) = block.time.as_deref().and_then(minutes) else { continue };
                if at > minute {
                    self.wake = self.wake.map(|w| w.min(at_minute(at)));
                    continue;
                }
                let todo = items.iter().any(|i| !i.optional && !done.contains(&i.id));
                let key = (day, block.id.clone());
                if minute - at >= 30 || !todo || self.reminded.contains(&key) {
                    continue;
                }
                self.reminded.push(key);
                due.push(format!("{} — {}", block.title, clock(at)));
            }
        }
        // The hour's cap also covers the Mac sleeping and the clocks changing.
        (due, self.wake.map_or(3600, |wake| (wake - now).clamp(1, 3600)) as u64)
    }
}

/// What an item opens.
enum Open {
    /// A prayer or another text, with where its wording comes from.
    Text {
        text: String,
        source: String,
    },
    /// A chapter (and verse) in the reader.
    Read(ChapterIdx, Option<u8>),
    /// A reading of the day in the reader, its verses selected.
    Passage(Passage),
    /// The Church Fathers panel on a verse.
    Fathers(VerseRef),
    /// The Jesus Prayer's counter, up to this many.
    Count(u16),
    /// A reading plan never started.
    Start(&'static str),
    Nothing,
    /// Nothing to do on this day (no kathisma on Sunday evening): no check box, and it counts
    /// as done.
    NotToday,
}

impl Open {
    fn label(&self, kind: &Kind) -> Option<&'static str> {
        Some(match self {
            Open::Text { .. } if *kind == Kind::Reflection => "Open",
            Open::Text { .. } | Open::Count(_) => "Pray",
            Open::Read(..) | Open::Passage(_) => "Read",
            Open::Fathers(_) => "Open",
            Open::Start(_) => "Start plan",
            Open::Nothing | Open::NotToday => return None,
        })
    }
}

/// The day's kathismata for a kathisma item: at Matins in the morning block (or any block
/// before noon), at Vespers in the others.
fn kathismata(block: &Block, d: Day) -> &'static [u8] {
    let (matins, vespers) = calendar::kathismata(calendar::weekday(d));
    let before_noon = block.time.as_deref().and_then(minutes).is_some_and(|m| m < 12 * 60);
    if block.id == "morning" || before_noon { matins } else { vespers }
}

/// The reading plans under way that the rule's items follow, with their progress.
pub(super) fn following<'a>(rule: &Rule, started: &'a [PlanProgress], study: &StudyStore) -> Vec<Under<'a>> {
    let followed = |id: &str| {
        let mut items = rule.blocks.iter().flat_map(|b| &b.items);
        items.any(|i| matches!(&i.kind, Kind::Plan { plan } if plan == id))
    };
    started
        .iter()
        .filter(|s| s.finished.is_none() && followed(&s.id))
        .filter_map(|s| Some(Under::new(plans::get(&s.id)?, s, study)))
        .collect()
}

/// The day of a plan's reading for day `d`, if any: today, the Plans page's day to continue
/// with (yesterday's while that's still to read); on other days, the one that falls on them.
pub(super) fn plan_day(u: &Under<'_>, d: Day, today: Day) -> Option<usize> {
    let day = u.today as i64 + (d - today);
    (1..=u.plan.len() as i64).contains(&day).then(|| if d == today { u.current_day() } else { day as usize })
}

/// The ids of day `d`'s items that follow a plan whose reading for that day is read: those are
/// done without a tick of their own.
pub(super) fn plans_read(rule: &Rule, d: Day, style: Style, today: Day, under: &[Under<'_>]) -> Vec<String> {
    let read = |plan: &str| {
        under.iter().any(|u| u.plan.id == plan && plan_day(u, d, today).is_some_and(|day| u.day_done(day)))
    };
    let items = rule.for_day(d, style).into_iter().flat_map(|(_, items)| items);
    items.filter(|i| matches!(&i.kind, Kind::Plan { plan } if read(plan))).map(|i| i.id.clone()).collect()
}

/// The ids of day `d`'s items with nothing to do then (the kathisma on Sunday evening): they
/// count as done for `Rule::kept` and the reminders.
pub(super) fn idle(rule: &Rule, d: Day, style: Style) -> Vec<String> {
    let mut out = Vec::new();
    for (block, items) in rule.for_day(d, style) {
        let none = kathismata(block, d).is_empty();
        out.extend(items.into_iter().filter(|i| none && i.kind == Kind::Kathisma).map(|i| i.id.clone()));
    }
    out
}

/// A plan's day that an item follows: its box ticks the day's readings, and it's done once
/// they're read.
struct PlanDay {
    plan: &'static str,
    day: usize,
    passages: usize,
}

/// A plan item's line when there's no reading to open: the plan's name, unless the title is
/// it, and how the plan stands.
fn plan_note(item: &Item, plan: &plans::Plan, state: &str) -> String {
    if item.title.trim() == plan.title { state.into() } else { format!("{} · {state}", plan.title) }
}

/// What the rule's Scripture and the Fathers open on a day with these `lessons`: its Liturgy's
/// Gospel or, on a day without one (a Lenten weekday, Holy Friday), its first reading; and
/// whether it is that Gospel.
fn day_gospel(lessons: &[Reading]) -> Option<(&Reading, bool)> {
    let gospel = lessons.iter().find(|r| r.at_liturgy() && r.kind == lectionary::Kind::Gospel);
    gospel.map(|g| (g, true)).or_else(|| lessons.first().map(|r| (r, false)))
}

/// Says plainly that day `d` has no Liturgy Gospel, and what is read `first` instead: "No
/// Gospel today · Isaiah 1:1–20 at the Sixth Hour".
fn instead(first: &Reading, lessons: &[Reading], d: Day, today: Day) -> String {
    let none =
        if lessons.iter().any(|r| r.kind == lectionary::Kind::Gospel) { "No Liturgy" } else { "No Gospel" };
    let when = if d == today { "today" } else { "on this day" };
    let at =
        if first.service.starts_with("Matins") || first.service.starts_with("Vespers") { "" } else { "the " };
    format!("{none} {when} · {} at {at}{}", first.label(), first.service)
}

/// What `item` (in `block`) opens on day `d` (`today` being today, `lessons` its readings), the
/// line under its title, and the plan's day it follows, if it's a plan's.
fn resolve(
    item: &Item,
    block: &Block,
    (d, today): (Day, Day),
    lessons: &[Reading],
    t: Option<&Translation>,
    started: &[PlanProgress],
    under: &[Under<'_>],
) -> (Open, String, Option<PlanDay>) {
    let mut detail = item.note.clone();
    let mut follows = None;
    let open = match &item.kind {
        Kind::Prayers { prayer, text } => match (prayer.as_deref().and_then(prayers::get), text) {
            (Some(p), _) => Open::Text { text: p.text.into(), source: p.source.into() },
            (None, Some(own)) if !own.trim().is_empty() => {
                Open::Text { text: own.clone(), source: String::new() }
            }
            _ => Open::Nothing,
        },
        Kind::Kathisma => {
            let ks = kathismata(block, d);
            detail = match ks {
                [] => return (Open::NotToday, "No kathisma on Sunday evening".into(), None),
                [k] => format!("Kathisma {k} · {}", calendar::kathisma_psalms(*k)),
                [first @ .., last] => {
                    let first: Vec<String> = first.iter().map(u8::to_string).collect();
                    format!("Kathismata {} and {last}", first.join(", "))
                }
            };
            ks.first()
                .and_then(|k| calendar::kathisma_start(*k))
                .and_then(|(b, c)| ChapterIdx::new(b, c))
                .map_or(Open::Nothing, |c| Open::Read(c, None))
        }
        Kind::Psalm { lxx } => {
            // The number as the reader shows it, when the title has another (the starter's
            // Psalm 90 is 91 in the Hebrew numbering).
            let n = shown_psalm(*lxx).to_string();
            if !item.title.split(|c: char| !c.is_ascii_digit()).any(|w| w == n) {
                let psalm = format!("Psalm {n}");
                detail = if detail.is_empty() { psalm } else { format!("{psalm} · {detail}") };
            }
            lxx::from_lxx(*lxx, 1)
                .and_then(|(b, c, _)| ChapterIdx::new(b, c))
                .map_or(Open::Nothing, |c| Open::Read(c, None))
        }
        Kind::Plan { plan } => match (plans::get(plan), started.iter().find(|s| s.id == *plan)) {
            (None, _) => Open::Nothing,
            (Some(p), None) => {
                detail = plan_note(item, p, "Not started");
                Open::Start(p.id)
            }
            // Ticked like any other item; reading it again starts on the Plans page.
            (Some(p), Some(s)) if s.finished.is_some() => {
                detail = plan_note(item, p, "Finished");
                Open::Nothing
            }
            // The plan's reading for the day, at its first passage not yet read.
            (Some(p), Some(_)) => {
                let u = under.iter().find(|u| u.plan.id == p.id);
                match u.and_then(|u| Some((u, plan_day(u, d, today)?))) {
                    Some((u, day)) => {
                        detail = format!("Day {day} · {}", super::plans::day_label(p.day(day), t));
                        follows = Some(PlanDay { plan: p.id, day, passages: p.day(day).len() });
                        let i = (0..p.day(day).len()).find(|i| !u.passage_done(day, *i)).unwrap_or(0);
                        p.day(day)
                            .get(i)
                            .map_or(Open::Nothing, |ps| Open::Read(ps.chapter, ps.verses.map(|(a, _)| a)))
                    }
                    None => {
                        detail = plan_note(item, p, "No reading on this day");
                        Open::Nothing
                    }
                }
            }
        },
        // The day's Epistle and Gospel, and Read opens the Gospel.
        Kind::Readings => match day_gospel(lessons) {
            Some((g, true)) => {
                let epistle = lessons.iter().find(|r| r.at_liturgy() && r.kind == lectionary::Kind::Epistle);
                detail = epistle.into_iter().chain([g]).map(|r| r.label()).collect::<Vec<_>>().join(" · ");
                g.passages.first().map_or(Open::Nothing, |p| Open::Passage(*p))
            }
            Some((first, false)) => {
                detail = instead(first, lessons, d, today);
                first.passages.first().map_or(Open::Nothing, |p| Open::Passage(*p))
            }
            None => Open::Nothing,
        },
        // The Fathers on the Gospel's first verse.
        Kind::Fathers => match day_gospel(lessons) {
            Some((r, gospel)) => {
                let on = if gospel { r.label() } else { instead(r, lessons, d, today) };
                detail = if detail.is_empty() || !gospel { on } else { format!("{detail} · {on}") };
                r.passages.first().map_or(Open::Nothing, |p| Open::Fathers(p.first()))
            }
            None => Open::Nothing,
        },
        Kind::JesusPrayer { count } => {
            let times = format!("{count} times");
            detail = if detail.is_empty() { times } else { format!("{detail} · {times}") };
            Open::Count(*count)
        }
        Kind::Reflection => Open::Text { text: prayers::REFLECTION.into(), source: String::new() },
        Kind::Other => Open::Nothing,
    };
    if item.optional {
        detail = if detail.is_empty() { "Optional".into() } else { format!("{detail} · Optional") };
    }
    (open, detail, follows)
}

/// The rule's blocks for day `d` from `top`, in a column `width` wide at `left`; returns the
/// bottom. Ticking an item marks it done for that day; the rest of its row opens what it needs.
#[allow(clippy::too_many_arguments)]
pub fn section(
    ui: &mut Ui,
    env: &PageEnv<'_>,
    rule: &Rule,
    left: f32,
    width: f32,
    top: f32,
    d: Day,
    view: &mut Option<View>,
    actions: &mut Vec<Action>,
) -> f32 {
    let p = env.settings.theme.palette();
    let painter = ui.painter().clone();
    group_label(&painter, p, left, top, "Your rule");
    let edit = Button::new(ButtonKind::Ghost, "Edit").small();
    let (_, r) =
        edit.show_right(ui, p, left + width - 12.0 + edit.rest_inset(), top + 8.0, ui.id().with("edit-rule"));
    if r.clicked() {
        let editor = super::rule_editor::Editor::new(rule.clone(), env.settings.rule_reminders);
        *view = Some(View::Edit(Box::new(editor)));
    }
    let mut y = top + 26.0;
    let style = env.settings.calendar;
    let blocks = rule.for_day(d, style);
    if blocks.is_empty() {
        text(
            &painter,
            left,
            y,
            20.0,
            "Nothing in your rule for this day.",
            theme::ui_font(13.0),
            p.ink_2,
            false,
        );
        return y + 20.0;
    }
    let today = super::today::today();
    let lessons = lectionary::readings(d, style);
    let t = super::reading_text(env);
    let started = env.study.plans();
    let under = following(rule, &started, env.study);
    let mut done: HashSet<String> = env.study.rule_done(d);
    done.extend(plans_read(rule, d, style, today, &under));
    for (i, (block, items)) in blocks.iter().enumerate() {
        if i > 0 {
            y += 14.0;
        }
        // The name, shortened to leave room for the time after it.
        let at = block.time.as_deref().and_then(minutes).map(clock);
        let tw = at.as_ref().map_or(0.0, |at| {
            8.0 + painter.layout_no_wrap(at.clone(), theme::ui_font(13.0), p.ink_3).size().x
        });
        let name = super::elide(&painter, &block.title, &theme::ui_semibold(15.0), width - 24.0 - tw);
        let w = text(&painter, left + 12.0, y, 24.0, &name, theme::ui_semibold(15.0), p.ink, false);
        if let Some(at) = at {
            text(&painter, left + 12.0 + w + 8.0, y + 2.0, 20.0, &at, theme::ui_font(13.0), p.ink_3, false);
        }
        y += 28.0;
        for item in items {
            let (open, detail, follows) =
                resolve(item, block, (d, today), &lessons, t.as_deref(), &started, &under);
            let row = Rect::from_min_size(pos2(left, y), vec2(width, 48.0));
            let id = ui.id().with(("rule-item", &item.id));
            let idle = matches!(open, Open::NotToday);
            let resp = ui.interact(row, id, if idle { Sense::hover() } else { Sense::click() });
            if !idle && ui.rect_contains_pointer(row) {
                painter.rect_filled(
                    row,
                    10.0,
                    if resp.is_pointer_button_down_on() { p.fill_strong } else { p.fill },
                );
            }
            widgets::focus_ring(&painter, p, &resp, row, 10.0);
            let is_done = done.contains(&item.id);
            let b = Rect::from_min_size(pos2(row.left() + 12.0, row.center().y - 8.0), vec2(16.0, 16.0));
            let tick = (!idle).then(|| widgets::tick(ui, p, b, is_done, id.with("tick")));
            let x = b.right() + 12.0;
            let label = open.label(&item.kind);
            let mut room = row.right() - 12.0 - x;
            if let Some(l) = label {
                let font = theme::ui_medium(13.0);
                room -= text(&painter, row.right() - 12.0, row.top() + 14.0, 20.0, l, font, p.accent, true)
                    + 16.0;
            }
            let title_top = if detail.is_empty() { row.top() + 14.0 } else { row.top() + 5.0 };
            let title = super::elide(&painter, &item.title, &theme::ui_medium(14.0), room);
            text(&painter, x, title_top, 20.0, &title, theme::ui_medium(14.0), p.ink, false);
            if !detail.is_empty() {
                let detail = super::elide(&painter, &detail, &theme::ui_font(12.0), room);
                text(&painter, x, row.top() + 25.0, 18.0, &detail, theme::ui_font(12.0), p.ink_3, false);
            }
            if tick.is_some_and(|t| t.clicked()) {
                // A plan's item ticks the plan's readings for the day, which say whether it's done.
                if let Some(pd) = &follows {
                    for i in 0..pd.passages {
                        let (plan, day, passage) = (pd.plan.to_string(), pd.day as u16, i as u8);
                        actions.push(Action::SetPlanRead { plan, day, passage, read: !is_done });
                    }
                }
                let done = !is_done && follows.is_none();
                actions.push(Action::SetRuleDone { day: d, item: item.id.clone(), done });
            } else if !idle && resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                activate(open, item, d, is_done, view, actions);
                ui.ctx().request_repaint();
            }
            y += 48.0;
        }
    }
    y
}

/// Opens what an item opens on day `d`; an item with nothing to open is ticked (or unticked),
/// as its box would be.
fn activate(open: Open, item: &Item, d: Day, done: bool, view: &mut Option<View>, actions: &mut Vec<Action>) {
    let (day, id, title) = (d, item.id.clone(), item.title.clone());
    match open {
        Open::Text { text, source } => *view = Some(View::Text { day, item: id, title, text, source }),
        Open::Read(chapter, verse) => actions.push(Action::Jump { chapter, verse, flash: verse.is_some() }),
        Open::Passage(p) => actions.push(Action::ReadPassage(p)),
        // The verse selected in the reader, which the panel follows.
        Open::Fathers(v) => {
            actions.push(Action::Jump { chapter: v.chapter_idx(), verse: Some(v.verse), flash: true });
            actions.push(Action::ShowFathers);
        }
        Open::Count(target) => {
            *view = Some(View::Count { day, item: id, title, target: target.max(1), n: 0 })
        }
        Open::Start(plan) => actions.push(Action::StartPlan(plan.to_string())),
        Open::Nothing => actions.push(Action::SetRuleDone { day, item: id, done: !done }),
        Open::NotToday => {}
    }
}

/// Opens the item `id` of the rule on day `d`, as clicking its row does (demo captures).
pub fn open_item(
    rule: &Rule,
    id: &str,
    d: Day,
    env: &PageEnv<'_>,
    view: &mut Option<View>,
    actions: &mut Vec<Action>,
) {
    let t = super::reading_text(env);
    let started = env.study.plans();
    let under = following(rule, &started, env.study);
    let lessons = lectionary::readings(d, env.settings.calendar);
    for (block, items) in rule.for_day(d, env.settings.calendar) {
        if let Some(item) = items.into_iter().find(|i| i.id == id) {
            let today = super::today::today();
            let (open, _, _) = resolve(item, block, (d, today), &lessons, t.as_deref(), &started, &under);
            activate(open, item, d, env.study.rule_done(d).contains(id), view, actions);
        }
    }
}

/// What opens in the Today page's place.
pub enum View {
    /// A prayer or a text to read, for `item` on `day`.
    Text { day: Day, item: String, title: String, text: String, source: String },
    /// The Jesus Prayer, counted to `n` of `target`.
    Count { day: Day, item: String, title: String, target: u16, n: u16 },
    /// Editing the rule.
    Edit(Box<super::rule_editor::Editor>),
}

impl View {
    /// Draws the view in the page's `content`; returns its bottom and whether to go back to
    /// Today. Escape goes back too, unless a field has the keyboard or the editor has changes
    /// to lose.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        content: Rect,
        env: &PageEnv<'_>,
        actions: &mut Vec<Action>,
    ) -> (f32, bool) {
        let escape = !ui.ctx().egui_wants_keyboard_input()
            && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        let (bottom, back) = match self {
            View::Text { day, item, title, text, source } => {
                reading(ui, content, env, (*day, item.as_str()), title, text, source, actions)
            }
            View::Count { day, item, title, target, n } => {
                counter(ui, content, env, (*day, item.as_str()), title, *target, n, actions)
            }
            View::Edit(editor) => {
                let (bottom, back) = editor.show(ui, content, env, actions);
                (bottom, back || escape && !editor.changed())
            }
        };
        (bottom, back || escape && !matches!(self, View::Edit(_)))
    }
}

/// "Today › title" at the top of a view, the title shortened to `width`; returns whether
/// "Today" was clicked.
pub fn crumb(ui: &mut Ui, p: &Palette, left: f32, top: f32, width: f32, title: &str) -> bool {
    let painter = ui.painter().clone();
    let w = text(&painter, left, top, 16.0, "Today", theme::ui_medium(13.0), p.accent, false);
    let link = Rect::from_min_size(pos2(left, top), vec2(w, 16.0));
    let resp = ui.interact(link, ui.id().with("crumb"), Sense::click());
    widgets::focus_ring(&painter, p, &resp, link.expand(2.0), 4.0);
    let clicked = resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
    let chev = Rect::from_min_size(pos2(left + w + 6.0, top + 2.0), vec2(12.0, 12.0));
    icons::disclosure(&painter, chev, p.ink_3);
    let title = super::elide(&painter, title, &theme::ui_medium(13.0), left + width - chev.right() - 6.0);
    text(&painter, chev.right() + 6.0, top, 16.0, &title, theme::ui_medium(13.0), p.ink_2, false);
    clicked
}

/// A prayer's text in the reading face at the reader's size, with where its wording comes from,
/// and Done, which ticks the item.
#[allow(clippy::too_many_arguments)]
fn reading(
    ui: &mut Ui,
    content: Rect,
    env: &PageEnv<'_>,
    (day, item): (Day, &str),
    title: &str,
    body: &str,
    source: &str,
    actions: &mut Vec<Action>,
) -> (f32, bool) {
    let p = env.settings.theme.palette();
    let painter = ui.painter().clone();
    let mut back = crumb(ui, p, content.left(), content.top(), content.width(), title);
    let measure = content.width().min(560.0);
    let x = content.center().x - measure / 2.0;
    let mut y = content.top() + 64.0;
    // The title, on as many lines as it needs.
    let heading = FontId::new(28.0, theme::family(theme::SERIF_HEADING));
    for mut runs in widgets::flow(&painter, title, &heading, 28.0, measure, 4) {
        runs.iter_mut().for_each(|r| r.tracking -= 0.01);
        widgets::paint_runs(&painter, x, widgets::css_baseline(y, 36.0, 28.0, true), &runs, p.ink);
        y += 36.0;
    }
    y += 24.0;
    let size = env.settings.typo.size;
    let mut job = egui::text::LayoutJob::simple(
        body.to_string(),
        FontId::new(size, theme::family(theme::SERIF)),
        p.ink,
        measure,
    );
    job.sections[0].format.line_height = Some((size * env.settings.typo.line_height).round());
    let g = painter.layout_job(job);
    let h = g.size().y;
    painter.galley(pos2(x, y), g, p.ink);
    y += h;
    if !source.is_empty() {
        y += 16.0;
        text(&painter, x, y, 16.0, source, theme::ui_font(12.0), p.ink_3, false);
        y += 16.0;
    }
    y += 32.0;
    if Button::new(ButtonKind::Primary, "Done")
        .show_left(ui, p, x, y + 16.0, ui.id().with("done"))
        .1
        .clicked()
    {
        actions.push(Action::SetRuleDone { day, item: item.to_string(), done: true });
        back = true;
    }
    (y + 32.0, back)
}

/// The Jesus Prayer's counter: a large circle that counts a prayer for each click or Space, its
/// knots around it like a prayer rope's, and the item ticked once the count is reached.
#[allow(clippy::too_many_arguments)]
fn counter(
    ui: &mut Ui,
    content: Rect,
    env: &PageEnv<'_>,
    (day, item): (Day, &str),
    title: &str,
    target: u16,
    n: &mut u16,
    actions: &mut Vec<Action>,
) -> (f32, bool) {
    let p = env.settings.theme.palette();
    let painter = ui.painter().clone();
    let mut back = crumb(ui, p, content.left(), content.top(), content.width(), title);
    let cx = content.center().x;
    let mut y = content.top() + 64.0;
    let mut job = egui::text::LayoutJob::simple(
        JESUS_PRAYER.into(),
        FontId::new(20.0, theme::family(theme::SERIF)),
        p.ink_2,
        content.width().min(600.0),
    );
    job.halign = egui::Align::Center;
    job.sections[0].format.line_height = Some(30.0);
    let g = painter.layout_job(job);
    let h = g.size().y;
    painter.galley(pos2(cx - g.rect.center().x, y), g, p.ink_2);
    y += h + 40.0;

    let finished = *n >= target;
    let radius = 104.0;
    let c = pos2(cx, y + radius + 16.0);
    let circle = Rect::from_center_size(c, vec2(2.0 * radius, 2.0 * radius));
    let resp =
        ui.interact(circle, ui.id().with("count"), if finished { Sense::hover() } else { Sense::click() });
    painter.circle_filled(c, radius, p.sidebar);
    if !finished && resp.is_pointer_button_down_on() {
        painter.circle_filled(c, radius, p.fill_strong);
    } else if !finished && resp.hovered() {
        painter.circle_filled(c, radius, p.fill);
    }
    if resp.has_focus() {
        painter.circle_stroke(c, radius + 1.5, egui::Stroke::new(3.0, p.focus));
    }
    // Space counts unless a field (the toolbar's search) is being typed in.
    let space = !finished
        && !ui.ctx().egui_wants_keyboard_input()
        && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Space));
    if !finished && (resp.clicked() || space) {
        *n += 1;
        if *n >= target {
            actions.push(Action::SetRuleDone { day, item: item.to_string(), done: true });
        }
    }
    // The knots, when there are few enough to see one by one.
    if target <= 100 {
        for k in 0..target {
            let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * f32::from(k) / f32::from(target);
            let at = c + vec2(a.cos(), a.sin()) * (radius + 16.0);
            painter.circle_filled(at, 3.0, if k < *n { p.accent } else { p.fill_strong });
        }
    }
    if *n >= target {
        icons::heavy(|| {
            icons::check(&painter, Rect::from_center_size(c - vec2(0.0, 18.0), vec2(28.0, 28.0)), p.accent)
        });
        painter.text(
            c + vec2(0.0, 20.0),
            Align2::CENTER_CENTER,
            "Glory to God",
            FontId::new(20.0, theme::family(theme::SERIF)),
            p.ink,
        );
    } else {
        painter.text(
            c - vec2(0.0, 8.0),
            Align2::CENTER_CENTER,
            n.to_string(),
            FontId::new(48.0, theme::family(theme::SERIF)),
            p.ink,
        );
        painter.text(
            c + vec2(0.0, 30.0),
            Align2::CENTER_CENTER,
            format!("of {target}"),
            theme::ui_font(13.0),
            p.ink_3,
        );
    }
    y = c.y + radius + 16.0 + 32.0;

    if *n >= target {
        let done = Button::new(ButtonKind::Primary, "Done");
        let again = Button::new(ButtonKind::Plain, "Pray again");
        let w = done.width(&painter) + 8.0 + again.width(&painter);
        let (r, resp) = again.show_left(ui, p, cx - w / 2.0, y + 16.0, ui.id().with("again"));
        if resp.clicked() {
            *n = 0;
        }
        back |= done.show_left(ui, p, r.right() + 8.0, y + 16.0, ui.id().with("done")).1.clicked();
    } else {
        // Space, or a click on the circle.
        let hint = "or click the circle for each prayer";
        let kw = widgets::keycap_width(&painter, "Space");
        let tw = painter.layout_no_wrap(hint.into(), theme::ui_font(12.0), p.ink_3).size().x;
        let x = cx - (kw + 8.0 + tw) / 2.0;
        widgets::keycap(&painter, p, x, y + 16.0, "Space");
        painter.text(pos2(x + kw + 8.0, y + 16.0), Align2::LEFT_CENTER, hint, theme::ui_font(12.0), p.ink_3);
        if *n > 0 {
            let again = Button::new(ButtonKind::Ghost, "Start again").small();
            let w = again.width(&painter);
            if again.show_left(ui, p, cx - w / 2.0, y + 52.0, ui.id().with("restart")).1.clicked() {
                *n = 0;
            }
        }
    }
    (y + 72.0, back)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The starter rule's Scripture and Fathers items on day `d` (seen from `today`): what each
    /// opens, and the line under it.
    fn scripture_and_fathers(d: Day, today: Day) -> Vec<(String, String)> {
        let rule = Rule::starter();
        let lessons = lectionary::readings(d, Style::New);
        let mut out = Vec::new();
        for (block, items) in rule.for_day(d, Style::New) {
            for item in items.into_iter().filter(|i| matches!(i.kind, Kind::Readings | Kind::Fathers)) {
                let (open, detail, _) = resolve(item, block, (d, today), &lessons, None, &[], &[]);
                let opens = match open {
                    Open::Passage(p) => format!("Read {}", lectionary::label(&[p])),
                    Open::Fathers(v) => format!("Fathers on {v}"),
                    _ => "nothing".into(),
                };
                out.push((opens, detail));
            }
        }
        out
    }

    fn pairs(v: [(&str, &str); 2]) -> Vec<(String, String)> {
        v.into_iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn the_readings_and_the_fathers_follow_the_days_gospel() {
        let day = calendar::civil;
        // A weekday with a saint's Epistle: the Epistle and Gospel named, the Gospel opened.
        let tuesday = day(2026, 10, 20);
        assert_eq!(
            scripture_and_fathers(tuesday, tuesday),
            pairs([
                ("Read Luke 9:23–27", "2 Timothy 2:1–10 · Luke 9:23–27"),
                ("Fathers on Luke 9:23", "On the day's Gospel · Luke 9:23–27"),
            ])
        );
        let sunday = day(2026, 10, 25);
        assert_eq!(
            scripture_and_fathers(sunday, tuesday),
            pairs([
                ("Read Luke 8:26–39", "Galatians 2:16–20 · Luke 8:26–39"),
                ("Fathers on Luke 8:26", "On the day's Gospel · Luke 8:26–39"),
            ])
        );
        let nativity = day(2026, 12, 25);
        assert_eq!(
            scripture_and_fathers(nativity, tuesday),
            pairs([
                ("Read Matthew 2:1–12", "Galatians 4:4–7 · Matthew 2:1–12"),
                ("Fathers on Matthew 2:1", "On the day's Gospel · Matthew 2:1–12"),
            ])
        );
        // A Lenten weekday has no Liturgy Gospel: the first reading, and saying so.
        let lenten = day(2026, 3, 4);
        let instead = "No Gospel today · Isaiah 5:16–25 at the Sixth Hour";
        assert_eq!(
            scripture_and_fathers(lenten, lenten),
            pairs([("Read Isaiah 5:16–25", instead), ("Fathers on Isaiah 5:16", instead)])
        );
        let (_, other_day) = scripture_and_fathers(lenten, tuesday).remove(0);
        assert_eq!(other_day, "No Gospel on this day · Isaiah 5:16–25 at the Sixth Hour");
        // Holy Friday has Gospels but no Liturgy.
        let friday = day(2026, 4, 10);
        let (opens, detail) = scripture_and_fathers(friday, friday).remove(0);
        assert_eq!(opens, "Read John 13:31–18:1");
        assert_eq!(detail, "No Liturgy today · John 13:31–18:1 at Matins of the Twelve Passion Gospels");
    }

    /// The app left open on the starter rule, waking only when `due` says to: each timed block
    /// is reminded of at its time, the morning's too after the night, and none twice.
    #[test]
    fn reminders_come_at_each_blocks_time() {
        let mut study = StudyStore::open_in_memory().unwrap();
        // Local midnight of a day in June, away from a change of the clocks.
        let midnight = crate::time::day_start(1_781_000_000);
        let (h, m) = (3600, 60);
        let mut r = Reminders::default();
        let mut now = midnight + 6 * h + 59 * m + 50;
        let mut seen = Vec::new();
        while now < midnight + 24 * h + 21 * h {
            let (due, wait) = r.due(now, &study, Style::New);
            seen.extend(due.into_iter().map(|d| (now - midnight, d)));
            now += wait as i64;
        }
        let at = |s: i64, d: &str| (s, d.to_string());
        let day = |s: i64| 24 * h + s;
        assert_eq!(
            seen,
            [
                at(7 * h, "Morning — 7:00"),
                at(12 * h, "Midday — 12:00"),
                at(20 * h + 30 * m, "Evening — 20:30"),
                at(day(7 * h), "Morning — 7:00"),
                at(day(12 * h), "Midday — 12:00"),
                at(day(20 * h + 30 * m), "Evening — 20:30"),
            ]
        );

        // The next evening, a block moved to a few minutes on is reminded of then, once the
        // rule is saved.
        let evening = midnight + 48 * h + 19 * h;
        let (_, wait) = r.due(evening, &study, Style::New);
        assert_eq!(wait, 3600);
        let mut rule = Rule::starter();
        rule.blocks.iter_mut().find(|b| b.id == "evening").unwrap().time = Some("19:05".into());
        study.set_rule(&rule).unwrap();
        r.reset();
        let (due, wait) = r.due(evening + 40, &study, Style::New);
        assert!(due.is_empty());
        assert_eq!(wait, 5 * 60 - 40);
        let (due, _) = r.due(evening + 5 * m, &study, Style::New);
        assert_eq!(due, ["Evening — 19:05"]);
    }
}
