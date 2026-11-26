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
    /// Also search UTF-16LE / UTF-16BE encodings when searching text.
    pub all_tables: bool,
    /// `text` is hexadecimal bytes such as `50 4B 03 04`.
    pub hex: bool,
    pub subfolders: bool,
    /// Search regular files on disk (outside archives).
    pub in_files: bool,
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
            all_tables: false,
            hex: false,
            subfolders: true,
            in_files: true,
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

fn needles(q: &FindQuery) -> Vec<Vec<u8>> {
    if q.text.is_empty() {
        return Vec::new();
    }
    if q.hex {
        let digits: String = q.text.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        let bytes: Vec<u8> = (0..digits.len() / 2).filter_map(|i| u8::from_str_radix(&digits[i * 2..i * 2 + 2], 16).ok()).collect();
        return if bytes.is_empty() { Vec::new() } else { vec![bytes] };
    }
    let text = if q.match_case { q.text.clone() } else { q.text.to_lowercase() };
    let mut out = vec![text.as_bytes().to_vec()];
    if q.all_tables {
        let le: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let be: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
        out.push(le);
        out.push(be);
    }
    out
}

/// Does the stream contain any of `needles`? Reads in blocks, keeping an overlap.
fn stream_contains_any(r: &mut dyn Read, needles: &[Vec<u8>], fold: bool) -> bool {
    if needles.is_empty() {
        return true;
    }
    let max_len = needles.iter().map(Vec::len).max().unwrap_or(0);
    if max_len == 0 {
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
        if needles.iter().any(|n| !n.is_empty() && chunk.windows(n.len()).any(|w| w == n.as_slice())) {
            return true;
        }
        let keep = max_len.saturating_sub(1).min(chunk.len());
        carry = chunk[chunk.len() - keep..].to_vec();
    }
}

fn name_ok(q: &FindQuery, name: &str) -> bool {
    q.names.is_empty() || q.names.iter().any(|p| wildcard(p, name))
}

pub fn find(root: &Path, q: &FindQuery, ctl: &Ctl, on_hit: &mut dyn FnMut(Hit)) -> Result<usize> {
    let n = needles(q);
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
            if q.in_files && name_ok(q, &name) {
                let hit = if n.is_empty() {
                    true
                } else {
                    fs::File::open(&p).map(|mut f| stream_contains_any(&mut f, &n, fold)).unwrap_or(false)
                };
                if hit {
                    count += 1;
                    on_hit(Hit { file: p.clone(), inner: None, size: meta.len() });
                }
            }
            if q.inside_archives && looks_like_archive(&name) && q.archive_types.iter().any(|t| wildcard(t, &name)) {
                count += search_archive(&p, q, &n, fold, ctl, on_hit);
            }
        }
    }
    Ok(count)
}

fn search_archive(p: &Path, q: &FindQuery, n: &[Vec<u8>], fold: bool, ctl: &Ctl, on_hit: &mut dyn FnMut(Hit)) -> usize {
    let label = p.display().to_string();
    let Ok(listing) = reader::list(p, None) else {
        ctl.info(&label, "could not be searched (locked or damaged)");
        return 0;
    };
    let mut count = 0;
    if n.is_empty() {
        for e in listing.entries.iter().filter(|e| !e.is_dir && name_ok(q, e.name())) {
            count += 1;
            on_hit(Hit { file: p.to_path_buf(), inner: Some(e.path.clone()), size: e.size });
        }
    } else {
        let mut want = |e: &Entry| !e.is_dir && name_ok(q, e.name()) && !(q.skip_locked && e.encrypted);
        let mut f = |e: &Entry, r: &mut dyn Read| -> Result<()> {
            ctl.check()?;
            if stream_contains_any(r, n, fold) {
                count += 1;
                on_hit(Hit { file: p.to_path_buf(), inner: Some(e.path.clone()), size: e.size });
            }
            Ok(())
        };
        if let Err(e) = reader::visit(p, None, &mut want, &mut f) {
            ctl.info(&label, format!("search stopped inside: {e}"));
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
        assert!(stream_contains_any(&mut data.as_slice(), &[b"needle".to_vec()], true));
        assert!(!stream_contains_any(&mut data.as_slice(), &[b"needle".to_vec()], false));
    }
}
