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
    pub wrap_comment: bool,
    pub always_on_top: bool,
    pub full_path_in_title: bool,
    pub keep_history: bool,
    pub dialog_history: bool,
}

impl Default for Interface {
    fn default() -> Self {
        Interface {
            theme: ThemeMode::System,
            large_buttons: true,
            button_labels: true,
            guide_on_start: false,
            show_comment_on_open: true,
            wrap_comment: true,
            always_on_top: false,
            full_path_in_title: false,
            keep_history: true,
            dialog_history: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct System {
    /// Worker threads for packing (7z LZMA2, recovery data).
    pub threads: u32,
    pub low_priority: bool,
    pub large_pages: bool,
    pub log_problems_to_file: bool,
    pub limit_log_size: bool,
    pub log_limit_kb: u32,
}

impl Default for System {
    fn default() -> Self {
        System {
            threads: archivecraft_engine::model::default_threads(),
            low_priority: false,
            large_pages: false,
            log_problems_to_file: false,
            limit_log_size: true,
            log_limit_kb: 1000,
        }
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

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Paths {
    pub temp_folder: Option<PathBuf>,
    pub temp_removable_only: bool,
    pub start_folder: Option<PathBuf>,
    pub reopen_last_folder: bool,
    pub last_folder: Option<PathBuf>,
    pub archives_folder: Option<PathBuf>,
    pub extract_folder: Option<PathBuf>,
    /// Unpack into a folder named after the archive inside the default extract folder.
    pub add_archive_name: bool,
    pub remove_redundant: bool,
}

impl Default for Paths {
    fn default() -> Self {
        Paths {
            temp_folder: Some(std::env::temp_dir()),
            temp_removable_only: true,
            start_folder: None,
            reopen_last_folder: false,
            last_folder: None,
            archives_folder: None,
            extract_folder: None,
            add_archive_name: false,
            remove_redundant: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct ColumnSet {
    pub size: bool,
    pub packed: bool,
    pub kind: bool,
    pub modified: bool,
    pub crc: bool,
}

impl ColumnSet {
    pub fn disk_default() -> Self {
        ColumnSet { size: true, packed: false, kind: true, modified: true, crc: false }
    }
    pub fn archive_default() -> Self {
        ColumnSet { size: true, packed: true, kind: true, modified: true, crc: true }
    }
}

impl Default for ColumnSet {
    fn default() -> Self {
        Self::archive_default()
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum UnderlineMode {
    #[default]
    Never,
    Hover,
    Always,
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
    pub underline: UnderlineMode,
    pub show_hidden: bool,
    pub disk_cols: ColumnSet,
    pub archive_cols: ColumnSet,
    pub font_size: f32,
    pub font_mono: bool,
    pub font_bold: bool,
}

impl Default for ListPrefs {
    fn default() -> Self {
        ListPrefs {
            kind: ListKind::Details,
            archives_first: true,
            exact_sizes: false,
            show_seconds: false,
            stripes: true,
            single_click_opens: false,
            underline: UnderlineMode::Never,
            show_hidden: false,
            disk_cols: ColumnSet::disk_default(),
            archive_cols: ColumnSet::archive_default(),
            font_size: 13.5,
            font_mono: false,
            font_bold: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Viewer {
    pub kind: ViewerKind,
    pub word_wrap: bool,
    pub detect_encoding: bool,
    pub reuse_window: bool,
    pub unpack_all_for: String,
    pub ignore_modifications_for: String,
    pub external_program: String,
}

impl Default for Viewer {
    fn default() -> Self {
        Viewer {
            kind: ViewerKind::Internal,
            word_wrap: true,
            detect_encoding: true,
            reuse_window: false,
            unpack_all_for: "*.htm *.html".into(),
            ignore_modifications_for: String::new(),
            external_program: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum WipeTemp {
    #[default]
    Never,
    Always,
    EncryptedOnly,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Security {
    pub block_types: bool,
    pub blocked_patterns: String,
    pub default_overwrite: Overwrite,
    pub wipe_temp: WipeTemp,
}

impl Default for Security {
    fn default() -> Self {
        Security { block_types: false, blocked_patterns: "*.exe *.com *.bat *.cmd *.scr *.msi *.lnk *.desktop".into(), default_overwrite: Overwrite::Ask, wipe_temp: WipeTemp::Never }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct ContextItems {
    pub open: bool,
    pub open_sfx: bool,
    pub extract_dialog: bool,
    pub extract_here: bool,
    pub extract_folder: bool,
    pub extract_each_folder: bool,
    pub add_dialog: bool,
    pub add_zip: bool,
    pub test_archive: bool,
    pub convert_archive: bool,
    pub new_archive: bool,
    pub new_zip: bool,
    pub dnd_pack: bool,
    pub dnd_extract: bool,
    pub sfx_local: bool,
    pub sfx_network: bool,
    pub sfx_other: bool,
    pub sfx_cloud: bool,
    pub always_pack_for: String,
}

impl Default for ContextItems {
    fn default() -> Self {
        ContextItems {
            open: true,
            open_sfx: true,
            extract_dialog: true,
            extract_here: true,
            extract_folder: true,
            extract_each_folder: true,
            add_dialog: true,
            add_zip: true,
            test_archive: false,
            convert_archive: false,
            new_archive: true,
            new_zip: true,
            dnd_pack: true,
            dnd_extract: true,
            sfx_local: true,
            sfx_network: true,
            sfx_other: true,
            sfx_cloud: false,
            always_pack_for: "*.iso".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct IntegrationPrefs {
    pub targets: Targets,
    pub desktop_shortcut: bool,
    pub cascaded_menus: bool,
    pub menu_icons: bool,
    pub items: ContextItems,
    /// The one-time "add ArchiveCraft to the right-click menu?" question was answered.
    pub offered: bool,
}

impl Default for IntegrationPrefs {
    fn default() -> Self {
        IntegrationPrefs {
            targets: Targets::default(),
            desktop_shortcut: false,
            cascaded_menus: true,
            menu_icons: true,
            items: ContextItems::default(),
            offered: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Bookmark {
    pub name: String,
    pub path: PathBuf,
    /// Folder inside an archive, if the bookmark points into one.
    pub inner: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct FindDefaults {
    pub names: String,
    pub text: String,
    pub match_case: bool,
    pub all_tables: bool,
    pub hex: bool,
    pub types: String,
    pub subfolders: bool,
    pub in_files: bool,
    pub inside: bool,
    pub skip_locked: bool,
}

impl Default for FindDefaults {
    fn default() -> Self {
        FindDefaults {
            names: "*".into(),
            text: String::new(),
            match_case: false,
            all_tables: false,
            hex: false,
            types: "*".into(),
            subfolders: true,
            in_files: true,
            inside: true,
            skip_locked: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Saved {
    pub recent_archives: Vec<PathBuf>,
    pub bookmarks: Vec<Bookmark>,
    pub path_mode: PathMode,
    pub find: FindDefaults,
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
