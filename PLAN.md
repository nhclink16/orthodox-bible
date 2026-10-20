# Orthodox Bible: fork plan

A personal Mac fork of [bible-rs](https://github.com/michaelshimeles/bible-rs) (MIT), made for Orthodox reading.
Planned 2026-10-20 against upstream commit `cdc5430`.

## Decisions

- **Audience:** just me. Personal Mac build. No Apple developer account and no store listing.
- **Fathers:** everyone before 1054 who was not condemned by a council.
- **Folder:** `~/Projects/personal/orthodox-bible`.
- **Mac first.** iPhone comes later.
- **Who does what:** Opus builds and reviews. No Sol: Codex isn't set up on this Mac (Nicholas, 2026-10-20).

## Sources

| What | Source | License |
|---|---|---|
| Old Testament, including the extra books | Brenton Septuagint, 1851 (`ebible.org/Scriptures/eng-Brenton_usfm.zip`) | Public domain |
| New Testament | KJV, already built into the app | Public domain |
| Fathers' quotes | [HistoricalChristianFaith/Commentaries-Database](https://github.com/HistoricalChristianFaith/Commentaries-Database), `commentaries.sqlite` release | The compilation and public-domain excerpts are public domain. The modern excerpts are fair use only, so we drop them. |

The Orthodox Study Bible is under copyright (Thomas Nelson) and is not used.

What we checked in the data:
- Brenton is the Old Testament only. It uses Septuagint numbering: 151 Psalms, and the Miserere is Psalm 50. It counts psalm headings as verses (Ps 50:1–2 is the heading).
- The extra books in the zip are TOB, JDT, ESG, WIS, SIR, BAR, LJE, SUS, BEL, DAG, 1MA, 2MA, 3MA, 4MA, 1ES and MAN.
- The commentary database uses Protestant Psalm numbering (the Miserere is 51). Each location is `chapter * 1_000_000 + verse`, and books have names like `sirach`.
- The database keeps 93,467 quotes. After the filter, about 34,500 remain, roughly 34 MB of text.
- The database's `condemned_by_council` flag misses Origen (4,335 quotes), Theodore of Mopsuestia (493), Didymus (322) and Evagrius (115). All four were condemned in 553. We remove them by name.

## Phase 0: Fork and set up (done 2026-10-20)

Done on branch `orthodox/identity`:
- Fork: https://github.com/nhclink16/orthodox-bible (remote `origin`, with `upstream` = michaelshimeles/bible-rs).
- Rust 1.99.1 installed with rustup. `cargo test --workspace` passes.
- `scripts/bundle-macos.sh` builds `dist/Orthodox Bible.app` (bundle ID `dev.nhclink16.orthodox-bible`), with no Sparkle.
- The data folder is `~/Library/Application Support/orthodox-bible`.
- Not changed: `scripts/release-macos.sh` and `.github/workflows/release.yml` still describe upstream's signed releases. We don't use them for a personal build.

Original checklist:

1. Fork the repo on GitHub. Clone it here and add `upstream`.
2. Install Rust with rustup. It needs 1.95 or newer. Homebrew is not needed.
3. Run `cargo test --workspace`, then `cargo run --release`.
4. Change the app's identity so it can't clash with the installed Bible app:
   - Bundle ID `dev.bible-rs.app` (`scripts/bundle-macos.sh:82`, `release-macos.sh:218`)
   - App name (`bundle-macos.sh:80-81`, `crates/app/src/lib.rs` ~162)
   - Data folder `bible-rs` (`crates/app/src/lib.rs:44-48`)
   - Keychain service (`crates/core/src/secrets.rs:39-41`)
   - Turn off the Sparkle updater. Remove `SUFeedURL` and `SUPublicEDKey` (`bundle-macos.sh:29-30`) and the updater (`crates/app/src/updater.rs`). The fork must never pull upstream's updates.
   - Leave the export tag `app: "bible-rs"` (`study.rs:546, 698`) alone, so notes can still move between the two apps.

## Phase 1: Extra books (done 2026-10-20 on `orthodox/extra-books`)

Done. The books sit in their Orthodox places, so grid order is reading order (see DECISIONS.md, "Orthodox fork"). Brenton imports, and Tobit, Sirach, 4 Maccabees and Greek Daniel 3 all read.

Left for later:
- The book picker lists the extra books even for the KJV. Picking one jumps to the next chapter the KJV has.
- SWORD imports don't read the extra books yet.
- Online translations never show them.
- Brenton's own book titles are archaic (Esaias, Osee, Kings I–IV). A setting for modern names could come later.

Original checklist:

Today the app holds exactly 66 books (`BOOK_COUNT`, `CHAPTER_COUNT=1189` at `books.rs:114-116`), and DECISIONS.md:373-374 says extra books are skipped "to keep the 66-book grid".

1. Add the extra books after Revelation (index 66 and up), so existing indexes don't move. Use a separate list for the order shown in the book picker (Orthodox order).
2. Bump the `.bbl` format to v2 (`format.rs:25`) and keep reading v1. Rebuild the bundled translations with `cargo run -p xtask -- build-data`.
3. Stop the importers from skipping these books: USFM `usfm.rs:163-178`, OSIS `osis.rs:142-150`, Zefania `zefania.rs:58-67`, SWORD `sword.rs:175` (read the KJVA/NRSVA apocrypha).
4. Fix the places that assume 66 books:
   - `model.rs:411` and `:472`
   - `plans.rs:82-89`
   - `search.rs:61-67`: add a third scope
   - `reference.rs:98-111`: accept a leading 4, as in "4 Maccabees"
   - `reference.rs:155`
   - `book_picker.rs:27, 100-115`
   - `sidebar.rs:268, 448, 771, 814`
   - `import_sheet.rs:463-489, 587, 625, 743, 797`
   - `pages/translations.rs:401, 430`
   - Literal book ids in `app.rs:83`, `onboarding.rs:515, 525` and `settings_ui.rs:1307`
   - The FFI: `ffi/src/lib.rs:315, 381, 490-507` and its tests
5. Book order: `testament()` is index-based today (`books.rs:150`). Give each book a "section" instead: OT, extra books, NT.
6. Before forking deeply, ask upstream in a GitHub issue whether they'd take this phase.

## Phase 2: Septuagint numbering and the built-in Orthodox text (done 2026-10-20 on `orthodox/septuagint`)

Done:
- A new built-in `lxx` Bible, now the default: LXX2012 (Brenton, updated spelling) for the Old Testament plus the bundled KJV New Testament. Built with `cargo run -p xtask -- build-orthodox`. It uses the grid's modern book names and drops LXX2012's "⌃" plural marker.
- Its psalms are stored at Hebrew/KJV numbers (`crates/core/src/lxx.rs`).
- A *Septuagint psalm numbers* setting (Settings → Reading, on by default) shows and reads psalm numbers the Orthodox way in every translation.
- New book: the Prayer of Azariah (Song of the Three Youths), after Daniel. The grid is now 82 books and 1,365 chapters.

Left for later:
- Only the Psalms are mapped. Jeremiah 25–51, Exodus 35–40, 1 Kings, Job and a few others keep LXX2012's numbering, so side by side with the KJV they can be a verse or a chapter apart. Fixing this needs a full Septuagint versification table under a license that fits MIT.
- Imported Septuagints (raw Brenton, for example) aren't remapped.

Original checklist:

1. Keep one internal numbering system, the Protestant one. That keeps notes, the side-by-side view and the commentary aligned.
2. Add a mapping table, LXX ↔ internal, used at import and for showing numbers. It has to cover:
   - Psalms 9–147: the one-off shift, plus the merges and splits at 9/10, 114/115, 116 and 147
   - Psalm headings counted as verses
   - Psalm 151
   - The Greek additions to Esther and Daniel
   - Brenton's Jeremiah order: check `99-XXAeng-Brenton.usfm` first
   Use a mapping source whose license fits MIT (Paratext `.vrs` files, after checking their terms). Don't copy SWORD's tables, which are GPL.
3. Show LXX numbers when reading an LXX text, with a setting for "Psalm numbering: Septuagint / Hebrew".
4. Build in "Orthodox (Brenton + KJV)": add an xtask source with a pinned SHA-256, add it to the `bundled.rs` table, and update `LICENSES/bibles.md`.

## Phase 3: Fathers panel

1. Add a new xtask step. It reads `commentaries.sqlite` and keeps authors before 1054 who aren't condemned, then also removes the four authors above.
2. In the same step, map book names to codes and Psalm numbers to the internal ones. Write a small zstd-compressed file (target about 10 MB) and build it in, so it works offline.
3. Keep author, year, source title and URL for every quote.
4. Add a commentary file that loads on demand and stays separate from `study.sqlite`.
5. Build a panel that copies `VersePanel` (`panel.rs:19`, inspector width 344). Give it its own slot so it doesn't collide with Compare (`app.rs:233, 744-751, 2304, 2379`).
6. Add a verse-menu item "Fathers" (`verse_menu.rs:41-73`, `actions.rs:75`), plus a small margin marker for verses that have quotes (`reader/mod.rs:594-640`).
7. Add a filter for East / West / everyone.
8. Record a new decision in DECISIONS.md. It replaces "no notes from source texts" for this feature only.

## Phase 4: Orthodox calendar (later; needs data research)

- A "Today's readings" page. The current reading plans are just day counts (`plans.rs`), so they can't model this.
- It needs the date of Pascha (Julian computus), the movable feasts counted from Pascha, and a table of the fixed-date readings.
- **Blocker:** we have no openly licensed table of the daily readings yet. Research this before building.

## Phase 5: iPhone (later)

- The books and numbering come through the FFI automatically.
- Hard-coded spots to fix: `ReaderView.swift:95` (`chapterCount = 1189`) and `ChapterLayout.swift:66` (`isPsalm` is book 18).
- The Fathers panel: add `commentary(verse) -> [CommentaryQuote]` to `ffi/src/lib.rs`, a new `Sheet.commentary` case (`Store.swift:60-74`, `RootView.swift:64-75`) and a button in `SelectionBar`.

## More Orthodox features (research, 2026-10-20)

All of these build on one piece: **computing Pascha, with an Old/New calendar setting**.

| Idea | Data | License | Effort |
|---|---|---|---|
| Fasting calendar, with levels (no meat / fish / wine and oil / strict) | Computed: Pascha plus fixed dates. OCA's outline of the rules (facts, not text). | Rules aren't copyrightable | Small to medium |
| Prayers: morning, evening, before and after Communion | Hapgood, *Service Book* (1906, 1922 scans; archive.org `cu31924029363128`). Coverdale Psalter on ponomar.net. | Public domain (pre-1929) | Medium (OCR cleanup) |
| Jesus Prayer with a prayer-rope counter | None needed | — | Small |
| Daily Epistle and Gospel | Compute it ourselves from the Typikon rules. Test against [orthocal-python](https://github.com/brianglass/orthocal-python) (MIT code; its data comes from church websites, so ask Brian Glass before reusing it). | Readings references are facts | Large |
| Kathisma of the day | The 20 kathismata plus the weekly cycle (a small table) | Not copyrightable | Small |
| Saints of the day | Names and dates only, written by us. Prologue from Ohrid English editions and Makarios' Synaxarion are copyrighted. | Facts only | Medium |
| Troparia for the major feasts | Hapgood | Public domain | Medium |
| Greek side by side | Swete LXX, Byzantine-text NT | Public domain (to verify) | Medium |

Phase 2 note: use **LXX2012** (eBible `eng-lxx2012`, public domain) instead of raw Brenton. It's Brenton with modern names (Isaiah, not Esaias).

Don't copy: Ponomar (GPL-3.0), OCA, GOARCH or Antiochian texts, or the Orthodox Study Bible.

Suggested order: Pascha + calendar setting, then fasting, then prayers + Jesus Prayer, then the daily readings, then the kathisma of the day.
