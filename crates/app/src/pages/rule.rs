//! The prayer rule on the Today page (Orthodox fork): the day's blocks with a check box for each
//! item and what the item opens (a prayer's text, the reader, the Jesus Prayer's counter) in the
//! page's place. Nothing counts streaks or scores: an item is done that day or it isn't.

use std::collections::HashSet;

use eframe::egui::{self, Align2, FontId, Key, Modifiers, Rect, Sense, Ui, pos2, vec2};

use bible_core::calendar::{self, Day};
use bible_core::rule::{Block, Item, Kind, Rule};
use bible_core::study::PlanProgress;
use bible_core::{ChapterIdx, Translation, lxx, plans, prayers};

use super::{PageEnv, group_label, text};
use crate::actions::Action;
use crate::icons;
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind as ButtonKind, css_baseline, paint_on_baseline};

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

/// What an item opens.
enum Open {
    /// A prayer or another text, with where its wording comes from.
    Text {
        text: String,
        source: String,
    },
    /// A chapter (and verse) in the reader.
    Read(ChapterIdx, Option<u8>),
    /// The Jesus Prayer's counter, up to this many.
    Count(u16),
    /// A reading plan that isn't under way.
    Start(&'static str),
    /// The day's Epistle and Gospel and the Fathers on it wait for the daily lectionary, which is
    /// built separately: with it, `resolve` opens the readings in the reader and the Fathers
    /// panel on the Gospel.
    Soon,
    Nothing,
}

impl Open {
    fn label(&self, kind: &Kind) -> Option<&'static str> {
        Some(match self {
            Open::Text { .. } if *kind == Kind::Reflection => "Open",
            Open::Text { .. } | Open::Count(_) => "Pray",
            Open::Read(..) => "Read",
            Open::Start(_) => "Start plan",
            Open::Soon => "Coming soon",
            Open::Nothing => return None,
        })
    }
}

/// The line under a kathisma item: the day's kathismata at Matins (the morning block) or at
/// Vespers (any other).
fn kathismata(block: &Block, d: Day) -> &'static [u8] {
    let (matins, vespers) = calendar::kathismata(calendar::weekday(d));
    if block.id == "morning" { matins } else { vespers }
}

/// What `item` (in `block`) opens on day `d`, and the line under its title.
fn resolve(
    item: &Item,
    block: &Block,
    d: Day,
    env: &PageEnv<'_>,
    t: Option<&Translation>,
    started: &[PlanProgress],
) -> (Open, String) {
    let mut detail = item.note.clone();
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
                [] => "No kathisma on Sunday evening".into(),
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
        Kind::Psalm { lxx } => lxx::from_lxx(*lxx, 1)
            .and_then(|(b, c, _)| ChapterIdx::new(b, c))
            .map_or(Open::Nothing, |c| Open::Read(c, None)),
        Kind::Plan { plan } => match plans::get(plan) {
            None => Open::Nothing,
            Some(p) => match started.iter().find(|s| s.id == p.id && s.finished.is_none()) {
                // The next day not yet read, at its first passage not yet read.
                Some(s) => {
                    let u = super::plans::Under::new(p, s, env);
                    match (1..=p.len()).find(|day| !u.day_done(*day)) {
                        Some(day) => {
                            detail = format!("Day {day} · {}", super::plans::day_label(p.day(day), t));
                            let i = (0..p.day(day).len()).find(|i| !u.passage_done(day, *i)).unwrap_or(0);
                            p.day(day)
                                .get(i)
                                .map_or(Open::Nothing, |ps| Open::Read(ps.chapter, ps.verses.map(|(a, _)| a)))
                        }
                        None => {
                            detail = "Every day read".into();
                            Open::Nothing
                        }
                    }
                }
                None => {
                    detail = format!("{} · Not started", p.title);
                    Open::Start(p.id)
                }
            },
        },
        Kind::Readings | Kind::Fathers => Open::Soon,
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
    (open, detail)
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
        *view = Some(View::Edit(Box::new(super::rule_editor::Editor::new(rule.clone()))));
    }
    let mut y = top + 26.0;
    let blocks = rule.for_day(d, env.settings.calendar);
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
    let done: HashSet<String> = env.study.rule_done(d);
    let t = super::reading_text(env);
    let started = env.study.plans();
    for (i, (block, items)) in blocks.iter().enumerate() {
        if i > 0 {
            y += 14.0;
        }
        let w = text(&painter, left + 12.0, y, 24.0, &block.title, theme::ui_semibold(15.0), p.ink, false);
        if let Some(at) = block.time.as_deref().and_then(minutes) {
            text(
                &painter,
                left + 12.0 + w + 8.0,
                y + 2.0,
                20.0,
                &clock(at),
                theme::ui_font(13.0),
                p.ink_3,
                false,
            );
        }
        y += 28.0;
        for item in items {
            let (open, detail) = resolve(item, block, d, env, t.as_deref(), &started);
            let row = Rect::from_min_size(pos2(left, y), vec2(width, 48.0));
            let id = ui.id().with(("rule-item", &item.id));
            let resp = ui.interact(row, id, Sense::click());
            if ui.rect_contains_pointer(row) {
                painter.rect_filled(
                    row,
                    10.0,
                    if resp.is_pointer_button_down_on() { p.fill_strong } else { p.fill },
                );
            }
            let is_done = done.contains(&item.id);
            let b = Rect::from_min_size(pos2(row.left() + 12.0, row.center().y - 8.0), vec2(16.0, 16.0));
            let tick = widgets::tick(ui, p, b, is_done, id.with("tick"));
            let x = b.right() + 12.0;
            let label = open.label(&item.kind);
            let mut room = row.right() - 12.0 - x;
            if let Some(l) = label {
                let (font, ink) = if matches!(open, Open::Soon) {
                    (theme::ui_font(12.0), p.ink_3)
                } else {
                    (theme::ui_medium(13.0), p.accent)
                };
                room -= text(&painter, row.right() - 12.0, row.top() + 14.0, 20.0, l, font, ink, true) + 16.0;
            }
            let title_top = if detail.is_empty() { row.top() + 14.0 } else { row.top() + 5.0 };
            let title = super::elide(&painter, &item.title, &theme::ui_medium(14.0), room);
            text(&painter, x, title_top, 20.0, &title, theme::ui_medium(14.0), p.ink, false);
            if !detail.is_empty() {
                let detail = super::elide(&painter, &detail, &theme::ui_font(12.0), room);
                text(&painter, x, row.top() + 25.0, 18.0, &detail, theme::ui_font(12.0), p.ink_3, false);
            }
            if tick.clicked() {
                actions.push(Action::SetRuleDone { day: d, item: item.id.clone(), done: !is_done });
            } else if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
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
        Open::Count(target) => {
            *view = Some(View::Count { day, item: id, title, target: target.max(1), n: 0 })
        }
        Open::Start(plan) => actions.push(Action::StartPlan(plan.to_string())),
        Open::Soon | Open::Nothing => actions.push(Action::SetRuleDone { day, item: id, done: !done }),
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
    for (block, items) in rule.for_day(d, env.settings.calendar) {
        if let Some(item) = items.into_iter().find(|i| i.id == id) {
            let (open, _) = resolve(item, block, d, env, t.as_deref(), &env.study.plans());
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
    /// Today.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        content: Rect,
        env: &PageEnv<'_>,
        actions: &mut Vec<Action>,
    ) -> (f32, bool) {
        match self {
            View::Text { day, item, title, text, source } => {
                reading(ui, content, env, (*day, item.as_str()), title, text, source, actions)
            }
            View::Count { day, item, title, target, n } => {
                counter(ui, content, env, (*day, item.as_str()), title, *target, n, actions)
            }
            View::Edit(editor) => editor.show(ui, content, env, actions),
        }
    }
}

/// "Today › title" at the top of a view; returns whether "Today" was clicked.
pub fn crumb(ui: &mut Ui, p: &Palette, left: f32, top: f32, title: &str) -> bool {
    let painter = ui.painter().clone();
    let w = text(&painter, left, top, 16.0, "Today", theme::ui_medium(13.0), p.accent, false);
    let link = Rect::from_min_size(pos2(left, top), vec2(w, 16.0));
    let clicked = ui
        .interact(link, ui.id().with("crumb"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked();
    let chev = Rect::from_min_size(pos2(left + w + 6.0, top + 2.0), vec2(12.0, 12.0));
    icons::disclosure(&painter, chev, p.ink_3);
    text(&painter, chev.right() + 6.0, top, 16.0, title, theme::ui_medium(13.0), p.ink_2, false);
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
    let mut back = crumb(ui, p, content.left(), content.top(), title);
    let measure = content.width().min(560.0);
    let x = content.center().x - measure / 2.0;
    let mut y = content.top() + 64.0;
    let g =
        widgets::line(&painter, title, FontId::new(28.0, theme::family(theme::SERIF_HEADING)), p.ink, -0.01);
    paint_on_baseline(&painter, x, css_baseline(y, 36.0, 28.0, true), g, p.ink);
    y += 36.0 + 24.0;
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
    let mut back = crumb(ui, p, content.left(), content.top(), title);
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
    let space = !finished && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Space));
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
