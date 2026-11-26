//! The step-by-step guide for beginners: unpack an archive, make a new one, or add files to an
//! existing one. Each path asks for one thing per page, then opens the usual window.

use super::Flow;
use crate::icons::{self, Icon};
use crate::state::{App, Pick};
use crate::tl;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Path {
    #[default]
    Unpack,
    Create,
    AddTo,
}

#[derive(Default)]
pub struct Wizard {
    path: Path,
    step: usize,
    archive: String,
    files: Vec<PathBuf>,
    help: bool,
}

impl Wizard {
    pub fn picked(&mut self, pick: Pick, paths: Vec<PathBuf>) {
        match pick {
            Pick::WizardFiles => {
                for p in paths {
                    if !self.files.contains(&p) {
                        self.files.push(p);
                    }
                }
            }
            _ => {
                if let Some(p) = paths.into_iter().next() {
                    self.archive = p.display().to_string();
                }
            }
        }
    }

    fn page(&mut self, ui: &mut egui::Ui, app: &mut App) {
        match (self.step, self.path) {
            (0, _) => {
                ui.label(tl!("What would you like to do? Choose one and press Next."));
                ui.add_space(10.0);
                ui.radio_value(&mut self.path, Path::Unpack, tl!("Get the files out of an archive"));
                ui.radio_value(&mut self.path, Path::Create, tl!("Make a new archive"));
                ui.radio_value(&mut self.path, Path::AddTo, tl!("Put more files into an archive I have"));
            }
            (1, Path::Unpack) | (1, Path::AddTo) => {
                ui.label(tl!("Which archive? Type its path, use Browse, or drop it onto this window."));
                ui.add_space(10.0);
                super::text_row(ui, &mut self.archive, None, |ui, _| {
                    if ui.button(tl!("Browse…")).clicked() {
                        app.pick(Pick::WizardArchive, craft_dialogs::Kind::Open { multiple: false }, tl!("Choose the archive"), None, None, true);
                    }
                });
            }
            (1, Path::Create) => {
                ui.label(tl!("Where should the new archive go, and what is it called?"));
                ui.add_space(10.0);
                super::text_row(ui, &mut self.archive, None, |ui, _| {
                    if ui.button(tl!("Browse…")).clicked() {
                        app.pick(Pick::WizardNewArchive, craft_dialogs::Kind::Save, tl!("Name the new archive"), None, Some("archive.zip".into()), false);
                    }
                });
            }
            _ => {
                ui.label(tl!("Which files and folders should go in?"));
                ui.add_space(6.0);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_min_height(90.0);
                    ui.set_width(ui.available_width());
                    for f in &self.files {
                        ui.label(f.display().to_string());
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button(tl!("Add files…")).clicked() {
                        app.pick(Pick::WizardFiles, craft_dialogs::Kind::Open { multiple: true }, tl!("Files to pack"), None, None, false);
                    }
                    if ui.button(tl!("Add a folder…")).clicked() {
                        app.pick(Pick::WizardFiles, craft_dialogs::Kind::Folder, tl!("Folder to pack"), None, None, false);
                    }
                });
            }
        }
    }

    fn last_step(&self) -> usize {
        if self.path == Path::Unpack { 1 } else { 2 }
    }

    fn can_go_on(&self) -> bool {
        match self.step {
            0 => true,
            1 if self.path == Path::Create => !self.archive.trim().is_empty(),
            1 => PathBuf::from(self.archive.trim()).is_file(),
            _ => !self.files.is_empty(),
        }
    }

    fn finish(&mut self, app: &mut App) {
        let archive = PathBuf::from(self.archive.trim());
        match self.path {
            Path::Unpack => {
                let d = super::extract::ExtractDialog::new(app, vec![archive], Vec::new(), String::new());
                app.ui_state.extract = Some(d);
            }
            Path::Create => {
                let mut d = super::add::AddDialog::new(app, self.files.clone());
                d.set_archive(archive);
                app.ui_state.add = Some(d);
            }
            Path::AddTo => {
                let mut d = super::add::AddDialog::new(app, self.files.clone());
                d.set_archive(archive);
                app.ui_state.add = Some(d);
            }
        }
    }

    pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> Flow {
        let mut flow = Flow::Keep;
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if let Some(p) = dropped.first()
            && self.step == 1
            && self.path != Path::Create
        {
            self.archive = p.display().to_string();
        } else if !dropped.is_empty() && self.step == 2 {
            self.files.extend(dropped);
        }
        egui::Modal::new(egui::Id::unique("ac-wizard")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.horizontal(|ui| {
                icons::show(ui, Icon::Guide, 48.0);
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.heading(tl!("Guide"));
                    self.page(ui, app);
                });
            });
            if self.help {
                ui.separator();
                ui.label(egui::RichText::new(tl!("Next goes on, Back goes one page back, Cancel closes the guide. At the end, the usual window opens with your choices filled in, so you can still change anything.")).weak());
            }
            ui.add_space(8.0);
            ui.separator();
            let btn_size = egui::vec2(88.0, 26.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(tl!("Help")).min_size(btn_size)).clicked() {
                    self.help = !self.help;
                }
                ui.add_space(8.0);
                if ui.add(egui::Button::new(tl!("Cancel")).min_size(btn_size)).clicked() {
                    flow = Flow::Close;
                }
                let last = self.step >= self.last_step();
                let next = if last { tl!("Finish") } else { tl!("Next >") };
                if ui.add_enabled(self.can_go_on(), egui::Button::new(next).min_size(btn_size)).clicked() {
                    if last {
                        self.finish(app);
                        flow = Flow::Close;
                    } else {
                        self.step += 1;
                    }
                }
                if ui.add_enabled(self.step > 0, egui::Button::new(tl!("< Back")).min_size(btn_size)).clicked() {
                    self.step -= 1;
                }
            });
        });
        flow
    }
}
