//! Today in the Church (Orthodox fork): the day's feasts, its fast, the Psalter's kathismata and
//! what's coming up, from `bible_core::calendar`. Days step back and forward; the church's
//! calendar (New or Old) is a setting.

use eframe::egui::{FontId, Rect, Sense, Stroke, Ui, pos2, vec2};

use bible_core::ChapterIdx;
use bible_core::calendar::{self, Day, Fast, Style};

use super::{PageEnv, group_label, text};
use crate::actions::Action;
use crate::theme;
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

#[derive(Default)]
pub struct State {
    /// Days from today.
    offset: i64,
}

fn today() -> Day {
    let t = crate::time::local(bible_core::study::now());
    calendar::civil(t.year, t.month, t.day)
}

fn date_label(d: Day) -> String {
    let (y, m, dd) = calendar::civil_date(d);
    format!("{}, {dd} {} {y}", WEEKDAYS[calendar::weekday(d) as usize], MONTHS[m as usize - 1])
}

fn short_date(d: Day) -> String {
    let (_, m, dd) = calendar::civil_date(d);
    format!("{} {dd} {}", &WEEKDAYS[calendar::weekday(d) as usize][..3], &MONTHS[m as usize - 1][..3])
}

/// "Psalms 24–31", in the numbering the reader shows.
fn psalms_label(k: u8) -> String {
    let (a, b) = calendar::KATHISMATA[k as usize - 1];
    let shown = |n: u8| {
        if bible_core::lxx::numbering() {
            n
        } else {
            bible_core::lxx::from_lxx(n, 1).map_or(n, |(_, c, _)| c)
        }
    };
    if a == b { format!("Psalm {}", shown(a)) } else { format!("Psalms {}–{}", shown(a), shown(b)) }
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
            // Sundays of the Triodion and Pentecostarion are left to the day itself.
            if !f.starts_with("Sunday of") && !f.starts_with("Holy ") && !f.starts_with("Monday of") {
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
fn paragraph(
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

impl State {
    pub fn show(&mut self, ui: &mut Ui, content: Rect, env: &PageEnv<'_>, actions: &mut Vec<Action>) -> f32 {
        let p = env.settings.theme.palette();
        let painter = ui.painter().clone();
        let (left, right, top) = (content.left(), content.right(), content.top());
        let style = env.settings.calendar;
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
        super::title(&painter, p, left, top, &title, &subtitle);

        // The calendar, then stepping through days.
        let cy = top + 26.0 + 16.0;
        let (_, pick) = widgets::segmented_tight(
            ui,
            p,
            pos2(right, cy),
            ui.id().with("calendar-style"),
            &["New calendar", "Old calendar"],
            usize::from(style == Style::Old),
        );
        if let Some(i) = pick {
            actions.push(Action::SetCalendar(if i == 0 { Style::New } else { Style::Old }));
        }
        let ny = top + 58.0 + 28.0;
        let (prev, r) = Button::new(Kind::Secondary, "Previous").icon(crate::icons::chevron_left).show_left(
            ui,
            p,
            left,
            ny,
            ui.id().with("prev-day"),
        );
        if r.clicked() {
            self.offset -= 1;
        }
        let (next, r) = Button::new(Kind::Secondary, "Next")
            .trailing_icon(crate::icons::chevron_right)
            .show_left(ui, p, prev.right() + 8.0, ny, ui.id().with("next-day"));
        if r.clicked() {
            self.offset += 1;
        }
        if self.offset != 0 {
            let (_, r) = Button::new(Kind::Ghost, "Back to Today").show_left(
                ui,
                p,
                next.right() + 8.0,
                ny,
                ui.id().with("today"),
            );
            if r.clicked() {
                self.offset = 0;
            }
        }

        let width = (right - left).min(640.0);
        let serif = FontId::new(20.0, theme::family(theme::SERIF));
        let mut y = ny + 16.0 + 32.0;
        let rule = |y: f32| painter.hline(left..=left + width, y, Stroke::new(1.0, p.separator));

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
                let label = format!("Kathisma {k} · {}", psalms_label(k));
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
                if resp.on_hover_cursor(eframe::egui::CursorIcon::PointingHand).clicked()
                    && let Some((book, chapter)) = calendar::kathisma_start(k)
                    && let Some(c) = ChapterIdx::new(book, chapter)
                {
                    actions.push(Action::Jump { chapter: c, verse: None, flash: false });
                    actions.push(Action::OpenPage(super::Page::Reader));
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
            }
            y += 36.0;
        }
        y
    }
}
