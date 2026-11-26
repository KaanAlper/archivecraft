//! Small helpers: name patterns, safe paths, readable sizes and dates.

use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// `*` and `?` matching, ASCII case-insensitive.
pub fn wildcard(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Split a list typed by a person ("*.jpg *.png; notes*") into patterns.
pub fn parse_patterns(s: &str) -> Vec<String> {
    s.split(|c: char| c.is_whitespace() || c == ';' || c == ',')
        .filter(|p| !p.is_empty())
        .map(|p| p.to_string())
        .collect()
}

pub fn join_patterns(p: &[String]) -> String {
    p.join(" ")
}

/// A pattern with a `/` matches the whole archive path, otherwise just the last name.
pub fn matches_any(patterns: &[String], path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    patterns.iter().any(|p| {
        let p = p.replace('\\', "/");
        if p.contains('/') { wildcard(&p, path) } else { wildcard(&p, name) }
    })
}

/// Turn an archive item name into a relative path that stays inside the destination folder.
/// Returns `None` (the item is refused) for absolute paths (leading `/` or `\`, or a drive
/// letter such as `C:`), for any `..` that would climb above the destination, for NUL bytes,
/// and for names that are empty after cleaning. A `..` that stays inside (`a/../b`) is resolved.
pub fn safe_relative(name: &str) -> Option<PathBuf> {
    if name.contains('\0') {
        return None;
    }
    let cleaned = name.replace('\\', "/");
    if cleaned.starts_with('/') {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for (i, part) in cleaned.split('/').enumerate() {
        let b = part.as_bytes();
        if i == 0 && b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
            return None; // drive letter
        }
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    let mut out = PathBuf::new();
    for p in &parts {
        out.push(p);
    }
    // Double-check with the platform's own parser.
    if out.as_os_str().is_empty() || out.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(out)
}

/// `name.txt` → `name (2).txt`, `name (3).txt`, … whichever does not exist yet.
pub fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = path.extension().map(|s| format!(".{}", s.to_string_lossy())).unwrap_or_default();
    for n in 2..100_000 {
        let p = parent.join(format!("{stem} ({n}){ext}"));
        if !p.exists() {
            return p;
        }
    }
    path.to_path_buf()
}

/// "1.4 MB" style, or exact digits with thin grouping.
pub fn human_size(bytes: u64, exact: bool) -> String {
    if exact {
        return group_digits(bytes);
    }
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 { format!("{bytes} B") } else { format!("{v:.1} {}", UNITS[u]) }
}

pub fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Parse "700m", "4.5 GB", "100k", "12345" into bytes.
pub fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase().replace(' ', "");
    if s.is_empty() {
        return None;
    }
    let idx = s.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(s.len());
    let (num, unit) = s.split_at(idx);
    let v: f64 = num.parse().ok()?;
    let mul: f64 = match unit.trim_end_matches('b').trim_end_matches('i') {
        "" => 1.0,
        "k" => 1024.0,
        "m" => 1024.0 * 1024.0,
        "g" => 1024.0 * 1024.0 * 1024.0,
        "t" => 1024f64.powi(4),
        _ => return None,
    };
    let bytes = (v * mul).round();
    if bytes < 1.0 { None } else { Some(bytes as u64) }
}

pub fn format_time(t: Option<SystemTime>, seconds: bool) -> String {
    let Some(t) = t else { return String::new() };
    let dt: chrono::DateTime<chrono::Local> = t.into();
    if seconds { dt.format("%Y-%m-%d %H:%M:%S").to_string() } else { dt.format("%Y-%m-%d %H:%M").to_string() }
}

pub fn unix_secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn from_unix_secs(s: i64) -> Option<SystemTime> {
    if s >= 0 {
        UNIX_EPOCH.checked_add(Duration::from_secs(s as u64))
    } else {
        UNIX_EPOCH.checked_sub(Duration::from_secs(s.unsigned_abs()))
    }
}

/// A friendly file type text from the name.
pub fn type_label(name: &str, is_dir: bool) -> String {
    if is_dir {
        return "Folder".into();
    }
    if let Some(f) = crate::format::Format::from_name(name) {
        return format!("{} package", f.label());
    }
    match Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()) {
        Some(e) => {
            let kind = match e.as_str() {
                "txt" | "md" | "log" => "Text",
                "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "ico" => "Picture",
                "mp3" | "ogg" | "flac" | "wav" => "Sound",
                "mp4" | "mkv" | "webm" | "avi" => "Video",
                "pdf" => "PDF document",
                "rs" | "c" | "h" | "py" | "js" | "ts" | "toml" | "json" | "yaml" | "yml" | "xml" | "html" | "css" => "Source text",
                "exe" | "dll" | "so" | "dylib" | "bin" => "Program file",
                "par2" => "Recovery data",
                _ => "",
            };
            if kind.is_empty() { format!("{} file", e.to_uppercase()) } else { kind.to_string() }
        }
        None => "File".into(),
    }
}

/// `"x.zip.001"` → `"x.zip"`; other names unchanged.
pub fn strip_volume_suffix(name: &str) -> &str {
    if name.len() > 4 {
        let (head, tail) = name.split_at(name.len() - 4);
        if tail.starts_with('.') && tail[1..].chars().all(|c| c.is_ascii_digit()) {
            return head;
        }
    }
    name
}

pub fn is_first_volume(path: &Path) -> bool {
    path.to_string_lossy().ends_with(".001")
}

pub fn part_name(base: &Path, index: usize) -> PathBuf {
    PathBuf::from(format!("{}.{:03}", base.display(), index))
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wildcards() {
        assert!(wildcard("*.TXT", "notes.txt"));
        assert!(wildcard("a?c", "abc"));
        assert!(!wildcard("*.zip", "a.zipx"));
        assert!(matches_any(&["docs/*.md".into()], "docs/x.md"));
        assert!(!matches_any(&["docs/*.md".into()], "x.md"));
    }
    #[test]
    fn safe_paths() {
        assert_eq!(safe_relative("a/b.txt"), Some(PathBuf::from("a").join("b.txt")));
        assert_eq!(safe_relative("/abs/file"), None);
        assert_eq!(safe_relative("\\\\server\\share"), None);
        assert_eq!(safe_relative("../../up"), None);
        assert_eq!(safe_relative("a/../../up"), None);
        assert_eq!(safe_relative("a/.."), None);
        assert_eq!(safe_relative("a/../b.txt"), Some(PathBuf::from("b.txt")));
        assert_eq!(safe_relative("./a/./b"), Some(PathBuf::from("a").join("b")));
        assert_eq!(safe_relative("C:\\win\\x"), None);
        assert_eq!(safe_relative("c:x"), None);
        assert_eq!(safe_relative(""), None);
    }
    #[test]
    fn sizes() {
        assert_eq!(parse_size("700m"), Some(700 * 1024 * 1024));
        assert_eq!(parse_size("1.5 KB"), Some(1536));
        assert_eq!(group_digits(78483), "78,483");
        assert_eq!(human_size(2048, false), "2.0 KB");
    }
}
