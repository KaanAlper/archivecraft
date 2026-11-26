//! Shared data: archive items, option sets and the progress/cancel channel.

use crate::error::{Error, Result};
use crate::format::Format;
use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// One item inside an archive.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Path inside the archive with `/` separators, no leading `./` or `/`, no trailing `/`.
    pub path: String,
    pub is_dir: bool,
    /// Links and other special items are listed but not extracted.
    pub is_link: bool,
    pub size: u64,
    /// Bytes the item takes inside the archive, when the format records it per item.
    pub packed: Option<u64>,
    pub modified: Option<SystemTime>,
    pub encrypted: bool,
    pub crc: Option<u32>,
    pub method: String,
    pub mode: Option<u32>,
    /// The name exactly as stored, before clean-up. Extraction checks this one, so a stored
    /// leading slash or drive letter is refused rather than silently made relative.
    pub raw: String,
}

impl Entry {
    pub fn new(path: &str, is_dir: bool) -> Self {
        Entry {
            path: normalize_name(path),
            is_dir,
            is_link: false,
            size: 0,
            packed: None,
            modified: None,
            encrypted: false,
            crc: None,
            method: String::new(),
            mode: None,
            raw: path.to_string(),
        }
    }
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

/// Make an archive item name uniform: `/` separators, no `./` prefix, no surrounding slashes.
pub fn normalize_name(p: &str) -> String {
    let p = p.replace('\\', "/");
    let parts: Vec<&str> = p.split('/').filter(|s| !s.is_empty() && *s != ".").collect();
    parts.join("/")
}

/// Everything known about an opened archive.
#[derive(Debug, Clone)]
pub struct Listing {
    pub format: Format,
    pub entries: Vec<Entry>,
    pub comment: Option<String>,
    pub solid: bool,
    pub names_encrypted: bool,
    pub volumes: usize,
    pub archive_size: u64,
    pub methods: Vec<String>,
}

impl Listing {
    pub fn file_count(&self) -> usize {
        self.entries.iter().filter(|e| !e.is_dir).count()
    }
    pub fn folder_count(&self) -> usize {
        let mut dirs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for e in &self.entries {
            if e.is_dir {
                dirs.insert(e.path.clone());
            }
            let mut p = e.path.as_str();
            while let Some(i) = p.rfind('/') {
                p = &p[..i];
                dirs.insert(p.to_string());
            }
        }
        dirs.len()
    }
    pub fn total_size(&self) -> u64 {
        self.entries.iter().map(|e| e.size).sum()
    }
    pub fn any_encrypted(&self) -> bool {
        self.names_encrypted || self.entries.iter().any(|e| e.encrypted)
    }
    /// Packed size of all items: per-item numbers when known, otherwise the file size.
    pub fn packed_size(&self) -> u64 {
        let per: u64 = self.entries.iter().filter_map(|e| e.packed).sum();
        if per > 0 { per } else { self.archive_size }
    }
    /// Packed / unpacked, as a percentage (0..=100+).
    pub fn ratio_percent(&self) -> u32 {
        let total = self.total_size();
        if total == 0 {
            return 100;
        }
        ((self.packed_size() as f64 / total as f64) * 100.0).round() as u32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PathMode {
    /// Keep paths below the folder the item was picked from.
    #[default]
    Relative,
    /// Only the file name.
    NoPaths,
    /// The whole path, without the drive or root.
    Full,
}

impl PathMode {
    pub const ALL: [PathMode; 3] = [PathMode::Relative, PathMode::NoPaths, PathMode::Full];
    pub fn label(self) -> &'static str {
        match self {
            PathMode::Relative => "Keep folders below the chosen items",
            PathMode::NoPaths => "Names only, drop folders",
            PathMode::Full => "Whole path (without drive or root)",
        }
    }
}

/// What to do when adding to an archive that already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum UpdateMode {
    #[default]
    AddReplace,
    Update,
    Freshen,
    Ask,
    SkipExisting,
    Sync,
}

impl UpdateMode {
    pub const ALL: [UpdateMode; 6] = [
        UpdateMode::AddReplace,
        UpdateMode::Update,
        UpdateMode::Freshen,
        UpdateMode::Ask,
        UpdateMode::SkipExisting,
        UpdateMode::Sync,
    ];
    pub fn label(self) -> &'static str {
        match self {
            UpdateMode::AddReplace => "Add everything, replace same names",
            UpdateMode::Update => "Add new, replace only if newer",
            UpdateMode::Freshen => "Refresh items already inside",
            UpdateMode::Ask => "Ask me for each item already inside",
            UpdateMode::SkipExisting => "Leave items already inside alone",
            UpdateMode::Sync => "Mirror the source (drop missing items)",
        }
    }
}

/// Which source files to include, by modification time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TimeFilter {
    #[default]
    Any,
    LastDay,
    LastWeek,
    LastMonth,
    OlderThanMonth,
}

impl TimeFilter {
    pub const ALL: [TimeFilter; 5] = [
        TimeFilter::Any,
        TimeFilter::LastDay,
        TimeFilter::LastWeek,
        TimeFilter::LastMonth,
        TimeFilter::OlderThanMonth,
    ];
    pub fn label(self) -> &'static str {
        match self {
            TimeFilter::Any => "Any date",
            TimeFilter::LastDay => "Changed in the last 24 hours",
            TimeFilter::LastWeek => "Changed in the last 7 days",
            TimeFilter::LastMonth => "Changed in the last 30 days",
            TimeFilter::OlderThanMonth => "Not changed for 30 days or more",
        }
    }
    pub fn accepts(self, t: Option<SystemTime>) -> bool {
        let Some(t) = t else { return self == TimeFilter::Any };
        let age = SystemTime::now().duration_since(t).unwrap_or_default().as_secs();
        const DAY: u64 = 86_400;
        match self {
            TimeFilter::Any => true,
            TimeFilter::LastDay => age <= DAY,
            TimeFilter::LastWeek => age <= 7 * DAY,
            TimeFilter::LastMonth => age <= 30 * DAY,
            TimeFilter::OlderThanMonth => age > 30 * DAY,
        }
    }
}

/// Everything the Add window can set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOptions {
    pub format: Format,
    /// 0 = store, 9 = strongest.
    pub level: u32,
    /// Dictionary size in bytes (7z and tar.xz); `None` = the level's default.
    pub dictionary: Option<u32>,
    pub password: Option<String>,
    pub encrypt_names: bool,
    /// Weak old-style ZIP encryption instead of AES-256, for very old tools.
    pub zip_legacy_crypto: bool,
    pub solid: bool,
    /// Split the result into parts of this many bytes.
    pub volume_size: Option<u64>,
    pub path_mode: PathMode,
    pub exclude: Vec<String>,
    pub store_patterns: Vec<String>,
    pub update: UpdateMode,
    pub comment: Option<String>,
    pub store_mtime: bool,
    pub time_filter: TimeFilter,
    pub delete_sources: bool,
    pub test_after: bool,
    /// PAR2 recovery data size as a percentage of the archive; 0 = none.
    pub recovery_percent: u32,
    pub threads: u32,
}

impl Default for CreateOptions {
    fn default() -> Self {
        CreateOptions {
            format: Format::Zip,
            level: 5,
            dictionary: None,
            password: None,
            encrypt_names: false,
            zip_legacy_crypto: false,
            solid: false,
            volume_size: None,
            path_mode: PathMode::Relative,
            exclude: Vec::new(),
            store_patterns: default_store_patterns(),
            update: UpdateMode::AddReplace,
            comment: None,
            store_mtime: true,
            time_filter: TimeFilter::Any,
            delete_sources: false,
            test_after: false,
            recovery_percent: 0,
            threads: default_threads(),
        }
    }
}

pub fn default_threads() -> u32 {
    std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(4)
}

/// Already-compressed kinds of file that gain nothing from compressing again.
pub fn default_store_patterns() -> Vec<String> {
    [
        "*.zip", "*.7z", "*.rar", "*.gz", "*.xz", "*.zst", "*.bz2", "*.jpg", "*.jpeg", "*.png",
        "*.webp", "*.mp3", "*.mp4", "*.mkv",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ExtractUpdate {
    #[default]
    All,
    NewerOnly,
    ExistingOnly,
}

impl ExtractUpdate {
    pub const ALL: [ExtractUpdate; 3] = [ExtractUpdate::All, ExtractUpdate::NewerOnly, ExtractUpdate::ExistingOnly];
    pub fn label(self) -> &'static str {
        match self {
            ExtractUpdate::All => "Unpack everything",
            ExtractUpdate::NewerOnly => "Unpack only new or newer items",
            ExtractUpdate::ExistingOnly => "Only refresh files already on disk",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Overwrite {
    #[default]
    Ask,
    Always,
    Skip,
    Rename,
}

impl Overwrite {
    pub const ALL: [Overwrite; 4] = [Overwrite::Ask, Overwrite::Always, Overwrite::Skip, Overwrite::Rename];
    pub fn label(self) -> &'static str {
        match self {
            Overwrite::Ask => "Ask me each time",
            Overwrite::Always => "Replace without asking",
            Overwrite::Skip => "Keep the file already there",
            Overwrite::Rename => "Keep both (number the new one)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ExtractPaths {
    #[default]
    Relative,
    NoPaths,
}

/// Everything the Extract window can set.
#[derive(Debug, Clone)]
pub struct ExtractOptions {
    pub dest: PathBuf,
    pub update: ExtractUpdate,
    pub overwrite: Overwrite,
    pub keep_broken: bool,
    pub paths: ExtractPaths,
    pub set_mtime: bool,
    /// Name patterns that are never written to disk (security setting).
    pub blocked: Vec<String>,
    /// Only these archive paths (files or folders). Empty = everything.
    pub only: Vec<String>,
    /// Remove this folder prefix from the paths written to disk.
    pub base: String,
}

impl ExtractOptions {
    pub fn to(dest: impl Into<PathBuf>) -> Self {
        ExtractOptions {
            dest: dest.into(),
            update: ExtractUpdate::All,
            overwrite: Overwrite::Ask,
            keep_broken: false,
            paths: ExtractPaths::Relative,
            set_mtime: true,
            blocked: Vec::new(),
            only: Vec::new(),
            base: String::new(),
        }
    }
    pub fn wants(&self, path: &str) -> bool {
        selection_contains(&self.only, path)
    }
}

/// Is `path` one of the selected items or inside a selected folder? Empty selection = all.
pub fn selection_contains(only: &[String], path: &str) -> bool {
    only.is_empty()
        || only.iter().any(|s| {
            let s = s.trim_end_matches('/');
            s.is_empty() || path == s || (path.len() > s.len() && path.starts_with(s) && path.as_bytes()[s.len()] == b'/')
        })
}

// ---------- progress, cancel, questions and problems ----------

#[derive(Debug, Clone, Default)]
pub struct Progress {
    /// What is happening ("adding", "checking", "unpacking").
    pub stage: String,
    /// The archive being worked on.
    pub archive: String,
    pub current: String,
    pub item_done: u64,
    pub item_total: u64,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub done_items: u64,
    pub total_items: u64,
    /// Bytes written to the new archive so far (for the running ratio).
    pub out_bytes: u64,
}

impl Progress {
    pub fn fraction(&self) -> f32 {
        if self.total_bytes > 0 {
            (self.done_bytes as f64 / self.total_bytes as f64).clamp(0.0, 1.0) as f32
        } else if self.total_items > 0 {
            (self.done_items as f64 / self.total_items as f64).clamp(0.0, 1.0) as f32
        } else {
            0.0
        }
    }
    pub fn item_fraction(&self) -> f32 {
        if self.item_total > 0 {
            (self.item_done as f64 / self.item_total as f64).clamp(0.0, 1.0) as f32
        } else {
            0.0
        }
    }
    /// Output so far compared with input so far, in percent.
    pub fn running_ratio(&self) -> Option<u32> {
        if self.done_bytes == 0 || self.out_bytes == 0 {
            return None;
        }
        Some(((self.out_bytes as f64 / self.done_bytes as f64) * 100.0).round() as u32)
    }
}

#[derive(Debug, Clone)]
pub enum Question {
    /// A file is already on disk (extract) or in the archive (add).
    Replace {
        path: String,
        old_size: Option<u64>,
        old_time: Option<SystemTime>,
        new_size: u64,
        new_time: Option<SystemTime>,
    },
    /// A locked item needs a password.
    Password { archive: String, item: String, wrong: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    Yes,
    YesToAll,
    No,
    NoToAll,
    Rename,
    RenameAll,
    Cancel,
    Password(String),
}

pub type Asker = Box<dyn FnMut(Question) -> Answer + Send>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// One problem (or note) found while a job ran.
#[derive(Debug, Clone, PartialEq)]
pub struct Diag {
    pub severity: Severity,
    pub message: String,
    pub archive: String,
}

/// Shared between a running job and whoever watches it: progress, stop and pause flags,
/// questions, and the list of problems found.
pub struct Ctl {
    cancel: AtomicBool,
    pause: AtomicBool,
    pub progress: Mutex<Progress>,
    asker: Mutex<Option<Asker>>,
    pub diags: Mutex<Vec<Diag>>,
    pub started: std::time::Instant,
}

impl Default for Ctl {
    fn default() -> Self {
        Ctl {
            cancel: AtomicBool::new(false),
            pause: AtomicBool::new(false),
            progress: Mutex::new(Progress::default()),
            asker: Mutex::new(None),
            diags: Mutex::new(Vec::new()),
            started: std::time::Instant::now(),
        }
    }
}

impl Ctl {
    pub fn new() -> Arc<Ctl> {
        Arc::new(Ctl::default())
    }
    pub fn with_asker(asker: Asker) -> Arc<Ctl> {
        let c = Ctl::default();
        *c.asker.lock().unwrap() = Some(asker);
        Arc::new(c)
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        self.pause.store(false, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
    pub fn set_paused(&self, p: bool) {
        self.pause.store(p, Ordering::SeqCst);
    }
    pub fn is_paused(&self) -> bool {
        self.pause.load(Ordering::SeqCst)
    }
    /// Stop here if cancelled; wait here while paused.
    pub fn check(&self) -> Result<()> {
        while self.is_paused() && !self.is_cancelled() {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        if self.is_cancelled() { Err(Error::Cancelled) } else { Ok(()) }
    }
    pub fn stage(&self, s: &str) {
        self.progress.lock().unwrap().stage = s.to_string();
    }
    pub fn archive(&self, s: &str) {
        self.progress.lock().unwrap().archive = s.to_string();
    }
    pub fn set_totals(&self, items: u64, bytes: u64) {
        let mut p = self.progress.lock().unwrap();
        p.total_items = items;
        p.total_bytes = bytes;
        p.done_items = 0;
        p.done_bytes = 0;
        p.out_bytes = 0;
    }
    pub fn item(&self, name: &str, size: u64) {
        let mut p = self.progress.lock().unwrap();
        p.current = name.to_string();
        p.done_items += 1;
        p.item_done = 0;
        p.item_total = size;
    }
    pub fn add_bytes(&self, n: u64) {
        let mut p = self.progress.lock().unwrap();
        p.done_bytes += n;
        p.item_done += n;
    }
    pub fn set_out_bytes(&self, n: u64) {
        self.progress.lock().unwrap().out_bytes = n;
    }
    pub fn report(&self, severity: Severity, archive: &str, message: impl Into<String>) {
        self.diags.lock().unwrap().push(Diag { severity, message: message.into(), archive: archive.to_string() });
    }
    pub fn error(&self, archive: &str, message: impl Into<String>) {
        self.report(Severity::Error, archive, message);
    }
    pub fn warn(&self, archive: &str, message: impl Into<String>) {
        self.report(Severity::Warning, archive, message);
    }
    pub fn info(&self, archive: &str, message: impl Into<String>) {
        self.report(Severity::Info, archive, message);
    }
    pub fn error_count(&self) -> usize {
        self.diags.lock().unwrap().iter().filter(|d| d.severity == Severity::Error).count()
    }
    pub fn has_asker(&self) -> bool {
        self.asker.lock().unwrap().is_some()
    }
    /// Ask whoever runs the job. Without anyone to ask: "No" for replace, cancel for passwords.
    pub fn ask(&self, q: Question) -> Answer {
        let mut guard = self.asker.lock().unwrap();
        match guard.as_mut() {
            Some(f) => f(q),
            None => match q {
                Question::Replace { .. } => Answer::No,
                Question::Password { .. } => Answer::Cancel,
            },
        }
    }
}

/// Counts bytes for the progress bar and stops reading when the job is cancelled.
pub struct ProgressReader<'a, R> {
    pub inner: R,
    pub ctl: &'a Ctl,
}

impl<R: Read> Read for ProgressReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Err(e) = self.ctl.check() {
            return Err(io::Error::other(e));
        }
        let n = self.inner.read(buf)?;
        self.ctl.add_bytes(n as u64);
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection() {
        let only = vec!["docs".to_string()];
        assert!(selection_contains(&only, "docs/a.txt"));
        assert!(selection_contains(&only, "docs"));
        assert!(!selection_contains(&only, "docs2/a.txt"));
        assert!(selection_contains(&[], "x"));
        assert_eq!(normalize_name("./a\\b//c/"), "a/b/c");
    }
}
