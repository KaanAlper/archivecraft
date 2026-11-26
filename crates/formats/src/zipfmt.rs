//! ZIP: reading (including split parts and AES/legacy passwords) and writing (Deflate or
//! stored, AES-256 or opt-in legacy encryption).

use crate::conv::Ac;
use crate::read::{VisitFn, Want};
use crate::volumes::{self, MultiReader};
use crate::write::{Out, Sink};
use archivecraft_model::{Entry, Error, Format, Listing, Result};
use std::collections::BTreeSet;
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::SystemTime;

// ---------------------------------------------------------------- ZIP

fn zip_time(t: Option<zip::DateTime>) -> Option<SystemTime> {
    let t = t?;
    let naive: chrono::NaiveDateTime = t.try_into().ok()?;
    use chrono::TimeZone;
    let local = chrono::Local.from_local_datetime(&naive).earliest()?;
    Some(local.into())
}

macro_rules! zip_entry {
    ($e:expr) => {{
        let e = $e;
        let name = e.name().map(|c| c.to_string()).unwrap_or_else(|_| String::from_utf8_lossy(e.name_raw()).to_string());
        let mut en = Entry::new(&name, e.is_dir());
        en.is_link = e.is_symlink();
        en.size = e.size();
        en.packed = Some(e.compressed_size());
        en.modified = zip_time(e.last_modified());
        en.encrypted = e.encrypted();
        en.crc = if e.is_dir() { None } else { Some(e.crc32()) };
        en.method = e.compression().to_string();
        en.mode = e.unix_mode();
        en
    }};
}

fn zip_open(path: &Path) -> Result<zip::ZipArchive<MultiReader>> {
    zip::ZipArchive::new(volumes::open_reader(path)?).ac()
}

pub(crate) fn list(path: &Path) -> Result<Listing> {
    let za = zip_open(path)?;
    let mut entries = Vec::with_capacity(za.len());
    let mut methods = BTreeSet::new();
    for i in 0..za.len() {
        let e = za.by_index_data(i).ac()?;
        let en = zip_entry!(&e);
        if !en.is_dir {
            methods.insert(en.method.clone());
        }
        entries.push(en);
    }
    let comment = String::from_utf8_lossy(za.comment()).to_string();
    Ok(Listing {
        format: Format::Zip,
        entries,
        comment: if comment.is_empty() { None } else { Some(comment) },
        solid: false,
        names_encrypted: false,
        volumes: 1,
        archive_size: 0,
        methods: methods.into_iter().collect(),
    })
}

pub(crate) fn visit(path: &Path, password: Option<&str>, want: Want, f: VisitFn) -> Result<()> {
    let mut za = zip_open(path)?;
    for i in 0..za.len() {
        let en = {
            let e = za.by_index_data(i).ac()?;
            zip_entry!(&e)
        };
        if !want(&en) {
            continue;
        }
        if en.is_dir {
            f(&en, &mut io::empty())?;
            continue;
        }
        let mut file = if en.encrypted {
            match password {
                Some(p) if !p.is_empty() => za.by_index_decrypt(i, p.as_bytes()).ac()?,
                _ => return Err(Error::PasswordRequired),
            }
        } else {
            za.by_index(i).ac()?
        };
        f(&en, &mut file)?;
    }
    Ok(())
}


// ---------------------------------------------------------------- ZIP

pub(crate) struct ZipSink {
    w: zip::ZipWriter<Out>,
    level: u32,
    password: Option<String>,
    legacy: bool,
}

impl ZipSink {
    pub(crate) fn new(out: Out, level: u32, password: Option<String>, legacy: bool) -> Self {
        ZipSink { w: zip::ZipWriter::new(out), level, password, legacy }
    }
}

fn to_zip_time(t: SystemTime) -> Option<zip::DateTime> {
    let local: chrono::DateTime<chrono::Local> = t.into();
    zip::DateTime::try_from(local.naive_local()).ok()
}

fn zip_options<'k>(
    level: u32,
    password: Option<&'k str>,
    legacy: bool,
    mtime: Option<SystemTime>,
    mode: Option<u32>,
    store: bool,
    large: bool,
) -> Result<zip::write::FileOptions<'k, 'static, ()>> {
    use zip::CompressionMethod;
    use zip::unstable::write::FileOptionsExt;
    let stored = store || level == 0;
    let mut o: zip::write::FileOptions<'k, 'static, ()> = zip::write::FileOptions::default()
        .compression_method(if stored { CompressionMethod::Stored } else { CompressionMethod::Deflated })
        .compression_level(if stored { None } else { Some(level as i64) })
        .large_file(large);
    if let Some(dt) = mtime.and_then(to_zip_time) {
        o = o.last_modified_time(dt);
    }
    if let Some(m) = mode {
        o = o.unix_permissions(m & 0o7777);
    }
    if let Some(pw) = password {
        o = if legacy { o.with_deprecated_encryption(pw.as_bytes()).ac()? } else { o.with_aes_encryption(zip::AesMode::Aes256, pw) };
    }
    Ok(o)
}

impl Sink for ZipSink {
    fn add_dir(&mut self, name: &str, mtime: Option<SystemTime>) -> Result<()> {
        // Folders carry no data, so they are never encrypted.
        let o = zip_options(0, None, false, mtime, None, true, false)?;
        self.w.add_directory(format!("{}/", name.trim_end_matches('/')), o).ac()?;
        Ok(())
    }

    fn add_file(&mut self, name: &str, mtime: Option<SystemTime>, size: u64, mode: Option<u32>, store: bool, data: &mut dyn Read) -> Result<()> {
        let o = zip_options(self.level, self.password.as_deref(), self.legacy, mtime, mode, store, size >= 0xFFFF_FFFF)?;
        self.w.start_file(name, o).ac()?;
        io::copy(data, &mut self.w)?;
        Ok(())
    }

    fn set_comment(&mut self, comment: &str) -> Result<()> {
        self.w.set_comment(comment.to_string()).ac()?;
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<()> {
        let mut out = self.w.finish().ac()?;
        out.flush()?;
        Ok(())
    }
}

// ---------------------------------------------------------------- salvage

/// Read ZIP items one after another from the start of the file, ignoring the (possibly lost)
/// central directory; stop at the first item that cannot be read and step past damage.
pub fn salvage(archive: &std::path::Path, sink: &mut dyn Sink, ctl: &archivecraft_model::Ctl, label: &str) -> Result<(usize, usize)> {
    let data = std::fs::read(archive)?;
    let mut saved = 0;
    let mut lost = 0;
    let mut pos = 0usize;
    while let Some(off) = find(&data[pos..], b"PK\x03\x04") {
        ctl.check()?;
        let start = pos + off;
        let mut cur = std::io::Cursor::new(&data[start..]);
        match zip::read::read_zipfile_from_stream(&mut cur) {
            Ok(Some(mut f)) => {
                let name = f.name().map(|c| c.to_string()).unwrap_or_default();
                let is_dir = f.is_dir();
                let encrypted = f.encrypted();
                let mtime = f.last_modified().and_then(|t| {
                    let n: chrono::NaiveDateTime = t.try_into().ok()?;
                    use chrono::TimeZone;
                    chrono::Local.from_local_datetime(&n).earliest().map(Into::into)
                });
                if is_dir {
                    sink.add_dir(&archivecraft_model::normalize_name(&name), mtime)?;
                    saved += 1;
                } else if encrypted {
                    lost += 1;
                    ctl.warn(label, format!("\"{name}\" is locked and could not be saved this way"));
                } else {
                    let mut buf = Vec::new();
                    match f.read_to_end(&mut buf) {
                        Ok(_) => {
                            drop(f);
                            sink.add_file(&archivecraft_model::normalize_name(&name), mtime, buf.len() as u64, None, false, &mut buf.as_slice())?;
                            saved += 1;
                        }
                        Err(e) => {
                            lost += 1;
                            ctl.error(label, format!("\"{name}\" is damaged: {e}"));
                        }
                    }
                }
                pos = start + 4;
            }
            Ok(None) => break,
            Err(e) => {
                lost += 1;
                ctl.error(label, format!("damaged item at byte {start}: {e}"));
                pos = start + 4;
            }
        }
    }
    Ok((saved, lost))
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

