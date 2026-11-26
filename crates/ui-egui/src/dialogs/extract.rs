//! "Unpack": destination folder (with a folder tree and "new folder"), what to unpack, what to do
//! with files already there, keep broken files, folder names, removing the archive afterwards,
//! and one folder per archive.

use super::{group, ok_cancel, tabs, window, Flow};
use crate::state::{App, Pick};
use crate::{tf, tl};
use archivecraft_engine::model::{ExtractPaths, ExtractUpdate, Overwrite};
use archivecraft_engine::ops;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DeleteAfter {
    #[default]
    Never,
    Ask,
    Always,
}

pub struct ExtractDialog {
    pub archives: Vec<PathBuf>,
    pub items: Vec<String>,
    pub base: String,
    pub dest: String,
    pub update: ExtractUpdate,
    pub overwrite: Overwrite,
    pub keep_broken: bool,
    pub open_after: bool,
    pub set_times: bool,
    pub paths: ExtractPaths,
    pub delete_after: DeleteAfter,
    pub own_folder: bool,
    pub tab: usize,
    new_folder: Option<String>,
    tree: HashMap<PathBuf, Vec<PathBuf>>,
    error: Option<String>,
}

impl ExtractDialog {
    pub fn new(app: &App, archives: Vec<PathBuf>, items: Vec<String>, base: String) -> Self {
        let p = app.prefs.get();
        let first = archives.first().cloned().unwrap_or_default();
        let dest = match &p.paths.extract_folder {
            Some(d) if p.paths.add_archive_name => d.join(ops::folder_for(&first).file_name().unwrap_or_default()),
            Some(d) => d.clone(),
            None if archives.len() > 1 => first.parent().map(Path::to_path_buf).unwrap_or_default(),
            None => ops::folder_for(&first),
        };
        ExtractDialog {
            own_folder: archives.len() > 1,
            archives,
            items,
            base,
            dest: dest.display().to_string(),
            update: ExtractUpdate::All,
            overwrite: p.security.default_overwrite,
            keep_broken: false,
            open_after: false,
            set_times: true,
            paths: ExtractPaths::Relative,
            delete_after: DeleteAfter::Never,
            tab: 0,
            new_folder: None,
            tree: HashMap::new(),
            error: None,
        }
    }

    fn children(&mut self, dir: &Path) -> Vec<PathBuf> {
        self.tree
            .entry(dir.to_path_buf())
            .or_insert_with(|| {
                let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
                    .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_dir() && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'))).collect())
                    .unwrap_or_default();
                v.sort();
                v
            })
            .clone()
    }

    fn tree_node(&mut self, ui: &mut egui::Ui, dir: PathBuf, label: String, depth: usize) {
        let selected = self.dest == dir;
        let id = ui.make_persistent_id(("ac-tree", &dir));
        let state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
        state
            .show_header(ui, |ui| {
                crate::icons::show(ui, crate::icons::Icon::Folder, 14.0);
                if ui.selectable_label(selected, label).clicked() {
                    self.dest = dir.display().to_string();
                }
            })
            .body(|ui| {
                if depth < 12 {
                    for c in self.children(&dir) {
                        let name = c.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        self.tree_node(ui, c, name, depth + 1);
                    }
                }
            });
    }

    fn general(&mut self, ui: &mut egui::Ui, app: &mut App) {
        ui.label(tl!("Unpack into (created if missing)"));
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.dest).desired_width(ui.available_width() - 190.0));
            if ui.button(tl!("Browse…")).clicked() {
                app.pick(Pick::ExtractDest, craft_dialogs::Kind::Folder, tl!("Unpack into which folder?"), Some(PathBuf::from(&self.dest)), None, false);
            }
            if ui.button(tl!("New folder")).clicked() {
                self.new_folder = Some(String::new());
            }
        });
        if let Some(name) = &mut self.new_folder {
            let mut done = false;
            ui.horizontal(|ui| {
                ui.label(tl!("Name of the new folder:"));
                let r = ui.text_edit_singleline(name);
                if (ui.button(tl!("Create")).clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))) && !name.trim().is_empty() {
                    let p = PathBuf::from(&self.dest).join(name.trim());
                    match std::fs::create_dir_all(&p) {
                        Ok(()) => {
                            self.dest = p.display().to_string();
                            self.tree.clear();
                        }
                        Err(e) => self.error = Some(e.to_string()),
                    }
                    done = true;
                }
            });
            if done {
                self.new_folder = None;
            }
        }
        if let Some(dir) = self.archives.first().and_then(|a| a.parent()).map(Path::to_path_buf)
            && ui.link(tl!("Use the archive's own folder (unpack here)")).clicked()
        {
            self.dest = dir.display().to_string();
        }
        ui.add_space(6.0);
        ui.columns(2, |c| {
            group(&mut c[0], &tl!("What to unpack"), |ui| {
                for m in ExtractUpdate::ALL {
                    ui.radio_value(&mut self.update, m, tl!(m.label()));
                }
            });
            c[0].add_space(4.0);
            group(&mut c[0], &tl!("When a file is already there"), |ui| {
                for m in Overwrite::ALL {
                    ui.radio_value(&mut self.overwrite, m, tl!(m.label()));
                }
            });
            c[0].add_space(4.0);
            group(&mut c[0], &tl!("Also"), |ui| {
                ui.checkbox(&mut self.keep_broken, tl!("Keep files that turn out damaged"));
                ui.checkbox(&mut self.open_after, tl!("Open the folder when done"));
            });
            egui::Frame::group(c[1].style()).show(&mut c[1], |ui| {
                ui.set_min_height(300.0);
                egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                    if let Some(h) = dirs::home_dir() {
                        self.tree_node(ui, h, tl!("Home"), 0);
                    }
                    if let Some(d) = dirs::desktop_dir() {
                        self.tree_node(ui, d, tl!("Desktop"), 0);
                    }
                    if let Some(d) = dirs::download_dir() {
                        self.tree_node(ui, d, tl!("Downloads"), 0);
                    }
                    #[cfg(windows)]
                    for letter in b'A'..=b'Z' {
                        let root = PathBuf::from(format!("{}:\\", letter as char));
                        if root.exists() {
                            self.tree_node(ui, root.clone(), root.display().to_string(), 0);
                        }
                    }
                    #[cfg(not(windows))]
                    self.tree_node(ui, PathBuf::from("/"), "/".into(), 0);
                });
            });
        });
    }

    fn advanced(&mut self, ui: &mut egui::Ui) {
        ui.columns(2, |c| {
            group(&mut c[0], &tl!("Times"), |ui| {
                ui.checkbox(&mut self.set_times, tl!("Give files their stored last-changed time"));
            });
            c[0].add_space(4.0);
            group(&mut c[0], &tl!("Folder names"), |ui| {
                ui.radio_value(&mut self.paths, ExtractPaths::Relative, tl!("Keep the folders from the archive"));
                ui.radio_value(&mut self.paths, ExtractPaths::NoPaths, tl!("Put every file straight into the destination"));
            });
            group(&mut c[1], &tl!("Afterwards, the archive"), |ui| {
                ui.radio_value(&mut self.delete_after, DeleteAfter::Never, tl!("Stays where it is"));
                ui.radio_value(&mut self.delete_after, DeleteAfter::Ask, tl!("Goes to the recycle bin, if I agree"));
                ui.radio_value(&mut self.delete_after, DeleteAfter::Always, tl!("Goes to the recycle bin"));
                ui.label(egui::RichText::new(tl!("Only when everything unpacked without problems.")).weak());
            });
        });
        ui.add_space(6.0);
        ui.label(egui::RichText::new(tl!("Safety: names that would leave the destination (absolute paths, \"..\") are always refused, and links are never followed.")).weak());
    }

    fn options(&mut self, ui: &mut egui::Ui) {
        group(ui, &tl!("With several archives"), |ui| {
            ui.radio_value(&mut self.own_folder, false, tl!("Unpack all of them into the destination"));
            ui.radio_value(&mut self.own_folder, true, tl!("Give each archive its own folder there"));
        });
        ui.add_space(6.0);
        ui.label(tf!("{n} archive(s) chosen:", n = self.archives.len()));
        for a in &self.archives {
            ui.label(egui::RichText::new(a.display().to_string()).weak());
        }
        if !self.items.is_empty() {
            ui.label(tf!("Only the {n} chosen item(s) inside.", n = self.items.len()));
        }
    }

    pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> Flow {
        let mut flow = Flow::Keep;
        let mut open = true;
        window(tl!("Unpack"), "ac-extract").open(&mut open).default_width(640.0).show(ctx, |ui| {
            tabs(ui, &mut self.tab, &[tl!("Where"), tl!("More"), tl!("Several archives")]);
            match self.tab {
                0 => self.general(ui, app),
                1 => self.advanced(ui),
                _ => self.options(ui),
            }
            if let Some(e) = &self.error {
                ui.colored_label(crate::icons::RED, e);
            }
            let (ok, cancel) = ok_cancel(ui, &tl!("Unpack"), !self.dest.trim().is_empty());
            if cancel {
                flow = Flow::Close;
            }
            if ok {
                let dest = PathBuf::from(self.dest.trim());
                let mut o = app.default_extract_options(dest.clone());
                o.update = self.update;
                o.overwrite = self.overwrite;
                o.keep_broken = self.keep_broken;
                o.set_mtime = self.set_times;
                o.paths = self.paths;
                o.only = self.items.clone();
                o.base = self.base.clone();
                app.start_extract(self.archives.clone(), o, self.own_folder, self.delete_after);
                if self.open_after {
                    let _ = std::fs::create_dir_all(&dest);
                    let _ = super::viewer::open_outside(&dest, "");
                }
                flow = Flow::Close;
            }
        });
        if !open { Flow::Close } else { flow }
    }
}
