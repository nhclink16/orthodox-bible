//! Sync with the person's other devices through a shared folder (`bible_core::sync`): notes,
//! highlights, bookmarks, reading plans, the prayer rule and its check marks.

use std::path::Path;

use crate::{Bible, BibleError};

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
    /// Merges every device's file in `folder` (made if missing), then writes this device's own.
    /// The study store stays locked meanwhile, so call it off the main thread.
    pub fn sync(&self, folder: String) -> Result<SyncReport, BibleError> {
        let report = bible_core::sync::sync_folder(&mut self.study(), Path::new(&folder))?;
        Ok(SyncReport {
            applied: report.applied as u32,
            devices: report.devices as u32,
            skipped: report.skipped,
        })
    }
}
