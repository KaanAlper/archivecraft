//! ArchiveCraft's operations, shared by the window and `archivecraft-cli`: create, add,
//! extract, test, preview, delete, convert, comment, repair, search, speed test, and the
//! file-manager integration. Jobs report progress, can be paused or stopped, ask questions and
//! collect problems through `archivecraft_model::Ctl`. No user interface here.

pub mod bench;
pub mod find;
pub mod integration;
pub mod ops;
pub mod repair;

pub use archivecraft_formats as formats;
pub use archivecraft_model as model;
pub use archivecraft_par2 as par2;
pub use model::{CreateOptions, Ctl, Entry, Error, ExtractOptions, Format, Listing, Result};

/// Send files or folders to the system's recycle bin (they can be restored from there).
pub fn send_to_recycle_bin(paths: &[std::path::PathBuf]) -> Result<()> {
    trash::delete_all(paths).map_err(|e| Error::other(format!("could not move to the recycle bin: {e}")))
}
