//! 7z: reading (solid or not, AES-256 with or without locked names) and writing (LZMA2 or
//! stored, AES-256, optional solid blocks and locked names).

use crate::conv::Ac;
use crate::read::{VisitFn, Want};
use crate::volumes::{self, MultiReader};
use crate::write::{Out, Sink};
use archivecraft_model::{CreateOptions, Entry, Error, Format, Listing, Result};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::SystemTime;

// ---------------------------------------------------------------- 7z

fn sevenz_time(e: &sevenz_rust2::ArchiveEntry) -> Option<SystemTime> {
    if e.has_last_modified_date { Some(e.last_modified_date.into()) } else { None }
}

fn sevenz_entry(e: &sevenz_rust2::ArchiveEntry, encrypted: bool) -> Entry {
    let mut en = Entry::new(e.name(), e.is_directory());
    en.size = e.size();
    en.packed = Some(e.compressed_size);
    en.modified = sevenz_time(e);
    en.encrypted = encrypted;
    en.crc = if e.has_crc { Some(e.crc as u32) } else { None };
    en
}

fn sevenz_password(p: Option<&str>) -> sevenz_rust2::Password {
    match p {
        Some(p) if !p.is_empty() => sevenz_rust2::Password::from(p),
        _ => sevenz_rust2::Password::empty(),
    }
}

fn sevenz_open(path: &Path, password: Option<&str>) -> Result<sevenz_rust2::ArchiveReader<MultiReader>> {
    let r = volumes::open_reader(path)?;
    match sevenz_rust2::ArchiveReader::new(r, sevenz_password(password)) {
        Ok(a) => Ok(a),
        Err(e) => {
            let e = crate::conv::sevenz_err(e);
            // A locked header read with a wrong password looks like damage; say what it most likely is.
            if password.is_some() && matches!(e, Error::Corrupt(_)) { Err(Error::WrongPassword) } else { Err(e) }
        }
    }
}

pub(crate) fn list(path: &Path, password: Option<&str>) -> Result<Listing> {
    let reader = sevenz_open(path, password)?;
    let ar = reader.archive();
    let mut methods = BTreeSet::new();
    for b in &ar.blocks {
        for c in &b.coders {
            if let Some(m) = sevenz_rust2::EncoderMethod::by_id(c.encoder_method_id()) {
                methods.insert(m.name().to_string());
            }
        }
    }
    let entries = ar
        .files
        .iter()
        .enumerate()
        .map(|(i, e)| sevenz_entry(e, ar.is_file_encrypted(i)))
        .collect();
    // We could only read the names with a password if the names themselves are locked.
    let names_encrypted = password.is_some_and(|p| !p.is_empty()) && {
        let r = volumes::open_reader(path)?;
        sevenz_rust2::ArchiveReader::new(r, sevenz_rust2::Password::empty()).is_err()
    };
    Ok(Listing {
        format: Format::SevenZ,
        entries,
        comment: None,
        solid: ar.is_solid,
        names_encrypted,
        volumes: 1,
        archive_size: 0,
        methods: methods.into_iter().collect(),
    })
}

pub(crate) fn visit(path: &Path, password: Option<&str>, want: Want, f: VisitFn) -> Result<()> {
    let mut reader = sevenz_open(path, password)?;
    let encrypted: Vec<bool> = (0..reader.archive().files.len()).map(|i| reader.archive().is_file_encrypted(i)).collect();
    if encrypted.iter().any(|e| *e) && password.is_none_or(|p| p.is_empty()) {
        // Still allow visiting folders and unlocked items; locked ones raise the error.
    }
    let mut failure: Option<Error> = None;
    let mut index = 0usize;
    let res = reader.for_each_entries(|e, r| {
        let enc = encrypted.get(index).copied().unwrap_or(false);
        index += 1;
        let en = sevenz_entry(e, enc);
        if !want(&en) {
            return Ok(true);
        }
        if en.encrypted && password.is_none_or(|p| p.is_empty()) {
            failure = Some(Error::PasswordRequired);
            return Err(sevenz_rust2::Error::PasswordRequired);
        }
        match f(&en, r) {
            Ok(()) => Ok(true),
            Err(err) => {
                failure = Some(err);
                Err(sevenz_rust2::Error::Other("stopped".into()))
            }
        }
    });
    if let Some(e) = failure {
        return Err(e);
    }
    res.map_err(|e| {
        let e = crate::conv::sevenz_err(e);
        if password.is_some() && matches!(e, Error::Corrupt(_)) && encrypted.iter().any(|x| *x) { Error::WrongPassword } else { e }
    })
}


// ---------------------------------------------------------------- 7z

pub(crate) struct SevenZSink {
    w: sevenz_rust2::ArchiveWriter<Out>,
    solid: bool,
    /// Solid mode: items wait here (with their data spooled to disk) until `finish`.
    queued: Vec<(sevenz_rust2::ArchiveEntry, Option<PathBuf>)>,
    spool: Option<tempfile::TempDir>,
}

impl SevenZSink {
    pub(crate) fn new(path: &Path, opts: &CreateOptions, pw: Option<String>, count: Arc<AtomicU64>) -> Result<Self> {
        use sevenz_rust2::encoder_options::{AesEncoderOptions, Lzma2Options};
        use sevenz_rust2::{EncoderConfiguration, EncoderMethod};
        let mut w = sevenz_rust2::ArchiveWriter::new(crate::write::out_file(path, count)?).ac()?;
        let mut methods: Vec<EncoderConfiguration> = Vec::new();
        if let Some(p) = &pw {
            methods.push(AesEncoderOptions::new(sevenz_rust2::Password::from(p.as_str())).into());
        }
        if opts.level == 0 {
            methods.push(EncoderConfiguration::new(EncoderMethod::COPY));
        } else {
            let mut l = if opts.threads > 1 {
                Lzma2Options::from_level_mt(opts.level.min(9), opts.threads, 4 * 1024 * 1024)
            } else {
                Lzma2Options::from_level(opts.level.min(9))
            };
            if let Some(d) = opts.dictionary {
                l.set_dictionary_size(d.clamp(4096, 1 << 30));
            }
            methods.push(l.into());
        }
        w.set_content_methods(methods);
        if pw.is_some() && opts.encrypt_names {
            w.set_encrypt_header(true);
        } else {
            w.set_encrypt_header(false);
        }
        let solid = opts.solid;
        Ok(SevenZSink { w, solid, queued: Vec::new(), spool: if solid { Some(tempfile::tempdir()?) } else { None } })
    }

    fn entry(name: &str, mtime: Option<SystemTime>, dir: bool) -> sevenz_rust2::ArchiveEntry {
        let mut e = if dir { sevenz_rust2::ArchiveEntry::new_directory(name) } else { sevenz_rust2::ArchiveEntry::new_file(name) };
        if let Some(t) = mtime.and_then(|t| sevenz_rust2::NtTime::try_from(t).ok()) {
            e.has_last_modified_date = true;
            e.last_modified_date = t;
        }
        e
    }
}

impl Sink for SevenZSink {
    fn add_dir(&mut self, name: &str, mtime: Option<SystemTime>) -> Result<()> {
        let e = Self::entry(name, mtime, true);
        if self.solid {
            self.queued.push((e, None));
        } else {
            self.w.push_archive_entry::<File>(e, None).ac()?;
        }
        Ok(())
    }

    fn add_file(&mut self, name: &str, mtime: Option<SystemTime>, size: u64, _mode: Option<u32>, _store: bool, data: &mut dyn Read) -> Result<()> {
        let e = Self::entry(name, mtime, false);
        if self.solid {
            let dir = self.spool.as_ref().expect("spool").path().to_path_buf();
            let p = dir.join(format!("{}", self.queued.len()));
            let mut f = BufWriter::new(File::create(&p)?);
            io::copy(data, &mut f)?;
            f.flush()?;
            self.queued.push((e, Some(p)));
        } else if size == 0 {
            // An empty file: an item without a data stream.
            let mut sink = Vec::new();
            data.read_to_end(&mut sink)?;
            self.w.push_archive_entry::<File>(e, None).ac()?;
        } else {
            self.w.push_archive_entry(e, Some(data)).ac()?;
        }
        Ok(())
    }

    fn add_path(&mut self, name: &str, path: &Path, mtime: Option<SystemTime>, mode: Option<u32>, store: bool, ctl: &archivecraft_model::Ctl) -> Result<()> {
        if self.solid {
            ctl.add_bytes(std::fs::metadata(path).map(|m| m.len()).unwrap_or(0));
            self.queued.push((Self::entry(name, mtime, false), Some(path.to_path_buf())));
            return Ok(());
        }
        let f = File::open(path)?;
        let size = f.metadata()?.len();
        let mut r = archivecraft_model::ProgressReader { inner: io::BufReader::new(f), ctl };
        self.add_file(name, mtime, size, mode, store, &mut r)
    }

    fn finish(mut self: Box<Self>) -> Result<()> {
        if self.solid {
            let queued = std::mem::take(&mut self.queued);
            let (mut with_data, mut without): (Vec<_>, Vec<_>) = queued.into_iter().partition(|(_, p)| {
                p.as_ref().is_some_and(|p| std::fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false))
            });
            for (e, _) in without.drain(..) {
                self.w.push_archive_entry::<File>(e, None).ac()?;
            }
            if !with_data.is_empty() {
                let mut entries = Vec::new();
                let mut readers = Vec::new();
                for (e, p) in with_data.drain(..) {
                    let f = io::BufReader::new(File::open(p.expect("path"))?);
                    entries.push(e);
                    readers.push(sevenz_rust2::SourceReader::new(f));
                }
                self.w.push_archive_entries(entries, readers).ac()?;
            }
        }
        let mut out = self.w.finish()?;
        out.flush()?;
        Ok(())
    }
}

