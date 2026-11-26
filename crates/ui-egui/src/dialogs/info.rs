//! Archive details: totals, compression ratio with a bar, and the archive's note (view/edit for
//! ZIP).

use super::{tabs, window, Flow};
use crate::state::{App, Pick};
use crate::{icons, tl};
use archivecraft_engine::model::util::group_digits;
use archivecraft_engine::model::Listing;
use archivecraft_engine::{formats, par2};
use std::path::{Path, PathBuf};

pub struct InfoDialog {
    pub path: PathBuf,
    pub listing: Result<Listing, String>,
    pub tab: usize,
    pub comment: String,
    original: String,
    recovery: bool,
}

impl InfoDialog {
    pub fn for_archive(app: &App, path: &Path, tab: usize) -> Self {
        let listing = match app.open_archive_view().filter(|v| v.path == path) {
            Some(v) => Ok(v.listing.clone()),
            None => formats::list(path, app.session_password.as_deref()).map_err(|e| e.to_string()),
        };
        let comment = listing.as_ref().ok().and_then(|l| l.comment.clone()).unwrap_or_default();
        InfoDialog { path: path.to_path_buf(), listing, tab, original: comment.clone(), comment, recovery: par2::find_for(path).is_some() }
    }

    fn summary(&self, ui: &mut egui::Ui, l: &Listing) {
        ui.horizontal(|ui| {
            // The ratio bar: the packed share in teal, from the bottom up.
            let ratio = (l.ratio_percent() as f32 / 100.0).clamp(0.0, 1.0);
            let (r, _) = ui.allocate_exact_size(egui::vec2(34.0, 300.0), egui::Sense::hover());
            let bar = egui::Rect::from_min_max(r.min, egui::pos2(r.left() + 22.0, r.bottom()));
            ui.painter().rect_filled(bar, egui::CornerRadius::same(3), icons::AMBER.gamma_multiply(0.35));
            let fill = egui::Rect::from_min_max(egui::pos2(bar.left(), bar.bottom() - bar.height() * ratio), bar.max);
            ui.painter().rect_filled(fill, egui::CornerRadius::same(3), icons::TEAL);
            ui.painter().text(egui::pos2(bar.right() + 3.0, fill.top()), egui::Align2::LEFT_CENTER, format!("{}%", l.ratio_percent()), egui::FontId::proportional(12.0), ui.visuals().strong_text_color());
            ui.add_space(18.0);
            ui.vertical(|ui| {
                ui.heading(tl!(l.format.describe()));
                ui.add_space(4.0);
                let yes = tl!("yes");
                let no = tl!("no");
                let rows: Vec<(String, String)> = vec![
                    (tl!("Methods"), l.methods.join(", ")),
                    (String::new(), String::new()),
                    (tl!("Folders"), group_digits(l.folder_count() as u64)),
                    (tl!("Files"), group_digits(l.file_count() as u64)),
                    (tl!("Unpacked size"), format!("{} bytes", group_digits(l.total_size()))),
                    (tl!("Packed size"), format!("{} bytes", group_digits(l.packed_size()))),
                    (tl!("On disk"), format!("{} bytes", group_digits(l.archive_size))),
                    (tl!("Ratio"), format!("{}%", l.ratio_percent())),
                    (String::new(), String::new()),
                    (tl!("Parts"), l.volumes.to_string()),
                    (tl!("Locked items"), if l.any_encrypted() { yes.clone() } else { no.clone() }),
                    (tl!("Locked names"), if l.names_encrypted { yes.clone() } else { no.clone() }),
                    (tl!("Solid"), if l.solid { yes.clone() } else { no.clone() }),
                    (tl!("Note"), if l.comment.as_deref().is_some_and(|c| !c.is_empty()) { yes.clone() } else { no.clone() }),
                    (tl!("Recovery data"), if self.recovery { yes } else { no }),
                ];
                egui::Grid::new("ac-info-grid").num_columns(2).spacing([40.0, 3.0]).show(ui, |ui| {
                    for (k, v) in rows {
                        if k.is_empty() {
                            ui.separator();
                            ui.separator();
                        } else {
                            ui.label(k);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(v));
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
                ui.label(egui::RichText::new(self.path.display().to_string()).weak());
            });
        });
    }

    pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> Flow {
        let mut flow = Flow::Keep;
        let mut open = true;
        window(tl!("Archive details"), "ac-info").open(&mut open).default_width(440.0).show(ctx, |ui| {
            tabs(ui, &mut self.tab, &[tl!("Summary"), tl!("Note")]);
            match &self.listing {
                Err(e) => {
                    ui.colored_label(icons::RED, e);
                }
                Ok(l) => {
                    let l = l.clone();
                    if self.tab == 0 {
                        self.summary(ui, &l);
                    } else {
                        let editable = l.format.supports_comment();
                        ui.label(tl!("The archive's note"));
                        ui.add_enabled(editable, egui::TextEdit::multiline(&mut self.comment).desired_rows(12).desired_width(f32::INFINITY));
                        if !editable {
                            ui.label(egui::RichText::new(tl!("Only ZIP archives can hold a note.")).weak());
                        }
                        ui.horizontal(|ui| {
                            if ui.add_enabled(editable, egui::Button::new(tl!("Load from a text file…"))).clicked() {
                                app.pick(Pick::InfoCommentFile, craft_dialogs::Kind::Open { multiple: false }, tl!("Text file with the note"), None, None, false);
                            }
                            if ui.add_enabled(editable && self.comment != self.original, egui::Button::new(tl!("Save the note"))).clicked() {
                                app.start_comment(self.path.clone(), self.comment.clone());
                                flow = Flow::Close;
                            }
                        });
                    }
                }
            }
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tl!("Close")).clicked() {
                    flow = Flow::Close;
                }
            });
        });
        if !open { Flow::Close } else { flow }
    }
}
