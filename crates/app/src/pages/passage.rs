//! Scripture read in a step on Today (Orthodox fork): the parts an item reads (the Epistle and
//! the Gospel, a kathisma's psalms, a plan's passage), each typeset from its first verse to its
//! last by the reader's typesetter ([`layout::layout_verses`]) and painted by the reader's own
//! code ([`paint_blocks`]), at the reader's size, measure and mode, with its highlights and note
//! markers. Nothing outside a part is laid out, so no neighbouring verse shows. A part has its
//! label when there are several, and an end line.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use bible_core::lectionary::Passage;
use bible_core::library::Chapter;
use bible_core::rule::Part;
use bible_core::{BookId, ChapterIdx, VerseRef};
use eframe::egui::{self, FontId, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::{PageEnv, group_label, text};
use crate::reader::layout::{self, ChapterLayout, Ctx};
use crate::reader::{Marks, paint_blocks};
use crate::theme::{self, Palette};
use crate::widgets::{Button, Kind};

/// From a part's label to its text, between its pieces, and around its end line.
const LABEL_H: f32 = 28.0;
const PIECE_GAP: f32 = 16.0;
const END_H: f32 = 56.0;
const PART_GAP: f32 = 24.0;

/// Where to bring into view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Goto {
    /// The top of the n-th part ("Go to the Gospel").
    Part(usize),
    /// A verse, to the top.
    Verse(VerseRef),
    /// A verse, only if it isn't in view.
    Show(VerseRef),
}

/// What a chapter's piece shows: its verses, or why it can't.
enum Text {
    Laid(Arc<ChapterLayout>),
    /// Loading, or a chapter this translation doesn't have, and for a chapter that couldn't be
    /// fetched, the chapter to try again.
    Message(String, Option<ChapterIdx>),
}

/// A stretch of one chapter, where it sits from the passage's top.
struct Piece {
    text: Text,
    top: f32,
    /// The verse it starts on, for "16:25" over a piece that doesn't follow on from the one
    /// before.
    first: VerseRef,
    gap_label: bool,
}

/// What the passage saw while it was drawn.
#[derive(Default)]
pub struct Shown {
    pub bottom: f32,
    /// The verse at the top of the view, and which of [`PassageView::stops`] is in view.
    pub top: Option<VerseRef>,
    pub stop: usize,
    /// The end of the last part is in view.
    pub end_seen: bool,
    /// The verse clicked, when verses can be.
    pub clicked: Option<VerseRef>,
    /// Where to scroll to bring what was asked for under the toolbar: the y to put at the top
    /// of the page's view. The caller scrolls, after any scroll area drawn over the text (the
    /// first to finish takes the request).
    pub scroll_to: Option<f32>,
}

/// The parts laid out, kept until they or the reader's settings change.
#[derive(Default)]
pub struct PassageView {
    key: u64,
    parts: Vec<Part>,
    pieces: Vec<Vec<Piece>>,
    /// Where each part's label (or text) and end line start, from the passage's top.
    part_tops: Vec<f32>,
    end_tops: Vec<f32>,
    height: f32,
    /// What the card and the strip name the passage by, and where each starts.
    stops: Vec<(String, f32)>,
    /// The verse at the top of the view last frame: a new layout keeps it there.
    last_top: Option<VerseRef>,
    /// How full the font atlas was last frame (see [`crate::reader::atlas_rebuilt`]).
    atlas: f32,
}

/// The stretches of chapters `passages` read: (chapter, first verse, last verse). A stretch that
/// goes straight on from the one before, with no heading of its own, joins it (`lxx`: psalms in
/// Septuagint numbering).
fn stretches(passages: &[Passage], lxx: bool) -> Vec<(ChapterIdx, u8, u8)> {
    let mut out: Vec<(ChapterIdx, u8, u8)> = Vec::new();
    for p in passages {
        for ch in p.from.0..=p.to.0 {
            let Some(c) = ChapterIdx::new(p.book, ch) else { continue };
            let a = if ch == p.from.0 { p.from.1.max(1) } else { 1 };
            let b = if ch == p.to.0 { p.to.1 } else { u8::MAX };
            match out.last_mut() {
                Some((pc, _, pb)) if *pc == c && pb.saturating_add(1) == a && !opens(c, a, lxx) => *pb = b,
                _ => out.push((c, a, b)),
            }
        }
    }
    out
}

/// Verse `v` of chapter `c` opens it, or opens a psalm as the psalms are numbered.
fn opens(c: ChapterIdx, v: u8, lxx: bool) -> bool {
    bible_core::lxx::shown_in(lxx, c.book(), c.chapter(), v.max(1)).2 == 1
}

/// The bottom of what a layout draws (it ends in blank space for the reader's scroll).
fn drawn_bottom(l: &ChapterLayout) -> f32 {
    let blocks = l.blocks.iter().map(|b| b.rect().bottom());
    let labels = l.labels.iter().map(|(pos, g)| pos.y + g.size().y);
    blocks.chain(labels).fold(0.0, f32::max)
}

impl PassageView {
    /// The names it goes by in the card: its parts when it has several ("Epistle · Gospel"),
    /// else the psalms or chapters it opens ("85 · 86 · 87", "John 13 · 14 · 15"); none when
    /// it is one stretch.
    pub fn stops(&self) -> Vec<String> {
        if self.stops.len() > 1 { self.stops.iter().map(|(s, _)| s.clone()).collect() } else { Vec::new() }
    }

    /// The verse at the top of the view when last drawn.
    pub fn top(&self) -> Option<VerseRef> {
        self.last_top
    }

    fn key(env: &PageEnv<'_>, parts: &[Part], width: f32) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        width.to_bits().hash(&mut h);
        serde_json::to_string(&env.settings.typo).unwrap_or_default().hash(&mut h);
        env.settings.theme.hash(&mut h);
        env.settings.translation.hash(&mut h);
        format!("{parts:?}").hash(&mut h);
        h.finish()
    }

    /// Typesets `parts` `width` wide, as the reader would.
    fn lay_out(&mut self, ui: &Ui, env: &PageEnv<'_>, parts: &[Part], width: f32) -> bool {
        let typo = &env.settings.typo;
        let cx = Ctx::new(typo, env.settings.theme, width, 0.0);
        let s = |pt: f32| pt * typo.size / layout::DESIGN_EM;
        let labelled = parts.len() > 1;
        let id = &env.settings.translation;
        let abbrev = env.library.get(id).map_or(id.clone(), |e| e.meta.abbrev.clone());
        let mut pending = false;
        self.pieces.clear();
        self.part_tops.clear();
        self.end_tops.clear();
        self.stops.clear();
        let mut y = 0.0;
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                y += PART_GAP;
            }
            self.part_tops.push(y);
            if labelled {
                self.stops.push((part.label.clone(), y));
                y += LABEL_H;
            }
            let mut pieces = Vec::new();
            let mut prev: Option<(ChapterIdx, u8)> = None;
            let lxx = typo.septuagint_psalms;
            for (k, (c, a, b)) in stretches(&part.passages, lxx).into_iter().enumerate() {
                let opens = opens(c, a, lxx);
                // Following on: the next verse, the next chapter's start, or a psalm's.
                let follows = opens
                    || prev.is_none_or(|(pc, pb)| {
                        (pc == c && pb.saturating_add(1) == a)
                            || (a == 1 && pb == u8::MAX && pc.next() == Some(c))
                    });
                let gap_label = k > 0 && !follows;
                if k > 0 {
                    y += if opens { s(24.0) } else { PIECE_GAP };
                }
                if gap_label {
                    y += 20.0;
                }
                if (opens || k == 0) && !labelled {
                    let (n, _) = VerseRef::new(c.book(), c.chapter(), a).shown_numbers();
                    let name = match (self.stops.is_empty(), c.book()) {
                        (true, b) if b != BookId::PSALMS => format!("{} {n}", b.ref_name()),
                        _ => n.to_string(),
                    };
                    self.stops.push((name, y));
                }
                let text = match env.library.chapter(id, c) {
                    Chapter::Ready(t) => Text::Laid(Arc::new(
                        ui.ctx().fonts_mut(|f| layout::layout_verses(f, &cx, &t, c, Some(a..=b))),
                    )),
                    Chapter::Pending => {
                        pending = true;
                        Text::Message("Loading…".into(), None)
                    }
                    Chapter::NoKey => Text::Message(
                        "Add your API key in Settings → Translations to read this translation.".into(),
                        None,
                    ),
                    Chapter::Failed(m) => Text::Message(format!("Couldn't load this chapter: {m}"), Some(c)),
                    Chapter::Missing => Text::Message(missing(env, &abbrev, c), None),
                };
                let h = match &text {
                    Text::Laid(l) => drawn_bottom(l),
                    Text::Message(..) => 28.0 + 48.0,
                };
                let first = VerseRef::new(c.book(), c.chapter(), a);
                pieces.push(Piece { text, top: y, first, gap_label });
                y += h;
                prev = Some((c, b));
            }
            self.pieces.push(pieces);
            // A part without an end line (the Fathers' one verse) ends with its text.
            if part.end.is_empty() {
                self.end_tops.push(y);
            } else {
                self.end_tops.push(y + s(16.0));
                y += s(16.0) + END_H;
            }
        }
        self.height = y;
        self.parts = parts.to_vec();
        !pending
    }

    /// Where verse `v` starts, from the passage's top.
    fn verse_y(&self, v: VerseRef) -> Option<f32> {
        self.pieces.iter().flatten().find_map(|p| match &p.text {
            Text::Laid(l)
                if l.chapter == v.chapter_idx() && l.anchors.iter().any(|a| a.number == v.verse) =>
            {
                l.anchor(0, v.verse).map(|a| p.top + a.top)
            }
            _ => None,
        })
    }

    /// Draws `parts` in a column `width` wide at `x`, from `top`. Text above `view_top` is under
    /// the toolbar (and the strip). `focus` is tinted; `clickable` lets a click pick a verse.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut Ui,
        env: &PageEnv<'_>,
        parts: &[Part],
        (x, top, width): (f32, f32, f32),
        view_top: f32,
        focus: Option<VerseRef>,
        clickable: bool,
        mut goto: Option<Goto>,
    ) -> Shown {
        let p = env.settings.theme.palette();
        let key = Self::key(env, parts, width);
        let rebuilt = crate::reader::atlas_rebuilt(ui.ctx(), &mut self.atlas);
        if key != self.key || self.parts != parts || rebuilt {
            // Typeset again for a new size, mode, width or translation, with the verse that was at
            // the top still there once the text is scrolled into.
            if !rebuilt && self.key != 0 && goto.is_none() && self.parts == parts && top < view_top {
                goto = self.last_top.map(Goto::Verse);
            }
            let settled = self.lay_out(ui, env, parts, width);
            self.key = if settled { key } else { 0 };
            if !settled {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
            }
        }
        let clip = ui.clip_rect();
        let painter = ui.painter().clone();
        let typo = &env.settings.typo;
        let marks = Marks {
            study: env.study,
            typo,
            selected: &[],
            lit: focus.map(|v| (v, 1.0)),
            markers_x: x + width + layout::MARKER_GAP * typo.size / layout::DESIGN_EM,
            note_open: None,
        };
        let view = Rect::from_min_max(pos2(clip.left(), view_top), clip.max);
        let pointer = ui.input(|i| i.pointer.hover_pos()).filter(|pt| view.contains(*pt));
        let mut shown = Shown { bottom: top + self.height, ..Default::default() };
        let mut hit = None;
        // The verse at the top: the first whose text reaches below the top of the view.
        let line = view_top + 8.0;
        for (i, part) in self.parts.iter().enumerate() {
            let pt = top + self.part_tops[i];
            if self.parts.len() > 1 && pt < clip.bottom() && pt + LABEL_H > clip.top() {
                let label = format!("{} · {}", part.label, part.reference);
                group_label(&painter, p, x, pt, &label);
            }
            for piece in &self.pieces[i] {
                let y = top + piece.top;
                if piece.gap_label {
                    let (c, v) = piece.first.shown_numbers();
                    let at = format!("{c}:{v}");
                    text(&painter, x, y - 20.0, 16.0, &at, theme::ui_medium(11.0), p.ink_3, false);
                }
                match &piece.text {
                    Text::Laid(l) => {
                        let origin = vec2(x, y);
                        if y > clip.bottom() || y + l.height < clip.top() {
                            continue;
                        }
                        for (pos, g) in &l.labels {
                            let at = (origin + *pos).to_pos2();
                            if at.y < clip.bottom() && at.y + g.size().y > clip.top() {
                                painter.galley(at, g.clone(), p.ink);
                            }
                        }
                        let painted = paint_blocks(&painter, p, l, origin, clip, &marks, pointer);
                        hit = painted.hit.or(hit);
                        for a in l.anchors.iter().filter(|a| a.column == 0) {
                            if shown.top.is_none() && y + a.bottom > line {
                                shown.top =
                                    Some(VerseRef::new(l.chapter.book(), l.chapter.chapter(), a.number));
                            }
                        }
                    }
                    Text::Message(m, retry) => {
                        let font = FontId::new(typo.size, theme::family(theme::SERIF_ITALIC));
                        text(&painter, x, y, 28.0, m, font, p.ink_2, false);
                        if let Some(c) = retry {
                            let b = Button::new(Kind::Secondary, "Try again").small();
                            let id = ui.id().with(("try-again", c.0));
                            if b.show_left(ui, p, x, y + 52.0, id).1.clicked()
                                && let Some(o) =
                                    env.library.get(&env.settings.translation).and_then(|e| e.online())
                            {
                                o.retry(*c);
                            }
                        }
                    }
                }
            }
            let ey = top + self.end_tops[i];
            if !part.end.is_empty() && ey < clip.bottom() && ey + END_H > clip.top() {
                end_line(&painter, p, x + width / 2.0, ey + 20.0, &part.end);
            }
            if i + 1 == self.parts.len() && ey + 28.0 < clip.bottom() {
                shown.end_seen = true;
            }
        }
        // The stop in view: the last to start above the upper third of the view.
        let third = view_top + (clip.bottom() - view_top) / 3.0;
        shown.stop = self.stops.iter().rposition(|(_, y)| top + y <= third).unwrap_or(0);
        if clickable {
            let area = Rect::from_min_max(pos2(x - 8.0, top), pos2(x + width + 8.0, top + self.height));
            let resp = ui.interact(area, ui.id().with("passage"), Sense::click());
            if hit.is_some() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                shown.clicked = hit;
            }
        }
        // Bring a part or a verse into view, below the toolbar.
        let under = view_top - clip.top() + 8.0;
        let target = match goto {
            Some(Goto::Part(i)) => self.part_tops.get(i).map(|y| top + y),
            Some(Goto::Verse(v)) => self.verse_y(v).map(|y| top + y),
            Some(Goto::Show(v)) => self
                .verse_y(v)
                .map(|y| top + y)
                .filter(|y| *y < view_top + 8.0 || *y > clip.bottom() - 3.0 * typo.size),
            None => None,
        };
        shown.scroll_to = target.map(|y| y - under);
        self.last_top = shown.top.or(self.last_top);
        shown
    }
}

/// "KJV doesn't include Wisdom of Solomon. LXX does.", as the reader says it.
fn missing(env: &PageEnv<'_>, abbrev: &str, c: ChapterIdx) -> String {
    let lxx = bible_core::bundled::DEFAULT_ID;
    let has = env.settings.translation != lxx && matches!(env.library.chapter(lxx, c), Chapter::Ready(_));
    let also = match env.library.get(lxx).filter(|_| has) {
        Some(e) => format!(" {} does.", e.meta.abbrev),
        None => String::new(),
    };
    format!("{abbrev} doesn't include {}.{also}", c.book().name())
}

/// A part's end, centred on `cx`: the cross over "End of the Gospel" between two hairlines.
pub fn end_line(painter: &egui::Painter, p: &Palette, cx: f32, top: f32, s: &str) {
    super::today::cross(painter, pos2(cx, top), p.ink_2);
    let font = theme::ui_medium(12.0);
    let g = painter.layout_no_wrap(s.to_string(), font.clone(), p.ink_2);
    let w = g.size().x;
    text(painter, cx - w / 2.0, top + 10.0, 18.0, s, font, p.ink_2, false);
    let y = top + 19.5;
    let stroke = Stroke::new(1.0, p.separator);
    painter.hline(cx - w / 2.0 - 52.0..=cx - w / 2.0 - 12.0, y, stroke);
    painter.hline(cx + w / 2.0 + 12.0..=cx + w / 2.0 + 52.0, y, stroke);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bible_core::lectionary::parse_passage;

    fn spans(refs: &[&str]) -> Vec<(String, u8, u8)> {
        let passages: Vec<Passage> = refs.iter().map(|r| parse_passage(r).unwrap()).collect();
        stretches(&passages, true).into_iter().map(|(c, a, b)| (c.display(), a, b)).collect()
    }

    #[test]
    fn a_reading_is_typeset_chapter_by_chapter_and_piece_by_piece() {
        assert_eq!(spans(&["Luke 9:44-50"]), [("Luke 9".to_string(), 44, 50)]);
        // Holy Friday's first Gospel: each chapter, from 13:31 to the end of 17 and 18:1.
        let all = u8::MAX;
        let friday: Vec<(String, u8, u8)> =
            [("John 13", 31, all), ("John 14", 1, all), ("John 15", 1, all), ("John 16", 1, all)]
                .into_iter()
                .chain([("John 17", 1, all), ("John 18", 1, 1)])
                .map(|(c, a, b)| (c.to_string(), a, b))
                .collect();
        assert_eq!(spans(&["John 13:31-18:1"]), friday);
        assert_eq!(spans(&["Rom 14:19-23", "Rom 16:25-27"]).len(), 2);
        // Kathisma 16's Psalms 114 and 115 share the grid's 116: two stretches, since 116:10 opens
        // the Septuagint's 115, or one in Hebrew numbering, where it opens nothing.
        let grid_116 = |lxx| -> Vec<(u8, u8)> {
            let k16 = stretches(&bible_core::calendar::kathisma_passages(16), lxx);
            k16.into_iter().filter(|(c, _, _)| c.chapter() == 116).map(|(_, a, b)| (a, b)).collect()
        };
        assert_eq!(grid_116(true), [(1, 9), (10, 19)]);
        assert_eq!(grid_116(false), [(1, 19)]);
    }
}
