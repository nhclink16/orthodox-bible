//! Core of bible-rs: the verse-indexed translation model, the `.bbl` binary format, importers,
//! full-text search, reference parsing and local study storage. No GUI code lives here.

pub mod books;
#[cfg(feature = "bundled")]
pub mod bundled;
pub mod calendar;
pub mod copy;
pub mod fathers;
pub mod format;
pub mod import;
pub mod lectionary;
pub mod library;
pub mod lxx;
pub mod model;
pub mod online;
pub mod plans;
pub mod prayers;
pub mod reference;
pub mod rule;
pub mod search;
pub mod secrets;
pub mod study;
pub mod sync;
pub mod usfm;
pub mod xml;

pub use books::{BookId, ChapterIdx, Testament, VerseRef};
pub use model::{Translation, TranslationMeta};
