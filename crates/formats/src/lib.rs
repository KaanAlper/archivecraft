//! Archive formats for ArchiveCraft: one module per format (reading and writing), split
//! volumes, and format detection. No user interface here.

mod conv;
pub mod detect;
pub mod isofmt;
pub mod rarfmt;
pub mod read;
pub mod sevenzfmt;
pub mod tarfmt;
pub mod volumes;
pub mod write;
pub mod zipfmt;

pub use read::{detect, list, read_one, visit, VisitFn, Want};
pub use write::{open_sink, CountingWriter, Sink};
