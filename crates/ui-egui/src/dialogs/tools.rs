//! Smaller windows: confirmations, selection by pattern, repair, convert, recovery data, speed
//! test and bookmarks.

use super::{group, ok_cancel, window, Flow};
use crate::rows::RowRef;
use crate::state::{App, Pick};
use crate::{tf, tl};
use archivecraft_engine::bench::BenchResult;
use archivecraft_engine::model::util::{human_size, wildcard};
use archivecraft_engine::model::{format::LEVELS, CreateOptions, Format};
use std::path::PathBuf;

pub enum ConfirmWhat {
    Recycle(Vec<PathBuf>),
    RemoveEntries(Vec<String>),
}

pub struct Confirm {
    what: ConfirmWhat,
}

impl Confirm {
    pub fn recycle(paths: Vec<PathBuf>) -> Self {
        Confirm { what: ConfirmWhat::Recycle(paths) }
    }
    pub fn remove_entries(items: Vec<String>) -> Self {
        Confirm { what: ConfirmWhat::RemoveEntries(items) }
    }

    pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> Flow {
        let mut flow = Flow::Keep;
        egui::Modal::new(egui::Id::unique("ac-confirm")).show(ctx, |ui| {
            ui.set_max_width(440.0);
            let text = match &self.what {
                ConfirmWhat::Recycle(p) if p.len() == 1 => tf!("Move \"{n}\" to the recycle bin?", n = p[0].display()),
                ConfirmWhat::Recycle(p) => tf!("Move these {n} items to the recycle bin?", n = p.len()),
                ConfirmWhat::RemoveEntries(i) if i.len() == 1 => tf!("Remove \"{n}\" from the archive? This cannot be undone.", n = i[0]),
                ConfirmWhat::RemoveEntries(i) => tf!("Remove these {n} items from the archive? This cannot be undone.", n = i.len()),
            };
            ui.horizontal(|ui| {
                crate::icons::show(ui, crate::icons::Icon::Delete, 32.0);
                ui.add(egui::Label::new(text).wrap());
            });
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tl!("No")).clicked() {
                    flow = Flow::Close;
                }
                if ui.button(tl!("Yes")).clicked() {
                    match &self.what {
                        ConfirmWhat::Recycle(p) => {
                            if let Err(e) = archivecraft_engine::send_to_recycle_bin(p) {
                                app.note(e.to_string());
                            }
                            app.refresh();
                        }
                        ConfirmWhat::RemoveEntries(i) => app.start_delete_entries(i.clone()),
                    }
                    flow = Flow::Close;
                }
            });
        });
        flow
    }
}

pub struct PatternPrompt {
    select: bool,
    text: String,
}

impl PatternPrompt {
    pub fn new(select: bool) -> Self {
        PatternPrompt { select, text: "*".into() }
    }

    pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> Flow {
        let mut flow = Flow::Keep;
        egui::Modal::new(egui::Id::unique("ac-pattern")).show(ctx, |ui| {
            ui.label(if self.select { tl!("Select names like:") } else { tl!("Unselect names like:") });
            let r = ui.text_edit_singleline(&mut self.text);
            r.request_focus();
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let (ok, cancel) = ok_cancel(ui, &tl!("OK"), true);
            if ok || enter {
                for (i, row) in app.rows.iter().enumerate() {
                    if row.target != RowRef::Parent && wildcard(&self.text, &row.name) {
                        if self.select {
                            app.selected.insert(i);
                        } else {
                            app.selected.remove(&i);
                        }
                    }
                }
                flow = Flow::Close;
            } else if cancel {
                flow = Flow::Close;
            }
        });
        flow
    }
}

pub struct RepairDialog {
    pub archive: PathBuf,
    pub out: String,
    pub treat_as: Option<Format>,
}

impl RepairDialog {
    pub fn new(archive: PathBuf) -> Self {
        let out = archive.parent().map(|p| p.display().to_string()).unwrap_or_default();
        RepairDialog { archive, out, treat_as: None }
    }

    pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> Flow {
        let mut flow = Flow::Keep;
        let mut open = true;
        window(tl!("Repair an archive"), "ac-repair").open(&mut open).resizable(false).default_width(460.0).show(ctx, |ui| {
            ui.label(egui::RichText::new(self.archive.display().to_string()).strong());
            ui.label(if archivecraft_engine::par2::find_for(&self.archive).is_some() {
                tl!("Recovery data was found: damaged parts can be rebuilt exactly.")
            } else {
                tl!("No recovery data: every item that can still be read is saved into a new archive.")
            });
            ui.add_space(6.0);
            ui.label(tl!("Put the repaired archive in"));
            super::text_row(ui, &mut self.out, None, |ui, out| {
                if ui.button(tl!("Browse…")).clicked() {
                    app.pick(Pick::RepairOut, craft_dialogs::Kind::Folder, tl!("Where should the repaired archive go?"), Some(PathBuf::from(out)), None, false);
                }
            });
            group(ui, &tl!("Treat the damaged file as"), |ui| {
                ui.radio_value(&mut self.treat_as, None, tl!("Work it out"));
                ui.horizontal_wrapped(|ui| {
                    for f in Format::WRITABLE {
                        ui.radio_value(&mut self.treat_as, Some(f), tl!(f.label()));
                    }
                });
            });
            let (ok, cancel) = ok_cancel(ui, &tl!("Repair"), !self.out.trim().is_empty());
            if ok {
                app.start_repair(self.archive.clone(), PathBuf::from(self.out.trim()), self.treat_as);
                flow = Flow::Close;
            } else if cancel {
                flow = Flow::Close;
            }
        });
        if !open { Flow::Close } else { flow }
    }
}

pub struct ConvertDialog {
    pub sources: Vec<PathBuf>,
    pub out: String,
    pub format: Format,
    pub level: u32,
    pub password: String,
    pub recycle: bool,
}

impl ConvertDialog {
    pub fn new(sources: Vec<PathBuf>, out: PathBuf) -> Self {
        ConvertDialog { sources, out: out.display().to_string(), format: Format::SevenZ, level: 5, password: String::new(), recycle: false }
    }

    pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> Flow {
        let mut flow = Flow::Keep;
        let mut open = true;
        window(tl!("Convert archives"), "ac-convert").open(&mut open).resizable(false).default_width(480.0).show(ctx, |ui| {
            if self.sources.is_empty() {
                ui.label(tl!("Choose archives in the list first, or open one."));
            }
            for s in &self.sources {
                ui.label(egui::RichText::new(s.display().to_string()).weak());
            }
            group(ui, &tl!("Into"), |ui| {
                ui.horizontal_wrapped(|ui| {
                    for f in Format::WRITABLE {
                        ui.radio_value(&mut self.format, f, tl!(f.label()));
                    }
                });
                egui::ComboBox::from_id_salt("ac-conv-level").selected_text(tl!(archivecraft_engine::model::format::level_name(self.level))).show_ui(ui, |ui| {
                    for (l, n) in LEVELS {
                        ui.selectable_value(&mut self.level, l, n);
                    }
                });
                ui.add_enabled_ui(self.format.supports_password(), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(tl!("Password (optional)"));
                        ui.add(egui::TextEdit::singleline(&mut self.password).password(true));
                    });
                });
            });
            ui.label(tl!("Save them in"));
            super::text_row(ui, &mut self.out, None, |ui, out| {
                if ui.button(tl!("Browse…")).clicked() {
                    app.pick(Pick::ConvertOut, craft_dialogs::Kind::Folder, tl!("Save converted archives in"), Some(PathBuf::from(out)), None, false);
                }
            });
            ui.checkbox(&mut self.recycle, tl!("Move the originals to the recycle bin afterwards"));
            let (ok, cancel) = ok_cancel(ui, &tl!("Convert"), !self.sources.is_empty());
            if ok {
                let mut o = CreateOptions { format: self.format, level: self.level, ..app.default_create_options() };
                o.password = if self.password.is_empty() || !self.format.supports_password() { None } else { Some(self.password.clone()) };
                app.start_convert(self.sources.clone(), PathBuf::from(self.out.trim()), o, self.recycle);
                flow = Flow::Close;
            } else if cancel {
                flow = Flow::Close;
            }
        });
        if !open { Flow::Close } else { flow }
    }
}

pub struct RecoveryDialog {
    pub archive: PathBuf,
    pub percent: u32,
}

impl RecoveryDialog {
    pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> Flow {
        let mut flow = Flow::Keep;
        egui::Modal::new(egui::Id::unique("ac-recovery")).show(ctx, |ui| {
            ui.set_width(400.0);
            ui.heading(tl!("Add recovery data"));
            ui.label(tf!("PAR2 files are written next to {n}. With them, Repair can rebuild damaged parts.", n = self.archive.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.percent).range(1..=100).suffix(" %"));
                ui.label(tl!("of the archive's size"));
            });
            let (ok, cancel) = ok_cancel(ui, &tl!("Write"), true);
            if ok {
                app.start_recovery(self.archive.clone(), self.percent);
                flow = Flow::Close;
            } else if cancel {
                flow = Flow::Close;
            }
        });
        flow
    }
}

pub struct BenchWindow {
    format: Format,
    level: u32,
    size_mb: usize,
    pub results: Vec<BenchResult>,
}

impl Default for BenchWindow {
    fn default() -> Self {
        BenchWindow { format: Format::SevenZ, level: 5, size_mb: 32, results: Vec::new() }
    }
}

impl BenchWindow {
    pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> Flow {
        let mut open = true;
        window(tl!("Speed test"), "ac-bench").open(&mut open).default_width(480.0).show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                for f in Format::WRITABLE {
                    ui.radio_value(&mut self.format, f, tl!(f.label()));
                }
            });
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("ac-bench-level").selected_text(tl!(archivecraft_engine::model::format::level_name(self.level))).show_ui(ui, |ui| {
                    for (l, n) in LEVELS {
                        ui.selectable_value(&mut self.level, l, n);
                    }
                });
                ui.add(egui::DragValue::new(&mut self.size_mb).range(1..=1024).suffix(" MB"));
                if ui.add_enabled(app.job.is_none(), egui::Button::new(tl!("Run"))).clicked() {
                    app.start_bench(self.format, self.level, self.size_mb);
                }
            });
            ui.separator();
            egui::Grid::new("ac-bench-grid").striped(true).num_columns(5).show(ui, |ui| {
                for h in [tl!("Type"), tl!("Level"), tl!("Packs"), tl!("Unpacks"), tl!("Size")] {
                    ui.strong(h);
                }
                ui.end_row();
                for r in &self.results {
                    ui.label(tl!(r.format.label()));
                    ui.label(tl!(archivecraft_engine::model::format::level_name(r.level)));
                    ui.label(format!("{:.1} MB/s", r.pack_speed));
                    ui.label(format!("{:.1} MB/s", r.unpack_speed));
                    ui.label(format!("{} → {}", human_size(r.input, false), human_size(r.output, false)));
                    ui.end_row();
                }
            });
        });
        if open { Flow::Keep } else { Flow::Close }
    }
}

#[derive(Default)]
pub struct BookmarksWindow {
    selected: Option<usize>,
}

impl BookmarksWindow {
    pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> Flow {
        let mut open = true;
        let mut list = app.prefs.get().saved.bookmarks.clone();
        let mut changed = false;
        let mut go = None;
        window(tl!("Bookmarks"), "ac-bookmarks").open(&mut open).default_width(480.0).show(ctx, |ui| {
            if list.is_empty() {
                ui.label(tl!("No bookmarks yet. Use Bookmarks > Bookmark this place."));
            }
            for (i, b) in list.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui.radio(self.selected == Some(i), "").clicked() {
                        self.selected = Some(i);
                    }
                    changed |= ui.add(egui::TextEdit::singleline(&mut b.name).desired_width(160.0)).changed();
                    ui.label(egui::RichText::new(b.path.display().to_string()).weak());
                });
            }
            ui.separator();
            ui.horizontal(|ui| {
                if let Some(i) = self.selected.filter(|i| *i < list.len()) {
                    if ui.button(tl!("Go there")).clicked() {
                        go = Some(list[i].clone());
                    }
                    if ui.button(tl!("Up")).clicked() && i > 0 {
                        list.swap(i, i - 1);
                        self.selected = Some(i - 1);
                        changed = true;
                    }
                    if ui.button(tl!("Down")).clicked() && i + 1 < list.len() {
                        list.swap(i, i + 1);
                        self.selected = Some(i + 1);
                        changed = true;
                    }
                    if ui.button(tl!("Remove")).clicked() {
                        list.remove(i);
                        self.selected = None;
                        changed = true;
                    }
                }
            });
        });
        if changed {
            app.prefs.update(|p| p.saved.bookmarks = list);
        }
        if let Some(b) = go {
            app.go_bookmark(&b);
        }
        if open { Flow::Keep } else { Flow::Close }
    }
}

pub fn keys_help() -> String {
    [
        tl!("Enter — open the chosen item"),
        tl!("Backspace — up one level"),
        tl!("Ctrl+O — open an archive"),
        tl!("Ctrl+Shift+A — pack into an archive"),
        tl!("Ctrl+E — unpack to a folder; Ctrl+Shift+E — unpack here"),
        tl!("Ctrl+T — check; Ctrl+I — details; F3 — look inside"),
        tl!("Delete — remove; Ctrl+F — search; Ctrl+D — bookmark"),
        tl!("Ctrl+A — select everything; + and - — select by pattern"),
        tl!("F5 — reload; Ctrl+, — preferences"),
    ]
    .join("\n")
}

pub fn about_text() -> String {
    tf!(
        "ArchiveCraft {v}\nAn open-source archive manager, written in Rust. Licensed MIT OR Apache-2.0.\nMakes and opens ZIP, 7z, tar, tar.gz, tar.xz and tar.zst{rar}.",
        v = env!("CARGO_PKG_VERSION"),
        rar = if cfg!(feature = "rar") { tl!("; opens RAR (read only)") } else { String::new() }
    )
}
