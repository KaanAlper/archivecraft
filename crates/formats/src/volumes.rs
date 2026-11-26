//! Split archives: `name.zip.001`, `name.zip.002`, … read back as one stream.

use archivecraft_model::Result;
pub use archivecraft_model::util::{is_first_volume, part_name, strip_volume_suffix};
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// All files that make up the archive at `path` (one file unless it is a `.001` part).
pub fn volume_parts(path: &Path) -> Vec<PathBuf> {
    let s = path.to_string_lossy().to_string();
    if !is_first_volume(path) {
        return vec![path.to_path_buf()];
    }
    let base = PathBuf::from(strip_volume_suffix(&s));
    let mut parts = Vec::new();
    for i in 1..10_000 {
        let p = part_name(&base, i);
        if p.exists() {
            parts.push(p);
        } else {
            break;
        }
    }
    if parts.is_empty() {
        parts.push(path.to_path_buf());
    }
    parts
}

/// Total size on disk of every part.
pub fn total_size(path: &Path) -> u64 {
    volume_parts(path).iter().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum()
}

/// Reads several files back to back as if they were one, with seeking.
pub struct MultiReader {
    files: Vec<(BufReader<File>, u64)>,
    starts: Vec<u64>,
    /// Where each underlying reader currently is, to avoid needless seeks.
    at: Vec<u64>,
    total: u64,
    pos: u64,
}

impl MultiReader {
    pub fn open(paths: &[PathBuf]) -> io::Result<MultiReader> {
        let mut files = Vec::new();
        let mut starts = Vec::new();
        let mut total = 0;
        for p in paths {
            let f = File::open(p)?;
            let len = f.metadata()?.len();
            starts.push(total);
            total += len;
            files.push((BufReader::with_capacity(256 * 1024, f), len));
        }
        let at = vec![0; files.len()];
        Ok(MultiReader { files, starts, at, total, pos: 0 })
    }
    pub fn len(&self) -> u64 {
        self.total
    }
    pub fn is_empty(&self) -> bool {
        self.total == 0
    }
}

impl Read for MultiReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.total || buf.is_empty() {
            return Ok(0);
        }
        let idx = match self.starts.binary_search(&self.pos) {
            Ok(i) => {
                // Several empty files can share a start; pick the last with that start that has data.
                let mut i = i;
                while i + 1 < self.starts.len() && self.starts[i + 1] == self.pos {
                    i += 1;
                }
                i
            }
            Err(i) => i - 1,
        };
        let off = self.pos - self.starts[idx];
        let (f, len) = &mut self.files[idx];
        if self.at[idx] != off {
            f.seek(SeekFrom::Start(off))?;
        }
        let want = ((*len - off) as usize).min(buf.len());
        let n = f.read(&mut buf[..want])?;
        self.at[idx] = off + n as u64;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for MultiReader {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let new = match to {
            SeekFrom::Start(s) => s as i128,
            SeekFrom::End(e) => self.total as i128 + e as i128,
            SeekFrom::Current(c) => self.pos as i128 + c as i128,
        };
        if new < 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "seek before start"));
        }
        self.pos = new as u64;
        Ok(self.pos)
    }
}

/// Open an archive (or the first of its parts) for reading.
pub fn open_reader(path: &Path) -> io::Result<MultiReader> {
    MultiReader::open(&volume_parts(path))
}

/// Cut `path` into parts of `size` bytes named `path.001`, `path.002`, …, then remove `path`.
pub fn split_file(path: &Path, size: u64) -> Result<Vec<PathBuf>> {
    let total = std::fs::metadata(path)?.len();
    if size == 0 || total <= size {
        // Still use the part naming so the person gets what they asked for.
        let p = part_name(path, 1);
        std::fs::rename(path, &p)?;
        return Ok(vec![p]);
    }
    let mut src = BufReader::new(File::open(path)?);
    let mut parts = Vec::new();
    let mut idx = 1;
    let mut left = total;
    let mut buf = vec![0u8; 1024 * 1024];
    while left > 0 {
        let p = part_name(path, idx);
        let mut out = BufWriter::new(File::create(&p)?);
        let mut this = size.min(left);
        while this > 0 {
            let n = (this as usize).min(buf.len());
            src.read_exact(&mut buf[..n])?;
            out.write_all(&buf[..n])?;
            this -= n as u64;
            left -= n as u64;
        }
        out.flush()?;
        parts.push(p);
        idx += 1;
    }
    drop(src);
    std::fs::remove_file(path)?;
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_and_join() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.bin");
        let data: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&p, &data).unwrap();
        let parts = split_file(&p, 3000).unwrap();
        assert_eq!(parts.len(), 4);
        let mut r = open_reader(&parts[0]).unwrap();
        let mut back = Vec::new();
        r.read_to_end(&mut back).unwrap();
        assert_eq!(back, data);
        r.seek(SeekFrom::Start(2999)).unwrap();
        let mut two = [0u8; 2];
        r.read_exact(&mut two).unwrap();
        assert_eq!(&two, &data[2999..3001]);
        assert_eq!(strip_volume_suffix("a.7z.002"), "a.7z");
    }
}
