//! The Church Fathers on the selected verse (`bible_core::fathers`), in the inspector's place
//! beside the reader. It follows the selection; each quote shows its first lines until opened.
//! In a step on Today it stays on the day's reading instead, its ‹ › stepping through its verses.

use std::collections::HashSet;

use bible_core::VerseRef;
use bible_core::fathers::{Fathers, Quote};
use eframe::egui::text::{LayoutJob, TextFormat, TextWrapping};
use eframe::egui::{self, Align2, FontId, Rect, ScrollArea, Sense, Stroke, Ui, pos2, vec2};

use crate::panel::round_button;
use crate::theme::{self, Palette, text};
use crate::{icons, widgets};

/// Where to get the database the panel is built from.
pub const SOURCE_URL: &str = "https://github.com/HistoricalChristianFaith/Commentaries-Database/releases";

/// Lines of a quote shown before it's opened.
const PREVIEW_ROWS: usize = 6;

pub struct FathersPanel {
    pub verse: VerseRef,
    /// The verses ‹ › step through (a step's reading), when they show.
    pub within: Vec<VerseRef>,
    /// In a step: no close button, and only the verse beside the title.
    pub step: bool,
    quotes: Vec<Quote>,
    /// The verse `quotes` were read for.
    loaded: Option<VerseRef>,
    /// Quotes opened to their full text, by index.
    open: HashSet<usize>,
}

/// What the panel asks of the app.
pub enum Outcome {
    Stay,
    Close,
    /// Go to the verse before (-1) or after (1) in `within`.
    Step(i32),
    /// Add this quote (formatted for a note) to the note on the panel's verse.
    AddToNote(String),
    /// Install the database from this `commentaries.sqlite`.
    Install(std::path::PathBuf),
}

impl FathersPanel {
    pub fn new(verse: VerseRef) -> Self {
        Self {
            verse,
            within: Vec::new(),
            step: false,
            quotes: Vec::new(),
            loaded: None,
            open: HashSet::new(),
        }
    }

    /// Draws the panel's content in `ui` (its panel already painted). `fathers` is `None` until
    /// the quotes are installed.
    pub fn show(&mut self, ui: &mut Ui, p: &Palette, fathers: Option<&Fathers>) -> Outcome {
        if let Some(f) = fathers
            && self.loaded != Some(self.verse)
        {
            self.quotes = f.quotes(self.verse);
            self.loaded = Some(self.verse);
            self.open.clear();
        }
        let rect = ui.max_rect();
        let mut outcome = Outcome::Stay;

        // Header: title, the verse and how many quotes, close.
        let header = Rect::from_min_size(rect.min, vec2(rect.width(), 66.0));
        let detail = match (fathers, self.quotes.len()) {
            (None, _) => self.verse.to_string(),
            _ if self.step => self.verse.to_string(),
            (Some(_), 1) => format!("{} · 1 quote", self.verse),
            (Some(_), n) => format!("{} · {n} quotes", self.verse),
        };
        ui.painter().text(
            pos2(header.left() + 16.0, header.top() + 26.0),
            Align2::LEFT_CENTER,
            "Church Fathers",
            text::UI_TITLE.font(),
            p.ink,
        );
        ui.painter().text(
            pos2(header.left() + 16.0, header.top() + 46.0),
            Align2::LEFT_CENTER,
            detail,
            text::SMALL.font(),
            p.ink_2,
        );
        let close =
            Rect::from_center_size(pos2(header.right() - 26.0, header.top() + 33.0), vec2(28.0, 28.0));
        if !self.within.is_empty() {
            // The reading's verses, one at a time.
            let at = self.within.iter().position(|v| *v == self.verse);
            let prev = close.translate(vec2(-32.0, 0.0));
            if at.is_some_and(|i| i > 0)
                && round_button(ui, p, prev, "fathers-prev", icons::chevron_left, "Previous verse (K)")
                    .clicked()
            {
                outcome = Outcome::Step(-1);
            }
            if at.is_some_and(|i| i + 1 < self.within.len())
                && round_button(ui, p, close, "fathers-next", icons::chevron_right, "Next verse (J)")
                    .clicked()
            {
                outcome = Outcome::Step(1);
            }
        } else if !self.step && round_button(ui, p, close, "fathers-close", icons::close, "Close").clicked() {
            outcome = Outcome::Close;
        }
        ui.painter().hline(header.x_range(), header.bottom() - 0.5, Stroke::new(1.0, p.separator));

        let body = Rect::from_min_max(pos2(rect.left(), header.bottom()), rect.max);
        let mut child = ui.new_child(
            egui::UiBuilder::new().max_rect(body).layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(body.intersect(ui.clip_rect()));
        match fathers {
            None => {
                if let Some(path) = not_installed(&mut child, p) {
                    outcome = Outcome::Install(path);
                }
            }
            Some(_) if self.quotes.is_empty() => {
                message(&mut child, p, "Nothing from the Fathers on this verse.");
            }
            Some(_) => {
                if let Some(note) = self.list(&mut child, p) {
                    outcome = Outcome::AddToNote(note);
                }
            }
        }
        outcome
    }

    /// The quotes; returns a quote's note text when its "Add to Note" is clicked.
    fn list(&mut self, ui: &mut Ui, p: &Palette) -> Option<String> {
        let mut add = None;
        ScrollArea::vertical().auto_shrink(false).id_salt(("fathers", self.verse)).show(ui, |ui| {
            egui::Frame::NONE.inner_margin(egui::Margin::symmetric(16, 12)).show(ui, |ui| {
                let width = ui.available_width();
                for (i, q) in self.quotes.iter().enumerate() {
                    if i > 0 {
                        ui.add_space(10.0);
                        let (r, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
                        ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, p.separator));
                        ui.add_space(10.0);
                    }
                    // Author and year, then the work.
                    let mut head = LayoutJob::default();
                    head.append(
                        &q.author,
                        0.0,
                        TextFormat {
                            font_id: theme::ui_semibold(text::SMALL.size),
                            color: p.ink,
                            ..Default::default()
                        },
                    );
                    head.append(
                        &format!("  {}", q.year),
                        0.0,
                        TextFormat { font_id: theme::ui_font(11.0), color: p.ink_3, ..Default::default() },
                    );
                    head.wrap.max_width = width;
                    let g = ui.painter().layout_job(head);
                    let (r, _) = ui.allocate_exact_size(g.size(), Sense::hover());
                    ui.painter().galley(r.min, g, p.ink);
                    if !q.source.is_empty() {
                        let mut job = LayoutJob::single_section(
                            q.source.clone(),
                            TextFormat {
                                font_id: theme::ui_font(11.0),
                                color: p.ink_3,
                                ..Default::default()
                            },
                        );
                        job.wrap.max_width = width;
                        let g = ui.painter().layout_job(job);
                        let (r, _) = ui.allocate_exact_size(g.size(), Sense::hover());
                        ui.painter().galley(r.min, g, p.ink_3);
                    }
                    ui.add_space(6.0);

                    let open = self.open.contains(&i);
                    let mut job = LayoutJob::single_section(
                        q.text.clone(),
                        TextFormat {
                            font_id: FontId::new(text::SNIPPET_SM.size, theme::family(theme::SERIF)),
                            color: p.ink,
                            line_height: Some(21.0),
                            ..Default::default()
                        },
                    );
                    job.wrap = TextWrapping {
                        max_width: width,
                        max_rows: if open { usize::MAX } else { PREVIEW_ROWS },
                        break_anywhere: false,
                        overflow_character: Some('…'),
                    };
                    let g = ui.painter().layout_job(job);
                    let cut = g.elided;
                    let (r, _) = ui.allocate_exact_size(g.size(), Sense::hover());
                    ui.painter().galley(r.min, g, p.ink);
                    ui.add_space(4.0);
                    let link = |ui: &mut Ui, label: &str| {
                        let g = ui.painter().layout_no_wrap(label.into(), theme::ui_medium(12.0), p.accent);
                        let (r, resp) = ui.allocate_exact_size(g.size(), Sense::click());
                        ui.painter().galley(r.min, g, p.accent);
                        resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
                    };
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 16.0;
                        if (cut || open) && link(ui, if open { "Show less" } else { "Show more" }) {
                            if open {
                                self.open.remove(&i);
                            } else {
                                self.open.insert(i);
                            }
                        }
                        if link(ui, "Add to Note") {
                            add = Some(q.as_note());
                        }
                    });
                }
                ui.add_space(16.0);
                let mut job = LayoutJob::single_section(
                    "From the Historical Christian Faith Commentaries Database: Fathers before 1054 \
                     the Orthodox Church receives."
                        .into(),
                    TextFormat { font_id: theme::ui_font(11.0), color: p.ink_3, ..Default::default() },
                );
                job.wrap.max_width = width;
                let g = ui.painter().layout_job(job);
                let (r, _) = ui.allocate_exact_size(g.size(), Sense::hover());
                ui.painter().galley(r.min, g, p.ink_3);
            });
        });
        add
    }
}

/// A line of secondary text at the top of the body.
fn message(ui: &mut Ui, p: &Palette, s: &str) {
    let width = ui.available_width() - 32.0;
    let mut job = LayoutJob::single_section(
        s.into(),
        TextFormat {
            font_id: text::SMALL.font(),
            color: p.ink_2,
            line_height: Some(18.0),
            ..Default::default()
        },
    );
    job.wrap.max_width = width;
    let g = ui.painter().layout_job(job);
    let top = ui.max_rect().left_top() + vec2(16.0, 16.0);
    ui.painter().galley(top, g.clone(), p.ink_2);
    ui.advance_cursor_after_rect(Rect::from_min_size(top, g.size()));
}

/// How to install the quotes: get `commentaries.sqlite`, then pick it. Returns the file picked.
fn not_installed(ui: &mut Ui, p: &Palette) -> Option<std::path::PathBuf> {
    message(
        ui,
        p,
        "Quotes from the Fathers aren't installed yet. Download commentaries.sqlite (about 150 MB) \
         from the Historical Christian Faith Commentaries Database, then choose it here. It stays on \
         this Mac.",
    );
    let left = ui.max_rect().left() + 16.0;
    let y = ui.cursor().top() + 16.0;
    let get = "Download Page";
    let w = widgets::secondary_button_width(ui, get);
    if widgets::secondary_button(
        ui,
        p,
        Rect::from_min_size(pos2(left, y), vec2(w, 32.0)),
        get,
        ui.id().with("get"),
    )
    .clicked()
    {
        crate::app::open_url(SOURCE_URL);
    }
    let choose = "Install from File…";
    let w2 = widgets::primary_button_width(ui, choose, false);
    let r = Rect::from_min_size(pos2(left + w + 8.0, y), vec2(w2, 32.0));
    if widgets::primary_button(ui, p, r, choose, None, ui.id().with("install")).clicked() {
        return rfd::FileDialog::new().add_filter("Commentaries Database", &["sqlite"]).pick_file();
    }
    None
}
