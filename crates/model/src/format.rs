//! Archive formats: names, file endings, what each one can do, and how to recognise them.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Format {
    Zip,
    SevenZ,
    Tar,
    TarGz,
    TarXz,
    TarZst,
    /// Read-only, and only when built with the `rar` feature.
    Rar,
}

impl Format {
    /// Formats this program can write.
    pub const WRITABLE: [Format; 6] = [
        Format::Zip,
        Format::SevenZ,
        Format::Tar,
        Format::TarGz,
        Format::TarXz,
        Format::TarZst,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Format::Zip => "ZIP",
            Format::SevenZ => "7z",
            Format::Tar => "TAR",
            Format::TarGz => "TAR.GZ",
            Format::TarXz => "TAR.XZ",
            Format::TarZst => "TAR.ZST",
            Format::Rar => "RAR",
        }
    }

    /// A longer description used in the path bar and the info window.
    pub fn describe(self) -> &'static str {
        match self {
            Format::Zip => "ZIP package",
            Format::SevenZ => "7z package",
            Format::Tar => "Tape archive (uncompressed)",
            Format::TarGz => "Tape archive, gzip-compressed",
            Format::TarXz => "Tape archive, xz-compressed",
            Format::TarZst => "Tape archive, Zstandard-compressed",
            Format::Rar => "RAR package (read-only)",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Zip => "zip",
            Format::SevenZ => "7z",
            Format::Tar => "tar",
            Format::TarGz => "tar.gz",
            Format::TarXz => "tar.xz",
            Format::TarZst => "tar.zst",
            Format::Rar => "rar",
        }
    }

    pub fn from_label(s: &str) -> Option<Format> {
        let s = s.trim().trim_start_matches('.').to_ascii_lowercase();
        Some(match s.as_str() {
            "zip" => Format::Zip,
            "7z" | "7zip" => Format::SevenZ,
            "tar" => Format::Tar,
            "tar.gz" | "tgz" | "targz" | "gz" => Format::TarGz,
            "tar.xz" | "txz" | "tarxz" | "xz" => Format::TarXz,
            "tar.zst" | "tzst" | "tarzst" | "zst" | "tar.zstd" => Format::TarZst,
            "rar" => Format::Rar,
            _ => return None,
        })
    }

    pub fn can_write(self) -> bool {
        self != Format::Rar
    }
    /// AES-256 password protection is available.
    pub fn supports_password(self) -> bool {
        matches!(self, Format::Zip | Format::SevenZ)
    }
    /// File names can be hidden behind the password.
    pub fn supports_name_encryption(self) -> bool {
        self == Format::SevenZ
    }
    pub fn supports_solid(self) -> bool {
        self == Format::SevenZ
    }
    pub fn supports_dictionary(self) -> bool {
        matches!(self, Format::SevenZ | Format::TarXz)
    }
    pub fn supports_comment(self) -> bool {
        self == Format::Zip
    }
    pub fn supports_levels(self) -> bool {
        self != Format::Tar && self != Format::Rar
    }
    /// Whether this build can read the format at all.
    pub fn readable(self) -> bool {
        self != Format::Rar || cfg!(feature = "rar")
    }
    pub fn is_tar_family(self) -> bool {
        matches!(self, Format::Tar | Format::TarGz | Format::TarXz | Format::TarZst)
    }

    /// Guess the format from the file name only.
    pub fn from_name(name: &str) -> Option<Format> {
        let lower = name.to_ascii_lowercase();
        let lower = crate::util::strip_volume_suffix(&lower);
        let pairs: [(&str, Format); 12] = [
            (".tar.gz", Format::TarGz),
            (".tgz", Format::TarGz),
            (".tar.xz", Format::TarXz),
            (".txz", Format::TarXz),
            (".tar.zst", Format::TarZst),
            (".tzst", Format::TarZst),
            (".tar.zstd", Format::TarZst),
            (".tar", Format::Tar),
            (".zip", Format::Zip),
            (".7z", Format::SevenZ),
            (".rar", Format::Rar),
            (".jar", Format::Zip),
        ];
        pairs
            .iter()
            .find(|(ext, _)| lower.ends_with(ext))
            .map(|(_, f)| *f)
    }
}

/// Does this file name look like an archive we can open?
pub fn looks_like_archive(name: &str) -> bool {
    match Format::from_name(name) {
        Some(f) => f.readable(),
        None => {
            let l = name.to_ascii_lowercase();
            l.ends_with(".001") && Format::from_name(&l[..l.len() - 4]).is_some()
        }
    }
}

/// Compression level names shown to people. Levels go 0 (store) to 9 (smallest).
pub const LEVELS: [(u32, &str); 6] = [
    (0, "Store only"),
    (1, "Quickest"),
    (3, "Quick"),
    (5, "Balanced"),
    (7, "Strong"),
    (9, "Strongest"),
];

pub fn level_name(level: u32) -> &'static str {
    LEVELS
        .iter()
        .rev()
        .find(|(l, _)| *l <= level)
        .map(|(_, n)| *n)
        .unwrap_or("Store only")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names() {
        assert_eq!(Format::from_name("a.tar.gz"), Some(Format::TarGz));
        assert_eq!(Format::from_name("A.ZIP"), Some(Format::Zip));
        assert_eq!(Format::from_name("x.7z.001"), Some(Format::SevenZ));
        assert_eq!(Format::from_name("x.txt"), None);
        assert_eq!(level_name(4), "Quick");
    }
}
