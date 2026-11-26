//! Ask for a name: a new folder in the current disk folder, or a new name for one item.

use super::{ok_cancel, Flow};
use crate::rows::RowRef;
use crate::state::App;
use crate::tl;
use std::path::PathBuf;

pub struct NamePrompt {
    folder: PathBuf,
    current: Option<PathBuf>,
    text: String,
    error: Option<String>,
}

impl NamePrompt {
    pub fn new_folder(folder: PathBuf) -> Self {
        NamePrompt { folder, current: None, text: tl!("New folder"), error: None }
    }

    pub fn change_name(item: PathBuf) -> Self {
        let text = item.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let folder = item.parent().map(PathBuf::from).unwrap_or_default();
        NamePrompt { folder, current: Some(item), text, error: None }
    }

    fn apply(&self, target: &std::path::Path) -> Result<(), String> {
        if target.exists() {
            return Err(tl!("Something with that name is already there."));
        }
        match &self.current {
            Some(from) => std::fs::rename(from, target).map_err(|e| e.to_string()),
            None => std::fs::create_dir(target).map_err(|e| e.to_string()),
        }
    }

    pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> Flow {
        let mut flow = Flow::Keep;
        egui::Modal::new(egui::Id::unique("ac-name")).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(if self.current.is_some() { tl!("New name:") } else { tl!("Name of the new folder:") });
            let r = ui.add(egui::TextEdit::singleline(&mut self.text).desired_width(f32::INFINITY));
            r.request_focus();
            if let Some(e) = &self.error {
                ui.colored_label(crate::icons::RED, e);
            }
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let name = self.text.trim().to_string();
            let valid = !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != "..";
            let (ok, cancel) = ok_cancel(ui, &tl!("OK"), valid);
            if (ok || enter) && valid {
                let target = self.folder.join(&name);
                match self.apply(&target) {
                    Ok(()) => {
                        app.refresh();
                        app.select_target(&RowRef::Disk(target));
                        flow = Flow::Close;
                    }
                    Err(e) => self.error = Some(e),
                }
            } else if cancel {
                flow = Flow::Close;
            }
        });
        flow
    }
}

/// Asked once, on the first normal start: put ArchiveCraft into the file manager's right-click menu?
pub struct IntegrationOffer;

impl IntegrationOffer {
    pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> Flow {
        let mut flow = Flow::Keep;
        egui::Modal::new(egui::Id::unique("ac-offer")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.horizontal(|ui| {
                crate::icons::show(ui, crate::icons::Icon::Archive, 40.0);
                ui.vertical(|ui| {
                    ui.heading(tl!("Right-click menu"));
                    ui.add(egui::Label::new(tl!("Add an ArchiveCraft submenu to your file manager? Right-clicking files and folders will then offer: pack into an archive, pack into a ZIP right away, and for archives unpack here or into their own folder.")).wrap());
                    ui.label(egui::RichText::new(tl!("You can take it out again under Settings at any time.")).weak());
                });
            });
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tl!("Not now")).clicked() {
                    app.prefs.update(|p| p.integration.offered = true);
                    flow = Flow::Close;
                }
                if ui.button(tl!("Add it")).clicked() {
                    app.file_manager_menu(true);
                    flow = Flow::Close;
                }
            });
        });
        flow
    }
}
