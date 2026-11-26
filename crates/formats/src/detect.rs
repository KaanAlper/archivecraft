//! Recognise an archive's format from its first bytes, then from its name.

use archivecraft_model::Format;
use std::io::Read;
use std::path::Path;

/// Recognise a format from its first bytes.
pub fn from_magic(head: &[u8]) -> Option<Format> {
    if head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06") || head.starts_with(b"PK\x07\x08") {
        return Some(Format::Zip);
    }
    if head.starts_with(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        return Some(Format::SevenZ);
    }
    if head.starts_with(&[0x1F, 0x8B]) {
        return Some(Format::TarGz);
    }
    if head.starts_with(&[0xFD, b'7', b'z', b'X', b'Z', 0x00]) {
        return Some(Format::TarXz);
    }
    if head.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
        return Some(Format::TarZst);
    }
    if head.starts_with(b"Rar!\x1A\x07") {
        return Some(Format::Rar);
    }
    if head.len() >= 262 && &head[257..262] == b"ustar" {
        return Some(Format::Tar);
    }
    None
}

/// Detect a file's format: contents first, then name.
pub fn detect(path: &Path) -> Option<Format> {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let mut head = vec![0u8; 512];
    let first = crate::volumes::volume_parts(path).into_iter().next();
    if let Some(Ok(mut f)) = first.map(std::fs::File::open) {
        let mut n = 0;
        while n < head.len() {
            match f.read(&mut head[n..]) {
                Ok(0) | Err(_) => break,
                Ok(k) => n += k,
            }
        }
        head.truncate(n);
        if let Some(f) = from_magic(&head) {
            // A bare .tar file whose name says otherwise is still a tar.
            return Some(f);
        }
        if n > 0 && n < 512 {
            // Too short to be anything we know except by name.
            return Format::from_name(&name);
        }
    }
    Format::from_name(&name)
}
