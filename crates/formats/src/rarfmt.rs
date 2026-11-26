//! RAR: read-only, and only with the `rar` feature (through the Apache-2.0 `rars` crate's
//! reading API). ArchiveCraft never writes RAR.

#![cfg_attr(not(feature = "rar"), allow(unused_imports))]

#[cfg(feature = "rar")]
use crate::conv::Ac;
use crate::read::{VisitFn, Want};
use crate::volumes;
use archivecraft_model::{Entry, Error, Format, Listing, Result};
use std::io;
use std::path::Path;
use std::time::SystemTime;

// ---------------------------------------------------------------- RAR (optional, read-only)

#[cfg(feature = "rar")]
pub fn open(path: &Path, password: Option<&str>) -> Result<rars::Archive> {
    let pw = password.filter(|p| !p.is_empty()).map(|p| p.as_bytes());
    let opts = rars::ArchiveReadOptions::with_optional_password(pw);
    rars::ArchiveReader::read_reader_with_options(volumes::open_reader(path)?, opts).ac()
}

#[cfg(feature = "rar")]
fn rar_time(t: Option<rars::Timestamp>) -> Option<SystemTime> {
    t.and_then(|t| archivecraft_model::util::from_unix_secs(t.unix_seconds()))
}

#[cfg(feature = "rar")]
pub(crate) fn list(path: &Path, password: Option<&str>) -> Result<Listing> {
    let ar = open(path, password)?;
    let mut entries = Vec::new();
    let mut solid = false;
    for m in ar.members() {
        let meta = &m.meta;
        let name = m.decoded_name(None).map(|c| String::from_utf8_lossy(&c).to_string()).unwrap_or_else(|_| meta.name_lossy());
        let mut en = Entry::new(&name, meta.is_directory);
        en.is_link = meta.is_redirection;
        en.size = meta.unpacked_size;
        en.packed = Some(meta.packed_size);
        en.modified = rar_time(meta.modification_time());
        en.encrypted = meta.is_encrypted;
        en.method = if meta.is_stored { "stored".into() } else { "compressed".into() };
        solid |= m.is_solid();
        entries.push(en);
    }
    let comment = ar
        .comment(password.map(|p| p.as_bytes()))
        .ok()
        .flatten()
        .map(|c| String::from_utf8_lossy(&c).to_string());
    Ok(Listing {
        format: Format::Rar,
        entries,
        comment,
        solid,
        names_encrypted: false,
        volumes: 1,
        archive_size: 0,
        methods: vec![format!("{:?}", ar.family())],
    })
}

#[cfg(not(feature = "rar"))]
pub(crate) fn list(_: &Path, _: Option<&str>) -> Result<Listing> {
    Err(Error::Unsupported("RAR support was not included in this build (feature \"rar\")".into()))
}

#[cfg(feature = "rar")]
pub(crate) fn visit(path: &Path, password: Option<&str>, want: Want, f: VisitFn) -> Result<()> {
    use std::cell::RefCell;
    use std::rc::Rc;

    struct Shared(Rc<RefCell<Vec<u8>>>);
    impl io::Write for Shared {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let ar = open(path, password)?;
    let pw = password.filter(|p| !p.is_empty()).map(|p| p.as_bytes());
    // Each item's bytes are collected while the next item opens, then handed on.
    let mut pending: Option<(Entry, Rc<RefCell<Vec<u8>>>)> = None;
    let mut failure: Option<Error> = None;
    {
        let mut flush = |pending: &mut Option<(Entry, Rc<RefCell<Vec<u8>>>)>, failure: &mut Option<Error>| {
            if let Some((en, buf)) = pending.take()
                && failure.is_none()
            {
                let data = std::mem::take(&mut *buf.borrow_mut());
                if let Err(e) = f(&en, &mut io::Cursor::new(data)) {
                    *failure = Some(e);
                }
            }
        };
        let res = ar.extract_to(pw, |meta| {
            flush(&mut pending, &mut failure);
            let mut en = Entry::new(&meta.name_lossy(), meta.is_directory);
            en.modified = rar_time(meta.modified);
            if failure.is_some() || !want(&en) {
                return Ok(Box::new(io::sink()) as Box<dyn io::Write>);
            }
            let buf = Rc::new(RefCell::new(Vec::new()));
            pending = Some((en, buf.clone()));
            Ok(Box::new(Shared(buf)) as Box<dyn io::Write>)
        });
        if let Err(e) = res {
            failure.get_or_insert(crate::conv::rar_err(e));
        }
        flush(&mut pending, &mut failure);
    }
    match failure {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

#[cfg(not(feature = "rar"))]
pub(crate) fn visit(_: &Path, _: Option<&str>, _: Want, _: VisitFn) -> Result<()> {
    Err(Error::Unsupported("RAR support was not included in this build (feature \"rar\")".into()))
}
