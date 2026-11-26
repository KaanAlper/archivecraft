//! RAR is read-only. To test reading without any RAR writer, this builds a minimal RAR 5
//! archive (stored, uncompressed items) byte by byte from the published format description:
//! signature, main header, file headers with data, end-of-archive header.
#![cfg(feature = "rar")]

use archivecraft_formats as formats;
use archivecraft_model::Format;

fn vint(mut v: u64, out: &mut Vec<u8>) {
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

/// One block: CRC32 of (size field + header), the size field, then the header.
fn block(header: &[u8]) -> Vec<u8> {
    let mut sized = Vec::new();
    vint(header.len() as u64, &mut sized);
    sized.extend_from_slice(header);
    let mut out = crc32fast::hash(&sized).to_le_bytes().to_vec();
    out.extend(sized);
    out
}

fn file_block(name: &str, data: &[u8], dir: bool, mtime: u32) -> Vec<u8> {
    let mut h = Vec::new();
    vint(2, &mut h); // type: file
    vint(if dir { 0 } else { 0x0002 }, &mut h); // header flags: data area follows
    if !dir {
        vint(data.len() as u64, &mut h); // data size
    }
    vint(if dir { 0x0001 | 0x0002 } else { 0x0002 | 0x0004 }, &mut h); // dir | time | crc
    vint(if dir { 0 } else { data.len() as u64 }, &mut h); // unpacked size
    vint(if dir { 0o40755 } else { 0o100644 }, &mut h); // attributes (Unix)
    h.extend_from_slice(&mtime.to_le_bytes());
    if !dir {
        h.extend_from_slice(&crc32fast::hash(data).to_le_bytes());
    }
    vint(0, &mut h); // compression: version 0, stored
    vint(1, &mut h); // host OS: Unix
    vint(name.len() as u64, &mut h);
    h.extend_from_slice(name.as_bytes());
    let mut out = block(&h);
    if !dir {
        out.extend_from_slice(data);
    }
    out
}

fn sample_rar() -> Vec<u8> {
    let mut out = b"Rar!\x1A\x07\x01\x00".to_vec();
    out.extend(block(&[1, 0, 0])); // main header: type 1, no flags, no archive flags
    out.extend(file_block("folder", b"", true, 1_700_000_000));
    out.extend(file_block("folder/hello.txt", b"Hello, RAR reader.\n", false, 1_700_000_000));
    out.extend(file_block("top.bin", &[7u8; 300], false, 1_700_000_100));
    out.extend(block(&[5, 0, 0])); // end of archive
    out
}

#[test]
fn reads_list_extracts_and_tests_rar() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("sample.rar");
    std::fs::write(&p, sample_rar()).unwrap();
    assert_eq!(formats::detect(&p).unwrap(), Format::Rar);
    let l = formats::list(&p, None).unwrap();
    let names: Vec<&str> = l.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(names, ["folder", "folder/hello.txt", "top.bin"]);
    assert_eq!(l.entries[1].size, 19);
    assert_eq!(formats::read_one(&p, "folder/hello.txt", None, 1 << 20).unwrap(), b"Hello, RAR reader.\n");
    let mut seen = Vec::new();
    let mut want = |_: &archivecraft_model::Entry| true;
    let mut f = |e: &archivecraft_model::Entry, r: &mut dyn std::io::Read| -> archivecraft_model::Result<()> {
        let mut b = Vec::new();
        r.read_to_end(&mut b)?;
        seen.push((e.path.clone(), b.len()));
        Ok(())
    };
    formats::visit(&p, None, &mut want, &mut f).unwrap();
    assert_eq!(seen, vec![("folder".to_string(), 0), ("folder/hello.txt".to_string(), 19), ("top.bin".to_string(), 300)]);
}

#[test]
fn rar_is_never_written() {
    let dir = tempfile::tempdir().unwrap();
    let opts = archivecraft_model::CreateOptions { format: Format::Rar, ..Default::default() };
    let r = formats::open_sink(&dir.path().join("x.rar"), &opts, Default::default());
    assert!(matches!(r, Err(archivecraft_model::Error::Unsupported(_))));
}
