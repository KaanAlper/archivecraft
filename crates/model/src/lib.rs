//! Shared types for ArchiveCraft: archive items, option sets, formats, progress, questions and
//! problem reports, plus small helpers. No archive code and no user interface here.

pub mod error;
pub mod format;
pub mod types;
pub mod util;

pub use error::{Error, Result};
pub use format::Format;
pub use types::*;
