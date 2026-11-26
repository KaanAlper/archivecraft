//! Search folders, and the archives in them, by name and by contents.

use archivecraft_formats as reader;
use archivecraft_model::format::looks_like_archive;
use archivecraft_model::util::wildcard;
use archivecraft_model::{Ctl, Entry, Result};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FindQuery {
    /// Name patterns, e.g. `*.txt report*`. Empty = every name.
    pub names: Vec<String>,
    /// Text (or hex bytes) that must appear in the contents. Empty = no content check.
    pub text: String,
    pub match_case: bool,
    /// `text` is hexadecimal bytes such as `50 4B 03 04`.
    pub hex: bool,
    pub subfolders: bool,
    pub inside_archives: bool,
    /// Which archives to look into, e.g. `*.zip *.7z`.
    pub archive_types: Vec<String>,
    pub skip_locked: bool,
}

impl Default for FindQuery {
    fn default() -> Self {
        FindQuery {
            names: vec!["*".into()],
            text: String::new(),
            match_case: false,
            hex: false,
            subfolders: true,
            inside_archives: true,
            archive_types: vec!["*".into()],
            skip_locked: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    /// The file on disk (the archive, for hits inside one).
    pub file: PathBuf,
    /// The item inside the archive, if the hit is in one.
    pub inner: Option<String>,
    pub size: u64,
}

fn needle(q: &FindQuery) -> Option<Vec<u8>> {
    if q.text.is_empty() {
        return None;
    }
    if q.hex {
        let digits: String = q.text.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        let bytes: Vec<u8> = (0..digits.len() / 2).filter_map(|i| u8::from_str_radix(&digits[i * 2..i * 2 + 2], 16).ok()).collect();
        return Some(bytes);
    }
    Some(if q.match_case { q.text.as_bytes().to_vec() } else { q.text.to_lowercase().into_bytes() })
}

/// Does the stream contain `n`? Reads in blocks, keeping an overlap.
fn stream_contains(r: &mut dyn Read, n: &[u8], fold: bool) -> bool {
    if n.is_empty() {
        return true;
    }
    let mut buf = vec![0u8; 256 * 1024];
    let mut carry: Vec<u8> = Vec::new();
    loop {
        let k = match r.read(&mut buf) {
            Ok(0) | Err(_) => return false,
            Ok(k) => k,
        };
        let mut chunk = carry.clone();
        chunk.extend_from_slice(&buf[..k]);
        if fold {
            chunk.make_ascii_lowercase();
        }
        if chunk.windows(n.len()).any(|w| w == n) {
            return true;
        }
        let keep = n.len().saturating_sub(1).min(chunk.len());
        carry = chunk[chunk.len() - keep..].to_vec();
    }
}

fn name_ok(q: &FindQuery, name: &str) -> bool {
    q.names.is_empty() || q.names.iter().any(|p| wildcard(p, name))
}

pub fn find(root: &Path, q: &FindQuery, ctl: &Ctl, on_hit: &mut dyn FnMut(Hit)) -> Result<usize> {
    let n = needle(q);
    let fold = !q.match_case && !q.hex;
    let mut count = 0;
    let mut stack = vec![root.to_path_buf()];
    ctl.stage("Searching");
    while let Some(dir) = stack.pop() {
        ctl.check()?;
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        let mut items: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
        items.sort();
        for p in items {
            ctl.check()?;
            let Ok(meta) = fs::symlink_metadata(&p) else { continue };
            if meta.file_type().is_symlink() {
                continue;
            }
            let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            if meta.is_dir() {
                if q.subfolders {
                    stack.push(p.clone());
                }
                continue;
            }
            ctl.item(&p.display().to_string(), meta.len());
            if name_ok(q, &name) {
                let hit = match &n {
                    None => true,
                    Some(n) => fs::File::open(&p).map(|mut f| stream_contains(&mut f, n, fold)).unwrap_or(false),
                };
                if hit {
                    count += 1;
                    on_hit(Hit { file: p.clone(), inner: None, size: meta.len() });
                }
            }
            if q.inside_archives && looks_like_archive(&name) && q.archive_types.iter().any(|t| wildcard(t, &name)) {
                count += search_archive(&p, q, n.as_deref(), fold, ctl, on_hit);
            }
        }
    }
    Ok(count)
}

fn search_archive(p: &Path, q: &FindQuery, n: Option<&[u8]>, fold: bool, ctl: &Ctl, on_hit: &mut dyn FnMut(Hit)) -> usize {
    let label = p.display().to_string();
    let Ok(listing) = reader::list(p, None) else {
        ctl.info(&label, "could not be searched (locked or damaged)");
        return 0;
    };
    let mut count = 0;
    match n {
        None => {
            for e in listing.entries.iter().filter(|e| !e.is_dir && name_ok(q, e.name())) {
                count += 1;
                on_hit(Hit { file: p.to_path_buf(), inner: Some(e.path.clone()), size: e.size });
            }
        }
        Some(n) => {
            let mut want = |e: &Entry| !e.is_dir && name_ok(q, e.name()) && !(q.skip_locked && e.encrypted);
            let mut f = |e: &Entry, r: &mut dyn Read| -> Result<()> {
                ctl.check()?;
                if stream_contains(r, n, fold) {
                    count += 1;
                    on_hit(Hit { file: p.to_path_buf(), inner: Some(e.path.clone()), size: e.size });
                }
                Ok(())
            };
            if let Err(e) = reader::visit(p, None, &mut want, &mut f) {
                ctl.info(&label, format!("search stopped inside: {e}"));
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finds_text_across_block_edges() {
        let mut data = vec![b'x'; 256 * 1024 - 3];
        data.extend_from_slice(b"Needle");
        assert!(stream_contains(&mut data.as_slice(), b"needle", true));
        assert!(!stream_contains(&mut data.as_slice(), b"needle", false));
    }
}
