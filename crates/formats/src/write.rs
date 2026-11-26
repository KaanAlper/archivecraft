//! Writing archives: one `Sink` per format, fed item by item.

use crate::{sevenzfmt, tarfmt, zipfmt};
use archivecraft_model::{CreateOptions, Error, Format, Result};
use std::fs::File;
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

/// Something new archives are written into.
pub trait Sink {
    fn add_dir(&mut self, name: &str, mtime: Option<SystemTime>) -> Result<()>;
    /// `size` must be the exact number of bytes `data` yields.
    fn add_file(
        &mut self,
        name: &str,
        mtime: Option<SystemTime>,
        size: u64,
        mode: Option<u32>,
        store: bool,
        data: &mut dyn Read,
    ) -> Result<()>;
    /// Add a file straight from disk. Formats that want all inputs at once (solid 7z) override this.
    fn add_path(&mut self, name: &str, path: &Path, mtime: Option<SystemTime>, mode: Option<u32>, store: bool, ctl: &archivecraft_model::Ctl) -> Result<()> {
        let f = File::open(path)?;
        let size = f.metadata()?.len();
        let mut r = archivecraft_model::ProgressReader { inner: io::BufReader::new(f), ctl };
        self.add_file(name, mtime, size, mode, store, &mut r)
    }
    fn set_comment(&mut self, _comment: &str) -> Result<()> {
        Ok(())
    }
    fn finish(self: Box<Self>) -> Result<()>;
}

/// Counts bytes on their way to disk, for the running compression ratio.
pub struct CountingWriter<W> {
    pub(crate) inner: W,
    pub(crate) count: Arc<AtomicU64>,
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(b)?;
        self.count.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<W: Seek> Seek for CountingWriter<W> {
    fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
        self.inner.seek(p)
    }
}

pub(crate) type Out = CountingWriter<BufWriter<File>>;

pub(crate) fn out_file(path: &Path, count: Arc<AtomicU64>) -> Result<Out> {
    Ok(CountingWriter { inner: BufWriter::with_capacity(512 * 1024, File::create(path)?), count })
}

/// Open the right writer for `opts.format` at `path`. `count` receives bytes written.
pub fn open_sink(path: &Path, opts: &CreateOptions, count: Arc<AtomicU64>) -> Result<Box<dyn Sink>> {
    let pw = opts.password.clone().filter(|p| !p.is_empty());
    if pw.is_some() && !opts.format.supports_password() {
        return Err(Error::Unsupported(format!("{} cannot hold a password", opts.format.label())));
    }
    Ok(match opts.format {
        Format::Zip => Box::new(zipfmt::ZipSink::new(out_file(path, count)?, opts.level.min(9), pw, opts.zip_legacy_crypto)),
        Format::SevenZ => Box::new(sevenzfmt::SevenZSink::new(path, opts, pw, count)?),
        Format::Tar | Format::TarGz | Format::TarXz | Format::TarZst => Box::new(tarfmt::TarSink::new(path, opts, count)?),
        Format::Rar | Format::Iso => return Err(Error::Unsupported(format!("{} archives can only be read, never written", opts.format.label()))),
    })
}
