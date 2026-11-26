//! Search: names and contents, in a folder and inside the archives there.

use super::{group, window, Flow};
use crate::rows::RowRef;
use crate::state::{App, Pick};
use crate::{tf, tl};
use archivecraft_engine::find::{FindQuery, Hit};
use archivecraft_engine::model::util::{human_size, parse_patterns};
use std::path::PathBuf;

pub struct FindDialog {
    pub names: String,
    pub text: String,
    pub match_case: bool,
    pub all_tables: bool,
    pub hex: bool,
    pub root: String,
    pub types: String,
    pub subfolders: bool,
    pub in_files: bool,
    pub inside: bool,
    pub skip_locked: bool,
    help: bool,
}

impl FindDialog {
    pub fn new(root: PathBuf) -> Self {
        Self::from_saved(root, &crate::settings::FindDefaults::default())
    }

    pub fn from_saved(root: PathBuf, saved: &crate::settings::FindDefaults) -> Self {
        FindDialog {
            names: saved.names.clone(),
            text: saved.text.clone(),
            match_case: saved.match_case,
            all_tables: saved.all_tables,
            hex: saved.hex,
            root: root.display().to_string(),
            types: saved.types.clone(),
            subfolders: saved.subfolders,
            in_files: saved.in_files,
            inside: saved.inside,
            skip_locked: saved.skip_locked,
            help: false,
        }
    }

    pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> Flow {
        let mut flow = Flow::Keep;
        let mut open = true;
        window(tl!("Search"), "ac-find").open(&mut open).resizable(false).default_width(600.0).show(ctx, |ui| {
            ui.set_width(600.0);
            ui.columns(2, |c| {
                group(&mut c[0], &tl!("Look for"), |ui| {
                    ui.set_min_height(195.0);
                    ui.label(tl!("Names like"));
                    ui.add(egui::TextEdit::singleline(&mut self.names).desired_width(f32::INFINITY));
                    ui.add_space(4.0);
                    ui.label(tl!("Containing the text"));
                    ui.add(egui::TextEdit::singleline(&mut self.text).desired_width(f32::INFINITY));
                    ui.add_space(6.0);
                    ui.checkbox(&mut self.match_case, tl!("Capital letters must match"));
                    ui.checkbox(&mut self.all_tables, tl!("Search all encoding tables"));
                    ui.checkbox(&mut self.hex, tl!("The text is bytes in hexadecimal"));
                });
                group(&mut c[1], &tl!("Look in"), |ui| {
                    ui.set_min_height(195.0);
                    ui.label(tl!("Folder"));
                    super::text_row(ui, &mut self.root, None, |ui, root| {
                        if ui.button(tl!("Browse…")).clicked() {
                            app.pick(Pick::FindFolder, craft_dialogs::Kind::Folder, tl!("Search in which folder?"), Some(PathBuf::from(root)), None, false);
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(tl!("Archive types to open"));
                    ui.add(egui::TextEdit::singleline(&mut self.types).desired_width(f32::INFINITY));
                    ui.add_space(6.0);
                    ui.checkbox(&mut self.subfolders, tl!("Also the folders inside"));
                    ui.checkbox(&mut self.in_files, tl!("Find in files"));
                    ui.checkbox(&mut self.inside, tl!("Also inside archives"));
                    ui.checkbox(&mut self.skip_locked, tl!("Skip locked items"));
                });
            });
            if self.help {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(tl!("Use wildcards like *.txt or report*; separate several with spaces.")).weak());
            }
            ui.separator();
            let btn_size = egui::vec2(88.0, 26.0);
            let mut start = false;
            let mut save = false;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(tl!("Help")).min_size(btn_size)).clicked() {
                    self.help = !self.help;
                }
                ui.add_space(8.0);
                if ui.add(egui::Button::new(tl!("Save")).min_size(btn_size)).clicked() {
                    save = true;
                }
                if ui.add(egui::Button::new(tl!("Cancel")).min_size(btn_size)).clicked() {
                    flow = Flow::Close;
                }
                if ui.add_enabled(!self.root.trim().is_empty(), egui::Button::new(tl!("Search")).min_size(btn_size)).clicked() {
                    start = true;
                }
            });
            if save {
                let saved = crate::settings::FindDefaults {
                    names: self.names.clone(),
                    text: self.text.clone(),
                    match_case: self.match_case,
                    all_tables: self.all_tables,
                    hex: self.hex,
                    types: self.types.clone(),
                    subfolders: self.subfolders,
                    in_files: self.in_files,
                    inside: self.inside,
                    skip_locked: self.skip_locked,
                };
                app.prefs.update(|p| p.saved.find = saved);
                app.note(tl!("Search settings saved."));
            }
            if start {
                let q = FindQuery {
                    names: parse_patterns(&self.names),
                    text: self.text.clone(),
                    match_case: self.match_case,
                    all_tables: self.all_tables,
                    hex: self.hex,
                    subfolders: self.subfolders,
                    in_files: self.in_files,
                    inside_archives: self.inside,
                    archive_types: parse_patterns(&self.types),
                    skip_locked: self.skip_locked,
                };
                app.start_find(PathBuf::from(self.root.trim()), q);
                flow = Flow::Close;
            }
        });
        if !open { Flow::Close } else { flow }
    }
}

pub struct FoundWindow {
    pub hits: Vec<Hit>,
    pub selected: Option<usize>,
}

impl FoundWindow {
    pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> Flow {
        let mut flow = Flow::Keep;
        let mut open = true;
        let mut go: Option<Hit> = None;
        window(tf!("Found {n}", n = self.hits.len()), "ac-found").open(&mut open).default_size([640.0, 360.0]).show(ctx, |ui| {
            ui.label(egui::RichText::new(tl!("Double-click a result to go there.")).weak());
            egui::ScrollArea::vertical().auto_shrink(false).max_height(300.0).show(ui, |ui| {
                for (i, h) in self.hits.iter().enumerate() {
                    let label = match &h.inner {
                        Some(inner) => format!("{} / {}   ({})", h.file.display(), inner, human_size(h.size, false)),
                        None => format!("{}   ({})", h.file.display(), human_size(h.size, false)),
                    };
                    let r = ui.selectable_label(self.selected == Some(i), label);
                    if r.clicked() {
                        self.selected = Some(i);
                    }
                    if r.double_clicked() {
                        go = Some(h.clone());
                    }
                }
            });
            ui.separator();
            if ui.button(tl!("Close")).clicked() {
                flow = Flow::Close;
            }
        });
        if let Some(h) = go {
            match h.inner {
                Some(inner) => {
                    app.open_archive(h.file.clone());
                    let folder = inner.rsplit_once('/').map(|(a, _)| a.to_string()).unwrap_or_default();
                    app.enter_folder(folder);
                    app.select_target(&RowRef::Entry(inner));
                }
                None => {
                    if let Some(dir) = h.file.parent() {
                        app.go_disk(dir.to_path_buf());
                        app.select_target(&RowRef::Disk(h.file.clone()));
                    }
                }
            }
        }
        if !open { Flow::Close } else { flow }
    }
}
