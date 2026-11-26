//! The tar family: plain tar, and tar compressed with gzip, xz or Zstandard.

use crate::read::{VisitFn, Want};
use crate::volumes;
use crate::write::{Out, Sink};
use archivecraft_model::{CreateOptions, Entry, Error, Format, Listing, Result};
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::SystemTime;

// ---------------------------------------------------------------- tar family

fn tar_stream(path: &Path, format: Format) -> Result<Box<dyn Read>> {
    let r = BufReader::with_capacity(256 * 1024, volumes::open_reader(path)?);
    Ok(match format {
        Format::Tar => Box::new(r),
        Format::TarGz => Box::new(flate2::read::MultiGzDecoder::new(r)),
        Format::TarXz => Box::new(lzma_rust2::XzReader::new(r, true)),
        Format::TarZst => Box::new(
            ruzstd::decoding::StreamingDecoder::new(r).map_err(|e| Error::Corrupt(format!("zstd: {e}")))?,
        ),
        _ => return Err(Error::Unsupported("not a tar format".into())),
    })
}

fn tar_entry<R: Read>(e: &tar::Entry<R>) -> Result<Entry> {
    let h = e.header();
    let raw = e.path_bytes();
    let name = String::from_utf8_lossy(&raw).to_string();
    let t = h.entry_type();
    let is_dir = t.is_dir();
    let mut en = Entry::new(&name, is_dir);
    en.is_link = t.is_symlink() || t.is_hard_link();
    en.size = if is_dir { 0 } else { h.size().unwrap_or(0) };
    en.modified = h.mtime().ok().and_then(|s| archivecraft_model::util::from_unix_secs(s as i64));
    en.mode = h.mode().ok();
    en.method = match t {
        tar::EntryType::Regular | tar::EntryType::Continuous => "file".into(),
        tar::EntryType::Directory => "folder".into(),
        tar::EntryType::Symlink => "link".into(),
        tar::EntryType::Link => "hard link".into(),
        _ => "special".into(),
    };
    Ok(en)
}

fn tar_is_content(t: tar::EntryType) -> bool {
    !matches!(t, tar::EntryType::XGlobalHeader | tar::EntryType::XHeader | tar::EntryType::GNULongName | tar::EntryType::GNULongLink)
}

pub(crate) fn list(path: &Path, format: Format) -> Result<Listing> {
    let mut ar = tar::Archive::new(tar_stream(path, format)?);
    let mut entries = Vec::new();
    for e in ar.entries()? {
        let e = e?;
        if !tar_is_content(e.header().entry_type()) {
            continue;
        }
        let en = tar_entry(&e)?;
        if !en.path.is_empty() {
            entries.push(en);
        }
    }
    let method = match format {
        Format::Tar => "none",
        Format::TarGz => "gzip",
        Format::TarXz => "xz (LZMA2)",
        _ => "Zstandard",
    };
    Ok(Listing {
        format,
        entries,
        comment: None,
        solid: format != Format::Tar,
        names_encrypted: false,
        volumes: 1,
        archive_size: 0,
        methods: vec![method.into()],
    })
}

pub(crate) fn visit(path: &Path, format: Format, want: Want, f: VisitFn) -> Result<()> {
    let mut ar = tar::Archive::new(tar_stream(path, format)?);
    for e in ar.entries()? {
        let mut e = e?;
        if !tar_is_content(e.header().entry_type()) {
            continue;
        }
        let en = tar_entry(&e)?;
        if en.path.is_empty() || !want(&en) {
            continue;
        }
        if en.is_dir || en.is_link {
            f(&en, &mut io::empty())?;
        } else {
            f(&en, &mut e)?;
        }
    }
    Ok(())
}


// ---------------------------------------------------------------- tar family

enum Compressor {
    Plain(Out),
    Gz(flate2::write::GzEncoder<Out>),
    Xz(Box<lzma_rust2::XzWriter<Out>>),
    /// Zstandard: the tar is spooled to a temporary file and compressed at the end.
    Zst { spool: BufWriter<File>, spool_path: tempfile::TempPath, out: Out, level: u32 },
}

impl Compressor {
    fn new(path: &Path, opts: &CreateOptions, count: Arc<AtomicU64>) -> Result<Self> {
        let out = crate::write::out_file(path, count)?;
        Ok(match opts.format {
            Format::Tar => Compressor::Plain(out),
            Format::TarGz => Compressor::Gz(flate2::write::GzEncoder::new(out, flate2::Compression::new(opts.level.min(9)))),
            Format::TarXz => {
                let mut o = lzma_rust2::XzOptions::with_preset(opts.level.min(9));
                if let Some(d) = opts.dictionary {
                    o.lzma_options.dict_size = d.clamp(4096, 1 << 30);
                }
                Compressor::Xz(Box::new(lzma_rust2::XzWriter::new(out, o)?))
            }
            Format::TarZst => {
                let tmp = tempfile::NamedTempFile::new()?;
                let (f, spool_path) = tmp.into_parts();
                Compressor::Zst { spool: BufWriter::new(f), spool_path, out, level: opts.level }
            }
            _ => return Err(Error::Unsupported("not a tar format".into())),
        })
    }

    fn finish(self) -> Result<()> {
        match self {
            Compressor::Plain(mut o) => o.flush()?,
            Compressor::Gz(g) => g.finish()?.flush()?,
            Compressor::Xz(x) => x.finish()?.flush()?,
            Compressor::Zst { mut spool, spool_path, mut out, level } => {
                spool.flush()?;
                drop(spool);
                let src = io::BufReader::new(File::open(&spool_path)?);
                let lvl = if level == 0 { ruzstd::encoding::CompressionLevel::Uncompressed } else { ruzstd::encoding::CompressionLevel::Fastest };
                ruzstd::encoding::compress(src, &mut out, lvl);
                out.flush()?;
            }
        }
        Ok(())
    }
}

impl Write for Compressor {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        match self {
            Compressor::Plain(o) => o.write(b),
            Compressor::Gz(g) => g.write(b),
            Compressor::Xz(x) => x.write(b),
            Compressor::Zst { spool, .. } => spool.write(b),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Compressor::Plain(o) => o.flush(),
            Compressor::Gz(g) => g.flush(),
            Compressor::Xz(x) => x.flush(),
            Compressor::Zst { spool, .. } => spool.flush(),
        }
    }
}

pub(crate) struct TarSink {
    b: tar::Builder<Compressor>,
}

impl TarSink {
    pub(crate) fn new(path: &Path, opts: &CreateOptions, count: Arc<AtomicU64>) -> Result<Self> {
        Ok(TarSink { b: tar::Builder::new(Compressor::new(path, opts, count)?) })
    }
}

fn tar_header(kind: tar::EntryType, size: u64, mode: u32, mtime: Option<SystemTime>) -> tar::Header {
    let mut h = tar::Header::new_gnu();
    h.set_entry_type(kind);
    h.set_size(size);
    h.set_mode(mode);
    h.set_mtime(mtime.map(archivecraft_model::util::unix_secs).unwrap_or(0));
    h
}

impl Sink for TarSink {
    fn add_dir(&mut self, name: &str, mtime: Option<SystemTime>) -> Result<()> {
        let mut h = tar_header(tar::EntryType::Directory, 0, 0o755, mtime);
        self.b.append_data(&mut h, format!("{}/", name.trim_end_matches('/')), io::empty())?;
        Ok(())
    }

    fn add_file(&mut self, name: &str, mtime: Option<SystemTime>, size: u64, mode: Option<u32>, _store: bool, data: &mut dyn Read) -> Result<()> {
        let mut h = tar_header(tar::EntryType::Regular, size, mode.map(|m| m & 0o7777).unwrap_or(0o644), mtime);
        self.b.append_data(&mut h, name, data.take(size))?;
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<()> {
        let inner = self.b.into_inner()?;
        inner.finish()
    }
}
