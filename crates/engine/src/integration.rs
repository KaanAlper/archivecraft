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
pub const ARG_EXTRACT_HERE: &str = "--extract-here";
pub const ARG_EXTRACT_FOLDER: &str = "--extract-folder";

/// Archive kinds the app can be the default opener for: (label, MIME types, file endings).
pub const ASSOCIATIONS: &[(&str, &[&str], &[&str])] = &[
    ("ZIP", &["application/zip", "application/x-zip-compressed"], &["zip"]),
    ("7z", &["application/x-7z-compressed"], &["7z"]),
    ("TAR", &["application/x-tar"], &["tar"]),
    ("TAR.GZ", &["application/x-compressed-tar", "application/gzip"], &["tgz", "gz"]),
    ("TAR.XZ", &["application/x-xz-compressed-tar", "application/x-xz"], &["txz", "xz"]),
    ("TAR.ZST", &["application/x-zstd-compressed-tar", "application/zstd"], &["tzst", "zst"]),
    ("RAR", &["application/vnd.rar", "application/x-rar"], &["rar"]),
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
ARCHIVES = (".zip", ".7z", ".tar", ".gz", ".tgz", ".xz", ".txz", ".zst", ".tzst", ".rar", ".001")


class ArchiveCraftMenu(GObject.GObject, Nautilus.MenuProvider):
    def _paths(self, files):
        return [unquote(urlparse(f.get_uri()).path) for f in files if f.get_uri_scheme() == "file"]

    def _run(self, _item, arg, paths):
        subprocess.Popen([EXE, arg] + paths)

    def get_file_items(self, *args):
        files = args[-1]
        paths = self._paths(files)
        if not paths:
            return []
        top = Nautilus.MenuItem(name="ArchiveCraft::top", label="ArchiveCraft", icon="archivecraft")
        sub = Nautilus.Menu()
        top.set_submenu(sub)
        entries = [("add", "Pack into an archive…", "--add"), ("addzip", "Pack into a ZIP right away", "--add-zip")]
        if all(p.lower().endswith(ARCHIVES) for p in paths):
            entries += [("here", "Unpack here", "--extract-here"), ("folder", "Unpack into its own folder", "--extract-folder")]
        for key, label, arg in entries:
            item = Nautilus.MenuItem(name="ArchiveCraft::" + key, label=label, icon="archivecraft")
            item.connect("activate", self._run, arg, paths)
            sub.append_item(item)
        return [top]
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

const ACTIONS: [Action; 4] = [
    Action { id: "add", label: "Pack into an archive…", arg: ARG_ADD_DIALOG, archives_only: false },
    Action { id: "addzip", label: "Pack into a ZIP right away", arg: ARG_ADD_ZIP, archives_only: false },
    Action { id: "here", label: "Unpack here", arg: ARG_EXTRACT_HERE, archives_only: true },
    Action { id: "folder", label: "Unpack into its own folder", arg: ARG_EXTRACT_FOLDER, archives_only: true },
];

const ARCHIVE_EXTS: &[&str] = &["zip", "7z", "tar", "gz", "tgz", "xz", "txz", "zst", "tzst", "rar", "001"];

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

/// Install the chosen integrations on Linux. Returns the files written.
pub fn install(exe: &Path, places: &Places, targets: Targets) -> Result<Vec<PathBuf>> {
    if !cfg!(all(unix, not(target_os = "macos"))) {
        return Err(Error::Unsupported("file-manager integration is available on Linux in this version".into()));
    }
    let mut written = Vec::new();
    let cmd = quote(exe);
    if targets != Targets::default() {
        let icon = places.icon();
        if let Some(p) = icon.parent() {
            fs::create_dir_all(p)?;
        }
        fs::write(&icon, icon_png(128))?;
        written.push(icon);
    }
    if targets.desktop_entry {
        let mut t = format!(
            "[Desktop Entry]\nType=Application\nName=ArchiveCraft\nGenericName=Archive manager\nComment=Open, create and unpack archives\nExec={cmd} %F\nIcon=archivecraft\nTerminal=false\nCategories=Utility;Archiving;Compression;\nMimeType={};\nActions={};\n",
            all_mimes().join(";"),
            ACTIONS.iter().map(|a| a.id).collect::<Vec<_>>().join(";")
        );
        for a in &ACTIONS {
            t.push_str(&format!("\n[Desktop Action {}]\nName={}\nExec={cmd} {} %F\n", a.id, a.label, a.arg));
        }
        write(&places.desktop(), &t, false, &mut written)?;
    }
    if targets.nautilus {
        // A real "ArchiveCraft" submenu (needs nautilus-python); the scripts below work without it.
        let ext = NAUTILUS_EXTENSION.replace("@EXE@", &exe.display().to_string().replace('\\', "\\\\").replace('"', "\\\""));
        write(&places.nautilus_extension(), &ext, false, &mut written)?;
        for a in &ACTIONS {
            let script = format!("#!/bin/sh\n# ArchiveCraft right-click action\nexec {} {} \"$@\"\n", shell_quote(&exe.display().to_string()), a.arg);
            write(&places.nautilus().join(a.label.trim_end_matches('…')), &script, true, &mut written)?;
        }
    }
    if targets.dolphin {
        let files = format!(
            "[Desktop Entry]\nType=Service\nMimeType=all/allfiles;inode/directory;\nX-KDE-ServiceTypes=KonqPopupMenu/Plugin\nX-KDE-Submenu=ArchiveCraft\nIcon=archivecraft\nActions=acadd;acaddzip;\n\n[Desktop Action acadd]\nName={}\nIcon=archivecraft\nExec={cmd} {} %F\n\n[Desktop Action acaddzip]\nName={}\nIcon=archivecraft\nExec={cmd} {} %F\n",
            ACTIONS[0].label, ACTIONS[0].arg, ACTIONS[1].label, ACTIONS[1].arg
        );
        write(&places.dolphin().join("archivecraft-pack.desktop"), &files, true, &mut written)?;
        let archives = format!(
            "[Desktop Entry]\nType=Service\nMimeType={};\nX-KDE-ServiceTypes=KonqPopupMenu/Plugin\nX-KDE-Submenu=ArchiveCraft\nIcon=archivecraft\nActions=achere;acfolder;\n\n[Desktop Action achere]\nName={}\nIcon=archivecraft\nExec={cmd} {} %F\n\n[Desktop Action acfolder]\nName={}\nIcon=archivecraft\nExec={cmd} {} %F\n",
            all_mimes().join(";"),
            ACTIONS[2].label, ACTIONS[2].arg, ACTIONS[3].label, ACTIONS[3].arg
        );
        write(&places.dolphin().join("archivecraft-unpack.desktop"), &archives, true, &mut written)?;
    }
    if targets.nemo {
        for a in &ACTIONS {
            let ext = if a.archives_only { ARCHIVE_EXTS.join(";") } else { "any".into() };
            let t = format!("[Nemo Action]\nName={}\nComment=ArchiveCraft\nExec={cmd} {} %F\nIcon-Name=archivecraft\nSelection=notnone\nExtensions={ext};\n", a.label, a.arg);
            write(&places.nemo().join(format!("archivecraft-{}.nemo_action", a.id)), &t, false, &mut written)?;
        }
    }
    if targets.thunar {
        let path = places.thunar();
        let old = fs::read_to_string(&path).unwrap_or_else(|_| "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<actions>\n</actions>\n".into());
        let mut text = strip_thunar(&old);
        let mut block = String::from(THUNAR_BEGIN);
        block.push('\n');
        for a in &ACTIONS {
            let patterns = if a.archives_only { ARCHIVE_EXTS.iter().map(|e| format!("*.{e}")).collect::<Vec<_>>().join(";") } else { "*".into() };
            let kinds = if a.archives_only { "<other-files/>" } else { "<directories/><audio-files/><image-files/><other-files/><text-files/><video-files/>" };
            block.push_str(&format!(
                "<action>\n\t<icon>archivecraft</icon>\n\t<name>{}</name>\n\t<unique-id>archivecraft-{}</unique-id>\n\t<command>{} {} %F</command>\n\t<description>ArchiveCraft</description>\n\t<patterns>{patterns}</patterns>\n\t{kinds}\n</action>\n",
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
}
