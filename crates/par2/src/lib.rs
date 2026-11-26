//! Recovery data in the open PAR2 format (written from the public "Parity Volume Set
//! Specification 2.0"): create it next to an archive, check an archive against it, and rebuild
//! damaged parts.
//!
//! Reed–Solomon over GF(2^16) with the generator polynomial 0x1100B. Input slice *i* gets the
//! constant 2^n(i), where n(i) runs through the numbers whose greatest common divisor with 65535
//! is 1. A recovery slice with exponent *e* is the sum over all input slices of constant^e · slice.

use archivecraft_model::{Ctl, Error, Result};
use md5::{Digest, Md5};
use std::fs::{self, File};
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const MAGIC: &[u8; 8] = b"PAR2\0PKT";
const T_MAIN: &[u8; 16] = b"PAR 2.0\0Main\0\0\0\0";
const T_DESC: &[u8; 16] = b"PAR 2.0\0FileDesc";
const T_IFSC: &[u8; 16] = b"PAR 2.0\0IFSC\0\0\0\0";
const T_RECV: &[u8; 16] = b"PAR 2.0\0RecvSlic";
const T_CREATOR: &[u8; 16] = b"PAR 2.0\0Creator\0";

// ---------------------------------------------------------------- GF(2^16)

struct Gf {
    log: Vec<u32>,
    exp: Vec<u16>,
}

fn gf() -> &'static Gf {
    static G: OnceLock<Gf> = OnceLock::new();
    G.get_or_init(|| {
        let mut log = vec![0u32; 65536];
        let mut exp = vec![0u16; 65535 * 2];
        let mut x: u32 = 1;
        for i in 0..65535u32 {
            exp[i as usize] = x as u16;
            log[x as usize] = i;
            x <<= 1;
            if x & 0x10000 != 0 {
                x ^= 0x1100B;
            }
        }
        for i in 0..65535 {
            exp[i + 65535] = exp[i];
        }
        Gf { log, exp }
    })
}

fn mul(a: u16, b: u16) -> u16 {
    if a == 0 || b == 0 {
        return 0;
    }
    let g = gf();
    g.exp[(g.log[a as usize] + g.log[b as usize]) as usize]
}

fn pow(a: u16, e: u32) -> u16 {
    if e == 0 {
        return 1;
    }
    if a == 0 {
        return 0;
    }
    let g = gf();
    g.exp[((g.log[a as usize] as u64 * e as u64) % 65535) as usize]
}

fn inv(a: u16) -> u16 {
    let g = gf();
    g.exp[((65535 - g.log[a as usize]) % 65535) as usize]
}

/// The constant for each input slice.
fn bases(count: usize) -> Vec<u16> {
    let mut out = Vec::with_capacity(count);
    let mut n: u32 = 0;
    while out.len() < count {
        n += 1;
        if !n.is_multiple_of(3) && !n.is_multiple_of(5) && !n.is_multiple_of(17) && !n.is_multiple_of(257) {
            out.push(gf().exp[n as usize]);
        }
    }
    out
}

/// out ^= f · data, on little-endian 16-bit words.
fn mul_add(out: &mut [u8], data: &[u8], f: u16) {
    if f == 0 {
        return;
    }
    let mut lo = [0u16; 256];
    let mut hi = [0u16; 256];
    for b in 0..256u16 {
        lo[b as usize] = mul(f, b);
        hi[b as usize] = mul(f, b << 8);
    }
    for (o, d) in out.as_chunks_mut::<2>().0.iter_mut().zip(data.as_chunks::<2>().0.iter()) {
        let p = lo[d[0] as usize] ^ hi[d[1] as usize];
        let w = u16::from_le_bytes([o[0], o[1]]) ^ p;
        o.copy_from_slice(&w.to_le_bytes());
    }
}

/// Invert a square matrix in GF(2^16). `None` if it cannot be inverted.
fn invert(mut m: Vec<Vec<u16>>) -> Option<Vec<Vec<u16>>> {
    let n = m.len();
    let mut r: Vec<Vec<u16>> = (0..n).map(|i| (0..n).map(|j| u16::from(i == j)).collect()).collect();
    for col in 0..n {
        let piv = (col..n).find(|&i| m[i][col] != 0)?;
        m.swap(col, piv);
        r.swap(col, piv);
        let f = inv(m[col][col]);
        for j in 0..n {
            m[col][j] = mul(m[col][j], f);
            r[col][j] = mul(r[col][j], f);
        }
        for i in 0..n {
            if i != col && m[i][col] != 0 {
                let g = m[i][col];
                for j in 0..n {
                    m[i][j] ^= mul(g, m[col][j]);
                    r[i][j] ^= mul(g, r[col][j]);
                }
            }
        }
    }
    Some(r)
}

// ---------------------------------------------------------------- packets

fn md5(data: &[u8]) -> [u8; 16] {
    let mut h = Md5::new();
    h.update(data);
    h.finalize().into()
}

fn packet(set_id: &[u8; 16], kind: &[u8; 16], body: &[u8]) -> Vec<u8> {
    debug_assert!(body.len().is_multiple_of(4));
    let mut hashed = Vec::with_capacity(32 + body.len());
    hashed.extend_from_slice(set_id);
    hashed.extend_from_slice(kind);
    hashed.extend_from_slice(body);
    let mut p = Vec::with_capacity(64 + body.len());
    p.extend_from_slice(MAGIC);
    p.extend_from_slice(&(64 + body.len() as u64).to_le_bytes());
    p.extend_from_slice(&md5(&hashed));
    p.extend_from_slice(&hashed);
    p
}

fn pad4(mut v: Vec<u8>) -> Vec<u8> {
    while !v.len().is_multiple_of(4) {
        v.push(0);
    }
    v
}

/// par2 tools order file ids as 128-bit little-endian numbers.
fn id_cmp(a: &[u8; 16], b: &[u8; 16]) -> std::cmp::Ordering {
    a.iter().rev().cmp(b.iter().rev())
}

#[derive(Debug, Clone)]
struct FileDesc {
    id: [u8; 16],
    md5: [u8; 16],
    md5_16k: [u8; 16],
    len: u64,
    name: String,
}

#[derive(Debug, Default)]
struct Set {
    set_id: [u8; 16],
    slice: u64,
    order: Vec<[u8; 16]>,
    files: Vec<FileDesc>,
    checks: std::collections::HashMap<[u8; 16], Vec<([u8; 16], u32)>>,
    /// exponent → recovery data
    recovery: std::collections::BTreeMap<u32, Vec<u8>>,
}

fn read_slice(f: &mut File, idx: u64, slice: u64, len: u64) -> std::io::Result<Vec<u8>> {
    let mut buf = vec![0u8; slice as usize];
    let start = idx * slice;
    if start < len {
        f.seek(SeekFrom::Start(start))?;
        let want = (len - start).min(slice) as usize;
        let mut got = 0;
        while got < want {
            let n = f.read(&mut buf[got..want])?;
            if n == 0 {
                break;
            }
            got += n;
        }
    }
    Ok(buf)
}

fn slice_count(len: u64, slice: u64) -> u64 {
    len.div_ceil(slice)
}

/// The file name recovery data for `archive` lives in: `<archive>.par2` (first part for split archives).
fn base_name(paths: &[PathBuf]) -> PathBuf {
    let first = &paths[0];
    let s = first.to_string_lossy().to_string();
    if archivecraft_model::util::is_first_volume(first) {
        PathBuf::from(archivecraft_model::util::strip_volume_suffix(&s))
    } else {
        first.clone()
    }
}

/// The index file for an archive, if recovery data exists.
pub fn find_for(archive: &Path) -> Option<PathBuf> {
    let base = base_name(&[archive.to_path_buf()]);
    let p = PathBuf::from(format!("{}.par2", base.display()));
    p.exists().then_some(p)
}

/// Write recovery data covering `files` (all in one folder). Returns the files written.
pub fn create(files: &[PathBuf], percent: u32, ctl: &Ctl) -> Result<Vec<PathBuf>> {
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let total: u64 = files.iter().map(|p| fs::metadata(p).map(|m| m.len()).unwrap_or(0)).sum();
    if total == 0 {
        return Ok(Vec::new());
    }
    let mut slice = total.div_ceil(2000).max(512);
    slice = slice.div_ceil(4) * 4;

    // Describe each file.
    let mut descs = Vec::new();
    let mut checks = std::collections::HashMap::new();
    for p in files {
        ctl.check()?;
        let len = fs::metadata(p)?.len();
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let mut f = File::open(p)?;
        let mut whole = Md5::new();
        let mut first = Vec::new();
        let mut list = Vec::new();
        for i in 0..slice_count(len, slice) {
            let data = read_slice(&mut f, i, slice, len)?;
            let real = ((len - i * slice).min(slice)) as usize;
            whole.update(&data[..real]);
            if first.len() < 16384 {
                let take = (16384 - first.len()).min(real);
                first.extend_from_slice(&data[..take]);
            }
            list.push((md5(&data), crc32fast::hash(&data)));
        }
        let md5_16k = md5(&first);
        let mut id_src = Vec::new();
        id_src.extend_from_slice(&md5_16k);
        id_src.extend_from_slice(&len.to_le_bytes());
        id_src.extend_from_slice(name.as_bytes());
        let id = md5(&id_src);
        checks.insert(id, list);
        descs.push((FileDesc { id, md5: whole.finalize().into(), md5_16k, len, name }, p.clone()));
    }
    descs.sort_by(|a, b| id_cmp(&a.0.id, &b.0.id));

    let mut main = Vec::new();
    main.extend_from_slice(&slice.to_le_bytes());
    main.extend_from_slice(&(descs.len() as u32).to_le_bytes());
    for (d, _) in &descs {
        main.extend_from_slice(&d.id);
    }
    let set_id = md5(&main);

    let mut critical = Vec::new();
    critical.extend(packet(&set_id, T_MAIN, &main));
    critical.extend(packet(&set_id, T_CREATOR, &pad4(b"ArchiveCraft 0.1".to_vec())));
    for (d, _) in &descs {
        let mut body = Vec::new();
        body.extend_from_slice(&d.id);
        body.extend_from_slice(&d.md5);
        body.extend_from_slice(&d.md5_16k);
        body.extend_from_slice(&d.len.to_le_bytes());
        body.extend(pad4(d.name.as_bytes().to_vec()));
        critical.extend(packet(&set_id, T_DESC, &body));
        let mut ifsc = Vec::new();
        ifsc.extend_from_slice(&d.id);
        for (h, c) in &checks[&d.id] {
            ifsc.extend_from_slice(h);
            ifsc.extend_from_slice(&c.to_le_bytes());
        }
        critical.extend(packet(&set_id, T_IFSC, &ifsc));
    }

    // Recovery slices.
    let n_input: u64 = descs.iter().map(|(d, _)| slice_count(d.len, slice)).sum();
    let n_rec = ((n_input as f64 * percent as f64 / 100.0).ceil() as usize).clamp(1, 32768);
    let base = bases(n_input as usize);
    let mut rec: Vec<Vec<u8>> = vec![vec![0u8; slice as usize]; n_rec];
    ctl.stage("Writing recovery data");
    ctl.set_totals(n_input, total);
    let threads = archivecraft_model::default_threads().max(1) as usize;
    let mut gi = 0usize;
    for (d, p) in &descs {
        let mut f = File::open(p)?;
        for i in 0..slice_count(d.len, slice) {
            ctl.check()?;
            let data = read_slice(&mut f, i, slice, d.len)?;
            let b = base[gi];
            let chunk = n_rec.div_ceil(threads);
            std::thread::scope(|s| {
                for (ci, part) in rec.chunks_mut(chunk).enumerate() {
                    let data = &data;
                    s.spawn(move || {
                        for (k, r) in part.iter_mut().enumerate() {
                            let e = (ci * chunk + k) as u32;
                            mul_add(r, data, pow(b, e));
                        }
                    });
                }
            });
            ctl.add_bytes((d.len - i * slice).min(slice));
            gi += 1;
        }
    }

    let base_path = base_name(files);
    let index = PathBuf::from(format!("{}.par2", base_path.display()));
    let vol = PathBuf::from(format!("{}.vol000+{:03}.par2", base_path.display(), n_rec));
    File::create(&index)?.write_all(&critical)?;
    let mut w = std::io::BufWriter::new(File::create(&vol)?);
    for (e, r) in rec.iter().enumerate() {
        let mut body = Vec::with_capacity(4 + r.len());
        body.extend_from_slice(&(e as u32).to_le_bytes());
        body.extend_from_slice(r);
        w.write_all(&packet(&set_id, T_RECV, &body))?;
    }
    w.write_all(&critical)?;
    w.flush()?;
    Ok(vec![index, vol])
}

fn parse_into(data: &[u8], set: &mut Set) {
    let mut pos = 0usize;
    while pos + 64 <= data.len() {
        if &data[pos..pos + 8] != MAGIC {
            pos += 4;
            continue;
        }
        let len = u64::from_le_bytes(data[pos + 8..pos + 16].try_into().unwrap()) as usize;
        if len < 64 || pos + len > data.len() || !len.is_multiple_of(4) {
            pos += 4;
            continue;
        }
        let hash: [u8; 16] = data[pos + 16..pos + 32].try_into().unwrap();
        if md5(&data[pos + 32..pos + len]) != hash {
            pos += 4; // damaged packet
            continue;
        }
        let set_id: [u8; 16] = data[pos + 32..pos + 48].try_into().unwrap();
        let kind: [u8; 16] = data[pos + 48..pos + 64].try_into().unwrap();
        let body = &data[pos + 64..pos + len];
        if set.set_id == [0; 16] || set.set_id == set_id {
            match &kind {
                k if k == T_MAIN && body.len() >= 12 => {
                    set.set_id = set_id;
                    set.slice = u64::from_le_bytes(body[0..8].try_into().unwrap());
                    let n = u32::from_le_bytes(body[8..12].try_into().unwrap()) as usize;
                    set.order = (0..n).filter_map(|i| body.get(12 + i * 16..28 + i * 16)).map(|s| s.try_into().unwrap()).collect();
                }
                k if k == T_DESC && body.len() >= 56 => {
                    let id: [u8; 16] = body[0..16].try_into().unwrap();
                    if !set.files.iter().any(|f| f.id == id) {
                        let name = String::from_utf8_lossy(&body[56..]).trim_end_matches('\0').to_string();
                        set.files.push(FileDesc {
                            id,
                            md5: body[16..32].try_into().unwrap(),
                            md5_16k: body[32..48].try_into().unwrap(),
                            len: u64::from_le_bytes(body[48..56].try_into().unwrap()),
                            name,
                        });
                    }
                }
                k if k == T_IFSC && body.len() >= 16 => {
                    let id: [u8; 16] = body[0..16].try_into().unwrap();
                    let list = body[16..].as_chunks::<20>().0.iter().map(|c| (c[0..16].try_into().unwrap(), u32::from_le_bytes(c[16..20].try_into().unwrap()))).collect();
                    set.checks.insert(id, list);
                }
                k if k == T_RECV && body.len() >= 4 => {
                    let e = u32::from_le_bytes(body[0..4].try_into().unwrap());
                    set.recovery.entry(e).or_insert_with(|| body[4..].to_vec());
                }
                _ => {}
            }
        }
        pos += len;
    }
}

fn load(index: &Path) -> Result<(Set, PathBuf)> {
    let dir = index.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = index.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let prefix = stem.trim_end_matches(".par2").to_string();
    let mut set = Set::default();
    let mut names: Vec<PathBuf> = fs::read_dir(if dir.as_os_str().is_empty() { Path::new(".") } else { &dir })?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let n = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            n.starts_with(&prefix) && n.to_lowercase().ends_with(".par2")
        })
        .collect();
    names.sort();
    for p in names {
        if let Ok(data) = fs::read(&p) {
            parse_into(&data, &mut set);
        }
    }
    if set.slice == 0 || set.order.is_empty() {
        return Err(Error::Corrupt("the recovery data is unreadable".into()));
    }
    Ok((set, dir))
}

#[derive(Debug, Clone, Default)]
pub struct VerifyReport {
    pub files: Vec<(String, bool)>,
    pub damaged_slices: usize,
    pub recovery_slices: usize,
    pub repairable: bool,
}

/// Per file: which slices are damaged (by global index).
/// Damaged slice indexes, and for each file its name and whether it is whole.
type Damage = (Vec<usize>, Vec<(String, bool)>);

fn damaged(set: &Set, dir: &Path) -> Result<Damage> {
    let mut bad = Vec::new();
    let mut files = Vec::new();
    let mut gi = 0usize;
    for id in &set.order {
        let d = set.files.iter().find(|f| &f.id == id).ok_or_else(|| Error::Corrupt("recovery data is missing a file description".into()))?;
        let n = slice_count(d.len, set.slice) as usize;
        let list = set.checks.get(id);
        let path = dir.join(&d.name);
        let mut ok = true;
        match (File::open(&path), list) {
            (Ok(mut f), Some(list)) => {
                let actual = f.metadata()?.len();
                for i in 0..n {
                    let data = read_slice(&mut f, i as u64, set.slice, actual.min(d.len))?;
                    let good = list.get(i).is_some_and(|(h, c)| *h == md5(&data) && *c == crc32fast::hash(&data)) && (i as u64 * set.slice) < actual;
                    if !good {
                        bad.push(gi + i);
                        ok = false;
                    }
                }
                if actual != d.len {
                    ok = false;
                }
            }
            _ => {
                bad.extend(gi..gi + n);
                ok = false;
            }
        }
        files.push((d.name.clone(), ok));
        gi += n;
    }
    Ok((bad, files))
}

pub fn verify(index: &Path) -> Result<VerifyReport> {
    let (set, dir) = load(index)?;
    let (bad, files) = damaged(&set, &dir)?;
    Ok(VerifyReport { damaged_slices: bad.len(), recovery_slices: set.recovery.len(), repairable: bad.len() <= set.recovery.len(), files })
}

/// Rebuild damaged files. Repaired copies go to `out_dir` (same names); returns them.
pub fn repair(index: &Path, out_dir: &Path, ctl: &Ctl) -> Result<Vec<PathBuf>> {
    let (set, dir) = load(index)?;
    ctl.stage("Checking against the recovery data");
    let (bad, files) = damaged(&set, &dir)?;
    if bad.is_empty() {
        return Ok(Vec::new());
    }
    if bad.len() > set.recovery.len() {
        return Err(Error::Corrupt(format!("{} damaged blocks but only {} recovery blocks; not enough to repair", bad.len(), set.recovery.len())));
    }
    let slice = set.slice as usize;
    let descs: Vec<&FileDesc> = set.order.iter().map(|id| set.files.iter().find(|f| &f.id == id).unwrap()).collect();
    let total: usize = descs.iter().map(|d| slice_count(d.len, set.slice) as usize).sum();
    let base = bases(total);
    let exps: Vec<u32> = set.recovery.keys().copied().take(bad.len()).collect();
    // Syndromes: recovery minus the contribution of every good slice.
    let mut syn: Vec<Vec<u8>> = exps.iter().map(|e| set.recovery[e].clone()).collect();
    let bad_set: std::collections::HashSet<usize> = bad.iter().copied().collect();
    ctl.stage("Rebuilding damaged blocks");
    ctl.set_totals(total as u64, 0);
    let mut gi = 0usize;
    for d in &descs {
        let n = slice_count(d.len, set.slice);
        let mut f = File::open(dir.join(&d.name)).ok();
        let actual = f.as_ref().and_then(|f| f.metadata().ok()).map(|m| m.len()).unwrap_or(0).min(d.len);
        for i in 0..n {
            ctl.check()?;
            ctl.item(&d.name, 0);
            if !bad_set.contains(&gi) {
                let data = read_slice(f.as_mut().unwrap(), i, set.slice, actual)?;
                for (k, e) in exps.iter().enumerate() {
                    mul_add(&mut syn[k], &data, pow(base[gi], *e));
                }
            }
            gi += 1;
        }
    }
    let m: Vec<Vec<u16>> = exps.iter().map(|e| bad.iter().map(|&j| pow(base[j], *e)).collect()).collect();
    let mi = invert(m).ok_or_else(|| Error::Corrupt("the recovery blocks cannot solve this damage".into()))?;
    let mut fixed: std::collections::HashMap<usize, Vec<u8>> = std::collections::HashMap::new();
    for (j, &gidx) in bad.iter().enumerate() {
        let mut out = vec![0u8; slice];
        for (k, s) in syn.iter().enumerate() {
            mul_add(&mut out, s, mi[j][k]);
        }
        fixed.insert(gidx, out);
    }
    // Write repaired files.
    fs::create_dir_all(out_dir)?;
    let mut written = Vec::new();
    let mut gi = 0usize;
    for (fi, d) in descs.iter().enumerate() {
        let n = slice_count(d.len, set.slice);
        if files[fi].1 {
            gi += n as usize;
            continue;
        }
        let src_path = dir.join(&d.name);
        let mut src = File::open(&src_path).ok();
        let actual = src.as_ref().and_then(|f| f.metadata().ok()).map(|m| m.len()).unwrap_or(0).min(d.len);
        let out_path = out_dir.join(&d.name);
        let tmp = out_dir.join(format!(".{}.acfix", d.name));
        {
            let mut w = std::io::BufWriter::new(File::create(&tmp)?);
            for i in 0..n {
                let data = match fixed.get(&gi) {
                    Some(v) => v.clone(),
                    None => read_slice(src.as_mut().unwrap(), i, set.slice, actual)?,
                };
                let real = ((d.len - i * set.slice).min(set.slice)) as usize;
                w.write_all(&data[..real])?;
                gi += 1;
            }
            w.flush()?;
        }
        let mut check = Md5::new();
        let mut r = BufReader::new(File::open(&tmp)?);
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let k = r.read(&mut buf)?;
            if k == 0 {
                break;
            }
            check.update(&buf[..k]);
        }
        let got: [u8; 16] = check.finalize().into();
        if got != d.md5 {
            let _ = fs::remove_file(&tmp);
            return Err(Error::Corrupt(format!("{} could not be rebuilt correctly", d.name)));
        }
        fs::rename(&tmp, &out_path)?;
        written.push(out_path);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_math() {
        for a in [1u16, 2, 3, 1234, 65535] {
            assert_eq!(mul(a, inv(a)), 1);
        }
        assert_eq!(pow(2, 16), mul(pow(2, 8), pow(2, 8)));
        let b = bases(5);
        assert_eq!(b[0], 2); // 2^1
        assert_eq!(b[1], 4); // 2^2
        assert_eq!(b[2], 16); // 2^4 (3 is skipped)
    }

    #[test]
    fn repair_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.bin");
        let data: Vec<u8> = (0..200_000u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect();
        fs::write(&p, &data).unwrap();
        let ctl = Ctl::default();
        let made = create(std::slice::from_ref(&p), 5, &ctl).unwrap();
        assert_eq!(made.len(), 2);
        assert_eq!(verify(&made[0]).unwrap().damaged_slices, 0);
        // Damage two separate places.
        let mut bad = data.clone();
        for b in &mut bad[1000..1600] {
            *b ^= 0x5A;
        }
        for b in &mut bad[150_000..150_010] {
            *b = 0;
        }
        fs::write(&p, &bad).unwrap();
        let v = verify(&made[0]).unwrap();
        assert!(v.damaged_slices >= 2 && v.repairable);
        let out = repair(&made[0], dir.path(), &ctl).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(fs::read(&p).unwrap(), data);
    }
}
