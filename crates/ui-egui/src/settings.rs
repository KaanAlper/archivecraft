//! Saved preferences, stored by craft-settings in ArchiveCraft's `config.toml`.

use archivecraft_engine::integration::Targets;
use archivecraft_engine::model::{Format, Overwrite, PathMode};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum ListKind {
    #[default]
    Details,
    Names,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum ViewerKind {
    #[default]
    Internal,
    SystemApp,
    External,
    Ask,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Interface {
    pub theme: ThemeMode,
    pub large_buttons: bool,
    pub button_labels: bool,
    pub guide_on_start: bool,
    pub show_comment_on_open: bool,
    pub full_path_in_title: bool,
    pub keep_history: bool,
}

impl Default for Interface {
    fn default() -> Self {
        Interface {
            theme: ThemeMode::System,
            large_buttons: true,
            button_labels: true,
            guide_on_start: false,
            show_comment_on_open: true,
            full_path_in_title: false,
            keep_history: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct System {
    /// Worker threads for packing (7z LZMA2, recovery data).
    pub threads: u32,
    pub log_problems_to_file: bool,
    pub log_limit_kb: u32,
}

impl Default for System {
    fn default() -> Self {
        System { threads: archivecraft_engine::model::default_threads(), log_problems_to_file: false, log_limit_kb: 1000 }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Compression {
    pub format: Format,
    pub level: u32,
    /// Split sizes offered in the Add window, in bytes.
    pub volume_presets: Vec<u64>,
    pub recovery_percent: u32,
    pub store_patterns: String,
}

impl Default for Compression {
    fn default() -> Self {
        Compression {
            format: Format::Zip,
            level: 5,
            volume_presets: vec![10 << 20, 100 << 20, 700 << 20, 4092 << 20],
            recovery_percent: 3,
            store_patterns: archivecraft_engine::model::default_store_patterns().join(" "),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Paths {
    pub start_folder: Option<PathBuf>,
    pub reopen_last_folder: bool,
    pub last_folder: Option<PathBuf>,
    pub archives_folder: Option<PathBuf>,
    pub extract_folder: Option<PathBuf>,
    /// Unpack into a folder named after the archive inside the default extract folder.
    pub add_archive_name: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct ListPrefs {
    pub kind: ListKind,
    pub archives_first: bool,
    pub exact_sizes: bool,
    pub show_seconds: bool,
    pub stripes: bool,
    pub single_click_opens: bool,
    pub show_hidden: bool,
}

impl Default for ListPrefs {
    fn default() -> Self {
        ListPrefs { kind: ListKind::Details, archives_first: true, exact_sizes: false, show_seconds: false, stripes: true, single_click_opens: false, show_hidden: false }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Viewer {
    pub kind: ViewerKind,
    pub word_wrap: bool,
    pub detect_encoding: bool,
    pub external_program: String,
}

impl Default for Viewer {
    fn default() -> Self {
        Viewer { kind: ViewerKind::Internal, word_wrap: true, detect_encoding: true, external_program: String::new() }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Security {
    pub block_types: bool,
    pub blocked_patterns: String,
    pub default_overwrite: Overwrite,
}

impl Default for Security {
    fn default() -> Self {
        Security { block_types: false, blocked_patterns: "*.exe *.com *.bat *.cmd *.scr *.msi *.lnk *.desktop".into(), default_overwrite: Overwrite::Ask }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct IntegrationPrefs {
    pub targets: Targets,
    /// The one-time "add ArchiveCraft to the right-click menu?" question was answered.
    pub offered: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Bookmark {
    pub name: String,
    pub path: PathBuf,
    /// Folder inside an archive, if the bookmark points into one.
    pub inner: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Saved {
    pub recent_archives: Vec<PathBuf>,
    pub bookmarks: Vec<Bookmark>,
    pub path_mode: PathMode,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Prefs {
    pub interface: Interface,
    pub system: System,
    pub compression: Compression,
    pub paths: Paths,
    pub list: ListPrefs,
    pub viewer: Viewer,
    pub security: Security,
    pub integration: IntegrationPrefs,
    pub saved: Saved,
}

impl craft_settings::Settings for Prefs {
    const APP_ID: &'static str = "archivecraft";
    const SCHEMA_VERSION: u32 = 1;

    fn migrate(_from: u32, _doc: &mut craft_settings::DocumentMut) -> Result<(), craft_settings::MigrateError> {
        Ok(())
    }
}

pub const MAX_RECENT: usize = 12;

impl Prefs {
    pub fn remember(&mut self, archive: PathBuf) {
        if !self.interface.keep_history {
            return;
        }
        self.saved.recent_archives.retain(|p| p != &archive);
        self.saved.recent_archives.insert(0, archive);
        self.saved.recent_archives.truncate(MAX_RECENT);
    }
}
