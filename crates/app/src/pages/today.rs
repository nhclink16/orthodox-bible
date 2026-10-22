//! Today in the Church (Orthodox fork): the day's feasts and readings, the prayer rule with its
//! check boxes (`rule`), the reading plans under way, the fast, the Psalter's kathismata and
//! what's coming up, from `bible_core::calendar` and `bible_core::lectionary`; beside them a
//! month of fasts, feasts and days the rule was kept. Days step back and forward; the church's
//! calendar (New or Old) is a setting. At the top, the part of the rule due now, with Begin:
//! its items, the readings, the Psalter and the plans all open as steps in the page's place
//! (`step`).

use std::collections::{HashMap, HashSet};

use eframe::egui::{
    self, Align2, Color32, FontId, Key, Modifiers, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};

use bible_core::calendar::{self, Day, Fast, Style};
use bible_core::lectionary::{self, Confidence, Reading};
use bible_core::plans;
use bible_core::rule::{Kind as ItemKind, Now, Rule, kathismata, minutes};
use bible_core::study::HighlightColor;

use super::rule::{self as rule_view, Pick, View, clock};
use super::step::{self, Session, Walk};
use super::{PageEnv, group_label, text};
use crate::actions::Action;
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind};

const WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// How far ahead "Coming up" looks.
const AHEAD: i64 = 120;
/// The month's width, and the gap before it when it stands beside the day.
const MONTH_W: f32 = 320.0;
const MONTH_GAP: f32 = 48.0;

#[derive(Default)]
pub struct State {
    /// Days from today.
    offset: i64,
    /// The month the calendar shows, when it isn't the shown day's: (year, month).
    month: Option<(i32, u8)>,
    /// A prayer, the Jesus Prayer's counter or the rule's editor, in the page's place.
    view: Option<View>,
    /// A `today:` demo step, for the next frame.
    demo: Option<String>,
    /// How far the day was scrolled when a view opened, and that place to go back to once it
    /// has closed.
    scrolled: f32,
    restore: Option<f32>,
    /// A day was picked lower down the page (the month, Coming up): show it from the top.
    to_top: bool,
    /// The day whose readings all show, when it has more than [`FULL`] and they were asked for.
    all_readings: Option<Day>,
    /// The block (or readings) begun: shown while `view` is [`View::Step`], else paused.
    session: Option<Session>,
    /// The blocks whose end page was reached, on that day: the Now card passes over them.
    ended: (Day, HashSet<String>),
    /// The Jesus Prayer's count for each counter on each day, kept whichever block is begun.
    counts: HashMap<(Day, String), u16>,
    /// A block to begin on the next frame (a reminder's Begin, the palette), by id.
    begin: Option<String>,
    /// Points to scroll down on the next frame (the arrow and page keys).
    scroll: f32,
}

pub(super) fn today() -> Day {
    let t = crate::time::local(bible_core::study::now());
    calendar::civil(t.year, t.month, t.day)
}

fn date_label(d: Day) -> String {
    let (y, m, dd) = calendar::civil_date(d);
    format!("{}, {dd} {} {y}", WEEKDAYS[calendar::weekday(d) as usize], MONTHS[m as usize - 1])
}

pub(super) fn short_date(d: Day) -> String {
    let (_, m, dd) = calendar::civil_date(d);
    format!("{} {dd} {}", &WEEKDAYS[calendar::weekday(d) as usize][..3], &MONTHS[m as usize - 1][..3])
}

/// A feast to show as a great day: the Triodion's and Pentecostarion's Sundays and Holy Week's
/// days are left to the day itself.
fn great(feast: &str) -> bool {
    !feast.starts_with("Sunday of") && !feast.starts_with("Holy ") && !feast.starts_with("Monday of")
}

/// A fast's colour on the month: the highlight swatches, from dairy's yellow to Lent's purple.
fn fast_colour(p: &Palette, fast: Fast) -> Option<Color32> {
    let c = match fast {
        Fast::None => return None,
        Fast::NoMeat => HighlightColor::Yellow,
        Fast::Fish => HighlightColor::Blue,
        Fast::WineOil => HighlightColor::Green,
        Fast::Wine => HighlightColor::Pink,
        Fast::Strict => HighlightColor::Purple,
    };
    Some(p.swatch(c))
}

/// The month's legend for the fasts.
const FASTS: [(Fast, &str); 5] = [
    (Fast::NoMeat, "No meat"),
    (Fast::Fish, "Fish"),
    (Fast::WineOil, "Wine and oil"),
    (Fast::Wine, "Wine"),
    (Fast::Strict, "Strict fast"),
];

/// A great feast's mark: a small cross centred on `c`.
pub(super) fn cross(painter: &egui::Painter, c: egui::Pos2, color: Color32) {
    let s = Stroke::new(1.25, color);
    painter.line_segment([c - vec2(0.0, 4.0), c + vec2(0.0, 4.0)], s);
    painter.line_segment([c + vec2(-2.75, -1.5), c + vec2(2.75, -1.5)], s);
}

/// The great days and fast beginnings in the `AHEAD` days after `from`.
fn coming_up(from: Day, style: Style) -> Vec<(Day, String)> {
    let mut out = Vec::new();
    let mut season = calendar::day(from, style).season;
    for d in from + 1..=from + AHEAD {
        let info = calendar::day(d, style);
        if info.season != season
            && let Some(s) = info.season
            && (s.ends_with("Fast") || s == "Great Lent")
        {
            out.push((d, format!("{s} begins")));
        }
        season = info.season;
        for f in info.feasts {
            if great(f) {
                out.push((d, f.to_string()));
            }
        }
        if out.len() >= 6 {
            break;
        }
    }
    out.truncate(6);
    out
}

/// `s` wrapped to `width` on `lh` lines from `top`; returns the height used.
#[allow(clippy::too_many_arguments)]
pub(super) fn paragraph(
    painter: &eframe::egui::Painter,
    s: &str,
    font: &FontId,
    lh: f32,
    x: f32,
    top: f32,
    width: f32,
    color: eframe::egui::Color32,
) -> f32 {
    let reading = matches!(&font.family, eframe::egui::FontFamily::Name(n) if n.starts_with("serif"));
    let lines = widgets::flow(painter, s, font, font.size, width, 4);
    for (i, runs) in lines.iter().enumerate() {
        let base = widgets::css_baseline(top + i as f32 * lh, lh, font.size, reading);
        widgets::paint_runs(painter, x, base, runs, color);
    }
    lines.len() as f32 * lh
}

/// Whether a reading's occasion is a day of the moving cycle, named by its week or its place in
/// it ("Tuesday of the 5th week of Luke", "21st Sunday after Pentecost", "Palm Sunday", "Great
/// and Holy Friday", "Bright Monday") rather than by a saint or a fixed feast.
fn moving(occasion: &str) -> bool {
    const PLACES: [&str; 7] = [
        "Great and Holy",
        "Holy Pascha",
        "Holy Pentecost",
        "Bright ",
        "Eothinon",
        "Mid-Pentecost",
        "Ascension",
    ];
    let mut words = occasion.split(' ');
    let (first, second) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
    WEEKDAYS.contains(&first)
        || WEEKDAYS.contains(&second)
        || first.starts_with(|c: char| c.is_ascii_digit())
        || PLACES.iter().any(|p| occasion.starts_with(p))
}

/// Whether a day of the moving cycle is an ordinary day of its week ("Tuesday of the 5th week of
/// Luke", "21st Sunday after Pentecost", "Bright Monday", "Great and Holy Friday"), which says
/// nothing of why its readings are read; a named day ("Second Saturday of Great Lent (Saturday
/// of Souls)", "Sunday before Theophany") does.
fn ordinary(occasion: &str) -> bool {
    let head = occasion.split(" (").next().unwrap_or(occasion).to_lowercase();
    head.contains("week")
        || head.contains(" after the publican")
        || head.starts_with(|c: char| c.is_ascii_digit())
        || ["bright ", "great and holy", "eothinon"].iter().any(|p| head.starts_with(p))
}

/// An occasion's name, without what follows a colon or the day's other saints and feasts:
/// "Eve of Theophany: Vespers with…", "Forefeast of Theophany; St Sylvester". What's in brackets
/// stays whole.
fn name(occasion: &str) -> &str {
    let mut depth = 0;
    for (i, c) in occasion.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ':' | ';' if depth == 0 => return &occasion[..i],
            _ => {}
        }
    }
    occasion
}

fn capitalized(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

/// What a Liturgy reading is read for when the page doesn't say already: its title ("St
/// Artemius"), else the saint, feast or named day it belongs to. A title that only tells two
/// lessons apart ("of the saint", "forefeast") says whose. On a great `feast` the feast named
/// above is the day, and so is a day of the moving cycle when the day section `named` one.
fn read_for(r: &Reading, feast: bool, named: bool) -> Option<String> {
    let told = feast || (named && moving(r.occasion));
    match r.title {
        Some(t) if t.starts_with(char::is_lowercase) => {
            // "forefeast" in "Synaxis of the Seventy Apostles; forefeast of Theophany".
            let word = t.split(' ').next().unwrap_or(t);
            let part = r.occasion.split("; ").find(|p| p.to_lowercase().starts_with(word));
            let short = |s: &'static str| s.split(" (").next().unwrap_or(s);
            Some(match part {
                Some(part) => capitalized(short(part)),
                None if told => t.to_string(),
                None => format!("{} · {t}", short(name(r.occasion))),
            })
        }
        Some(t) => Some(t.to_string()),
        None => (!told && !ordinary(r.occasion)).then(|| name(r.occasion).to_string()),
    }
}

/// How many of the other services' readings show in full. A day with more (Holy Week, the eves
/// of the Nativity and Theophany: up to 30) shows each service's first until asked for all, so
/// the rule stays near the top.
const FULL: usize = 9;

/// The day's `lessons` (`bible_core::lectionary`) at `top`: the Liturgy's Epistle and Gospel
/// first, a row each with what it's read for (`feast` and `named` as for [`read_for`]), then the
/// other services' readings smaller, under each service, folded on a day of many unless `all`.
/// A row opens its reading as a step; a reading the tables are unsure of says to check the
/// parish calendar, first. Returns the bottom, and the reading clicked.
#[allow(clippy::too_many_arguments)]
fn readings(
    ui: &mut Ui,
    env: &PageEnv<'_>,
    left: f32,
    width: f32,
    top: f32,
    lessons: &[Reading],
    (feast, named): (bool, bool),
    all: &mut bool,
) -> (f32, Option<usize>) {
    let p = env.settings.theme.palette();
    let painter = ui.painter().clone();
    let right = left + width;
    group_label(&painter, p, left, top, "Readings");
    let mut y = top + 26.0;
    let mut picked = None;
    if lessons.is_empty() {
        text(&painter, left, y, 20.0, "No readings for this day.", theme::ui_font(13.0), p.ink_2, false);
        return (y + 20.0, None);
    }
    const CHECK: &str = "check your parish calendar";
    let unsure = |r: &Reading| r.confidence == Confidence::Low;
    let liturgy = |r: &Reading| r.at_liturgy() && r.kind != lectionary::Kind::OldTestament;
    // A row that opens a reading, with its fill and focus ring; returns whether it was clicked.
    let row = |ui: &mut Ui, r: Rect, i: usize| {
        let resp = ui.interact(r, ui.id().with(("reading", i)), Sense::click());
        if let Some(fill) = widgets::hover_fill(ui, &resp, p, false) {
            painter.rect_filled(r, 10.0, fill);
        }
        widgets::focus_ring(&painter, p, &resp, r, 10.0);
        resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
    };
    // Every passage starts in one column, after the widest kind ("Old Testament").
    let kind_w = painter.layout_no_wrap("Old Testament".into(), theme::ui_font(12.0), p.ink_3).size().x;
    let x = left + (12.0 + kind_w + 16.0).max(96.0);

    // The Liturgy's: the kind, the passage and under it what it's read for.
    for (i, r) in lessons.iter().enumerate().filter(|(_, r)| liturgy(r)) {
        let at = Rect::from_min_size(pos2(left, y), vec2(width, 48.0));
        let clicked = row(ui, at, i);
        // The hint first, so a narrow window cuts the rest.
        let mut note: Vec<String> = Vec::new();
        if unsure(r) {
            note.push(CHECK.into());
        }
        if r.service != "Liturgy" {
            note.push(r.service.into());
        }
        note.extend(read_for(r, feast, named));
        note.extend(r.served.map(String::from));
        let note = capitalized(&note.join(" · "));
        let ty = if note.is_empty() { at.top() + 14.0 } else { at.top() + 5.0 };
        text(&painter, left + 12.0, ty, 20.0, r.kind.label(), theme::ui_semibold(13.0), p.ink, false);
        let read = text(
            &painter,
            right - 12.0,
            at.top() + 14.0,
            20.0,
            "Read",
            theme::ui_medium(13.0),
            p.accent,
            true,
        );
        let room = right - 12.0 - read - 16.0 - x;
        let label = super::elide(&painter, &r.label(), &theme::ui_medium(14.0), room);
        text(&painter, x, ty, 20.0, &label, theme::ui_medium(14.0), p.ink, false);
        if !note.is_empty() {
            let note = super::elide(&painter, &note, &theme::ui_font(12.0), room);
            text(&painter, x, at.top() + 25.0, 18.0, &note, theme::ui_font(12.0), p.ink_3, false);
        }
        if clicked {
            picked = Some(i);
        }
        y += 48.0;
    }

    // The other services', smaller, under each service (and when it's served, if not then).
    let others: Vec<(usize, &Reading)> = lessons.iter().enumerate().filter(|(_, r)| !liturgy(r)).collect();
    let fold = others.len() > FULL && !*all;
    let mut last: Option<(&str, Option<&str>)> = None;
    for (i, r) in others {
        let first = last != Some((r.service, r.served));
        if fold && !first {
            continue;
        }
        if first {
            y += if last.is_some() || y > top + 26.0 { 8.0 } else { 0.0 };
            let head = r.served.map_or(r.service.to_string(), |s| format!("{} · {s}", r.service));
            let head = super::elide(&painter, &head, &theme::ui_semibold(12.0), width - 24.0);
            text(&painter, left + 12.0, y + 4.0, 16.0, &head, theme::ui_semibold(12.0), p.ink_2, false);
            y += 24.0;
            last = Some((r.service, r.served));
        }
        // A reading the tables are unsure of has a second line saying so (and its title), as the
        // Liturgy's do, so neither the hint nor the passage is cut for the other.
        let h = if unsure(r) { 48.0 } else { 32.0 };
        let at = Rect::from_min_size(pos2(left, y), vec2(width, h));
        let clicked = row(ui, at, i);
        let ty = if unsure(r) { at.top() + 5.0 } else { at.top() + 6.0 };
        text(&painter, left + 12.0, ty, 20.0, r.kind.label(), theme::ui_font(12.0), p.ink_3, false);
        let ry = if unsure(r) { at.top() + 14.0 } else { at.top() + 6.0 };
        let read = text(&painter, right - 12.0, ry, 20.0, "Read", theme::ui_medium(12.0), p.accent, true);
        let room = right - 12.0 - read - 16.0 - x;
        let label = super::elide(&painter, &r.label(), &theme::ui_font(13.0), room);
        let w = text(&painter, x, ty, 20.0, &label, theme::ui_font(13.0), p.ink, false);
        if unsure(r) {
            let note = capitalized(&[CHECK].into_iter().chain(r.title).collect::<Vec<_>>().join(" · "));
            let note = super::elide(&painter, &note, &theme::ui_font(12.0), room);
            text(&painter, x, at.top() + 25.0, 18.0, &note, theme::ui_font(12.0), p.ink_3, false);
        } else if let Some(title) = r.title
            && room - w > 40.0
        {
            let title = super::elide(&painter, &format!(" · {title}"), &theme::ui_font(12.0), room - w);
            text(&painter, x + w, ty, 20.0, &title, theme::ui_font(12.0), p.ink_3, false);
        }
        if clicked {
            picked = Some(i);
        }
        y += h;
    }
    if fold {
        let at = Rect::from_min_size(pos2(left, y + 8.0), vec2(width, 32.0));
        if row(ui, at, lessons.len()) {
            *all = true;
        }
        let show = format!("Show all {} readings", lessons.len());
        text(&painter, left + 12.0, at.top() + 6.0, 20.0, &show, theme::ui_medium(12.0), p.accent, false);
        y = at.bottom();
    }
    (y, picked)
}

impl State {
    /// A step of the Jesus Prayer's counter is open (Space counts).
    pub fn counting(&self) -> bool {
        self.stepping() && self.session.as_ref().is_some_and(|s| s.counting)
    }

    /// A view is open in the page's place (Escape and Back go back to Today).
    pub fn viewing(&self) -> bool {
        self.view.is_some()
    }

    /// A step (or a block's end page) is open.
    pub fn stepping(&self) -> bool {
        matches!(self.view, Some(View::Step))
    }

    /// The block (or readings) begun, open or paused.
    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    /// The Jesus Prayer's count so far on day `d` for the counter `item`.
    pub fn count(&self, d: Day, item: &str) -> u16 {
        self.counts.get(&(d, item.to_string())).copied().unwrap_or(0)
    }

    /// Back to Today from a view: a step is paused, for Continue. The rule's editor stays while
    /// it has changes to save, as with Escape.
    pub fn close(&mut self) {
        if !matches!(&self.view, Some(View::Edit(editor)) if editor.changed()) {
            self.view = None;
        }
    }

    /// Begins block `id` today on the next frame, at its first item not done: a reminder's
    /// Begin. Today shows today first.
    pub fn begin(&mut self, id: &str) {
        self.offset = 0;
        self.month = None;
        self.begin = Some(id.to_string());
    }

    /// Back on the paused step (from the reader's Back to Today).
    pub fn resume(&mut self) {
        if let Some(s) = &mut self.session {
            s.resume();
            self.view = Some(View::Step);
        }
    }

    /// Scrolls the page by `dy` points (the arrow and page keys).
    pub fn scroll(&mut self, dy: f32) {
        self.scroll += dy;
    }

    /// `edit` (or `edit:<item id>`, that item open), `day:<offset>`, `count:<n>`, `begin:<block>`,
    /// `done`, `skip`, `verse:<n>` (the Fathers' verse), `scroll:<points>`, `reader` (Open in
    /// reader), `kathisma:<n>`, `reading:<n>` (the day's readings from the n-th, from 0) or an
    /// item's id to open it as a step, on the next frame (demo captures).
    pub fn demo(&mut self, step: &str) {
        self.demo = Some(step.to_string());
    }

    fn run_demo(&mut self, env: &PageEnv<'_>, rule: &Rule) {
        let Some(step) = self.demo.take() else { return };
        let (cmd, arg) = step.split_once(':').unwrap_or((&step, ""));
        let d = today() + self.offset;
        match cmd {
            "edit" => {
                let mut editor = super::rule_editor::Editor::new(rule.clone(), env.settings.rule_reminders);
                if !arg.is_empty() {
                    editor.open(arg);
                }
                self.view = Some(View::Edit(Box::new(editor)));
            }
            "day" => self.offset = arg.parse().unwrap_or(0),
            "count" => {
                if let Some(s) = &self.session
                    && let Some(id) = s.item.clone()
                {
                    self.counts.insert((s.day, id), arg.parse().unwrap_or(0));
                }
            }
            "begin" => self.begin_block(env, rule, arg, d, None),
            "done" | "skip" => {
                if let Some(s) = &mut self.session {
                    s.press = Some(if cmd == "done" { step::Press::Done } else { step::Press::Skip });
                }
            }
            "scroll" => self.scroll += arg.parse::<f32>().unwrap_or(0.0),
            "kathisma" => self.open_kathisma(env, rule, d, arg.parse().unwrap_or(1)),
            "reading" => self.open_walk(Walk::Readings, d, arg.to_string()),
            "verse" => {
                if let Some(s) = &mut self.session {
                    s.demo_verse(arg.parse().unwrap_or(1));
                }
            }
            "reader" => {
                if let Some(s) = &mut self.session {
                    s.demo_reader = true;
                }
            }
            id => {
                let block = rule
                    .for_day(d, env.settings.calendar)
                    .into_iter()
                    .find(|(_, items)| items.iter().any(|i| i.id == id));
                if let Some((b, _)) = block {
                    let b = b.id.clone();
                    self.begin_block(env, rule, &b, d, Some(id));
                }
            }
        }
    }

    /// Begins (or continues) block `block` on day `d`: at `item`, else at the step paused there,
    /// else its first item not done. Beginning another block forgets where the last one was
    /// paused; its ticks stay. With nothing to do, Today stays as it is.
    fn begin_block(&mut self, env: &PageEnv<'_>, rule: &Rule, block: &str, d: Day, item: Option<&str>) {
        let walk = Walk::Block(block.to_string());
        if !self.session.as_ref().is_some_and(|s| s.walk == walk && s.day == d) {
            self.session = Some(Session::new(d, walk, None));
        }
        let Some(s) = &mut self.session else { return };
        let (_, _, steps) = step::steps(&s.walk, d, env, rule);
        let at = match item {
            Some(i) => Some(i.to_string()),
            None => step::resume_at(rule, env, s, &steps),
        };
        if let Some(at) = at {
            if s.item.as_deref() == Some(at.as_str()) {
                s.resume();
            } else {
                s.open(Some(at));
            }
            self.view = Some(View::Step);
        }
    }

    /// Opens `walk` on day `d` at step `item`.
    fn open_walk(&mut self, walk: Walk, d: Day, item: String) {
        let mut s = Session::new(d, walk, None);
        s.open(Some(item));
        self.session = Some(s);
        self.view = Some(View::Step);
    }

    /// The page, or the view open in its place. A view starts at the top; closing it goes back
    /// to where the day was.
    pub fn show(&mut self, ui: &mut Ui, content: Rect, env: &PageEnv<'_>, actions: &mut Vec<Action>) -> f32 {
        let viewing = self.view.is_some();
        let origin = ui.max_rect().top();
        let scroll_to = |ui: &Ui, offset: f32| {
            let at = Rect::from_min_size(pos2(content.left(), origin + offset), vec2(1.0, 1.0));
            ui.scroll_to_rect_animation(at, Some(egui::Align::TOP), egui::style::ScrollAnimation::none());
        };
        if let Some(offset) = self.restore.take() {
            scroll_to(ui, offset);
        }
        let (bottom, scrolled) = self.page(ui, content, env, actions);
        if std::mem::take(&mut self.to_top) {
            scroll_to(ui, 0.0);
        }
        match (viewing, self.view.is_some()) {
            (false, true) => {
                self.scrolled = ui.clip_rect().top() - origin;
                if !scrolled {
                    scroll_to(ui, 0.0);
                }
            }
            (true, false) => self.restore = Some(self.scrolled),
            _ => {}
        }
        let dy = std::mem::take(&mut self.scroll);
        if dy != 0.0 {
            ui.scroll_with_delta_animation(vec2(0.0, -dy), egui::style::ScrollAnimation::duration(0.12));
        }
        bottom
    }

    /// The page's content; returns its bottom and whether a step scrolled the page itself.
    fn page(
        &mut self,
        ui: &mut Ui,
        content: Rect,
        env: &PageEnv<'_>,
        actions: &mut Vec<Action>,
    ) -> (f32, bool) {
        let p = env.settings.theme.palette();
        let painter = ui.painter().clone();
        let (left, right, top) = (content.left(), content.right(), content.top());
        let style = env.settings.calendar;
        let prayer_rule = env.study.rule();
        if let Some(block) = self.begin.take() {
            self.begin_block(env, &prayer_rule, &block, today(), None);
        }
        self.run_demo(env, &prayer_rule);
        if let Some(View::Edit(editor)) = &mut self.view {
            editor.follow(&prayer_rule);
        }
        if self.ended.0 != today() {
            self.ended = (today(), HashSet::new());
        }
        if let (Some(View::Step), Some(s)) = (&self.view, &mut self.session) {
            let shown =
                step::show(s, ui, content, env, &prayer_rule, &self.ended.1, &mut self.counts, actions);
            if let (true, Walk::Block(b)) = (shown.ended, &s.walk)
                && s.day == self.ended.0
            {
                self.ended.1.insert(b.clone());
            }
            if shown.back {
                self.view = None;
                ui.ctx().request_repaint();
            }
            return (shown.bottom, shown.scrolled);
        }
        if let Some(view) = &mut self.view {
            let (bottom, back) = match view {
                View::Edit(editor) => {
                    let escape =
                        env.keyboard && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
                    let (bottom, back) = editor.show(ui, content, env, actions);
                    (bottom, back || escape && !editor.changed())
                }
                View::Step => (content.top(), true),
            };
            if back {
                self.view = None;
                ui.ctx().request_repaint();
            }
            return (bottom, false);
        }
        let d = today() + self.offset;
        let info = calendar::day(d, style);

        let (y0, _, _) = calendar::civil_date(d);
        let next_pascha =
            if calendar::pascha(y0) >= d { calendar::pascha(y0) } else { calendar::pascha(y0 + 1) };
        let mut subtitle = match style {
            Style::New => "New calendar".to_string(),
            Style::Old => {
                let (_, m, dd) = info.date;
                format!("{dd} {} on the Old calendar", MONTHS[m as usize - 1])
            }
        };
        let to_pascha = next_pascha - d;
        subtitle.push_str(&match to_pascha {
            0 => " · Christ is risen!".to_string(),
            1 => " · Pascha is tomorrow".to_string(),
            n => format!(" · Pascha in {n} days ({})", short_date(next_pascha)),
        });
        let title = match self.offset {
            0 => format!("Today · {}", date_label(d)),
            _ => date_label(d),
        };
        // The title with the calendar beside it; in a narrow window the title and its line wrap,
        // and the calendar goes beside the days' buttons, or under them.
        let calendars = ["New calendar", "Old calendar"];
        let selected = usize::from(style == Style::Old);
        let cal_w = widgets::segmented_tight_width(&painter, &calendars, selected);
        let heading = FontId::new(28.0, theme::family(theme::SERIF_HEADING));
        let title_w = painter.layout_no_wrap(title.clone(), heading.clone(), p.ink).size().x;
        let beside_title = title_w + 24.0 + cal_w <= right - left;
        let mut ny = top + 58.0 + 28.0;
        if title_w <= right - left {
            super::title(&painter, p, left, top, &title, &subtitle);
        } else {
            let mut y = top;
            for mut runs in widgets::flow(&painter, &title, &heading, 28.0, right - left, 3) {
                runs.iter_mut().for_each(|r| r.tracking -= 0.01);
                widgets::paint_runs(&painter, left, widgets::css_baseline(y, 36.0, 28.0, true), &runs, p.ink);
                y += 36.0;
            }
            y += paragraph(
                &painter,
                &subtitle,
                &theme::ui_font(13.0),
                20.0,
                left,
                y + 2.0,
                right - left,
                p.ink_2,
            );
            ny = y + 2.0 + 28.0;
        }
        let (prev, r) = Button::new(Kind::Secondary, "Previous").icon(crate::icons::chevron_left).show_left(
            ui,
            p,
            left,
            ny,
            ui.id().with("prev-day"),
        );
        if r.clicked() {
            self.offset -= 1;
            self.month = None;
        }
        let (next, r) = Button::new(Kind::Secondary, "Next")
            .trailing_icon(crate::icons::chevron_right)
            .show_left(ui, p, prev.right() + 8.0, ny, ui.id().with("next-day"));
        if r.clicked() {
            self.offset += 1;
            self.month = None;
        }
        let mut buttons = next.right();
        if self.offset != 0 {
            let (back, r) = Button::new(Kind::Ghost, "Back to Today").show_left(
                ui,
                p,
                next.right() + 8.0,
                ny,
                ui.id().with("today"),
            );
            buttons = back.right();
            if r.clicked() {
                self.offset = 0;
                self.month = None;
            }
        }
        let at = if beside_title {
            pos2(right, top + 26.0 + 16.0)
        } else if buttons + 16.0 + cal_w <= right {
            pos2(right, ny)
        } else {
            ny += 44.0;
            pos2(left + cal_w, ny)
        };
        let (_, pick) =
            widgets::segmented_tight(ui, p, at, ui.id().with("calendar-style"), &calendars, selected);
        if let Some(i) = pick {
            actions.push(Action::SetCalendar(if i == 0 { Style::New } else { Style::Old }));
        }

        // The month stands beside the day when there's room, else after the rule.
        let beside = right - left - MONTH_W - MONTH_GAP >= 440.0;
        let width = if beside { right - left - MONTH_W - MONTH_GAP } else { right - left }.min(640.0);
        let serif = FontId::new(20.0, theme::family(theme::SERIF));
        let mut y = ny + 16.0 + 32.0;
        let rule = |y: f32| painter.hline(left..=left + width, y, Stroke::new(1.0, p.separator));

        // The part of the rule due now, or next later today.
        if self.offset == 0
            && let Some(bottom) = self.now_card(ui, env, &prayer_rule, left, width, y)
        {
            y = bottom + 28.0;
        }

        // The day.
        group_label(&painter, p, left, y, "The day");
        y += 26.0;
        if info.feasts.is_empty() {
            let line = info.season.unwrap_or("No great feast");
            text(&painter, left, y, 28.0, line, serif.clone(), p.ink_2, false);
            y += 28.0;
        } else {
            for f in &info.feasts {
                y += paragraph(&painter, f, &serif, 28.0, left, y, width, p.ink);
            }
            if let Some(s) = info.season {
                text(&painter, left, y + 2.0, 20.0, s, theme::ui_font(13.0), p.ink_2, false);
                y += 22.0;
            }
        }
        y += 20.0;
        rule(y);
        y += 20.0;

        // The day's readings.
        let told = (info.feasts.iter().any(|f| great(f)), !info.feasts.is_empty());
        let mut all = self.all_readings == Some(d);
        let lessons = lectionary::readings_in(d, style, env.settings.usage);
        let (bottom, picked) = readings(ui, env, left, width, y, &lessons, told, &mut all);
        y = bottom + 20.0;
        if all {
            self.all_readings = Some(d);
        }
        if let Some(i) = picked {
            self.open_walk(Walk::Readings, d, i.to_string());
        }
        rule(y);
        y += 20.0;

        // The prayer rule, then the reading plans.
        let paused =
            self.session.as_ref().and_then(|s| s.paused_in(d)).map(|(b, i)| (b.to_string(), i.to_string()));
        let paused = paused.as_ref().map(|(b, i)| (b.as_str(), i.as_str()));
        let (bottom, pick) = rule_view::section(ui, env, &prayer_rule, left, width, y, d, paused, actions);
        y = bottom + 20.0;
        match pick {
            Some(Pick::Edit) => {
                let editor =
                    super::rule_editor::Editor::new(prayer_rule.clone(), env.settings.rule_reminders);
                self.view = Some(View::Edit(Box::new(editor)));
            }
            Some(Pick::Begin(b)) => self.begin_block(env, &prayer_rule, &b, d, None),
            Some(Pick::Item { block, item }) => self.begin_block(env, &prayer_rule, &block, d, Some(&item)),
            None => {}
        }
        rule(y);
        y += 20.0;
        if !beside {
            y = self.month(ui, env, &prayer_rule, left, y, d) + 20.0;
            rule(y);
            y += 20.0;
        }
        y = self.plans(ui, env, left, width, y, d, actions) + 20.0;
        rule(y);
        y += 20.0;

        // The fast.
        group_label(&painter, p, left, y, "Fasting");
        y += 26.0;
        let ink = if info.fast == Fast::None { p.ink } else { p.accent };
        text(&painter, left, y, 24.0, info.fast.label(), theme::ui_semibold(15.0), ink, false);
        y += 24.0;
        text(&painter, left, y, 20.0, info.fast.detail(), theme::ui_font(13.0), p.ink_2, false);
        y += 22.0;
        y += paragraph(
            &painter,
            "A general guide to the fasting rules most parishes publish. Practice varies; follow your \
             parish calendar and your priest's guidance.",
            &theme::ui_font(12.0),
            18.0,
            left,
            y,
            width,
            p.ink_3,
        );
        y += 20.0;
        rule(y);
        y += 20.0;

        // The Psalter.
        group_label(&painter, p, left, y, "Psalter");
        y += 22.0;
        let (matins, vespers) = calendar::kathismata(info.weekday);
        let services: [(&str, &[u8]); 2] = [("Matins", matins), ("Vespers", vespers)];
        for (service, ks) in services {
            if ks.is_empty() {
                let row = Rect::from_min_size(pos2(left, y), vec2(width, 40.0));
                text(
                    &painter,
                    row.left() + 12.0,
                    row.top() + 10.0,
                    20.0,
                    service,
                    theme::ui_semibold(13.0),
                    p.ink,
                    false,
                );
                text(
                    &painter,
                    row.left() + 96.0,
                    row.top() + 10.0,
                    20.0,
                    "No kathisma",
                    theme::ui_font(13.0),
                    p.ink_3,
                    false,
                );
                y += 40.0;
                continue;
            }
            for &k in ks {
                let row = Rect::from_min_size(pos2(left, y), vec2(width, 40.0));
                let resp = ui.interact(row, ui.id().with(("kathisma", service, k)), Sense::click());
                if ui.rect_contains_pointer(row) {
                    painter.rect_filled(
                        row,
                        10.0,
                        if resp.is_pointer_button_down_on() { p.fill_strong } else { p.fill },
                    );
                }
                widgets::focus_ring(&painter, p, &resp, row, 10.0);
                text(
                    &painter,
                    row.left() + 12.0,
                    row.top() + 10.0,
                    20.0,
                    service,
                    theme::ui_semibold(13.0),
                    p.ink,
                    false,
                );
                let label = format!("Kathisma {k} · {}", calendar::kathisma_psalms(k));
                text(
                    &painter,
                    row.left() + 96.0,
                    row.top() + 10.0,
                    20.0,
                    &label,
                    theme::ui_font(13.0),
                    p.ink_2,
                    false,
                );
                text(
                    &painter,
                    row.right() - 12.0,
                    row.top() + 10.0,
                    20.0,
                    "Read",
                    theme::ui_medium(13.0),
                    p.accent,
                    true,
                );
                if resp.on_hover_cursor(eframe::egui::CursorIcon::PointingHand).clicked() {
                    self.open_kathisma(env, &prayer_rule, d, k);
                }
                y += 40.0;
            }
        }
        y += 4.0
            + paragraph(
                &painter,
                "The weekly order for ordinary weeks. In winter a third kathisma is read at Matins, and \
                 in Great Lent the Psalter is read twice a week.",
                &theme::ui_font(12.0),
                18.0,
                left,
                y + 4.0,
                width,
                p.ink_3,
            );
        y += 24.0;
        rule(y);
        y += 20.0;

        // What's coming.
        group_label(&painter, p, left, y, "Coming up");
        y += 22.0;
        for (day, what) in coming_up(d, style) {
            let row = Rect::from_min_size(pos2(left, y), vec2(width, 36.0));
            let resp = ui.interact(row, ui.id().with(("coming", day)), Sense::click());
            if ui.rect_contains_pointer(row) {
                painter.rect_filled(
                    row,
                    10.0,
                    if resp.is_pointer_button_down_on() { p.fill_strong } else { p.fill },
                );
            }
            widgets::focus_ring(&painter, p, &resp, row, 10.0);
            text(
                &painter,
                row.left() + 12.0,
                row.top() + 8.0,
                20.0,
                &short_date(day),
                theme::ui_medium(13.0),
                p.ink_2,
                false,
            );
            let room = width - 96.0 - 90.0;
            let name = super::elide(&painter, &what, &theme::ui_font(13.0), room);
            text(
                &painter,
                row.left() + 96.0,
                row.top() + 8.0,
                20.0,
                &name,
                theme::ui_font(13.0),
                p.ink,
                false,
            );
            let n = day - d;
            let when = if n == 1 { "tomorrow".to_string() } else { format!("in {n} days") };
            text(
                &painter,
                row.right() - 12.0,
                row.top() + 8.0,
                20.0,
                &when,
                theme::ui_font(12.0),
                p.ink_3,
                true,
            );
            if resp.on_hover_cursor(eframe::egui::CursorIcon::PointingHand).clicked() {
                self.offset += n;
                self.month = None;
                self.to_top = true;
            }
            y += 36.0;
        }
        // Woken at the next block's time, so the Now card moves on.
        let now = bible_core::study::now();
        let t = crate::time::local(now);
        let minute = i64::from(t.hour) * 60 + i64::from(t.minute);
        let times = prayer_rule.blocks.iter().filter_map(|b| b.time.as_deref().and_then(minutes));
        let wait = times.filter(|m| *m > minute).min().unwrap_or(24 * 60) - minute;
        let secs = (wait * 60 - now.rem_euclid(60)).clamp(1, 3600) as u64;
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(secs));
        if beside {
            let bottom = self.month(ui, env, &prayer_rule, right - MONTH_W, ny + 16.0 + 32.0, d);
            (y.max(bottom), false)
        } else {
            (y, false)
        }
    }

    /// A kathisma of the Psalter section on day `d`: the rule's kathisma item when it reads it
    /// that day and it isn't done (Done then ticks it), else the kathisma alone.
    fn open_kathisma(&mut self, env: &PageEnv<'_>, rule: &Rule, d: Day, k: u8) {
        let done = env.study.rule_done(d);
        let item = rule.for_day(d, env.settings.calendar).into_iter().find_map(|(b, items)| {
            let reads =
                |i: &&bible_core::rule::Item| i.kind == ItemKind::Kathisma && kathismata(b, d).contains(&k);
            items
                .into_iter()
                .find(|i| reads(i) && !done.contains(&i.id))
                .map(|i| (b.id.clone(), i.id.clone()))
        });
        match item {
            Some((b, i)) => self.begin_block(env, rule, &b, d, Some(&i)),
            None => self.open_walk(Walk::Kathisma(k), d, k.to_string()),
        }
    }

    /// The Now card at `top`, `width` wide: the part of the rule due now (or next later today),
    /// its items, done ones ticked, and Begin, or Continue once begun. ⌘↩ presses it. Returns its
    /// bottom, or `None` when there is nothing to offer.
    fn now_card(
        &mut self,
        ui: &mut Ui,
        env: &PageEnv<'_>,
        rule: &Rule,
        left: f32,
        width: f32,
        top: f32,
    ) -> Option<f32> {
        let p = env.settings.theme.palette();
        let painter = ui.painter().clone();
        let d = today();
        let t = crate::time::local(bible_core::study::now());
        let minute = i64::from(t.hour) * 60 + i64::from(t.minute);
        let (now, steps) = step::now(rule, env, d, minute, &self.ended.1)?;
        let block = match now {
            Now::Due(b) | Now::Next(b) => b,
        };
        let paused = self.session.as_ref().and_then(|s| s.paused_in(d)).is_some_and(|(b, _)| b == block.id);
        let begun = paused || steps.iter().any(|s| s.done);
        // Its items, done ones ticked in the tertiary ink; a line breaks after a dot, never
        // inside a name.
        let mut job = egui::text::LayoutJob::default();
        for (i, st) in steps.iter().filter(|s| !s.idle).enumerate() {
            let ink = if st.done { p.ink_3 } else { p.ink_2 };
            let format = egui::TextFormat {
                font_id: theme::ui_font(13.0),
                color: ink,
                line_height: Some(20.0),
                ..Default::default()
            };
            if i > 0 {
                job.append("\u{a0}· ", 0.0, egui::TextFormat { color: p.ink_3, ..format.clone() });
            }
            let name = if st.done { format!("✓ {}", st.title) } else { st.title.clone() };
            job.append(&name.replace(' ', "\u{a0}"), 0.0, format);
        }
        job.wrap.max_width = width - 32.0;
        let names = painter.layout_job(job);
        let next = begun.then(|| {
            let at = self.session.as_ref().filter(|s| s.paused_in(d).is_some_and(|(b, _)| b == block.id));
            let first = match at {
                Some(s) => step::resume_at(rule, env, s, &steps),
                None => step::first(rule, env, &Walk::Block(block.id.clone()), d, &steps, &HashSet::new()),
            };
            first.and_then(|id| steps.iter().find(|s| s.id == id)).map(|s| format!("Next: {}", s.title))
        });
        let next = next.flatten();
        let h = 16.0 + 24.0 + 6.0 + names.size().y + if next.is_some() { 24.0 } else { 0.0 } + 16.0;
        let card = Rect::from_min_size(pos2(left, top), vec2(width, h));
        painter.rect_filled(card, 12.0, p.fill);
        let title = match block.time.as_deref().and_then(minutes) {
            Some(at) => format!("{} · {}", block.title, clock(at)),
            None => block.title.clone(),
        };
        text(&painter, left + 16.0, top + 16.0, 24.0, &title, theme::ui_semibold(15.0), p.ink, false);
        painter.galley(pos2(left + 16.0, top + 16.0 + 24.0 + 6.0), names.clone(), p.ink_2);
        if let Some(next) = &next {
            let y = top + 16.0 + 24.0 + 6.0 + names.size().y + 4.0;
            text(&painter, left + 16.0, y, 20.0, next, theme::ui_medium(13.0), p.ink, false);
        }
        let b = Button::new(Kind::Primary, if begun { "Continue" } else { "Begin" });
        let (_, resp) = b.show_right(ui, p, card.right() - 16.0, top + 28.0, ui.id().with("now-begin"));
        let enter = env.keyboard && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter));
        if resp.on_hover_text("⌘↩").clicked() || enter {
            let id = block.id.clone();
            self.begin_block(env, rule, &id, d, None);
        }
        Some(card.bottom())
    }

    /// The reading plans under way with their passages for day `d`, a row and a check box
    /// each (the plan and its day under the first), and Read. The Plans page starts and stops
    /// plans.
    #[allow(clippy::too_many_arguments)]
    fn plans(
        &mut self,
        ui: &mut Ui,
        env: &PageEnv<'_>,
        left: f32,
        width: f32,
        top: f32,
        d: Day,
        actions: &mut Vec<Action>,
    ) -> f32 {
        let p = env.settings.theme.palette();
        let painter = ui.painter().clone();
        let right = left + width;
        group_label(&painter, p, left, top, "Reading plans");
        let started = env.study.plans();
        let under: Vec<_> = started.iter().filter(|s| s.finished.is_none()).collect();
        let link =
            Button::new(Kind::Ghost, if under.is_empty() { "Browse plans" } else { "All plans" }).small();
        let at = right - 12.0 + link.rest_inset();
        if link.show_right(ui, p, at, top + 8.0, ui.id().with("all-plans")).1.clicked() {
            actions.push(Action::OpenPage(super::Page::Plans));
        }
        let mut y = top + 26.0;
        let t = super::reading_text(env);
        let mut shown = 0;
        for s in under {
            let Some(plan) = plans::get(&s.id) else { continue };
            let u = super::plans::Under::new(plan, s, env.study);
            // The plan's day on the day shown.
            let Some(day) = rule_view::plan_day(&u, d, today()) else { continue };
            shown += 1;
            for (i, ps) in plan.day(day).iter().enumerate() {
                let h = if i == 0 { 48.0 } else { 36.0 };
                let row = Rect::from_min_size(pos2(left, y), vec2(width, h));
                let id = ui.id().with(("today-plan", plan.id, i));
                let resp = ui.interact(row, id, Sense::click());
                if ui.rect_contains_pointer(row) {
                    let fill = if resp.is_pointer_button_down_on() { p.fill_strong } else { p.fill };
                    painter.rect_filled(row, 10.0, fill);
                }
                widgets::focus_ring(&painter, p, &resp, row, 10.0);
                let done = u.passage_done(day, i);
                let b = Rect::from_min_size(pos2(row.left() + 12.0, row.center().y - 8.0), vec2(16.0, 16.0));
                let tick = widgets::tick(ui, p, b, done, id.with("tick"));
                let x = b.right() + 12.0;
                let (cy, ty) =
                    (row.center().y - 10.0, row.center().y - 10.0 - if i == 0 { 9.0 } else { 0.0 });
                let read =
                    text(&painter, right - 12.0, cy, 20.0, "Read", theme::ui_medium(13.0), p.accent, true);
                let room = right - 12.0 - read - 16.0 - x;
                let font = theme::ui_medium(14.0);
                let label = super::plans::day_label(std::slice::from_ref(ps), t.as_deref());
                let label = super::elide(&painter, &label, &font, room);
                text(&painter, x, ty, 20.0, &label, font, p.ink, false);
                if i == 0 {
                    let detail = format!("{} · Day {day} of {}", plan.title, plan.len());
                    let detail = super::elide(&painter, &detail, &theme::ui_font(12.0), room);
                    text(&painter, x, ty + 20.0, 18.0, &detail, theme::ui_font(12.0), p.ink_3, false);
                }
                if tick.clicked() {
                    let (plan, day, passage) = (plan.id.to_string(), day as u16, i as u8);
                    actions.push(Action::SetPlanRead { plan, day, passage, read: !done });
                } else if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.open_walk(Walk::Plan(plan.id, day), d, i.to_string());
                }
                y += h;
            }
        }
        if shown == 0 {
            let none = if started.iter().all(|s| s.finished.is_some()) {
                "No plan under way."
            } else {
                "No plan reading on this day."
            };
            text(&painter, left, y, 20.0, none, theme::ui_font(13.0), p.ink_2, false);
            y += 20.0;
        }
        y
    }

    /// The month (`d`'s, or the one stepped to) at `x`, `top`: each day's date, its fast as a
    /// band, a cross on great feasts and, up to today, a dot when the rule was kept (hollow when
    /// some of it was). Clicking a day shows it. Returns the bottom.
    fn month(&mut self, ui: &mut Ui, env: &PageEnv<'_>, rule: &Rule, x: f32, top: f32, d: Day) -> f32 {
        let p = env.settings.theme.palette();
        let painter = ui.painter().clone();
        let style = env.settings.calendar;
        let now = today();
        let (year, month) = self.month.unwrap_or_else(|| {
            let (y, m, _) = calendar::civil_date(d);
            (y, m)
        });
        let first = calendar::civil(year, month, 1);
        let next = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
        let last = calendar::civil(next.0, next.1, 1) - 1;
        let lead = i64::from(calendar::weekday(first));
        let weeks = (lead + last - first + 7) / 7;
        let card_bg = painter.add(egui::Shape::Noop);

        // The month, and stepping through months.
        let title = format!("{} {year}", MONTHS[month as usize - 1]);
        text(&painter, x + 16.0, top + 18.0, 24.0, &title, theme::ui_semibold(15.0), p.ink, false);
        let step = |k: i32| {
            let m = i32::from(month) - 1 + k;
            (year + m.div_euclid(12), (m.rem_euclid(12) + 1) as u8)
        };
        let next = Rect::from_min_size(pos2(x + MONTH_W - 12.0 - 28.0, top + 16.0), vec2(28.0, 28.0));
        let prev = next.translate(vec2(-32.0, 0.0));
        if widgets::round_button(ui, p, prev, crate::icons::chevron_left, ui.id().with("prev-month"))
            .on_hover_text("Previous month")
            .clicked()
        {
            self.month = Some(step(-1));
        }
        if widgets::round_button(ui, p, next, crate::icons::chevron_right, ui.id().with("next-month"))
            .on_hover_text("Next month")
            .clicked()
        {
            self.month = Some(step(1));
        }

        // The days, Sunday first.
        let col = (MONTH_W - 24.0) / 7.0;
        let gx = x + 12.0;
        let mut y = top + 56.0;
        for wd in 0..7u8 {
            let c = pos2(gx + col * (f32::from(wd) + 0.5), y + 8.0);
            let label = crate::time::weekday_short(wd);
            painter.text(c, Align2::CENTER_CENTER, label, theme::ui_medium(11.0), p.ink_3);
        }
        y += 22.0;
        let mut done = env.study.rule_done_between(first, last);
        let started = env.study.plans();
        let under = rule_view::following(rule, &started, env.study);
        let cell_h = 46.0;
        for day in first..=last {
            let k = day - first + lead;
            let at = pos2(gx + col * (k % 7) as f32, y + cell_h * (k / 7) as f32);
            let cell = Rect::from_min_size(at, vec2(col, cell_h));
            let resp = ui.interact(cell, ui.id().with(("month-day", day)), Sense::click());
            if day == d {
                painter.rect_filled(cell.shrink(2.0), 10.0, p.accent_soft);
            } else if let Some(fill) = widgets::hover_fill(ui, &resp, p, false) {
                painter.rect_filled(cell.shrink(2.0), 10.0, fill);
            }
            widgets::focus_ring(&painter, p, &resp, cell.shrink(2.0), 10.0);
            let info = calendar::day(day, style);
            let (_, _, dom) = calendar::civil_date(day);
            let (font, ink) =
                if day == now { (theme::ui_bold(13.0), p.accent) } else { (theme::ui_medium(13.0), p.ink) };
            let c = pos2(cell.center().x, cell.top() + 15.0);
            painter.text(c, Align2::CENTER_CENTER, dom.to_string(), font, ink);
            if info.feasts.iter().any(|f| great(f)) {
                cross(&painter, pos2(cell.right() - 8.0, cell.top() + 9.0), p.rubric);
            }
            // Nothing on days to come.
            if day <= now {
                // What was ticked, and a plan's item once that day's reading was read; what
                // had nothing to do that day (Sunday evening's kathisma) counts as kept.
                let ticked = done.entry(day).or_default();
                ticked.extend(rule_view::plans_read(rule, day, style, now, &under));
                let some =
                    rule.for_day(day, style).iter().flat_map(|(_, i)| i).any(|i| ticked.contains(&i.id));
                let mut kept_or_idle = ticked.clone();
                kept_or_idle.extend(rule.idle(day, style));
                let (kept, of) = rule.kept(day, style, &kept_or_idle);
                let c = pos2(cell.center().x, cell.top() + 29.0);
                if of > 0 && some && kept == of {
                    painter.circle_filled(c, 2.5, p.accent);
                } else if some {
                    painter.circle_stroke(c, 2.25, Stroke::new(1.0, p.accent));
                }
            }
            if let Some(c) = fast_colour(p, info.fast) {
                let band = Rect::from_center_size(pos2(cell.center().x, cell.top() + 38.0), vec2(20.0, 3.0));
                painter.rect_filled(band, 1.5, c);
            }
            let mut about: Vec<&str> = info.feasts.iter().copied().filter(|f| great(f)).collect();
            about.push(info.fast.label());
            if resp.on_hover_text(about.join(" · ")).clicked() {
                self.offset = day - now;
                self.month = None;
                self.to_top = true;
                ui.ctx().request_repaint();
            }
        }
        y += cell_h * weeks as f32 + 12.0;

        // The legend, in rows as wide as the month.
        painter.hline(x + 16.0..=x + MONTH_W - 16.0, y, Stroke::new(1.0, p.separator));
        y += 12.0;
        let mut lx = x + 16.0;
        let mut entry = |mark: &dyn Fn(egui::Pos2), label: &str| {
            let w = painter.layout_no_wrap(label.into(), theme::ui_medium(11.0), p.ink_3).size().x;
            if lx + 18.0 + w > x + MONTH_W - 16.0 {
                lx = x + 16.0;
                y += 20.0;
            }
            mark(pos2(lx + 6.0, y + 8.0));
            text(&painter, lx + 18.0, y, 16.0, label, theme::ui_medium(11.0), p.ink_3, false);
            lx += 18.0 + w + 14.0;
        };
        for (fast, label) in FASTS {
            let c = fast_colour(p, fast).unwrap_or(p.ink_3);
            entry(
                &|at| {
                    painter.rect_filled(Rect::from_center_size(at, vec2(12.0, 3.0)), 1.5, c);
                },
                label,
            );
        }
        entry(&|at| cross(&painter, at, p.rubric), "Great feast");
        entry(
            &|at| {
                painter.circle_filled(at, 2.5, p.accent);
            },
            "Rule kept",
        );
        entry(
            &|at| {
                painter.circle_stroke(at, 2.25, Stroke::new(1.0, p.accent));
            },
            "Partly kept",
        );
        y += 16.0 + 16.0;
        let card = Rect::from_min_max(pos2(x, top), pos2(x + MONTH_W, y));
        let shape = egui::epaint::RectShape::new(
            card,
            14.0,
            p.surface,
            Stroke::new(0.5, p.border),
            StrokeKind::Outside,
        );
        painter.set(card_bg, shape);
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The names of a lectionary table's entries.
    fn names(json: &str) -> Vec<String> {
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        v["entries"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap().to_string()).collect()
    }

    macro_rules! table {
        ($name:literal) => {
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/lectionary/", $name, ".json"))
        };
    }

    #[test]
    fn the_moving_cycles_days_are_told_from_saints_and_feasts() {
        for json in [table!("pentecostarion"), table!("triodion"), table!("epistles"), table!("gospels")] {
            for name in names(json) {
                assert!(moving(&name), "{name}");
            }
        }
        // The Menaion's are saints and feasts, but for the Saturdays and Sundays around the
        // great feasts, which are named days too.
        for name in names(table!("menaion")) {
            let around = ["Saturday", "Sunday"].iter().any(|d| name.starts_with(d));
            assert_eq!(moving(&name), around, "{name}");
        }
    }

    #[test]
    fn ordinary_days_are_told_from_named_ones() {
        // The weekly cycle's days, but for All Saints.
        for json in [table!("epistles"), table!("gospels")] {
            for name in names(json) {
                assert_eq!(ordinary(&name), !name.starts_with("Sunday of All Saints"), "{name}");
            }
        }
        for name in [
            "Tuesday of the 4th Week of Pascha (Eve of Mid-Pentecost)",
            "Bright Monday",
            "Great and Holy Friday",
            "Monday after the Publican and Pharisee",
        ] {
            assert!(ordinary(name), "{name}");
        }
        for name in [
            "Second Saturday of Great Lent (Saturday of Souls)",
            "Saturday of the Akathist (Fifth Saturday of Great Lent)",
            "Palm Sunday (Entry of the Lord into Jerusalem)",
        ] {
            assert!(!ordinary(name), "{name}");
        }
        // The Menaion's saints and feasts, and its Saturdays and Sundays around the great feasts.
        for name in names(table!("menaion")) {
            assert!(!ordinary(&name), "{name}");
        }
    }

    /// What the day's Liturgy readings are read for, as the page shows it.
    fn read_for_on(y: i32, m: u8, d: u8) -> Vec<Option<String>> {
        read_for_in(y, m, d, lectionary::Usage::Greek)
    }

    fn read_for_in(y: i32, m: u8, d: u8, usage: lectionary::Usage) -> Vec<Option<String>> {
        let d = calendar::civil(y, m, d);
        let feasts = calendar::day(d, Style::New).feasts;
        let (feast, named) = (feasts.iter().any(|f| great(f)), !feasts.is_empty());
        let lessons = lectionary::readings_in(d, Style::New, usage);
        let liturgy = lessons.iter().filter(|r| r.at_liturgy() && r.kind != lectionary::Kind::OldTestament);
        liturgy.map(|r| read_for(r, feast, named)).collect()
    }

    fn both(s: &str) -> [Option<String>; 2] {
        [Some(s.into()), Some(s.into())]
    }

    #[test]
    fn a_reading_says_what_its_read_for_unless_the_page_does() {
        let artemius = [Some("St Artemius".into()), None];
        assert_eq!(read_for_on(2026, 10, 20), artemius, "the saint's Epistle");
        let thekla = "Protomartyr Thekla, Equal to the Apostles";
        // In Greek usage St Thekla gives the Epistle and the Myrtidiotissa icon the Gospel.
        let thekla_and_icon = [Some(thekla.into()), Some("Theotokos Myrtidiotissa".into())];
        assert_eq!(read_for_on(2026, 9, 24), thekla_and_icon, "the saints' readings in the day's place");
        assert_eq!(read_for_on(2026, 10, 25), [None, None], "a Sunday after Pentecost");
        assert_eq!(read_for_on(2026, 12, 25), [None, None], "the Nativity, named above");
        assert_eq!(read_for_on(2026, 4, 12), [None, None], "Pascha");
        assert_eq!(read_for_on(2026, 3, 1), [None, None], "the Sunday of Orthodoxy, named above");
        // Named days the day section doesn't name.
        let fathers = "Sunday of the Holy Fathers of the Seventh Ecumenical Council";
        assert_eq!(read_for_on(2026, 10, 11), both(fathers));
        assert_eq!(read_for_on(2026, 3, 7), both("Second Saturday of Great Lent (Saturday of Souls)"));
        let genealogy = "Sunday before the Nativity of Christ (Holy Fathers; the Genealogy)";
        assert_eq!(read_for_on(2026, 12, 20), both(genealogy));
        // A title that only tells two lessons apart says whose they are.
        let theodore = "Saturday of St Theodore the Recruit";
        assert_eq!(
            read_for_on(2026, 2, 28),
            [Some(format!("{theodore} · of the saint")), Some(format!("{theodore} · of the day"))]
        );
        let forefeast = [Some("St Sylvester".into()), Some("Forefeast of Theophany".into())];
        assert_eq!(read_for_on(2026, 1, 2), forefeast);
        // The Antiochian Archdiocese's days that move, which the day section doesn't name.
        let antioch = [Some("All Saints of Antioch".into()), None];
        assert_eq!(read_for_in(2026, 6, 14, lectionary::Usage::Antiochian), antioch);
        assert_eq!(read_for_in(2026, 11, 7, lectionary::Usage::Antiochian), both("St Raphael of Brooklyn"));
    }
}
