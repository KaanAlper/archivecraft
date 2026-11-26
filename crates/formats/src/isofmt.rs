//! Read-only ISO-9660 (with Joliet UTF-16BE support) disc image reader.

use crate::read::{VisitFn, Want};
use archivecraft_model::{normalize_name, Entry, Error, Format, Listing, Result};
use chrono::{NaiveDate, TimeZone, Utc};
use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::SystemTime;

const SECTOR_SIZE: u64 = 2048;

struct IsoEntry {
    entry: Entry,
    extent_lba: u32,
}

fn read_u32_le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn parse_iso_time(b: &[u8]) -> Option<SystemTime> {
    if b.len() < 7 {
        return None;
    }
    let year = 1900 + b[0] as i32;
    let month = b[1] as u32;
    let day = b[2] as u32;
    let hour = b[3] as u32;
    let min = b[4] as u32;
    let sec = b[5] as u32;
    let d = NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, min, sec)?;
    let offset_quarters = b[6] as i8 as i64;
    let ts = Utc.from_utc_datetime(&d).timestamp() - offset_quarters * 15 * 60;
    archivecraft_model::util::from_unix_secs(ts)
}

fn decode_name(raw: &[u8], joliet: bool) -> String {
    let s = if joliet {
        let units: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(raw).into_owned()
    };
    let s = s.split(';').next().unwrap_or(&s);
    s.trim_end_matches('.').to_string()
}

fn find_root_descriptor(file: &mut File) -> Result<(u32, u32, bool)> {
    let mut sector = [0u8; SECTOR_SIZE as usize];
    let mut primary: Option<(u32, u32)> = None;
    let mut joliet: Option<(u32, u32)> = None;
    for lba in 16u64..64 {
        if file.seek(SeekFrom::Start(lba * SECTOR_SIZE)).is_err() || file.read_exact(&mut sector).is_err() {
            break;
        }
        if &sector[1..6] != b"CD001" {
            break;
        }
        let kind = sector[0];
        if kind == 255 {
            break;
        }
        if kind == 1 || kind == 2 {
            let root = &sector[156..190];
            if root[0] >= 34 {
                let lba = read_u32_le(&root[2..6]);
                let size = read_u32_le(&root[10..14]);
                if kind == 1 && primary.is_none() {
                    primary = Some((lba, size));
                } else if kind == 2 {
                    let esc = &sector[88..91];
                    if esc == [0x25, 0x2F, 0x40] || esc == [0x25, 0x2F, 0x43] || esc == [0x25, 0x2F, 0x45] {
                        joliet = Some((lba, size));
                    }
                }
            }
        }
    }
    if let Some((lba, size)) = joliet {
        Ok((lba, size, true))
    } else if let Some((lba, size)) = primary {
        Ok((lba, size, false))
    } else {
        Err(Error::Corrupt("ISO-9660 primary volume descriptor not found".into()))
    }
}

fn read_dir_tree(file: &mut File, lba: u32, size: u32, prefix: &str, joliet: bool, visited: &mut HashSet<u32>, out: &mut Vec<IsoEntry>) -> Result<()> {
    if !visited.insert(lba) || size == 0 || size > 16 * 1024 * 1024 {
        return Ok(());
    }
    let mut buf = vec![0u8; size as usize];
    file.seek(SeekFrom::Start(lba as u64 * SECTOR_SIZE))?;
    file.read_exact(&mut buf)?;

    let mut pos = 0usize;
    let mut subdirs: Vec<(u32, u32, String)> = Vec::new();
    while pos < buf.len() {
        let len = buf[pos] as usize;
        if len == 0 {
            let next_sector = ((pos / SECTOR_SIZE as usize) + 1) * SECTOR_SIZE as usize;
            pos = next_sector;
            continue;
        }
        if pos + len > buf.len() || len < 34 {
            break;
        }
        let rec = &buf[pos..pos + len];
        let name_len = rec[32] as usize;
        if 33 + name_len <= len {
            let raw_name = &rec[33..33 + name_len];
            // Skip "." (0x00) and ".." (0x01).
            if !(name_len == 1 && (raw_name[0] == 0 || raw_name[0] == 1)) {
                let name = decode_name(raw_name, joliet);
                if !name.is_empty() {
                    let ext_lba = read_u32_le(&rec[2..6]);
                    let data_len = read_u32_le(&rec[10..14]);
                    let flags = rec[25];
                    let is_dir = (flags & 0x02) != 0;
                    let modified = parse_iso_time(&rec[18..25]);
                    let full = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
                    let full = normalize_name(&full);
                    if !full.is_empty() {
                        let mut entry = Entry::new(&full, is_dir);
                        entry.size = if is_dir { 0 } else { data_len as u64 };
                        entry.packed = if is_dir { None } else { Some(data_len as u64) };
                        entry.modified = modified;
                        entry.method = "ISO-9660".into();
                        out.push(IsoEntry { entry, extent_lba: ext_lba });
                        if is_dir {
                            subdirs.push((ext_lba, data_len, full));
                        }
                    }
                }
            }
        }
        pos += len;
    }
    for (sub_lba, sub_size, sub_path) in subdirs {
        read_dir_tree(file, sub_lba, sub_size, &sub_path, joliet, visited, out)?;
    }
    Ok(())
}

fn collect(path: &Path) -> Result<(File, Vec<IsoEntry>)> {
    let mut file = File::open(path)?;
    let (root_lba, root_size, joliet) = find_root_descriptor(&mut file)?;
    let mut visited = HashSet::new();
    let mut entries = Vec::new();
    read_dir_tree(&mut file, root_lba, root_size, "", joliet, &mut visited, &mut entries)?;
    Ok((file, entries))
}

pub fn list(path: &Path) -> Result<Listing> {
    let (_, items) = collect(path)?;
    Ok(Listing {
        format: Format::Iso,
        entries: items.into_iter().map(|i| i.entry).collect(),
        comment: None,
        solid: false,
        names_encrypted: false,
        volumes: 1,
        archive_size: 0,
        methods: vec!["ISO-9660".into()],
    })
}

pub fn visit(path: &Path, want: Want, f: VisitFn) -> Result<()> {
    let (mut file, items) = collect(path)?;
    for item in items {
        if !want(&item.entry) {
            continue;
        }
        if item.entry.is_dir {
            let mut empty = std::io::empty();
            f(&item.entry, &mut empty)?;
        } else {
            file.seek(SeekFrom::Start(item.extent_lba as u64 * SECTOR_SIZE))?;
            let mut limited = (&mut file).take(item.entry.size);
            f(&item.entry, &mut limited)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn reads_minimal_iso() {
        let dir = tempfile::tempdir().unwrap();
        let iso = dir.path().join("test.iso");
        let mut img = vec![0u8; 20 * SECTOR_SIZE as usize];
        // Sector 16: Primary Volume Descriptor
        let pvd = &mut img[16 * 2048..17 * 2048];
        pvd[0] = 1;
        pvd[1..6].copy_from_slice(b"CD001");
        pvd[6] = 1;
        // Root dir at LBA 18, size 2048
        let root = &mut pvd[156..190];
        root[0] = 34;
        root[2..6].copy_from_slice(&18u32.to_le_bytes());
        root[10..14].copy_from_slice(&2048u32.to_le_bytes());
        root[25] = 0x02;
        // Sector 17: Terminator
        img[17 * 2048] = 255;
        img[17 * 2048 + 1..17 * 2048 + 6].copy_from_slice(b"CD001");
        // Sector 18: Root directory with hello.txt at LBA 19
        let dir_sec = &mut img[18 * 2048..19 * 2048];
        let rec = &mut dir_sec[0..44];
        rec[0] = 44;
        rec[2..6].copy_from_slice(&19u32.to_le_bytes());
        rec[10..14].copy_from_slice(&5u32.to_le_bytes());
        rec[25] = 0;
        rec[32] = 11;
        rec[33..44].copy_from_slice(b"hello.txt;1");
        // Sector 19: file contents
        img[19 * 2048..19 * 2048 + 5].copy_from_slice(b"hello");
        File::create(&iso).unwrap().write_all(&img).unwrap();

        let l = list(&iso).unwrap();
        assert_eq!(l.format, Format::Iso);
        assert_eq!(l.entries.len(), 1);
        assert_eq!(l.entries[0].path, "hello.txt");
        assert_eq!(l.entries[0].size, 5);
        let data = crate::read_one(&iso, "hello.txt", None, 1024).unwrap();
        assert_eq!(data, b"hello");
    }
}
