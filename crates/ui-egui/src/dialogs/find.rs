//! Search: names and contents, in a folder and inside the archives there.

use super::{group, ok_cancel, window, Flow};
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
    pub hex: bool,
    pub root: String,
    pub types: String,
    pub subfolders: bool,
    pub inside: bool,
    pub skip_locked: bool,
}

impl FindDialog {
    pub fn new(root: PathBuf) -> Self {
        FindDialog { names: "*".into(), text: String::new(), match_case: false, hex: false, root: root.display().to_string(), types: "*".into(), subfolders: true, inside: true, skip_locked: true }
    }

    pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> Flow {
        let mut flow = Flow::Keep;
        let mut open = true;
        window(tl!("Search"), "ac-find").open(&mut open).default_width(620.0).show(ctx, |ui| {
            ui.columns(2, |c| {
                group(&mut c[0], &tl!("Look for"), |ui| {
                    ui.label(tl!("Names like"));
                    ui.text_edit_singleline(&mut self.names);
                    ui.label(tl!("Containing the text"));
                    ui.text_edit_singleline(&mut self.text);
                    ui.checkbox(&mut self.match_case, tl!("Capital letters must match"));
                    ui.checkbox(&mut self.hex, tl!("The text is bytes in hexadecimal"));
                });
                group(&mut c[1], &tl!("Look in"), |ui| {
                    ui.label(tl!("Folder"));
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.root).desired_width(ui.available_width() - 80.0));
                        if ui.button(tl!("Browse…")).clicked() {
                            app.pick(Pick::FindFolder, craft_dialogs::Kind::Folder, tl!("Search in which folder?"), Some(PathBuf::from(&self.root)), None, false);
                        }
                    });
                    ui.label(tl!("Archive types to open"));
                    ui.text_edit_singleline(&mut self.types);
                    ui.checkbox(&mut self.subfolders, tl!("Also the folders inside"));
                    ui.checkbox(&mut self.inside, tl!("Also inside archives"));
                    ui.checkbox(&mut self.skip_locked, tl!("Skip locked items"));
                });
            });
            let (ok, cancel) = ok_cancel(ui, &tl!("Search"), !self.root.trim().is_empty());
            if cancel {
                flow = Flow::Close;
            }
            if ok {
                let q = FindQuery {
                    names: parse_patterns(&self.names),
                    text: self.text.clone(),
                    match_case: self.match_case,
                    hex: self.hex,
                    subfolders: self.subfolders,
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
