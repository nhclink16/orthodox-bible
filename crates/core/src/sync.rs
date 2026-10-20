//! Sync between the user's devices through a shared folder (iCloud Drive), with no server.
//!
//! Each device logs the latest change to every record (see [`crate::study`]) and writes the whole
//! log to `<device-id>.json` in the folder. Syncing merges the other devices' files: for each
//! record the newest change wins (a tie goes to the larger device id, so every device picks the
//! same one), and a deletion is a change like any other.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::study::StudyStore;

/// The sync file format this version writes; newer ones are refused.
const VERSION: u32 = 1;

/// The latest change to one record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Change {
    pub key: String,
    /// The record as JSON; `None` when it was deleted.
    pub value: Option<String>,
    /// Unix seconds.
    pub at: i64,
    pub device: String,
}

#[derive(Serialize, Deserialize)]
struct SyncFile {
    version: u32,
    device: String,
    changes: Vec<Change>,
}

/// What a folder sync did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncReport {
    /// Changes from other devices applied here.
    pub applied: usize,
    /// Other devices' files read.
    pub devices: usize,
    /// Files that couldn't be read (half downloaded, or from a newer version), with why.
    pub skipped: Vec<String>,
}

/// This device's sync file: every record's latest change.
pub fn export(store: &StudyStore) -> Result<String> {
    let file = SyncFile { version: VERSION, device: store.device_id(), changes: store.changes()? };
    Ok(serde_json::to_string(&file)?)
}

/// Merges another device's sync file. Returns how many of its changes were applied, so merging
/// the same file again returns 0.
pub fn merge(store: &mut StudyStore, json: &str) -> Result<usize> {
    let file: SyncFile = serde_json::from_str(json).context("not a sync file")?;
    if file.version > VERSION {
        bail!("made by a newer version of the app (sync format {})", file.version);
    }
    let mut latest: HashMap<String, (i64, String)> =
        store.changes()?.into_iter().map(|c| (c.key, (c.at, c.device))).collect();
    let mut newer = Vec::new();
    for c in file.changes {
        if latest.get(&c.key).is_none_or(|(at, device)| (c.at, &c.device) > (*at, device)) {
            latest.insert(c.key.clone(), (c.at, c.device.clone()));
            newer.push(c);
        }
    }
    store.apply_changes(&newer)
}

/// Merges every device's file in `dir` (made if missing), then writes this device's own. Its own
/// file is read too: a device restored from an older backup gets back what it had sent since. A
/// file that can't be read is skipped and reported, not fatal.
pub fn sync_folder(store: &mut StudyStore, dir: &Path) -> Result<SyncReport> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let own = format!("{}.json", store.device_id());
    // Hidden names are never sync files: `._<id>.json` is macOS's AppleDouble sidecar, made when
    // the folder sits on a disk that isn't APFS or HFS+. (iCloud's `.<name>.json.icloud`
    // placeholders and our own temp file don't end in `.json`.)
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|name| name.ends_with(".json") && !name.starts_with('.'))
        .collect();
    names.sort();
    let mut report = SyncReport::default();
    for name in names {
        let merged = std::fs::read_to_string(dir.join(&name))
            .map_err(anyhow::Error::from)
            .and_then(|json| merge(store, &json));
        match merged {
            Ok(n) => {
                report.applied += n;
                report.devices += usize::from(name != own);
            }
            Err(e) => report.skipped.push(format!("{name}: {e:#}")),
        }
    }
    let json = export(store)?;
    let path = dir.join(&own);
    // Written whole and then renamed, so other devices never read half a file; left alone when
    // nothing changed, so iCloud has nothing to upload.
    if std::fs::read_to_string(&path).ok().as_deref() != Some(json.as_str()) {
        let tmp = dir.join(format!(".{own}.tmp"));
        std::fs::write(&tmp, &json).with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, &path).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;
    use crate::books::{BookId, VerseRef};
    use crate::study::{HighlightColor, Include, ReadingSession, now};

    fn jn316() -> VerseRef {
        VerseRef::new(BookId::from_code("JHN").unwrap(), 3, 16)
    }

    fn ps23() -> VerseRef {
        VerseRef::new(BookId::from_code("PSA").unwrap(), 23, 1)
    }

    fn session(started: i64) -> ReadingSession {
        ReadingSession {
            started,
            book: BookId::from_code("JHN").unwrap(),
            chapter: 3,
            translation: "kjv".into(),
            seconds: 300,
            furthest: 36,
            verses: 36,
        }
    }

    fn store() -> StudyStore {
        StudyStore::open_in_memory().unwrap()
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bible-rs-sync-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// The change logged for `key`, as (time, device).
    fn stamp(s: &StudyStore, key: &str) -> (i64, String) {
        let c = s.changes().unwrap().into_iter().find(|c| c.key == key).unwrap();
        (c.at, c.device)
    }

    #[test]
    fn edits_on_one_device_appear_on_the_other() {
        let (mut a, mut b) = (store(), store());
        assert_ne!(a.device_id(), b.device_id());
        a.set_highlight(jn316(), Some(HighlightColor::Blue)).unwrap();
        a.set_note(jn316(), "God so loved").unwrap();
        a.set_bookmark(ps23(), Some("Shepherd")).unwrap();
        a.start_plan("john-21", 1_000).unwrap();
        a.set_plan_read("john-21", 1, 0, true).unwrap();
        a.set_plan_read("odd:id", 3, 2, true).unwrap();
        a.record_reading(&session(5_000)).unwrap();
        a.finish_plan("john-21", Some(2_000)).unwrap();

        let g = b.generation();
        assert_eq!(merge(&mut b, &export(&a).unwrap()).unwrap(), 7);
        assert!(b.generation() > g);
        assert_eq!(b.highlight(jn316()), Some(HighlightColor::Blue));
        assert_eq!(b.note(jn316()), a.note(jn316()));
        assert_eq!(b.bookmark(ps23()), a.bookmark(ps23()));
        assert_eq!(b.plans(), a.plans());
        assert_eq!(b.plan_reads("john-21"), a.plan_reads("john-21"));
        assert_eq!(b.plan_reads("odd:id").into_iter().collect::<Vec<_>>(), [(3, 2)]);
        assert_eq!(b.reading_since(0), a.reading_since(0));
        // Changes keep the time and device they were made with.
        assert_eq!(stamp(&b, "note:JHN.3.16"), stamp(&a, "note:JHN.3.16"));
        assert_eq!(stamp(&b, "note:JHN.3.16").1, a.device_id());

        // An import is logged too.
        let mut c = store();
        c.import_json(&a.export_json_with("kjv", |_| None, Include::EVERYTHING).unwrap()).unwrap();
        let keys: Vec<_> = c.changes().unwrap().into_iter().map(|c| c.key).collect();
        assert_eq!(
            keys,
            [
                "bookmark:PSA.23.1",
                "highlight:JHN.3.16",
                "note:JHN.3.16",
                "plan-read:john-21:1:0",
                "plan:john-21",
                "reading:5000"
            ]
        );
    }

    #[test]
    fn merging_twice_applies_nothing() {
        let (mut a, mut b) = (store(), store());
        a.set_note(jn316(), "once").unwrap();
        let json = export(&a).unwrap();
        assert_eq!(merge(&mut b, &json).unwrap(), 1);
        let g = b.generation();
        assert_eq!(merge(&mut b, &json).unwrap(), 0);
        assert_eq!(b.generation(), g);
        // Nor does A's own change coming back from B.
        assert_eq!(merge(&mut a, &export(&b).unwrap()).unwrap(), 0);
    }

    #[test]
    fn deletions_travel() {
        let (mut a, mut b) = (store(), store());
        a.set_highlight(jn316(), Some(HighlightColor::Yellow)).unwrap();
        a.set_note(jn316(), "a note").unwrap();
        a.set_bookmark(ps23(), Some("")).unwrap();
        a.start_plan("john-21", 1_000).unwrap();
        a.set_plan_read("john-21", 1, 0, true).unwrap();
        a.record_reading(&session(5_000)).unwrap();
        merge(&mut b, &export(&a).unwrap()).unwrap();
        assert_eq!(b.counts(), (1, 1, 1));

        b.set_highlight(jn316(), None).unwrap();
        b.set_note(jn316(), " ").unwrap();
        assert!(!b.toggle_bookmark(ps23()).unwrap());
        b.stop_plan("john-21").unwrap();
        b.clear_reading(0).unwrap();
        assert_eq!(merge(&mut a, &export(&b).unwrap()).unwrap(), 6);
        assert_eq!(a.counts(), (0, 0, 0));
        assert!(a.plans().is_empty() && a.plan_reads("john-21").is_empty());
        assert!(a.reading_since(0).is_empty());
    }

    #[test]
    fn the_latest_edit_wins_on_both_sides() {
        let (mut a, mut b) = (store(), store());
        let key = "note:JHN.3.16";
        // An edit made after a sync wins, even within the same second.
        a.set_note(jn316(), "first").unwrap();
        merge(&mut b, &export(&a).unwrap()).unwrap();
        b.set_note(jn316(), "second").unwrap();
        merge(&mut a, &export(&b).unwrap()).unwrap();
        assert_eq!(a.note(jn316()).unwrap().body, "second");

        // Edits made at once on both: the later one, or on a tie the larger device id, wins
        // everywhere.
        a.set_note(jn316(), "from a").unwrap();
        b.set_note(jn316(), "from b").unwrap();
        let expected = if stamp(&a, key) > stamp(&b, key) { "from a" } else { "from b" };
        let (ja, jb) = (export(&a).unwrap(), export(&b).unwrap());
        merge(&mut a, &jb).unwrap();
        merge(&mut b, &ja).unwrap();
        assert_eq!(a.note(jn316()).unwrap().body, expected);
        assert_eq!(b.note(jn316()).unwrap().body, expected);
        assert_eq!(stamp(&a, key), stamp(&b, key));
    }

    #[test]
    fn ties_go_to_the_larger_device_id() {
        let mut a = store();
        a.set_note(jn316(), "mine").unwrap();
        let (at, _) = stamp(&a, "note:JHN.3.16");
        let file = |at: i64, device: &str| {
            let value = json!({"body": "theirs", "created": at, "updated": at}).to_string();
            json!({"version": 1, "device": device,
                   "changes": [{"key": "note:JHN.3.16", "value": value, "at": at, "device": device}]})
            .to_string()
        };
        // Device ids are hex, so "" sorts below this one and "z" above it.
        assert_eq!(merge(&mut a, &file(at, "")).unwrap(), 0);
        assert_eq!(merge(&mut a, &file(at - 10, "z")).unwrap(), 0);
        assert_eq!(a.note(jn316()).unwrap().body, "mine");
        assert_eq!(merge(&mut a, &file(at, "z")).unwrap(), 1);
        assert_eq!(a.note(jn316()).unwrap().body, "theirs");
    }

    #[test]
    fn existing_data_is_sent_on_the_first_sync() {
        let dir = temp_dir("backfill");
        let path = dir.join("study.sqlite");
        let mut rule = store().rule();
        rule.blocks.truncate(1);
        {
            let mut old = StudyStore::open(&path).unwrap();
            old.set_highlight(jn316(), Some(HighlightColor::Green)).unwrap();
            old.set_note(jn316(), "from before sync").unwrap();
            old.start_plan("john-21", 1_000).unwrap();
            old.set_plan_read("john-21", 1, 0, true).unwrap();
            old.record_reading(&session(5_000)).unwrap();
            old.set_rule(&rule).unwrap();
            old.set_rule_done(20_000, "morning-prayers", true).unwrap();
        }
        // A database from before sync has no change log.
        rusqlite::Connection::open(&path).unwrap().execute_batch("DROP TABLE changes").unwrap();

        let a = StudyStore::open(&path).unwrap();
        let changes = a.changes().unwrap();
        assert_eq!(changes.len(), 7);
        assert!(changes.iter().all(|c| c.device == a.device_id()));
        assert_eq!(stamp(&a, "reading:5000").0, 5_000, "each row's own time");
        assert_eq!(stamp(&a, "plan:john-21").0, 1_000);

        let mut b = store();
        assert_eq!(merge(&mut b, &export(&a).unwrap()).unwrap(), 7);
        assert_eq!(b.note(jn316()).unwrap().body, "from before sync");
        assert_eq!(b.highlight(jn316()), Some(HighlightColor::Green));
        assert_eq!(b.plan_reads("john-21").len(), 1);
        assert_eq!(b.reading_since(0).len(), 1);
        assert_eq!(b.rule(), rule);
        assert_eq!(b.rule_done(20_000).len(), 1);
        drop(a);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_rule_syncs_but_other_settings_stay() {
        let (mut a, mut b) = (store(), store());
        let mut rule = a.rule();
        rule.blocks.truncate(1);
        a.set_rule(&rule).unwrap();
        a.set_rule_done(20_000, "morning-prayers", true).unwrap();
        a.set_setting("app", "{\"theme\":\"dark\"}").unwrap();
        merge(&mut b, &export(&a).unwrap()).unwrap();
        assert_eq!(b.rule(), rule);
        assert_eq!(b.rule_done(20_000).into_iter().collect::<Vec<_>>(), ["morning-prayers"]);
        assert_eq!(b.setting("app"), None);
        assert_ne!(b.device_id(), a.device_id());

        b.set_rule_done(20_000, "morning-prayers", false).unwrap();
        merge(&mut a, &export(&b).unwrap()).unwrap();
        assert!(a.rule_done(20_000).is_empty());
    }

    #[test]
    fn erasing_reaches_the_other_device() {
        let (mut a, mut b) = (store(), store());
        a.set_highlight(jn316(), Some(HighlightColor::Pink)).unwrap();
        a.set_note(jn316(), "erased everywhere").unwrap();
        a.set_bookmark(ps23(), Some("")).unwrap();
        a.set_rule_done(20_000, "morning-prayers", true).unwrap();
        let mut rule = a.rule();
        rule.blocks.truncate(1);
        a.set_rule(&rule).unwrap();
        merge(&mut b, &export(&a).unwrap()).unwrap();
        assert_eq!(b.counts(), (1, 1, 1));

        a.erase_all().unwrap();
        assert_eq!(a.counts(), (0, 0, 0));
        assert_eq!(merge(&mut b, &export(&a).unwrap()).unwrap(), 3);
        assert_eq!(b.counts(), (0, 0, 0));
        assert_eq!(b.rule(), rule, "the rule is kept");
        assert!(b.rule_done(20_000).contains("morning-prayers"), "and so are its ticks");
        assert_eq!(merge(&mut a, &export(&b).unwrap()).unwrap(), 0, "nothing comes back");
    }

    #[test]
    fn an_edit_after_erasing_beats_what_was_seen_before() {
        // Another device, its clock two minutes ahead, wrote a note; this one saw it, erased
        // everything, and wrote its own.
        let ahead = now() + 120;
        let value = json!({"body": "old", "created": ahead, "updated": ahead}).to_string();
        let theirs = json!({"version": 1, "device": "b",
                            "changes": [{"key": "note:JHN.3.16", "value": value, "at": ahead, "device": "b"}]})
        .to_string();
        let mut a = store();
        assert_eq!(merge(&mut a, &theirs).unwrap(), 1);
        a.erase_all().unwrap();
        a.set_note(jn316(), "new").unwrap();
        assert_eq!(merge(&mut a, &theirs).unwrap(), 0);
        assert_eq!(a.note(jn316()).unwrap().body, "new");
    }

    #[test]
    fn restarting_a_plan_clears_its_ticks_on_both_devices() {
        let (mut a, mut b) = (store(), store());
        a.start_plan("john-21", 1_000).unwrap();
        a.set_plan_read("john-21", 1, 0, true).unwrap();
        merge(&mut b, &export(&a).unwrap()).unwrap();
        // B ticks a passage, and A restarts the plan on a later day before B's file arrives.
        b.set_plan_read("john-21", 3, 0, true).unwrap();
        a.start_plan("john-21", now() + 86_400).unwrap();
        let (ja, jb) = (export(&a).unwrap(), export(&b).unwrap());
        merge(&mut a, &jb).unwrap();
        merge(&mut b, &ja).unwrap();
        assert!(a.plan_reads("john-21").is_empty());
        assert!(b.plan_reads("john-21").is_empty());
        assert_eq!(b.plans(), a.plans());
    }

    #[test]
    fn bad_input() {
        let mut a = store();
        assert!(merge(&mut a, "not json").is_err());
        assert!(merge(&mut a, r#"{"version": 2, "device": "x", "changes": []}"#).is_err());
        // Unknown keys and bad values are skipped; the rest still applies.
        let json = json!({"version": 1, "device": "x", "changes": [
            {"key": "fast:20000", "value": "{}", "at": 1, "device": "x"},
            {"key": "note:JHN.3.16", "value": "{\"body\": 5}", "at": 1, "device": "x"},
            {"key": "highlight:XYZ.1.1", "value": "{\"color\": 1, \"created\": 1}", "at": 1, "device": "x"},
            {"key": "highlight:JHN.3.16", "value": "{\"color\": 1, \"created\": 1}", "at": 1, "device": "x"},
        ]});
        assert_eq!(merge(&mut a, &json.to_string()).unwrap(), 1);
        assert_eq!(a.highlight(jn316()), Some(HighlightColor::Green));
        assert!(a.note(jn316()).is_none());
    }

    #[test]
    fn folder_sync_end_to_end() {
        let dir = temp_dir("folder");
        let (mut a, mut b) = (store(), store());
        a.set_note(jn316(), "from the Mac").unwrap();
        a.set_highlight(jn316(), Some(HighlightColor::Blue)).unwrap();
        assert_eq!(sync_folder(&mut a, &dir).unwrap(), SyncReport::default());
        assert!(dir.join(format!("{}.json", a.device_id())).is_file());

        // A broken file is reported; an iCloud placeholder and an AppleDouble file are ignored.
        std::fs::write(dir.join("broken.json"), "{\"version\": 1,").unwrap();
        std::fs::write(dir.join(".other.json.icloud"), "").unwrap();
        std::fs::write(dir.join("._other.json"), "\0\x05\x16\x07").unwrap();
        let report = sync_folder(&mut b, &dir).unwrap();
        assert_eq!((report.applied, report.devices), (2, 1));
        assert_eq!(report.skipped.len(), 1);
        assert!(report.skipped[0].starts_with("broken.json: "), "{:?}", report.skipped);
        assert_eq!(b.note(jn316()).unwrap().body, "from the Mac");

        b.set_note(jn316(), "").unwrap();
        sync_folder(&mut b, &dir).unwrap();
        assert_eq!(sync_folder(&mut a, &dir).unwrap().applied, 1);
        assert!(a.note(jn316()).is_none());
        assert_eq!(a.highlight(jn316()), Some(HighlightColor::Blue));
        assert_eq!(sync_folder(&mut a, &dir).unwrap().applied, 0);
        assert_eq!(sync_folder(&mut b, &dir).unwrap().applied, 0);

        let mut names: Vec<_> =
            std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        names.sort();
        let mut expected = vec![
            format!("{}.json", a.device_id()),
            format!("{}.json", b.device_id()),
            ".other.json.icloud".into(),
            "._other.json".into(),
            "broken.json".into(),
        ];
        expected.sort();
        assert_eq!(names, expected, "no temporary files left");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_device_restored_from_a_backup_gets_back_what_it_sent() {
        let dir = temp_dir("restored");
        let db = dir.join("db");
        std::fs::create_dir_all(&db).unwrap();
        let shared = dir.join("sync");
        let mut a = StudyStore::open(&db.join("study.sqlite")).unwrap();
        a.set_note(jn316(), "before the backup").unwrap();
        sync_folder(&mut a, &shared).unwrap();
        drop(a);
        std::fs::copy(db.join("study.sqlite"), db.join("backup.sqlite")).unwrap();

        let mut a = StudyStore::open(&db.join("study.sqlite")).unwrap();
        a.set_note(jn316(), "after the backup").unwrap();
        sync_folder(&mut a, &shared).unwrap();
        drop(a);

        // Same device id, older data: its own file brings the later note back.
        let mut restored = StudyStore::open(&db.join("backup.sqlite")).unwrap();
        let report = sync_folder(&mut restored, &shared).unwrap();
        assert_eq!((report.applied, report.devices), (1, 0));
        assert_eq!(restored.note(jn316()).unwrap().body, "after the backup");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
