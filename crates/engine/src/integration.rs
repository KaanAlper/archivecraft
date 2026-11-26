//! File-manager integration: right-click actions ("Add to archive…", "Add to <name>.zip",
//! "Extract here", "Extract to <name>/") and which archive types open in ArchiveCraft.
//!
//! Linux: a .desktop entry with actions, plus Nautilus scripts, a Dolphin service menu, Nemo
//! actions and Thunar custom actions. Everything lives in the person's own folders and is
//! removed again by `uninstall`. Windows (context-menu verbs) and macOS (Finder Quick Actions)
//! plug in through the same `Integration` type later.

use archivecraft_model::{Error, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// The command-line switches the right-click actions call.
pub const ARG_ADD_DIALOG: &str = "--add";
pub const ARG_ADD_ZIP: &str = "--add-zip";
pub const ARG_EXTRACT_DIALOG: &str = "--extract";
pub const ARG_EXTRACT_HERE: &str = "--extract-here";
pub const ARG_EXTRACT_FOLDER: &str = "--extract-folder";
pub const ARG_TEST: &str = "--test";
pub const ARG_CONVERT: &str = "--convert";

/// Archive kinds the app can be the default opener for: (label, MIME types, file endings).
pub const ASSOCIATIONS: &[(&str, &[&str], &[&str])] = &[
    ("ZIP", &["application/zip", "application/x-zip", "application/x-zip-compressed"], &["zip"]),
    ("7z", &["application/x-7z-compressed", "application/x-7z"], &["7z"]),
    ("TAR", &["application/x-tar"], &["tar"]),
    ("TAR.GZ", &["application/x-compressed-tar", "application/gzip", "application/x-gzip"], &["tgz", "gz"]),
    ("TAR.XZ", &["application/x-xz-compressed-tar", "application/x-xz"], &["txz", "xz"]),
    ("TAR.ZST", &["application/x-zstd-compressed-tar", "application/zstd"], &["tzst", "zst"]),
    ("RAR", &["application/vnd.rar", "application/x-rar", "application/x-rar-compressed"], &["rar"]),
    ("JAR", &["application/java-archive", "application/x-java-archive"], &["jar"]),
    ("ISO", &["application/x-cd-image", "application/x-iso9660-image", "application/vnd.efi.iso"], &["iso"]),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Targets {
    pub desktop_entry: bool,
    pub nautilus: bool,
    pub dolphin: bool,
    pub nemo: bool,
    pub thunar: bool,
}

impl Targets {
    pub fn all() -> Self {
        Targets { desktop_entry: true, nautilus: true, dolphin: true, nemo: true, thunar: true }
    }
}

/// The file managers found on this computer (and always the app menu entry).
pub fn detect() -> Targets {
    let has = |name: &str| {
        std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|d| d.join(name).is_file()))
    };
    Targets { desktop_entry: true, nautilus: has("nautilus"), dolphin: has("dolphin"), nemo: has("nemo"), thunar: has("thunar") }
}

const NAUTILUS_EXTENSION: &str = r#"# ArchiveCraft right-click submenu for Files (Nautilus), written by ArchiveCraft.
import subprocess
from urllib.parse import unquote, urlparse
from gi.repository import GObject, Nautilus

EXE = "@EXE@"
CASCADED = @CASCADED@
MENU_ICONS = @MENU_ICONS@
ARCHIVES = (".zip", ".jar", ".7z", ".tar", ".gz", ".tgz", ".xz", ".txz", ".zst", ".tzst", ".rar", ".iso", ".001")
PACK_ENTRIES = @PACK_ENTRIES@
ARCHIVE_ENTRIES = @ARCHIVE_ENTRIES@


class ArchiveCraftMenu(GObject.GObject, Nautilus.MenuProvider):
    def _paths(self, files):
        return [unquote(urlparse(f.get_uri()).path) for f in files if f.get_uri_scheme() == "file"]

    def _run(self, _item, arg, paths):
        cmd = [EXE, arg] + paths if arg else [EXE] + paths
        subprocess.Popen(cmd)

    def get_file_items(self, *args):
        files = args[-1]
        paths = self._paths(files)
        if not paths:
            return []
        entries = list(PACK_ENTRIES)
        if all(p.lower().endswith(ARCHIVES) for p in paths):
            entries += list(ARCHIVE_ENTRIES)
        if not entries:
            return []
        icon = "archivecraft" if MENU_ICONS else ""
        if CASCADED:
            top = Nautilus.MenuItem(name="ArchiveCraft::top", label="ArchiveCraft", icon=icon)
            sub = Nautilus.Menu()
            top.set_submenu(sub)
            for key, label, arg in entries:
                item = Nautilus.MenuItem(name="ArchiveCraft::" + key, label=label, icon=icon)
                item.connect("activate", self._run, arg, paths)
                sub.append_item(item)
            return [top]
        out = []
        for key, label, arg in entries:
            item = Nautilus.MenuItem(name="ArchiveCraft::" + key, label=label, icon=icon)
            item.connect("activate", self._run, arg, paths)
            out.append(item)
        return out
"#;

/// ArchiveCraft's own icon (an amber parcel with a teal band), drawn in code as a PNG.
pub fn icon_png(size: u32) -> Vec<u8> {
    let s = size as f32;
    let mut px = vec![0u8; (size * size * 4) as usize];
    let inside = |x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32| {
        let cx = x.clamp(x0 + r, x1 - r);
        let cy = y.clamp(y0 + r, y1 - r);
        (x - cx).powi(2) + (y - cy).powi(2) <= r * r && x >= x0 && x <= x1 && y >= y0 && y <= y1
    };
    for yi in 0..size {
        for xi in 0..size {
            let (x, y) = (xi as f32 + 0.5, yi as f32 + 0.5);
            let (u, v) = (x / s, y / s);
            let mut c: Option<[u8; 4]> = None;
            if inside(x, y, 0.10 * s, 0.30 * s, 0.90 * s, 0.90 * s, 0.07 * s) {
                c = Some([0xE2, 0xA2, 0x3B, 255]);
                if !inside(x, y, 0.13 * s, 0.33 * s, 0.87 * s, 0.87 * s, 0.05 * s) {
                    c = Some([0x6B, 0x45, 0x12, 255]);
                }
            }
            if inside(x, y, 0.06 * s, 0.18 * s, 0.94 * s, 0.36 * s, 0.05 * s) {
                c = Some([0xB9, 0x7A, 0x1E, 255]);
            }
            if c.is_some() && (0.43..=0.57).contains(&u) && (0.18..=0.90).contains(&v) {
                c = Some([0x2C, 0x8C, 0x99, 255]);
            }
            if let Some(c) = c {
                let i = ((yi * size + xi) * 4) as usize;
                px[i..i + 4].copy_from_slice(&c);
            }
        }
    }
    let mut out = std::io::Cursor::new(Vec::new());
    if let Some(img) = image::RgbaImage::from_raw(size, size, px) {
        let _ = img.write_to(&mut out, image::ImageFormat::Png);
    }
    out.into_inner()
}

/// Where the per-user folders are; tests point these at a temporary folder.
#[derive(Debug, Clone)]
pub struct Places {
    pub data: PathBuf,
    pub config: PathBuf,
}

impl Places {
    pub fn for_user() -> Option<Self> {
        Some(Places { data: dirs::data_dir()?, config: dirs::config_dir()? })
    }
    fn desktop(&self) -> PathBuf {
        self.data.join("applications/archivecraft.desktop")
    }
    fn icon(&self) -> PathBuf {
        self.data.join("icons/hicolor/128x128/apps/archivecraft.png")
    }
    fn nautilus_extension(&self) -> PathBuf {
        self.data.join("nautilus-python/extensions/archivecraft.py")
    }
    fn nautilus(&self) -> PathBuf {
        self.data.join("nautilus/scripts/ArchiveCraft")
    }
    fn dolphin(&self) -> PathBuf {
        self.data.join("kio/servicemenus")
    }
    fn nemo(&self) -> PathBuf {
        self.data.join("nemo/actions")
    }
    fn thunar(&self) -> PathBuf {
        self.config.join("Thunar/uca.xml")
    }
    fn mimeapps(&self) -> PathBuf {
        self.config.join("mimeapps.list")
    }
}

struct Action {
    id: &'static str,
    label: &'static str,
    arg: &'static str,
    archives_only: bool,
}

const ACTIONS: [Action; 7] = [
    Action { id: "add", label: "Pack into an archive…", arg: ARG_ADD_DIALOG, archives_only: false },
    Action { id: "addzip", label: "Pack into a ZIP right away", arg: ARG_ADD_ZIP, archives_only: false },
    Action { id: "extract", label: "Extract files…", arg: ARG_EXTRACT_DIALOG, archives_only: true },
    Action { id: "here", label: "Extract Here", arg: ARG_EXTRACT_HERE, archives_only: true },
    Action { id: "folder", label: "Unpack into its own folder", arg: ARG_EXTRACT_FOLDER, archives_only: true },
    Action { id: "test", label: "Test archive", arg: ARG_TEST, archives_only: true },
    Action { id: "convert", label: "Convert archive…", arg: ARG_CONVERT, archives_only: true },
];

/// Options controlling which context-menu actions and presentation flags are written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallOptions {
    pub desktop_shortcut: bool,
    pub cascaded_menus: bool,
    pub menu_icons: bool,
    pub open: bool,
    pub extract_dialog: bool,
    pub extract_here: bool,
    pub extract_folder: bool,
    pub add_dialog: bool,
    pub add_zip: bool,
    pub test_archive: bool,
    pub convert_archive: bool,
    pub always_pack_for: String,
}

impl Default for InstallOptions {
    fn default() -> Self {
        InstallOptions {
            desktop_shortcut: false,
            cascaded_menus: true,
            menu_icons: true,
            open: true,
            extract_dialog: true,
            extract_here: true,
            extract_folder: true,
            add_dialog: true,
            add_zip: true,
            test_archive: false,
            convert_archive: false,
            always_pack_for: "*.iso".into(),
        }
    }
}

impl InstallOptions {
    fn action_enabled(&self, id: &str) -> bool {
        match id {
            "add" => self.add_dialog,
            "addzip" => self.add_zip,
            "extract" => self.extract_dialog,
            "here" => self.extract_here,
            "folder" => self.extract_folder,
            "test" => self.test_archive,
            "convert" => self.convert_archive,
            _ => true,
        }
    }
}

const ARCHIVE_EXTS: &[&str] = &["zip", "jar", "7z", "tar", "gz", "tgz", "xz", "txz", "zst", "tzst", "rar", "iso", "001"];

fn quote(exe: &Path) -> String {
    let s = exe.display().to_string();
    if s.contains([' ', '"', '\'', '\\', '$', '`']) {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace('$', "\\$").replace('`', "\\`"))
    } else {
        s
    }
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn all_mimes() -> Vec<&'static str> {
    ASSOCIATIONS
        .iter()
        .filter(|(l, _, _)| *l != "RAR" || cfg!(feature = "rar"))
        .flat_map(|(_, m, _)| m.iter().copied())
        .collect()
}

fn write(path: &Path, text: &str, executable: bool, written: &mut Vec<PathBuf>) -> Result<()> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    fs::write(path, text)?;
    #[cfg(unix)]
    if executable {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(not(unix))]
    let _ = executable;
    written.push(path.to_path_buf());
    Ok(())
}

const THUNAR_BEGIN: &str = "<!-- archivecraft:begin -->";
const THUNAR_END: &str = "<!-- archivecraft:end -->";

fn strip_thunar(text: &str) -> String {
    let mut out = text.to_string();
    while let (Some(a), Some(b)) = (out.find(THUNAR_BEGIN), out.find(THUNAR_END)) {
        if b < a {
            break;
        }
        let mut end = b + THUNAR_END.len();
        if out[end..].starts_with('\n') {
            end += 1;
        }
        out.replace_range(a..end, "");
    }
    out
}

/// Install the chosen integrations on Linux using default options. Returns the files written.
pub fn install(exe: &Path, places: &Places, targets: Targets) -> Result<Vec<PathBuf>> {
    install_with_options(exe, places, targets, &InstallOptions::default())
}

/// Install the chosen integrations on Linux with custom menu options. Returns the files written.
pub fn install_with_options(exe: &Path, places: &Places, targets: Targets, opts: &InstallOptions) -> Result<Vec<PathBuf>> {
    if !cfg!(all(unix, not(target_os = "macos"))) {
        return Err(Error::Unsupported("file-manager integration is available on Linux in this version".into()));
    }
    let mut written = Vec::new();
    let cmd = quote(exe);
    let active: Vec<&Action> = ACTIONS.iter().filter(|a| opts.action_enabled(a.id)).collect();
    if targets != Targets::default() || opts.desktop_shortcut {
        let icon = places.icon();
        if let Some(p) = icon.parent() {
            fs::create_dir_all(p)?;
        }
        fs::write(&icon, icon_png(128))?;
        written.push(icon);
    }
    let build_desktop = || {
        let mut t = format!(
            "[Desktop Entry]\nType=Application\nName=ArchiveCraft\nGenericName=Archive manager\nComment=Open, create and unpack archives\nExec={cmd} %F\nIcon=archivecraft\nTerminal=false\nCategories=Utility;Archiving;Compression;\nMimeType={};\nActions={};\n",
            all_mimes().join(";"),
            active.iter().map(|a| a.id).collect::<Vec<_>>().join(";")
        );
        for a in &active {
            t.push_str(&format!("\n[Desktop Action {}]\nName={}\nExec={cmd} {} %F\n", a.id, a.label, a.arg));
        }
        t
    };
    if targets.desktop_entry {
        write(&places.desktop(), &build_desktop(), false, &mut written)?;
    }
    if opts.desktop_shortcut
        && let Some(desk) = dirs::desktop_dir()
    {
        write(&desk.join("archivecraft.desktop"), &build_desktop(), true, &mut written)?;
    }
    if targets.nautilus {
        let py_list = |items: &[(&str, &str, &str)]| {
            let inner: Vec<String> = items
                .iter()
                .map(|(k, l, a)| format!("(\"{}\", \"{}\", \"{}\")", k, l.replace('"', "\\\""), a))
                .collect();
            format!("[{}]", inner.join(", "))
        };
        let pack_items: Vec<(&str, &str, &str)> = active.iter().filter(|a| !a.archives_only).map(|a| (a.id, a.label, a.arg)).collect();
        let mut arch_items: Vec<(&str, &str, &str)> = Vec::new();
        if opts.open {
            arch_items.push(("open", "Open with ArchiveCraft", ""));
        }
        arch_items.extend(active.iter().filter(|a| a.archives_only).map(|a| (a.id, a.label, a.arg)));
        let ext = NAUTILUS_EXTENSION
            .replace("@EXE@", &exe.display().to_string().replace('\\', "\\\\").replace('"', "\\\""))
            .replace("@CASCADED@", if opts.cascaded_menus { "True" } else { "False" })
            .replace("@MENU_ICONS@", if opts.menu_icons { "True" } else { "False" })
            .replace("@PACK_ENTRIES@", &py_list(&pack_items))
            .replace("@ARCHIVE_ENTRIES@", &py_list(&arch_items));
        write(&places.nautilus_extension(), &ext, false, &mut written)?;
        for a in &active {
            let script = format!("#!/bin/sh\n# ArchiveCraft right-click action\nexec {} {} \"$@\"\n", shell_quote(&exe.display().to_string()), a.arg);
            write(&places.nautilus().join(a.label.trim_end_matches('…')), &script, true, &mut written)?;
        }
    }
    if targets.dolphin {
        let icon_line = if opts.menu_icons { "Icon=archivecraft\n" } else { "" };
        let sub_line = if opts.cascaded_menus { "X-KDE-Submenu=ArchiveCraft\n" } else { "" };
        let always_exts: Vec<String> = opts
            .always_pack_for
            .split(|c: char| c.is_whitespace() || c == ';' || c == ',')
            .map(|s| s.trim().trim_start_matches("*.").trim_start_matches('.').to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        let exclude_mimes: Vec<&str> = ASSOCIATIONS
            .iter()
            .filter(|(l, _, exts)| (*l != "RAR" || cfg!(feature = "rar")) && !exts.iter().any(|e| always_exts.iter().any(|ae| ae == e)))
            .flat_map(|(_, m, _)| m.iter().copied())
            .collect();
        let pack_actions: Vec<&Action> = active.iter().copied().filter(|a| !a.archives_only).collect();
        if !pack_actions.is_empty() {
            let ids: Vec<String> = pack_actions.iter().map(|a| format!("ac{}", a.id)).collect();
            let mut files = format!(
                "[Desktop Entry]\nType=Service\nMimeType=all/allfiles;inode/directory;\nExcludeMimeTypes={};\nX-KDE-ServiceTypes=KonqPopupMenu/Plugin\nX-KDE-Priority=TopLevel\n{sub_line}{icon_line}Actions={};\n",
                exclude_mimes.join(";"),
                ids.join(";")
            );
            for a in &pack_actions {
                files.push_str(&format!("\n[Desktop Action ac{}]\nName={}\n{icon_line}Exec={cmd} {} %F\n", a.id, a.label, a.arg));
            }
            write(&places.dolphin().join("archivecraft-pack.desktop"), &files, true, &mut written)?;
        }
        let mimes = all_mimes().join(";");
        let mut unpack_ids: Vec<String> = Vec::new();
        let mut unpack_body = String::new();
        if opts.open {
            unpack_ids.push("acopen".into());
            unpack_body.push_str(&format!("\n[Desktop Action acopen]\nName=Open with ArchiveCraft\n{icon_line}Exec={cmd} %F\n"));
        }
        for a in active.iter().filter(|a| a.archives_only) {
            let id = format!("ac{}", a.id);
            unpack_ids.push(id.clone());
            unpack_body.push_str(&format!("\n[Desktop Action {id}]\nName={}\n{icon_line}Exec={cmd} {} %F\n", a.label, a.arg));
        }
        if !unpack_ids.is_empty() {
            let archives = format!(
                "[Desktop Entry]\nType=Service\nMimeType={mimes};\nX-KDE-ServiceTypes=KonqPopupMenu/Plugin\nX-KDE-Priority=TopLevel\n{sub_line}{icon_line}Actions={};\n{unpack_body}",
                unpack_ids.join(";")
            );
            write(&places.dolphin().join("archivecraft-unpack.desktop"), &archives, true, &mut written)?;
        }
    }
    if targets.nemo {
        let icon_line = if opts.menu_icons { "Icon-Name=archivecraft\n" } else { "" };
        for a in &active {
            let ext = if a.archives_only { ARCHIVE_EXTS.join(";") } else { "any".into() };
            let t = format!("[Nemo Action]\nName={}\nComment=ArchiveCraft\nExec={cmd} {} %F\n{icon_line}Selection=notnone\nExtensions={ext};\n", a.label, a.arg);
            write(&places.nemo().join(format!("archivecraft-{}.nemo_action", a.id)), &t, false, &mut written)?;
        }
    }
    if targets.thunar {
        let path = places.thunar();
        let old = fs::read_to_string(&path).unwrap_or_else(|_| "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<actions>\n</actions>\n".into());
        let mut text = strip_thunar(&old);
        let mut block = String::from(THUNAR_BEGIN);
        block.push('\n');
        let icon_xml = if opts.menu_icons { "\t<icon>archivecraft</icon>\n" } else { "" };
        for a in &active {
            let patterns = if a.archives_only { ARCHIVE_EXTS.iter().map(|e| format!("*.{e}")).collect::<Vec<_>>().join(";") } else { "*".into() };
            let kinds = if a.archives_only { "<other-files/>" } else { "<directories/><audio-files/><image-files/><other-files/><text-files/><video-files/>" };
            block.push_str(&format!(
                "<action>\n{icon_xml}\t<name>{}</name>\n\t<unique-id>archivecraft-{}</unique-id>\n\t<command>{} {} %F</command>\n\t<description>ArchiveCraft</description>\n\t<patterns>{patterns}</patterns>\n\t{kinds}\n</action>\n",
                xml_escape(a.label), a.id, xml_escape(&cmd), a.arg
            ));
        }
        block.push_str(THUNAR_END);
        block.push('\n');
        match text.rfind("</actions>") {
            Some(i) => text.insert_str(i, &block),
            None => text.push_str(&format!("<actions>\n{block}</actions>\n")),
        }
        write(&path, &text, false, &mut written)?;
    }
    Ok(written)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Remove everything `install` wrote. Returns what was removed.
pub fn uninstall(places: &Places) -> Result<Vec<PathBuf>> {
    let mut removed = Vec::new();
    let mut rm = |p: PathBuf| {
        if p.is_dir() {
            if fs::remove_dir_all(&p).is_ok() {
                removed.push(p);
            }
        } else if fs::remove_file(&p).is_ok() {
            removed.push(p);
        }
    };
    rm(places.desktop());
    if let Some(desk) = dirs::desktop_dir() {
        rm(desk.join("archivecraft.desktop"));
    }
    rm(places.nautilus());
    rm(places.nautilus_extension());
    rm(places.icon());
    rm(places.dolphin().join("archivecraft-pack.desktop"));
    rm(places.dolphin().join("archivecraft-unpack.desktop"));
    for a in &ACTIONS {
        rm(places.nemo().join(format!("archivecraft-{}.nemo_action", a.id)));
    }
    let t = places.thunar();
    if let Ok(old) = fs::read_to_string(&t) {
        let new = strip_thunar(&old);
        if new != old {
            fs::write(&t, new)?;
            removed.push(t);
        }
    }
    Ok(removed)
}

/// Which integrations are present now.
pub fn installed(places: &Places) -> Targets {
    Targets {
        desktop_entry: places.desktop().exists(),
        nautilus: places.nautilus().exists(),
        dolphin: places.dolphin().join("archivecraft-pack.desktop").exists(),
        nemo: places.nemo().join("archivecraft-add.nemo_action").exists(),
        thunar: fs::read_to_string(places.thunar()).is_ok_and(|t| t.contains(THUNAR_BEGIN)),
    }
}

/// Make ArchiveCraft the default opener for the chosen archive kinds (by label), and stop being
/// it for the others it had claimed. Edits only the person's own `mimeapps.list`.
pub fn set_defaults(places: &Places, labels: &[String]) -> Result<()> {
    let path = places.mimeapps();
    let text = fs::read_to_string(&path).unwrap_or_default();
    let ours: Vec<&str> = ASSOCIATIONS.iter().flat_map(|(_, m, _)| m.iter().copied()).collect();
    let wanted: Vec<&str> = ASSOCIATIONS.iter().filter(|(l, _, _)| labels.iter().any(|x| x == l)).flat_map(|(_, m, _)| m.iter().copied()).collect();
    let mut out = String::new();
    let mut in_defaults = false;
    let mut wrote_section = false;
    let emit = |out: &mut String| {
        for m in &wanted {
            out.push_str(&format!("{m}=archivecraft.desktop\n"));
        }
    };
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            if in_defaults && !wrote_section {
                emit(&mut out);
                wrote_section = true;
            }
            in_defaults = trimmed == "[Default Applications]";
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if in_defaults
            && let Some((k, v)) = trimmed.split_once('=')
            && ours.contains(&k.trim())
            && v.contains("archivecraft.desktop")
        {
            continue; // ours; rewritten below
        }
        if in_defaults && let Some((k, _)) = trimmed.split_once('=') && wanted.contains(&k.trim()) {
            continue; // another app's default we are replacing
        }
        out.push_str(line);
        out.push('\n');
    }
    if in_defaults && !wrote_section {
        emit(&mut out);
        wrote_section = true;
    }
    if !wrote_section && !wanted.is_empty() {
        out.push_str("[Default Applications]\n");
        emit(&mut out);
    }
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    fs::write(path, out)?;
    Ok(())
}

/// Labels of the archive kinds that currently open in ArchiveCraft.
pub fn current_defaults(places: &Places) -> Vec<String> {
    let text = fs::read_to_string(places.mimeapps()).unwrap_or_default();
    ASSOCIATIONS
        .iter()
        .filter(|(_, mimes, _)| mimes.iter().any(|m| text.lines().any(|l| l.trim().starts_with(&format!("{m}=")) && l.contains("archivecraft.desktop"))))
        .map(|(l, _, _)| l.to_string())
        .collect()
}

#[cfg(all(test, unix, not(target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn install_and_remove_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let places = Places { data: dir.path().join("data"), config: dir.path().join("config") };
        fs::create_dir_all(places.config.join("Thunar")).unwrap();
        let theirs = "<?xml version=\"1.0\"?>\n<actions>\n<action><name>Theirs</name></action>\n</actions>\n";
        fs::write(places.thunar(), theirs).unwrap();
        let exe = Path::new("/opt/Archive Craft/archivecraft");
        let w = install(exe, &places, Targets::all()).unwrap();
        assert!(w.len() >= 10);
        assert_eq!(installed(&places), Targets::all());
        let desktop = fs::read_to_string(places.desktop()).unwrap();
        assert!(desktop.contains("Exec=\"/opt/Archive Craft/archivecraft\" --extract-here %F"));
        let pack = fs::read_to_string(places.dolphin().join("archivecraft-pack.desktop")).unwrap();
        assert!(pack.contains("ExcludeMimeTypes="));
        let unpack = fs::read_to_string(places.dolphin().join("archivecraft-unpack.desktop")).unwrap();
        assert!(unpack.contains("Actions=acopen;acextract;achere;acfolder;"));
        assert!(unpack.contains("X-KDE-Submenu=ArchiveCraft"));
        let uca = fs::read_to_string(places.thunar()).unwrap();
        assert!(uca.contains("Theirs") && uca.contains("archivecraft-add"));
        // Installing twice does not duplicate Thunar actions.
        install(exe, &places, Targets::all()).unwrap();
        assert_eq!(fs::read_to_string(places.thunar()).unwrap().matches("<unique-id>archivecraft-add</unique-id>").count(), 1);
        uninstall(&places).unwrap();
        assert_eq!(installed(&places), Targets::default());
        assert_eq!(fs::read_to_string(places.thunar()).unwrap(), theirs);
    }

    #[test]
    fn default_openers() {
        let dir = tempfile::tempdir().unwrap();
        let places = Places { data: dir.path().join("d"), config: dir.path().join("c") };
        fs::create_dir_all(&places.config).unwrap();
        fs::write(places.mimeapps(), "[Default Applications]\ntext/plain=editor.desktop\napplication/zip=other.desktop\n").unwrap();
        set_defaults(&places, &["ZIP".into(), "7z".into()]).unwrap();
        let t = fs::read_to_string(places.mimeapps()).unwrap();
        assert!(t.contains("text/plain=editor.desktop"));
        assert!(t.contains("application/zip=archivecraft.desktop"));
        assert!(!t.contains("other.desktop"));
        assert_eq!(current_defaults(&places), vec!["ZIP".to_string(), "7z".to_string()]);
        set_defaults(&places, &[]).unwrap();
        assert!(current_defaults(&places).is_empty());
    }

    #[test]
    fn install_with_options_respects_flags() {
        let dir = tempfile::tempdir().unwrap();
        let places = Places { data: dir.path().join("data"), config: dir.path().join("config") };
        let exe = Path::new("/usr/bin/archivecraft");
        let opts = InstallOptions {
            desktop_shortcut: false,
            cascaded_menus: false,
            menu_icons: false,
            open: false,
            test_archive: true,
            convert_archive: true,
            always_pack_for: "*.iso".into(),
            ..InstallOptions::default()
        };
        install_with_options(exe, &places, Targets::all(), &opts).unwrap();
        let unpack = fs::read_to_string(places.dolphin().join("archivecraft-unpack.desktop")).unwrap();
        assert!(!unpack.contains("X-KDE-Submenu="));
        assert!(!unpack.contains("Icon=archivecraft"));
        assert!(!unpack.contains("acopen"));
        assert!(unpack.contains("actest") && unpack.contains("acconvert"));
        let pack = fs::read_to_string(places.dolphin().join("archivecraft-pack.desktop")).unwrap();
        assert!(!pack.contains("application/x-cd-image"));
        let nautilus = fs::read_to_string(places.nautilus_extension()).unwrap();
        assert!(nautilus.contains("CASCADED = False") && nautilus.contains("MENU_ICONS = False"));
        assert!(nautilus.contains("--test") && nautilus.contains("--convert"));
    }
}
