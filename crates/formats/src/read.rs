//! Reading archives of every supported format through one interface:
//! `list` returns the items, `visit` streams the chosen items' contents.

use crate::volumes;
use crate::{isofmt, rarfmt, sevenzfmt, tarfmt, zipfmt};
use archivecraft_model::{normalize_name, Entry, Error, Format, Listing, Result};
use std::io::Read;
use std::path::Path;

/// Decide for each item whether its contents are wanted.
pub type Want<'a> = &'a mut dyn FnMut(&Entry) -> bool;
/// Receives each wanted item with a reader over its contents (empty for folders).
pub type VisitFn<'a> = &'a mut dyn FnMut(&Entry, &mut dyn Read) -> Result<()>;

pub fn detect(path: &Path) -> Result<Format> {
    let f = crate::detect::detect(path).ok_or_else(|| Error::Unsupported(format!("{} is not an archive this program knows", path.display())))?;
    if !f.readable() {
        return Err(Error::Unsupported("RAR support was not included in this build (feature \"rar\")".into()));
    }
    Ok(f)
}

pub fn list(path: &Path, password: Option<&str>) -> Result<Listing> {
    let format = detect(path)?;
    let mut listing = match format {
        Format::Zip => zipfmt::list(path)?,
        Format::SevenZ => sevenzfmt::list(path, password)?,
        Format::Rar => rarfmt::list(path, password)?,
        Format::Iso => isofmt::list(path)?,
        _ => tarfmt::list(path, format)?,
    };
    listing.volumes = volumes::volume_parts(path).len();
    listing.archive_size = volumes::total_size(path);
    Ok(listing)
}

pub fn visit(path: &Path, password: Option<&str>, want: Want, f: VisitFn) -> Result<()> {
    match detect(path)? {
        Format::Zip => zipfmt::visit(path, password, want, f),
        Format::SevenZ => sevenzfmt::visit(path, password, want, f),
        Format::Rar => rarfmt::visit(path, password, want, f),
        Format::Iso => isofmt::visit(path, want, f),
        fmt => tarfmt::visit(path, fmt, want, f),
    }
}

/// Read one item fully into memory (for previews), up to `limit` bytes.
pub fn read_one(path: &Path, name: &str, password: Option<&str>, limit: u64) -> Result<Vec<u8>> {
    let target = normalize_name(name);
    let mut out: Option<Vec<u8>> = None;
    let mut want = |e: &Entry| out.is_none() && !e.is_dir && e.path == target;
    let mut got: Option<Vec<u8>> = None;
    let mut f = |_: &Entry, r: &mut dyn Read| -> Result<()> {
        let mut buf = Vec::new();
        r.take(limit).read_to_end(&mut buf)?;
        got = Some(buf);
        Ok(())
    };
    visit(path, password, &mut want, &mut f)?;
    out = got;
    out.ok_or_else(|| Error::other(format!("\"{name}\" is not in the archive")))
}
