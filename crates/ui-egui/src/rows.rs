//! What the file list shows: the items of a disk folder, or of a folder inside an archive.

use crate::settings::ListPrefs;
use archivecraft_engine::model::format::looks_like_archive;
use archivecraft_engine::model::util::type_label;
use archivecraft_engine::model::Listing;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq)]
pub enum RowRef {
    /// The ".." row.
    Parent,
    Disk(PathBuf),
    /// Path inside the open archive.
    Entry(String),
}

#[derive(Clone, Debug)]
pub struct Row {
    pub name: String,
    pub is_dir: bool,
    pub is_archive: bool,
    pub size: Option<u64>,
    pub packed: Option<u64>,
    pub kind: String,
    pub modified: Option<SystemTime>,
    pub crc: Option<u32>,
    pub locked: bool,
    pub target: RowRef,
    pub created: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    /// Permission / attribute bits, when known.
    pub attrs: Option<u32>,
    /// Position as found on disk or stored in the archive ("as stored" order).
    pub order: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Size,
    Packed,
    Kind,
    Modified,
    Crc,
    Created,
    Accessed,
    Attributes,
    Stored,
}

impl SortKey {
    /// The orders offered in the list's right-click menu.
    pub const MENU: [SortKey; 9] = [SortKey::Name, SortKey::Size, SortKey::Packed, SortKey::Kind, SortKey::Modified, SortKey::Created, SortKey::Accessed, SortKey::Attributes, SortKey::Stored];

    pub fn label(self) -> &'static str {
        match self {
            SortKey::Name => "Name",
            SortKey::Size => "Size",
            SortKey::Packed => "Packed size",
            SortKey::Kind => "Type",
            SortKey::Modified => "Last changed",
            SortKey::Created => "Created",
            SortKey::Accessed => "Last opened",
            SortKey::Attributes => "Permissions",
            SortKey::Stored => "As stored",
            SortKey::Crc => "Checksum",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sort {
    pub key: SortKey,
    pub ascending: bool,
}

impl Default for Sort {
    fn default() -> Self {
        Sort { key: SortKey::Name, ascending: true }
    }
}

pub fn parent_row() -> Row {
    Row {
        name: "..".into(),
        is_dir: true,
        is_archive: false,
        size: None,
        packed: None,
        kind: crate::tl!("Up one level"),
        modified: None,
        crc: None,
        locked: false,
        target: RowRef::Parent,
        created: None,
        accessed: None,
        attrs: None,
        order: 0,
    }
}

pub fn disk_rows(dir: &std::path::Path, prefs: &ListPrefs) -> std::io::Result<Vec<Row>> {
    let mut rows = Vec::new();
    if dir.parent().is_some() {
        rows.push(parent_row());
    }
    for e in std::fs::read_dir(dir)? {
        let Ok(e) = e else { continue };
        let name = e.file_name().to_string_lossy().to_string();
        if !prefs.show_hidden && name.starts_with('.') {
            continue;
        }
        let path = e.path();
        let meta = std::fs::metadata(&path).or_else(|_| std::fs::symlink_metadata(&path));
        let (is_dir, size, modified, created, accessed, attrs) = match &meta {
            Ok(m) => (m.is_dir(), if m.is_dir() { None } else { Some(m.len()) }, m.modified().ok(), m.created().ok(), m.accessed().ok(), attrs_of(m)),
            Err(_) => (false, None, None, None, None, None),
        };
        let is_archive = !is_dir && looks_like_archive(&name);
        let order = rows.len();
        rows.push(Row { kind: crate::tl!(&type_label(&name, is_dir)), name, is_dir, is_archive, size, packed: None, modified, crc: None, locked: false, target: RowRef::Disk(path), created, accessed, attrs, order });
    }
    Ok(rows)
}

#[cfg(unix)]
fn attrs_of(m: &std::fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(m.permissions().mode() & 0o7777)
}

#[cfg(not(unix))]
fn attrs_of(m: &std::fs::Metadata) -> Option<u32> {
    Some(u32::from(m.permissions().readonly()))
}

/// Children of `folder` in the archive, with folder sizes summed from what is inside them.
pub fn archive_rows(listing: &Listing, folder: &str) -> Vec<Row> {
    let mut rows = vec![parent_row()];
    let prefix = if folder.is_empty() { String::new() } else { format!("{folder}/") };
    struct Agg {
        size: u64,
        packed: u64,
        has_packed: bool,
        modified: Option<SystemTime>,
        locked: bool,
        explicit_time: Option<SystemTime>,
        order: usize,
    }
    let mut dirs: BTreeMap<String, Agg> = BTreeMap::new();
    for (index, e) in listing.entries.iter().enumerate() {
        let Some(rest) = e.path.strip_prefix(&prefix) else { continue };
        if rest.is_empty() {
            continue;
        }
        match rest.split_once('/') {
            Some((child, _)) => {
                let a = dirs.entry(child.to_string()).or_insert(Agg { size: 0, packed: 0, has_packed: false, modified: None, locked: false, explicit_time: None, order: index });
                a.size += e.size;
                if let Some(p) = e.packed {
                    a.packed += p;
                    a.has_packed = true;
                }
                a.modified = a.modified.max(e.modified);
                a.locked |= e.encrypted;
            }
            None if e.is_dir => {
                let a = dirs.entry(rest.to_string()).or_insert(Agg { size: 0, packed: 0, has_packed: false, modified: None, locked: false, explicit_time: None, order: index });
                a.explicit_time = e.modified;
            }
            None => rows.push(Row {
                name: rest.to_string(),
                is_dir: false,
                is_archive: looks_like_archive(rest),
                size: Some(e.size),
                packed: e.packed,
                kind: crate::tl!(&type_label(rest, false)),
                modified: e.modified,
                crc: e.crc,
                locked: e.encrypted,
                target: RowRef::Entry(e.path.clone()),
                created: None,
                accessed: None,
                attrs: e.mode.map(|m| m & 0o7777),
                order: index,
            }),
        }
    }
    for (name, a) in dirs {
        rows.push(Row {
            kind: crate::tl!(&type_label(&name, true)),
            target: RowRef::Entry(format!("{prefix}{name}")),
            name,
            is_dir: true,
            is_archive: false,
            size: Some(a.size),
            packed: a.has_packed.then_some(a.packed),
            modified: a.explicit_time.or(a.modified),
            crc: None,
            locked: a.locked,
            created: None,
            accessed: None,
            attrs: None,
            order: a.order,
        });
    }
    rows
}

pub fn sort_rows(rows: &mut [Row], sort: Sort, archives_first: bool) {
    rows.sort_by(|a, b| {
        use std::cmp::Ordering;
        let rank = |r: &Row| -> u8 {
            if r.target == RowRef::Parent {
                0
            } else if r.is_dir {
                1
            } else if archives_first && r.is_archive {
                2
            } else {
                3
            }
        };
        let by_rank = rank(a).cmp(&rank(b));
        if by_rank != Ordering::Equal {
            return by_rank;
        }
        let o = match sort.key {
            SortKey::Name => natural(&a.name, &b.name),
            SortKey::Size => a.size.cmp(&b.size),
            SortKey::Packed => a.packed.cmp(&b.packed),
            SortKey::Kind => a.kind.to_lowercase().cmp(&b.kind.to_lowercase()),
            SortKey::Modified => a.modified.cmp(&b.modified),
            SortKey::Crc => a.crc.cmp(&b.crc),
            SortKey::Created => a.created.cmp(&b.created),
            SortKey::Accessed => a.accessed.cmp(&b.accessed),
            SortKey::Attributes => a.attrs.cmp(&b.attrs),
            SortKey::Stored => a.order.cmp(&b.order),
        }
        .then_with(|| natural(&a.name, &b.name));
        if sort.ascending { o } else { o.reverse() }
    });
}

/// Names compared case-insensitively, with digit runs by value ("file2" before "file10").
pub fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let mut n1 = String::new();
                while let Some(c) = x.peek().copied().filter(char::is_ascii_digit) {
                    n1.push(c);
                    x.next();
                }
                let mut n2 = String::new();
                while let Some(d) = y.peek().copied().filter(char::is_ascii_digit) {
                    n2.push(d);
                    y.next();
                }
                let o = n1.trim_start_matches('0').len().cmp(&n2.trim_start_matches('0').len()).then_with(|| n1.trim_start_matches('0').cmp(n2.trim_start_matches('0')));
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
            }
            (Some(c), Some(d)) => {
                let o = c.to_lowercase().cmp(d.to_lowercase());
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
                x.next();
                y.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archivecraft_engine::model::{Entry, Format};

    #[test]
    fn natural_order() {
        let mut v = vec!["file10", "File2", "file1"];
        v.sort_by(|a, b| natural(a, b));
        assert_eq!(v, ["file1", "File2", "file10"]);
    }

    #[test]
    fn archive_folders_are_built_from_paths() {
        let mut a = Entry::new("docs/a.txt", false);
        a.size = 10;
        a.packed = Some(4);
        let mut b = Entry::new("docs/sub/b.txt", false);
        b.size = 5;
        b.packed = Some(5);
        let c = Entry::new("top.txt", false);
        let l = Listing { format: Format::Zip, entries: vec![a, b, c], comment: None, solid: false, names_encrypted: false, volumes: 1, archive_size: 0, methods: vec![] };
        let root = archive_rows(&l, "");
        assert_eq!(root.len(), 3);
        let docs = root.iter().find(|r| r.name == "docs").unwrap();
        assert!(docs.is_dir);
        assert_eq!(docs.size, Some(15));
        assert_eq!(docs.packed, Some(9));
        let inside = archive_rows(&l, "docs");
        assert!(inside.iter().any(|r| r.name == "sub" && r.is_dir));
        assert!(inside.iter().any(|r| r.name == "a.txt"));
    }
}
