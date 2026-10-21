//! Editing the prayer rule (Orthodox fork), in the Today page's place: the blocks of the day with
//! their names and reminder times, the items in each (added by kind, opened to change, moved,
//! removed, kept to a season or made optional), the reminders, and the starter rule to go back
//! to. Everything is a draft until saved.

use std::collections::{HashMap, HashSet};

use eframe::egui::{self, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use bible_core::rule::{Block, Item, Kind, Rule, Season};
use bible_core::{lxx, plans, prayers};

use super::rule::{clock, crumb, minutes, shown_psalm};
use super::{PageEnv, text};
use crate::actions::Action;
use crate::icons;
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind as ButtonKind};

/// What can be added to a block, as the menu names it (see [`new_item`]).
const ADD: [&str; 9] = [
    "Prayers",
    "The day's kathisma",
    "A psalm",
    "Scripture readings",
    "A reading plan",
    "The Fathers",
    "The Jesus Prayer",
    "Examination of conscience",
    "Something else",
];

const SEASONS: [(Season, &str); 3] = [
    (Season::Always, "Always"),
    (Season::GreatLent, "Great Lent only"),
    (Season::OutsideLent, "Outside Lent"),
];

/// Where an open item's fields start, after their labels.
const FIELD_X: f32 = 104.0;

pub struct Editor {
    blocks: Vec<Draft>,
    saved: Rule,
    /// The item open for changing, by id.
    open: Option<String>,
    /// Numbers as typed (a psalm, a count), by item id, kept while they don't read as numbers.
    numbers: HashMap<String, String>,
    /// The reminders switch, and how it was: a draft like the rest.
    reminders: bool,
    saved_reminders: bool,
}

/// A block being edited, with its time as typed.
struct Draft {
    block: Block,
    time: String,
}

/// A change to the rule's shape, made once the editor is drawn.
enum Change {
    MoveBlock(usize, usize),
    RemoveBlock(usize),
    MoveItem(usize, usize, usize),
    RemoveItem(usize, usize),
    AddItem(usize, usize),
    AddBlock,
    Reset,
}

fn drafts(rule: &Rule) -> Vec<Draft> {
    rule.blocks
        .iter()
        .map(|b| Draft {
            block: b.clone(),
            time: b.time.as_deref().and_then(minutes).map(clock).unwrap_or_default(),
        })
        .collect()
}

/// A time as typed ("7:00", "20:30", "7", "730") as the rule keeps it ("07:00"): `Some(None)`
/// for none, `None` when it doesn't read as a time.
fn typed_time(s: &str) -> Option<Option<String>> {
    let s = s.trim();
    if s.is_empty() {
        return Some(None);
    }
    // Digits and separators only, so the split below falls between characters.
    if !s.bytes().all(|b| b.is_ascii_digit() || b == b':' || b == b'.') {
        return None;
    }
    let (h, m) = match s.split_once([':', '.']) {
        Some(hm) => hm,
        None if s.len() > 2 => s.split_at(s.len() - 2),
        None => (s, "0"),
    };
    let (h, m) = (h.parse::<u8>().ok()?, m.parse::<u8>().ok()?);
    (h < 24 && m < 60).then(|| Some(format!("{h:02}:{m:02}")))
}

/// An id nothing in the rule has, for something new. Ids never change with titles, so the days
/// already ticked keep their meaning.
fn new_id(prefix: &str, taken: &HashSet<String>) -> String {
    let stamp =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    (0..)
        .map(|n| if n == 0 { format!("{prefix}-{stamp:x}") } else { format!("{prefix}-{stamp:x}-{n}") })
        .find(|id| !taken.contains(id))
        .unwrap_or_default()
}

/// A psalm number as the app shows it, in the Septuagint numbering.
fn lxx_psalm(shown: u8) -> u8 {
    if lxx::numbering() || shown >= 151 { shown } else { lxx::to_lxx(shown, 1).0 }
}

/// A new item of kind `k` (an index into [`ADD`]): its title, kind and note.
fn new_item(k: usize) -> (String, Kind, &'static str) {
    let plan = "psalter-20";
    match k {
        0 => ("Prayers".into(), Kind::Prayers { prayer: None, text: None }, "From your prayer book"),
        1 => ("The Psalter".into(), Kind::Kathisma, "The day's kathisma"),
        2 => (format!("Psalm {}", shown_psalm(50)), Kind::Psalm { lxx: 50 }, ""),
        3 => ("Scripture".into(), Kind::Readings, "The day's Epistle and Gospel"),
        4 => (
            plans::get(plan).map_or("Reading plan", |p| p.title).into(),
            Kind::Plan { plan: plan.into() },
            "",
        ),
        5 => ("The Fathers".into(), Kind::Fathers, "On the day's Gospel"),
        6 => ("The Jesus Prayer".into(), Kind::JesusPrayer { count: 33 }, ""),
        7 => ("Examination of conscience".into(), Kind::Reflection, "Before sleep"),
        _ => ("Something else".into(), Kind::Other, ""),
    }
}

/// The title an item of `kind` starts with. While it's still the title, it follows the psalm,
/// plan or prayer chosen; a title typed stays.
fn default_title(kind: &Kind) -> Option<String> {
    Some(match kind {
        Kind::Prayers { prayer: Some(id), .. } => prayers::get(id)?.title.into(),
        Kind::Prayers { .. } => "Prayers".into(),
        Kind::Psalm { lxx } => format!("Psalm {}", shown_psalm(*lxx)),
        Kind::Plan { plan } => plans::get(plan)?.title.into(),
        _ => return None,
    })
}

/// The note a prayers item starts with, which follows the choice the same way: "From your
/// prayer book" only while it opens nothing.
fn default_note(kind: &Kind) -> Option<&'static str> {
    match kind {
        Kind::Prayers { prayer: None, text: None } => Some("From your prayer book"),
        Kind::Prayers { .. } => Some(""),
        _ => None,
    }
}

fn prayer_choice(prayer: &Option<String>, own: &Option<String>) -> String {
    match (prayer, own) {
        (Some(id), _) => prayers::get(id).map_or("A built-in prayer", |p| p.title).into(),
        (None, Some(_)) => "Your own words".into(),
        (None, None) => "From your prayer book".into(),
    }
}

/// What an item is (unless its title says so), its season and whether it's optional, under its
/// title; its note when there's nothing else to say.
fn summary(item: &Item) -> String {
    let what = match &item.kind {
        Kind::Prayers { prayer, text } => prayer_choice(prayer, text),
        Kind::Kathisma => "The day's kathisma".into(),
        Kind::Psalm { lxx } => format!("Psalm {}", shown_psalm(*lxx)),
        Kind::Readings => "The day's readings".into(),
        Kind::Plan { plan } => plans::get(plan).map_or(plan.as_str(), |p| p.title).into(),
        Kind::Fathers => "The Fathers on the day's Gospel".into(),
        Kind::JesusPrayer { count } => format!("{count} times"),
        Kind::Reflection => "Examination of conscience".into(),
        Kind::Other => String::new(),
    };
    let mut parts: Vec<String> = Vec::new();
    if !what.is_empty() && what != item.title.trim() {
        parts.push(what);
    }
    match item.season {
        Season::Always => {}
        Season::GreatLent => parts.push("Great Lent only".into()),
        Season::OutsideLent => parts.push("Outside Lent".into()),
    }
    if item.optional {
        parts.push("Optional".into());
    }
    if parts.is_empty() { item.note.clone() } else { parts.join(" · ") }
}

/// A number field: the number as typed, taken into `value` once it reads as one in `range`.
#[allow(clippy::too_many_arguments)]
fn number_field(
    ui: &mut Ui,
    p: &Palette,
    r: Rect,
    numbers: &mut HashMap<String, String>,
    key: String,
    value: &mut u16,
    range: std::ops::RangeInclusive<u16>,
) {
    let id = Id::new(("rule-number", &key));
    let typed = numbers.entry(key).or_insert_with(|| value.to_string());
    let read = |s: &str| s.trim().parse::<u16>().ok().filter(|n| range.contains(n));
    let bad = read(typed).is_none();
    widgets::text_field(ui, p, r, typed, "", None, false, bad, id);
    if let Some(n) = read(typed) {
        *value = n;
    }
}

impl Editor {
    /// The editor for `rule`, with the reminders on or off.
    pub fn new(rule: Rule, reminders: bool) -> Self {
        Self {
            blocks: drafts(&rule),
            saved: rule,
            open: None,
            numbers: HashMap::new(),
            reminders,
            saved_reminders: reminders,
        }
    }

    /// Something would be lost by leaving without saving.
    pub fn changed(&self) -> bool {
        self.rule().is_none_or(|r| r != self.saved) || self.reminders != self.saved_reminders
    }

    /// Opens the item `id` (demo captures).
    pub fn open(&mut self, id: &str) {
        self.open = Some(id.to_string());
    }

    /// The rule as edited, or `None` while a time doesn't read as one.
    fn rule(&self) -> Option<Rule> {
        let blocks = self
            .blocks
            .iter()
            .map(|d| Some(Block { time: typed_time(&d.time)?, ..d.block.clone() }))
            .collect::<Option<Vec<_>>>()?;
        Some(Rule { blocks })
    }

    fn taken(&self) -> HashSet<String> {
        let rules = self.blocks.iter().map(|d| &d.block).chain(&self.saved.blocks);
        rules
            .flat_map(|b| std::iter::once(b.id.clone()).chain(b.items.iter().map(|i| i.id.clone())))
            .collect()
    }

    /// Draws the editor in the page's `content`; returns its bottom and whether to go back.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        content: Rect,
        env: &PageEnv<'_>,
        actions: &mut Vec<Action>,
    ) -> (f32, bool) {
        let p = env.settings.theme.palette();
        let painter = ui.painter().clone();
        let left = content.left();
        let width = content.width().min(640.0);
        let right = left + width;
        let mut close = crumb(ui, p, left, content.top(), width, "Your rule");
        let top = content.top() + 36.0;
        super::title(&painter, p, left, top, "Your rule", "Changes to your rule apply when you save.");
        // Saved only while every time reads as one.
        let edited = self.rule();
        let switched = self.reminders != self.saved_reminders;
        let changed = edited.as_ref().is_some_and(|r| *r != self.saved || switched);
        let (save, resp) = Button::new(ButtonKind::Primary, "Save").enabled(changed).show_right(
            ui,
            p,
            right,
            top + 18.0,
            ui.id().with("save-rule"),
        );
        if resp.clicked()
            && let Some(rule) = edited
        {
            if rule != self.saved {
                actions.push(Action::SaveRule(Box::new(rule)));
            }
            if switched {
                actions.push(Action::SetRuleReminders(self.reminders));
            }
            close = true;
        }
        close |= Button::new(ButtonKind::Plain, "Cancel")
            .show_right(ui, p, save.left() - 8.0, top + 18.0, ui.id().with("cancel-rule"))
            .1
            .clicked();

        // Reminders.
        let mut y = top + 58.0 + 24.0;
        let note = "A message in the app at each block's time, while it's open";
        let font = theme::ui_font(12.0);
        let lines = widgets::flow(&painter, note, &font, 12.0, width - 32.0 - 40.0 - 12.0, 3);
        let card = Rect::from_min_size(pos2(left, y), vec2(width, 44.0 + 16.0 * lines.len() as f32));
        painter.rect_filled(card, 12.0, p.surface);
        painter.rect_stroke(card, 12.0, Stroke::new(0.5, p.border), StrokeKind::Outside);
        text(&painter, left + 16.0, y + 12.0, 20.0, "Reminders", theme::ui_medium(13.0), p.ink, false);
        for (i, runs) in lines.iter().enumerate() {
            let base = widgets::css_baseline(y + 32.0 + 16.0 * i as f32, 16.0, 12.0, false);
            widgets::paint_runs(&painter, left + 16.0, base, runs, p.ink_3);
        }
        let at = pos2(card.right() - 16.0, card.center().y);
        widgets::toggle(ui, p, at, ui.id().with("rule-reminders"), &mut self.reminders);
        y = card.bottom() + 24.0;

        // The blocks, each a card.
        let mut change = None;
        let Editor { blocks, open, numbers, .. } = &mut *self;
        let count = blocks.len();
        for (bi, draft) in blocks.iter_mut().enumerate() {
            let bg = painter.add(egui::Shape::Noop);
            let card_top = y;
            let cy = y + 28.0;
            // The name and time fields share what the arrows leave.
            let room = width - 16.0 - 8.0 - 3.0 * 32.0 - 16.0;
            let time_w = 104.0f32.min(room / 2.0);
            let name_w = (room - 8.0 - time_w).min(220.0);
            let field = Rect::from_min_size(pos2(left + 16.0, cy - 16.0), vec2(name_w, 32.0));
            let id = Id::new(("rule-block", &draft.block.id));
            widgets::text_field(
                ui,
                p,
                field,
                &mut draft.block.title,
                "Name",
                None,
                false,
                false,
                id.with("title"),
            );
            let clock_field = Rect::from_min_size(pos2(field.right() + 8.0, cy - 16.0), vec2(time_w, 32.0));
            let bad = typed_time(&draft.time).is_none();
            let icon = Some(icons::clock as widgets::IconFn);
            widgets::text_field(
                ui,
                p,
                clock_field,
                &mut draft.time,
                "No time",
                icon,
                false,
                bad,
                id.with("time"),
            );
            // What a time looks like: beside the fields when there's room before the arrows,
            // else under them.
            let mut below = 0.0;
            if bad {
                let hint = "A time like 7:00 or 20:30";
                let hw = painter.layout_no_wrap(hint.into(), theme::ui_font(12.0), p.danger).size().x;
                let end = right - 16.0 - 3.0 * 32.0 - 8.0;
                let (hx, hy) = if clock_field.right() + 12.0 + hw <= end {
                    (clock_field.right() + 12.0, cy - 8.0)
                } else {
                    below = 22.0;
                    let x = if clock_field.left() + hw <= end { clock_field.left() } else { left + 16.0 };
                    (x, clock_field.bottom() + 6.0)
                };
                text(&painter, hx, hy, 16.0, hint, theme::ui_font(12.0), p.danger, false);
            }
            if let Some(c) = arrows(ui, p, right - 16.0, cy, id, bi, count) {
                change = Some(match c {
                    Arrow::Up => Change::MoveBlock(bi, bi - 1),
                    Arrow::Down => Change::MoveBlock(bi, bi + 1),
                    Arrow::Remove => Change::RemoveBlock(bi),
                });
            }
            y += 56.0 + below;

            let items = draft.block.items.len();
            for (ii, item) in draft.block.items.iter_mut().enumerate() {
                painter.hline(left + 16.0..=right, y + 0.5, Stroke::new(1.0, p.separator));
                y += 1.0;
                let row = Rect::from_min_size(pos2(left, y), vec2(width, 52.0));
                let id = Id::new(("rule-edit-item", &item.id));
                let is_open = open.as_deref() == Some(item.id.as_str());
                let resp = ui.interact(row, id, Sense::click());
                if !is_open && ui.rect_contains_pointer(row) {
                    painter.rect_filled(row.shrink2(vec2(4.0, 2.0)), 8.0, p.fill);
                }
                let chev = Rect::from_center_size(pos2(left + 22.0, row.center().y), vec2(12.0, 12.0));
                if is_open {
                    icons::chevron_down(&painter, chev, p.ink_3);
                } else {
                    icons::disclosure(&painter, chev, p.ink_3);
                }
                let x = left + 40.0;
                let room = right - 16.0 - 3.0 * 32.0 - 8.0 - x;
                let title = if item.title.trim().is_empty() { "Untitled" } else { item.title.as_str() };
                let title = super::elide(&painter, title, &theme::ui_medium(13.0), room);
                let sum = super::elide(&painter, &summary(item), &theme::ui_font(12.0), room);
                let title_top = if sum.is_empty() { row.top() + 16.0 } else { row.top() + 8.0 };
                text(&painter, x, title_top, 20.0, &title, theme::ui_medium(13.0), p.ink, false);
                text(&painter, x, row.top() + 28.0, 16.0, &sum, theme::ui_font(12.0), p.ink_3, false);
                if let Some(c) = arrows(ui, p, right - 16.0, row.center().y, id, ii, items) {
                    change = Some(match c {
                        Arrow::Up => Change::MoveItem(bi, ii, ii - 1),
                        Arrow::Down => Change::MoveItem(bi, ii, ii + 1),
                        Arrow::Remove => Change::RemoveItem(bi, ii),
                    });
                } else if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    *open = if is_open { None } else { Some(item.id.clone()) };
                }
                y += 52.0;
                if is_open {
                    y = item_fields(ui, p, item, numbers, left, right, y);
                }
            }

            painter.hline(left + 16.0..=right, y + 0.5, Stroke::new(1.0, p.separator));
            y += 1.0;
            let add = Button::new(ButtonKind::Ghost, "Add item").small().icon(icons::plus);
            let (_, resp) =
                add.show_left(ui, p, left + 16.0 - add.rest_inset(), y + 22.0, id.with("add-item"));
            crate::menu::popup(&resp, p).show(|ui| {
                ui.set_min_width(220.0);
                for (k, label) in ADD.iter().enumerate() {
                    if ui.button(*label).clicked() {
                        change = Some(Change::AddItem(bi, k));
                        ui.close();
                    }
                }
            });
            y += 44.0;
            let card = Rect::from_min_max(pos2(left, card_top), pos2(right, y));
            painter.set(
                bg,
                egui::epaint::RectShape::new(
                    card,
                    12.0,
                    p.surface,
                    Stroke::new(0.5, p.border),
                    StrokeKind::Outside,
                ),
            );
            y += 16.0;
        }

        let (_, resp) = Button::new(ButtonKind::Secondary, "Add block").icon(icons::plus).show_left(
            ui,
            p,
            left,
            y + 16.0,
            ui.id().with("add-block"),
        );
        if resp.clicked() {
            change = Some(Change::AddBlock);
        }
        let reset = Button::new(ButtonKind::Ghost, "Reset to the starter rule");
        let (_, resp) =
            reset.show_right(ui, p, right + reset.rest_inset(), y + 16.0, ui.id().with("reset-rule"));
        if resp.clicked() {
            change = Some(Change::Reset);
        }
        y += 32.0;

        match change {
            Some(Change::MoveBlock(a, b)) => self.blocks.swap(a, b),
            Some(Change::RemoveBlock(i)) => {
                self.blocks.remove(i);
            }
            Some(Change::MoveItem(b, i, j)) => self.blocks[b].block.items.swap(i, j),
            Some(Change::RemoveItem(b, i)) => {
                self.blocks[b].block.items.remove(i);
            }
            Some(Change::AddItem(b, k)) => {
                let id = new_id("item", &self.taken());
                let (title, kind, note) = new_item(k);
                self.blocks[b].block.items.push(Item {
                    id: id.clone(),
                    title,
                    kind,
                    note: note.into(),
                    season: Season::Always,
                    optional: false,
                });
                self.open = Some(id);
            }
            Some(Change::AddBlock) => {
                let id = new_id("block", &self.taken());
                self.blocks.push(Draft {
                    block: Block { id, title: "New block".into(), time: None, items: Vec::new() },
                    time: String::new(),
                });
            }
            Some(Change::Reset) => {
                self.blocks = drafts(&Rule::starter());
                self.open = None;
                self.numbers.clear();
            }
            None => {}
        }
        (y, close)
    }
}

enum Arrow {
    Up,
    Down,
    Remove,
}

/// Move up, move down and remove, as round buttons ending at `right`, centred on `cy`; the
/// first of `n` has no up and the last no down.
fn arrows(ui: &mut Ui, p: &Palette, right: f32, cy: f32, id: Id, i: usize, n: usize) -> Option<Arrow> {
    let at = |k: f32| Rect::from_min_size(pos2(right - 28.0 - 32.0 * k, cy - 14.0), vec2(28.0, 28.0));
    let mut out = None;
    if widgets::round_button(ui, p, at(0.0), icons::trash, id.with("remove"))
        .on_hover_text("Remove")
        .clicked()
    {
        out = Some(Arrow::Remove);
    }
    if i + 1 < n
        && widgets::round_button(ui, p, at(1.0), icons::chevron_down, id.with("down"))
            .on_hover_text("Move down")
            .clicked()
    {
        out = Some(Arrow::Down);
    }
    if i > 0
        && widgets::round_button(ui, p, at(2.0), icons::chevron_up, id.with("up"))
            .on_hover_text("Move up")
            .clicked()
    {
        out = Some(Arrow::Up);
    }
    out
}

/// An open item's fields from `top`: its title and note, what it opens, its season and whether
/// it's optional. Returns the bottom.
fn item_fields(
    ui: &mut Ui,
    p: &Palette,
    item: &mut Item,
    numbers: &mut HashMap<String, String>,
    left: f32,
    right: f32,
    top: f32,
) -> f32 {
    let painter = ui.painter().clone();
    let id = Id::new(("rule-item-field", &item.id));
    let x = left + FIELD_X;
    let w = right - 16.0 - x;
    let mut y = top;
    let label = |y: f32, s: &str| {
        text(&painter, left + 40.0, y + 12.0, 20.0, s, theme::ui_font(13.0), p.ink_2, false)
    };
    let row = |y: f32, w: f32| Rect::from_min_size(pos2(x, y + 6.0), vec2(w, 32.0));

    label(y, "Title");
    widgets::text_field(ui, p, row(y, w), &mut item.title, "Title", None, false, false, id.with("title"));
    y += 44.0;
    label(y, "Note");
    widgets::text_field(
        ui,
        p,
        row(y, w),
        &mut item.note,
        "A line under the title",
        None,
        false,
        false,
        id.with("note"),
    );
    y += 44.0;

    let about = |y: f32, s: &str| {
        let s = super::elide(&painter, s, &theme::ui_font(13.0), w);
        text(&painter, x, y + 12.0, 20.0, &s, theme::ui_font(13.0), p.ink_3, false);
    };
    let was = (default_title(&item.kind), default_note(&item.kind));
    match &mut item.kind {
        Kind::Prayers { prayer, text: own } => {
            label(y, "Prayer");
            let value = prayer_choice(prayer, own);
            let resp = widgets::select_field(ui, p, row(y, w.min(260.0)), &value, None, id.with("prayer"));
            crate::menu::popup(&resp, p).show(|ui| {
                ui.set_min_width(260.0);
                for pr in prayers::all() {
                    if ui.selectable_label(prayer.as_deref() == Some(pr.id), pr.title).clicked() {
                        (*prayer, *own) = (Some(pr.id.to_string()), None);
                    }
                }
                crate::menu::separator(ui);
                if ui.selectable_label(prayer.is_none() && own.is_none(), "From your prayer book").clicked() {
                    (*prayer, *own) = (None, None);
                }
                if ui.selectable_label(prayer.is_none() && own.is_some(), "Your own words").clicked() {
                    *prayer = None;
                    own.get_or_insert_with(String::new);
                }
            });
            y += 44.0;
            if let Some(own) = own {
                label(y, "Words");
                y = words(ui, p, own, Rect::from_min_size(pos2(x, y + 6.0), vec2(w, 0.0)), id.with("words"))
                    + 6.0;
            }
        }
        Kind::Psalm { lxx } => {
            label(y, "Psalm");
            let mut n = u16::from(shown_psalm(*lxx));
            number_field(ui, p, row(y, 72.0), numbers, format!("{}:psalm", item.id), &mut n, 1..=151);
            *lxx = lxx_psalm(n as u8);
            if lxx::numbering() {
                let hint = super::elide(&painter, "Septuagint numbering", &theme::ui_font(12.0), w - 84.0);
                text(&painter, x + 72.0 + 12.0, y + 12.0, 20.0, &hint, theme::ui_font(12.0), p.ink_3, false);
            }
            y += 44.0;
        }
        Kind::Plan { plan } => {
            label(y, "Plan");
            let value = plans::get(plan).map_or(plan.as_str(), |p| p.title).to_string();
            let resp = widgets::select_field(ui, p, row(y, w.min(300.0)), &value, None, id.with("plan"));
            crate::menu::popup(&resp, p).show(|ui| {
                ui.set_min_width(300.0);
                for pl in plans::catalogue() {
                    if ui.selectable_label(plan == pl.id, pl.title).clicked() {
                        *plan = pl.id.to_string();
                    }
                }
            });
            y += 44.0;
        }
        Kind::JesusPrayer { count } => {
            label(y, "Times");
            number_field(ui, p, row(y, 72.0), numbers, format!("{}:count", item.id), count, 1..=1000);
            y += 44.0;
        }
        Kind::Kathisma => {
            about(y, "At Matins in a block before noon, at Vespers in the others.");
            y += 44.0;
        }
        Kind::Readings => {
            about(y, "The day's Epistle and Gospel. Read opens the Gospel.");
            y += 44.0;
        }
        Kind::Fathers => {
            about(y, "The Church Fathers on the day's Gospel, beside the reader.");
            y += 44.0;
        }
        Kind::Reflection => {
            about(y, "A few lines for looking back over the day.");
            y += 44.0;
        }
        Kind::Other => {}
    }
    let now = (default_title(&item.kind), default_note(&item.kind));
    if now.0 != was.0
        && was.0.as_deref() == Some(item.title.trim())
        && let Some(title) = now.0
    {
        item.title = title;
    }
    if now.1 != was.1
        && was.1 == Some(item.note.trim())
        && let Some(note) = now.1
    {
        item.note = note.into();
    }

    // The season: a segmented control, or a menu where that doesn't fit.
    label(y, "When");
    let labels: Vec<&str> = SEASONS.iter().map(|(_, l)| *l).collect();
    let selected = SEASONS.iter().position(|(s, _)| *s == item.season).unwrap_or(0);
    let sw = widgets::segmented_tight_width(&painter, &labels, selected);
    if sw <= w {
        let (_, pick) =
            widgets::segmented_tight(ui, p, pos2(x + sw, y + 22.0), id.with("season"), &labels, selected);
        if let Some(i) = pick {
            item.season = SEASONS[i].0;
        }
    } else {
        let resp = widgets::select_field(ui, p, row(y, w), labels[selected], None, id.with("season"));
        crate::menu::popup(&resp, p).show(|ui| {
            for (season, l) in SEASONS {
                if ui.selectable_label(item.season == season, l).clicked() {
                    item.season = season;
                }
            }
        });
    }
    y += 44.0;
    let b = Rect::from_min_size(pos2(x, y + 6.0), vec2(16.0, 16.0));
    if widgets::checkbox(ui, p, b, item.optional, "Optional").clicked() {
        item.optional = !item.optional;
    }
    // What optional means: after the box, or under it when there isn't room.
    let lw = painter.layout_no_wrap("Optional".into(), theme::text::UI.font(), p.ink).size().x;
    let hint = "Done or not, it doesn't decide whether the day was kept";
    let font = theme::ui_font(12.0);
    let hx = b.right() + 8.0 + lw + 12.0;
    if painter.layout_no_wrap(hint.into(), font.clone(), p.ink_3).size().x <= right - 16.0 - hx {
        text(&painter, hx, y + 4.0, 20.0, hint, font, p.ink_3, false);
        return y + 36.0;
    }
    let lines = widgets::flow(&painter, hint, &font, 12.0, w, 3);
    for (i, runs) in lines.iter().enumerate() {
        let base = widgets::css_baseline(y + 30.0 + 18.0 * i as f32, 18.0, 12.0, false);
        widgets::paint_runs(&painter, x, base, runs, p.ink_3);
    }
    y + 36.0 + 18.0 * lines.len() as f32
}

/// A field for the person's own prayer: several lines, as tall as the words need.
fn words(ui: &mut Ui, p: &Palette, own: &mut String, at: Rect, id: Id) -> f32 {
    let focused = ui.memory(|m| m.has_focus(id));
    let h = ui.ctx().data(|d| d.get_temp::<f32>(id.with("h"))).unwrap_or(60.0).max(60.0);
    let field = Rect::from_min_size(at.min, vec2(at.width(), h + 16.0));
    let painter = ui.painter();
    if focused {
        painter.rect_filled(field.expand(3.0), 11.0, p.focus);
    }
    painter.rect_filled(field, 8.0, p.field);
    let ring = if focused { p.accent } else { p.border };
    painter.rect_stroke(field, 8.0, Stroke::new(1.0, ring), StrokeKind::Inside);
    let inner = field.shrink2(vec2(10.0, 8.0));
    let resp = ui
        .scope_builder(egui::UiBuilder::new().max_rect(inner.with_max_y(inner.top() + 400.0)), |ui| {
            ui.visuals_mut().text_cursor.stroke = Stroke::new(1.5, p.accent);
            ui.add(
                egui::TextEdit::multiline(own)
                    .id(id)
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .desired_width(inner.width())
                    .desired_rows(3)
                    .font(theme::ui_font(13.0))
                    .text_color(p.ink)
                    .hint_text(
                        egui::RichText::new("Type your prayer").font(theme::ui_font(13.0)).color(p.ink_3),
                    ),
            )
        })
        .inner;
    ui.ctx().data_mut(|d| d.insert_temp(id.with("h"), resp.rect.height()));
    field.bottom()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_as_typed() {
        assert_eq!(typed_time("7:00"), Some(Some("07:00".into())));
        assert_eq!(typed_time("20.30"), Some(Some("20:30".into())));
        assert_eq!(typed_time("2030"), Some(Some("20:30".into())));
        assert_eq!(typed_time("7"), Some(Some("07:00".into())));
        assert_eq!(typed_time("  "), Some(None));
        assert_eq!(typed_time("25:00"), None);
        assert_eq!(typed_time("noon"), None);
        // Characters of more than one byte don't split the string mid-character.
        for s in ["•", "é7", "–", "7:3–", "١٢"] {
            assert_eq!(typed_time(s), None, "{s}");
        }
        assert_eq!(minutes("20:30"), Some(20 * 60 + 30));
        assert_eq!((clock(7 * 60), clock(20 * 60 + 30)), ("7:00".into(), "20:30".into()));
    }

    #[test]
    fn new_items_start_with_the_title_that_follows_their_choice() {
        for k in [0, 2, 4] {
            let (title, kind, _) = new_item(k);
            assert_eq!(default_title(&kind), Some(title));
        }
        let (_, kind, note) = new_item(0);
        assert_eq!(default_note(&kind), Some(note));
    }

    #[test]
    fn an_unchanged_rule_saves_as_it_was_and_new_ids_are_new() {
        let mut editor = Editor::new(Rule::starter(), false);
        assert_eq!(editor.rule(), Some(Rule::starter()));
        assert!(!editor.changed());
        editor.reminders = true;
        assert!(editor.changed(), "the reminders switch is part of the draft");
        let taken = editor.taken();
        assert!(taken.contains("morning") && taken.contains("evening-kathisma"));
        assert!(!taken.contains(&new_id("item", &taken)));
    }
}
