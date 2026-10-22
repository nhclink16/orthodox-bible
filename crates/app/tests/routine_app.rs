//! A block of the prayer rule prayed through on Today in the running app (Orthodox fork): ⌘↩ is
//! Done and plain ↩ does nothing, Escape and ⌘[ go back to Today, the palette and the shortcuts
//! over a step take its keys, Space counts only in the Jesus Prayer's counter and the count is
//! kept, the arrows and ⌘D leave the reader alone, Open in reader comes back with ⌘[, and after a
//! whole evening the reader is where it was. Its own test binary because it sets
//! `BIBLE_RS_OFFSCREEN` (no macOS menu bar, which needs the main thread) before anything runs.

use std::sync::Arc;
use std::time::{Duration, Instant};

use bible_core::books::{BookId, ChapterIdx, VerseRef};
use bible_rs::app::{BibleApp, Startup, initialize, online_counter};
use bible_rs::pages::Page;
use bible_rs::theme;
use eframe::egui::{self, Key, Modifiers};
use egui_kittest::Harness;

const JOHN: BookId = BookId(58);

/// The step open on Today, if one is.
fn step(h: &Harness<'_, BibleApp>) -> Option<String> {
    let today = &h.state().pages.today;
    today.stepping().then(|| today.session().and_then(|s| s.item.clone()).unwrap_or_default())
}

fn press(h: &mut Harness<'_, BibleApp>, modifiers: Modifiers, key: Key) {
    h.key_press_modifiers(modifiers, key);
    h.run_steps(3);
}

/// Opens the Today page with `demo` (`begin:<block>`, an item's id) run on it.
fn today(h: &mut Harness<'_, BibleApp>, demo: &str) {
    h.state_mut().pages.open(Page::Today);
    h.state_mut().pages.today.demo(demo);
    h.run_steps(3);
}

#[test]
fn a_block_is_prayed_through_on_today_and_the_reader_is_left_alone() {
    // SAFETY: the only test in this binary, so nothing else reads the environment meanwhile.
    unsafe { std::env::set_var("BIBLE_RS_OFFSCREEN", "1") };
    let dir = std::env::temp_dir().join(format!("bible-rs-routine-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut init = initialize(dir.clone(), Arc::new(|| {})).expect("loading data");
    init.settings.onboarded = true;
    init.settings.chapter = ChapterIdx::new(JOHN, 1).unwrap().0;
    let startup =
        Startup { t0: Instant::now(), probe: false, init: Duration::ZERO, first_ui: None, frames: 0 };
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 800.0)).build_eframe(|cc| {
        cc.egui_ctx.set_fonts(theme::font_definitions());
        BibleApp::new(cc, init, online_counter(), startup)
    });
    h.run_steps(4);
    // The reader as it is left: a verse selected, nothing in the inspector.
    h.state_mut().reader.set_selection(Some(VerseRef::new(JOHN, 1, 1)));
    let reader = |h: &Harness<'_, BibleApp>| {
        let s = h.state();
        (s.reader.location, s.reader.anchor, s.history.depth(), s.reader.selection, s.fathers.is_none())
    };
    let before = reader(&h);

    today(&mut h, "begin:evening");
    let first = step(&h).expect("Begin opens a step");
    // Plain ↩ does nothing in a step; the arrows and Space don't move the reader.
    for key in [Key::Enter, Key::ArrowRight, Key::ArrowLeft, Key::ArrowDown, Key::Space] {
        press(&mut h, Modifiers::NONE, key);
        assert_eq!(step(&h).as_ref(), Some(&first), "{key:?}");
    }
    // The palette and the Keyboard Shortcuts over a step take its keys: Escape closes them, not
    // the step, and ⌘↩ ticks nothing behind them. ⌘D doesn't bookmark the reader's verse.
    press(&mut h, Modifiers::COMMAND, Key::K);
    press(&mut h, Modifiers::NONE, Key::Escape);
    assert_eq!(step(&h).as_ref(), Some(&first), "Escape behind the palette");
    press(&mut h, Modifiers::COMMAND, Key::Slash);
    press(&mut h, Modifiers::COMMAND, Key::Enter);
    press(&mut h, Modifiers::NONE, Key::Escape);
    assert_eq!(step(&h).as_ref(), Some(&first), "⌘↩ or Escape behind the shortcuts");
    press(&mut h, Modifiers::COMMAND, Key::D);
    assert!(!h.state().study.is_bookmarked(VerseRef::new(JOHN, 1, 1)), "⌘D on Today");
    let day = bible_core::calendar::civil;
    let t = bible_rs::time::local(bible_core::study::now());
    let d = day(t.year, t.month, t.day);
    assert!(!h.state().study.rule_done(d).contains(&first), "↩ or ⌘↩ ticked {first}");

    // Escape goes back to Today, paused there; a second Escape leaves the reader's selection.
    press(&mut h, Modifiers::NONE, Key::Escape);
    assert_eq!(step(&h), None);
    assert_eq!(h.state().pages.page, Page::Today);
    press(&mut h, Modifiers::NONE, Key::Escape);
    // Continue comes back to the same step; ⌘[ goes back to Today too.
    today(&mut h, "begin:evening");
    assert_eq!(step(&h).as_ref(), Some(&first));
    press(&mut h, Modifiers::COMMAND, Key::OpenBracket);
    assert_eq!((step(&h), h.state().pages.page), (None, Page::Today));

    // ⌘↩ is Done: each step ticked in turn, down to the end page, and ⌘↩ there is Today.
    today(&mut h, "begin:evening");
    let mut ticked = Vec::new();
    while let Some(id) = step(&h).filter(|id| !id.is_empty()) {
        assert!(!ticked.contains(&id), "{id} again");
        press(&mut h, Modifiers::COMMAND, Key::Enter);
        ticked.push(id);
    }
    assert_eq!(step(&h).as_deref(), Some(""), "the end page");
    let done = h.state().study.rule_done(d);
    assert!(ticked.iter().all(|id| done.contains(id)), "{ticked:?} ticked: {done:?}");
    press(&mut h, Modifiers::COMMAND, Key::Enter);
    assert_eq!(step(&h), None);
    assert_eq!(reader(&h), before, "the reader is where it was");

    // Space counts in the Jesus Prayer's counter, and only there.
    today(&mut h, "midday-jesus-prayer");
    for _ in 0..3 {
        press(&mut h, Modifiers::NONE, Key::Space);
    }
    assert_eq!(h.state().pages.today.count(d, "midday-jesus-prayer"), 3);
    assert_eq!(reader(&h), before, "Space in the counter left the reader alone");
    // The count is kept for the day, whatever is begun meanwhile.
    today(&mut h, "meal-prayer");
    today(&mut h, "midday-jesus-prayer");
    press(&mut h, Modifiers::NONE, Key::Space);
    assert_eq!(h.state().pages.today.count(d, "midday-jesus-prayer"), 4);

    // Back from a prayer's view closes it, on Today.
    today(&mut h, "meal-prayer");
    assert_eq!(step(&h).as_deref(), Some("meal-prayer"));
    press(&mut h, Modifiers::COMMAND, Key::OpenBracket);
    assert_eq!((step(&h), h.state().pages.page), (None, Page::Today));

    // Open in reader (⌘O) shows the reading in the reader; ⌘[ there comes back to the step, and
    // the selection it made, untouched, goes.
    today(&mut h, "morning-reading");
    press(&mut h, Modifiers::COMMAND, Key::O);
    assert_eq!(h.state().pages.page, Page::Reader);
    assert!(h.state().today_return.is_some());
    assert!(h.state().reader.selection.is_some());
    press(&mut h, Modifiers::COMMAND, Key::OpenBracket);
    assert_eq!(step(&h).as_deref(), Some("morning-reading"));
    assert!(h.state().today_return.is_none());
    assert_eq!(h.state().reader.selection, None);
    let _ = std::fs::remove_dir_all(&dir);
}
