//! Sync with the person's other devices through a shared folder (`bible_core::sync`): notes,
//! highlights, bookmarks, reading plans, the prayer rule and its check marks.

use std::io;
use std::path::Path;

use crate::{Bible, BibleError, fail};

/// What a sync did.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SyncReport {
    /// Changes from other devices applied here.
    pub applied: u32,
    /// Other devices' files read.
    pub devices: u32,
    /// Files that couldn't be read (half downloaded, or from a newer version), each with why:
    /// "broken.json: not a sync file".
    pub skipped: Vec<String>,
}

#[uniffi::export]
impl Bible {
    /// Ties the study database to this device and install (the iPhone's identifierForVendor), so
    /// a copy restored onto another phone syncs under an id of its own rather than writing over
    /// this one's file (`StudyStore::bind_machine`). Call it once after opening, before any sync.
    pub fn bind_machine(&self, machine: String) -> Result<(), BibleError> {
        Ok(self.study().bind_machine(&machine)?)
    }

    /// Merges every device's file in `folder` (made if missing), then writes this device's own,
    /// as `bible_core::sync::sync_folder` does. iCloud Drive can keep a read waiting while it
    /// downloads the file, so the files are read and written without the study store's lock,
    /// which is held only to merge and export (a few milliseconds). Call it off the main thread.
    /// Fails with a sentence to show: "The app can no longer open the sync folder. Choose it again."
    pub fn sync(&self, folder: String) -> Result<SyncReport, BibleError> {
        let dir = Path::new(&folder);
        let own = format!("{}.json", self.study().device_id());
        std::fs::create_dir_all(dir).map_err(folder_error)?;
        // Hidden names are never sync files: AppleDouble sidecars, iCloud's placeholders and our
        // own temp file.
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .map_err(folder_error)?
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|name| name.ends_with(".json") && !name.starts_with('.'))
            .collect();
        names.sort();
        let files: Vec<(String, io::Result<String>)> = names
            .into_iter()
            .map(|name| {
                let text = std::fs::read_to_string(dir.join(&name));
                (name, text)
            })
            .collect();

        let mut report = SyncReport { applied: 0, devices: 0, skipped: vec![] };
        let json = {
            let mut study = self.study();
            for (name, text) in &files {
                let merged = text
                    .as_ref()
                    .map_err(|e| anyhow::anyhow!("{e}"))
                    .and_then(|json| bible_core::sync::merge(&mut study, json));
                match merged {
                    Ok(n) => {
                        report.applied += n as u32;
                        report.devices += u32::from(*name != own);
                    }
                    Err(e) => report.skipped.push(format!("{name}: {e:#}")),
                }
            }
            bible_core::sync::export(&study).map_err(|e| fail(format!("Couldn’t sync: {e:#}.")))?
        };

        // Written whole and then renamed, so other devices never read half a file; left alone
        // when nothing changed, so iCloud has nothing to upload.
        let current = files.iter().find(|(name, _)| *name == own).and_then(|(_, text)| text.as_ref().ok());
        if current != Some(&json) {
            let tmp = dir.join(format!(".{own}.tmp"));
            std::fs::write(&tmp, &json).map_err(folder_error)?;
            std::fs::rename(&tmp, dir.join(&own)).map_err(folder_error)?;
        }
        Ok(report)
    }
}

/// Why the folder couldn't be read or written, in plain words.
fn folder_error(e: io::Error) -> BibleError {
    fail(match e.kind() {
        io::ErrorKind::PermissionDenied => {
            "The app can no longer open the sync folder. Choose it again.".into()
        }
        io::ErrorKind::NotFound => "The sync folder can’t be found. Choose it again.".into(),
        io::ErrorKind::StorageFull => "There’s no room left to save to the sync folder.".into(),
        kind => format!("Couldn’t read or write the sync folder ({kind})."),
    })
}
