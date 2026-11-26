//! A quick speed test: pack and unpack the same generated data in each format.

use archivecraft_model::{CreateOptions, Ctl, Format, Result};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct BenchResult {
    pub format: Format,
    pub level: u32,
    pub input: u64,
    pub output: u64,
    /// MB/s
    pub pack_speed: f64,
    pub unpack_speed: f64,
}

/// Data that compresses like a mix of documents and binaries.
pub fn sample_data(size: usize) -> Vec<u8> {
    let words: [&[u8]; 12] = [b"archive ", b"craft ", b"folder ", b"the ", b"data ", b"pack ", b"stream ", b"of ", b"and ", b"bytes ", b"level ", b"\n"];
    let mut out = Vec::with_capacity(size);
    let mut x: u32 = 0x2545_F491;
    while out.len() < size {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        if x.is_multiple_of(7) {
            out.extend_from_slice(&x.to_le_bytes());
        } else {
            out.extend_from_slice(words[(x % 12) as usize]);
        }
    }
    out.truncate(size);
    out
}

pub fn run(format: Format, level: u32, size_mb: usize, threads: u32, ctl: &Ctl) -> Result<BenchResult> {
    let data = sample_data(size_mb * 1024 * 1024);
    let dir = tempfile::tempdir()?;
    let src = dir.path().join("sample.bin");
    std::fs::write(&src, &data)?;
    let archive = dir.path().join(format!("bench.{}", format.extension()));
    let opts = CreateOptions { format, level, threads, store_patterns: Vec::new(), ..CreateOptions::default() };
    ctl.stage("Packing");
    let t0 = Instant::now();
    crate::ops::create(std::slice::from_ref(&src), &archive, &opts, ctl)?;
    let pack = t0.elapsed().as_secs_f64().max(1e-6);
    let output = std::fs::metadata(&archive)?.len();
    ctl.stage("Unpacking");
    let t1 = Instant::now();
    let r = crate::ops::test(&archive, None, ctl)?;
    let unpack = t1.elapsed().as_secs_f64().max(1e-6);
    if r.errors > 0 {
        return Err(archivecraft_model::Error::Corrupt("the speed test read back different data".into()));
    }
    let mb = data.len() as f64 / (1024.0 * 1024.0);
    Ok(BenchResult { format, level, input: data.len() as u64, output, pack_speed: mb / pack, unpack_speed: mb / unpack })
}
