//! Sync with the iPhone (Orthodox fork): the study store's change log (`bible_core::sync`)
//! through a folder in iCloud Drive, while the setting is on. A sync runs at launch, every two
//! minutes, a few seconds after a local change (so a run of edits syncs once) and on quit.
//!
//! iCloud Drive can keep a read waiting while it downloads the file, so the folder is read and
//! this device's file written on a thread; the store is only touched on the UI thread, where
//! merging and exporting take a few milliseconds (a year of heavy use, 1.4 MB: about 6 and 2 ms).
//! A file merged once isn't merged again until it changes.

use std::collections::HashMap;
use std::ffi::OsString;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;

use bible_core::study::StudyStore;
use bible_core::sync::SyncReport;
use eframe::egui;

/// Why there's no folder to sync through.
pub const NO_ICLOUD: &str = "Turn on iCloud Drive in System Settings first";

/// How often to sync while the app is open, and how long after the latest local change (seconds).
const EVERY: f64 = 120.0;
const QUIET: f64 = 3.0;

/// The folder the devices share: `BIBLE_RS_SYNC_DIR` when set, else "Orthodox Bible/Sync" in
/// iCloud Drive, which must be on. Offscreen renders only ever use `BIBLE_RS_SYNC_DIR`.
pub fn folder() -> Result<PathBuf, &'static str> {
    let home = if crate::offscreen() { None } else { dirs::home_dir() };
    folder_in(std::env::var_os("BIBLE_RS_SYNC_DIR"), home)
}

fn folder_in(over: Option<OsString>, home: Option<PathBuf>) -> Result<PathBuf, &'static str> {
    if let Some(dir) = over.filter(|d| !d.is_empty()) {
        return Ok(dir.into());
    }
    let drive = home.ok_or(NO_ICLOUD)?.join("Library/Mobile Documents/com~apple~CloudDocs");
    if drive.is_dir() { Ok(drive.join("Orthodox Bible").join("Sync")) } else { Err(NO_ICLOUD) }
}

/// When to sync, from the store's generation and the clock (egui seconds).
#[derive(Debug, Default)]
struct Schedule {
    /// The generation as of the last sync's export, or of the latest local change seen.
    generation: u64,
    /// When that change was seen, and when the last sync started.
    changed: Option<f64>,
    last: Option<f64>,
}

impl Schedule {
    /// Whether a sync is due at `now` (it then counts as started), or else how many seconds
    /// until one is: two minutes after the last, or `QUIET` after the latest change.
    fn due(&mut self, now: f64, generation: u64) -> Result<(), f64> {
        if generation != self.generation {
            self.generation = generation;
            self.changed = Some(now);
        }
        let at = self.last.map_or(now, |t| t + EVERY).min(self.changed.map_or(f64::INFINITY, |t| t + QUIET));
        if now >= at {
            self.last = Some(now);
            self.changed = None;
            Ok(())
        } else {
            Err(at - now)
        }
    }

    /// A sync exported the store at `generation`: what it merged isn't a local change.
    fn exported(&mut self, generation: u64) {
        self.generation = generation;
    }
}

/// Each sync file in the folder: its name, a hash of its text, and the text or why it couldn't
/// be read.
type Files = Vec<(String, u64, Result<String, String>)>;

enum Job {
    /// Reading the folder at this path.
    Reading(JoinHandle<Result<Files, String>>, PathBuf),
    /// Writing this device's file, with what the sync did and the text written.
    Writing(JoinHandle<Result<(), String>>, SyncReport, String),
}

/// How the last sync went, for Settings.
#[derive(Debug, Default, Clone)]
pub struct Status {
    /// When it finished (Unix seconds).
    pub synced: Option<i64>,
    /// Why it failed, or the files it couldn't read and why.
    pub error: Option<String>,
    pub skipped: Vec<String>,
}

#[derive(Default)]
pub struct Sync {
    schedule: Schedule,
    job: Option<Job>,
    status: Status,
    /// The other devices' files as last merged (name → hash), and this device's file as it is in
    /// the folder: an unchanged file isn't merged again, nor an unchanged export written.
    merged: HashMap<String, u64>,
    own: Option<String>,
    /// The folder a sync has read this session. Only then may quitting write this device's file
    /// there: one restored from a backup must first take back what it had sent.
    read: Option<PathBuf>,
    /// The setting as of the last frame.
    on: bool,
}

impl Sync {
    pub fn status(&self) -> &Status {
        &self.status
    }

    /// Once a frame: finishes a sync under way, or starts one when it's due. Returns whether
    /// changes from another device were applied to `study`.
    pub fn tick(&mut self, ctx: &egui::Context, study: &mut StudyStore, on: bool) -> bool {
        if on && !self.on {
            // Just turned on: sync now.
            self.schedule = Schedule::default();
        }
        self.on = on;
        let mut applied = false;
        match self.job.take() {
            Some(Job::Reading(h, dir)) if h.is_finished() => match h.join() {
                Ok(Ok(files)) => applied = self.merge(ctx, study, files, dir),
                Ok(Err(e)) => self.failed(e),
                Err(_) => self.failed("The sync stopped unexpectedly".into()),
            },
            Some(Job::Writing(h, report, json)) if h.is_finished() => match h.join() {
                Ok(Ok(())) => {
                    self.own = Some(json);
                    self.finished(report);
                }
                Ok(Err(e)) => self.failed(e),
                Err(_) => self.failed("The sync stopped unexpectedly".into()),
            },
            job => self.job = job,
        }
        if !on || self.job.is_some() {
            return applied;
        }
        match self.schedule.due(ctx.input(|i| i.time), study.generation()) {
            Ok(()) => match folder() {
                Ok(dir) => {
                    let (ctx, at) = (ctx.clone(), dir.clone());
                    let read = self.spawn(move || {
                        let files = read_folder(&at);
                        ctx.request_repaint();
                        files
                    });
                    if let Some(h) = read {
                        self.job = Some(Job::Reading(h, dir));
                    }
                }
                Err(e) => self.failed(e.into()),
            },
            // egui sleeps while nothing happens: wake when the next sync is due.
            Err(wait) => ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait)),
        }
        applied
    }

    /// Merges the files read from `dir`, then writes this device's file there on a thread if it
    /// changed. Returns whether anything was applied.
    fn merge(&mut self, ctx: &egui::Context, study: &mut StudyStore, files: Files, dir: PathBuf) -> bool {
        let own = format!("{}.json", study.device_id());
        if !files.iter().any(|(name, _, text)| *name == own && text.is_ok()) {
            // Gone from the folder (or unreadable): written again below.
            self.own = None;
        }
        let mut report = SyncReport::default();
        for (name, hash, text) in files {
            let mine = name == own;
            let seen = if mine {
                text.as_ref().ok() == self.own.as_ref()
            } else {
                self.merged.get(&name) == Some(&hash)
            };
            let merged = match text {
                Ok(_) if seen => Ok(0),
                Ok(t) => {
                    let merged = bible_core::sync::merge(study, &t);
                    if merged.is_ok() && mine {
                        self.own = Some(t);
                    } else if merged.is_ok() {
                        self.merged.insert(name.clone(), hash);
                    }
                    merged
                }
                Err(e) => Err(anyhow::anyhow!(e)),
            };
            match merged {
                Ok(n) => {
                    report.applied += n;
                    report.devices += usize::from(!mine);
                }
                Err(e) => report.skipped.push(format!("{name}: {e:#}")),
            }
        }
        self.read = Some(dir.clone());
        let applied = report.applied > 0;
        let json = match bible_core::sync::export(study) {
            Ok(json) => json,
            Err(e) => {
                self.failed(format!("{e:#}"));
                return applied;
            }
        };
        self.schedule.exported(study.generation());
        if self.own.as_ref() == Some(&json) {
            self.finished(report);
            return applied;
        }
        let (ctx, text) = (ctx.clone(), json.clone());
        let write = self.spawn(move || {
            let written = write_own(&dir, &own, &text);
            ctx.request_repaint();
            written
        });
        if let Some(h) = write {
            self.job = Some(Job::Writing(h, report, json));
        }
        applied
    }

    fn spawn<T: Send + 'static>(&mut self, f: impl FnOnce() -> T + Send + 'static) -> Option<JoinHandle<T>> {
        let spawned = std::thread::Builder::new().name("sync".into()).spawn(f);
        spawned.map_err(|e| self.failed(format!("Couldn't start the sync: {e}"))).ok()
    }

    fn finished(&mut self, report: SyncReport) {
        self.status = Status { synced: Some(bible_core::study::now()), error: None, skipped: report.skipped };
    }

    fn failed(&mut self, error: String) {
        self.status.error = Some(error);
    }

    /// On quit: writes this device's file if it changed since the last sync, on this thread
    /// (after a write under way). Nothing is read; the next launch merges.
    pub fn quit(&mut self, study: &StudyStore, on: bool) {
        if let Some(Job::Writing(h, _, json)) = self.job.take()
            && let Ok(Ok(())) = h.join()
        {
            self.own = Some(json);
        }
        let (true, Some(dir), Ok(json)) = (on, &self.read, bible_core::sync::export(study)) else { return };
        if self.own.as_ref() != Some(&json) {
            let _ = write_own(dir, &format!("{}.json", study.device_id()), &json);
        }
    }
}

/// The sync files in `dir` (made if missing), as `bible_core::sync::sync_folder` reads them:
/// hidden names (macOS's `._` sidecars, iCloud's placeholders, the temporary file) are never
/// sync files.
fn read_folder(dir: &Path) -> Result<Files, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Couldn't make the sync folder: {e}"))?;
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| format!("Couldn't read the sync folder: {e}"))?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|name| name.ends_with(".json") && !name.starts_with('.'))
        .collect();
    names.sort();
    let read = |name: String| {
        let text = std::fs::read_to_string(dir.join(&name)).map_err(|e| e.to_string());
        let mut h = std::collections::hash_map::DefaultHasher::new();
        text.as_deref().unwrap_or("").hash(&mut h);
        (name, h.finish(), text)
    };
    Ok(names.into_iter().map(read).collect())
}

/// Writes this device's file whole and then renames it, so other devices never read half of it.
fn write_own(dir: &Path, name: &str, json: &str) -> Result<(), String> {
    let tmp = dir.join(format!(".{name}.tmp"));
    std::fs::write(&tmp, json)
        .and_then(|()| std::fs::rename(&tmp, dir.join(name)))
        .map_err(|e| format!("Couldn't write to the sync folder: {e}"))
}

#[cfg(test)]
mod tests {
    use bible_core::books::{BookId, VerseRef};
    use bible_core::study::HighlightColor;

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bible-rs-app-sync-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_folder_is_in_icloud_drive_unless_overridden() {
        let home = temp_dir("home");
        assert_eq!(folder_in(None, Some(home.clone())), Err(NO_ICLOUD), "iCloud Drive is off");
        assert_eq!(folder_in(None, None), Err(NO_ICLOUD));
        let drive = home.join("Library/Mobile Documents/com~apple~CloudDocs");
        std::fs::create_dir_all(&drive).unwrap();
        let ours = drive.join("Orthodox Bible").join("Sync");
        assert_eq!(folder_in(None, Some(home.clone())), Ok(ours.clone()));
        assert!(!drive.join("Orthodox Bible").exists(), "made by the first sync, not before");
        let over = home.join("elsewhere");
        assert_eq!(folder_in(Some(over.clone().into()), Some(home.clone())), Ok(over.clone()));
        assert_eq!(folder_in(Some(over.clone().into()), None), Ok(over));
        assert_eq!(folder_in(Some("".into()), Some(home.clone())), Ok(ours));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn syncs_at_launch_every_two_minutes_and_after_a_pause_in_edits() {
        let mut s = Schedule::default();
        // At launch, then not for two minutes.
        assert_eq!(s.due(5.0, 1), Ok(()));
        assert_eq!(s.due(6.0, 1), Err(119.0));
        assert_eq!(s.due(125.0, 1), Ok(()));
        // A run of edits (each bumps the generation) syncs once, three seconds after the last.
        assert_eq!(s.due(130.0, 2), Err(3.0));
        assert_eq!(s.due(131.5, 3), Err(3.0));
        assert_eq!(s.due(133.0, 4), Err(3.0));
        assert_eq!(s.due(135.5, 4), Err(0.5));
        assert_eq!(s.due(136.0, 4), Ok(()));
        // What a sync merges isn't an edit: the next sync comes two minutes on.
        s.exported(9);
        assert_eq!(s.due(140.0, 9), Err(116.0));
        // Edits that never pause still sync every two minutes.
        let mut t = 140.0;
        while s.due(t, t as u64).is_err() {
            t += 1.0;
        }
        assert_eq!(t, 256.0);
    }

    /// One sync of `study` through `dir`, as `tick` runs it, with its write waited for.
    fn sync(s: &mut Sync, study: &mut StudyStore, dir: &Path) -> bool {
        let ctx = egui::Context::default();
        let files = read_folder(dir).unwrap();
        let applied = s.merge(&ctx, study, files, dir.to_path_buf());
        if let Some(Job::Writing(h, report, json)) = s.job.take() {
            h.join().unwrap().unwrap();
            s.own = Some(json);
            s.finished(report);
        }
        applied
    }

    #[test]
    fn syncs_with_the_iphone_and_merges_only_what_changed() {
        let dir = temp_dir("folder");
        let jn316 = VerseRef::new(BookId::from_code("JHN").unwrap(), 3, 16);
        let mut mac = StudyStore::open_in_memory().unwrap();
        let mut phone = StudyStore::open_in_memory().unwrap();
        let mut s = Sync::default();
        mac.set_note(jn316, "from the Mac").unwrap();
        Sync::default().quit(&mac, true);
        assert!(!dir.exists(), "quitting before a sync has read the folder writes nothing");
        assert!(!sync(&mut s, &mut mac, &dir));
        let own = dir.join(format!("{}.json", mac.device_id()));
        assert!(own.is_file(), "the folder is made and the Mac's file written");
        assert!(s.status.synced.is_some() && s.status.error.is_none());

        // The iPhone (the core's folder sync) gets the note and adds a highlight.
        bible_core::sync::sync_folder(&mut phone, &dir).unwrap();
        assert_eq!(phone.note(jn316).unwrap().body, "from the Mac");
        phone.set_highlight(jn316, Some(HighlightColor::Blue)).unwrap();
        bible_core::sync::sync_folder(&mut phone, &dir).unwrap();
        let g = mac.generation();
        assert!(sync(&mut s, &mut mac, &dir));
        assert_eq!(mac.highlight(jn316), Some(HighlightColor::Blue));
        assert!(mac.generation() > g, "so the reader and the pages show it");

        // Nothing new: nothing merged, nothing written.
        let g = mac.generation();
        assert!(!sync(&mut s, &mut mac, &dir));
        assert!(s.job.is_none());
        assert_eq!(mac.generation(), g);
        // The Mac's file gone from the folder is written again.
        std::fs::remove_file(&own).unwrap();
        assert!(!sync(&mut s, &mut mac, &dir));
        assert!(own.is_file());

        // A broken file is reported, and the rest still syncs.
        std::fs::write(dir.join("broken.json"), "{\"version\": 1,").unwrap();
        std::fs::write(dir.join("._other.json"), "\0\x05\x16\x07").unwrap();
        mac.set_note(jn316, "").unwrap();
        assert!(!sync(&mut s, &mut mac, &dir));
        assert_eq!(s.status.skipped.len(), 1);
        assert!(s.status.skipped[0].starts_with("broken.json: "), "{:?}", s.status.skipped);
        bible_core::sync::sync_folder(&mut phone, &dir).unwrap();
        assert!(phone.note(jn316).is_none(), "the deletion reached the iPhone");

        // Quitting writes what changed since the last sync.
        mac.set_bookmark(jn316, Some("")).unwrap();
        s.quit(&mac, true);
        bible_core::sync::sync_folder(&mut phone, &dir).unwrap();
        assert!(phone.is_bookmarked(jn316));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
