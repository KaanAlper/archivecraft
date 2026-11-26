//! "Pack into an archive": archive name, type, level, dictionary, split size, password and the
//! other packing options, on five tabs. Options the chosen type cannot do are greyed out.

use super::{group, ok_cancel, tabs, window, Flow};
use crate::state::{App, Pick};
use crate::tl;
use archivecraft_engine::model::util::{human_size, join_patterns, parse_patterns, parse_size};
use archivecraft_engine::model::{format::LEVELS, CreateOptions, Format, PathMode, TimeFilter, UpdateMode};
use archivecraft_engine::ops;
use std::path::PathBuf;

pub struct AddDialog {
    pub sources: Vec<PathBuf>,
    pub archive: String,
    pub opts: CreateOptions,
    /// Adding to an archive that exists: its type is fixed.
    pub fixed_format: Option<Format>,
    pub tab: usize,
    pub volume_text: String,
    pub dict_mb: String,
    pub exclude: String,
    pub store: String,
    pub comment: String,
    pub per_file: bool,
    pub recovery: bool,
    pw: Option<PasswordForm>,
    pub password: String,
    error: Option<String>,
}

#[derive(Default)]
struct PasswordForm {
    first: String,
    second: String,
    show: bool,
    names: bool,
    legacy: bool,
}

impl AddDialog {
    pub fn new(app: &App, sources: Vec<PathBuf>) -> Self {
        let opts = app.default_create_options();
        let mut archive = if sources.is_empty() { app.disk_dir().join(format!("archive.{}", opts.format.extension())) } else { ops::archive_path_for(&sources, opts.format) };
        if let Some(dir) = &app.prefs.get().paths.archives_folder
            && let Some(name) = archive.file_name()
        {
            archive = dir.join(name);
        }
        let p = app.prefs.get();
        AddDialog {
            sources,
            archive: archive.display().to_string(),
            store: join_patterns(&opts.store_patterns),
            recovery: false,
            opts,
            fixed_format: None,
            tab: 0,
            volume_text: String::new(),
            dict_mb: String::new(),
            exclude: String::new(),
            comment: String::new(),
            per_file: false,
            pw: None,
            password: app.session_password.clone().unwrap_or_default(),
            error: None,
        }
        .with_recovery_percent(p.compression.recovery_percent)
    }

    fn with_recovery_percent(mut self, p: u32) -> Self {
        self.opts.recovery_percent = p.max(1);
        self
    }

    /// Add to the archive open in the window.
    pub fn to_open_archive(app: &App, sources: Vec<PathBuf>) -> Self {
        let mut d = AddDialog::new(app, sources);
        if let Some(v) = app.open_archive_view() {
            d.archive = v.path.display().to_string();
            d.fixed_format = Some(v.listing.format);
            d.opts.format = v.listing.format;
            if let Some(p) = &v.password {
                d.password = p.clone();
            }
        }
        d
    }

    pub fn set_archive(&mut self, p: PathBuf) {
        self.archive = p.display().to_string();
        self.sync_format_from_name();
    }

    pub fn add_sources(&mut self, paths: Vec<PathBuf>) {
        for p in paths {
            if !self.sources.contains(&p) {
                self.sources.push(p);
            }
        }
    }

    fn sync_format_from_name(&mut self) {
        let p = PathBuf::from(&self.archive);
        if p.exists()
            && let Some(f) = archivecraft_engine::formats::detect::detect(&p).filter(|f| f.can_write())
        {
            self.fixed_format = Some(f);
            self.opts.format = f;
        } else {
            self.fixed_format = None;
            if let Some(f) = Format::from_name(&self.archive).filter(|f| f.can_write()) {
                self.opts.format = f;
            }
        }
    }

    fn set_format(&mut self, f: Format) {
        let old = self.opts.format;
        self.opts.format = f;
        let lower = self.archive.to_lowercase();
        let old_ext = format!(".{}", old.extension());
        if lower.ends_with(&old_ext) {
            self.archive.truncate(self.archive.len() - old_ext.len());
            self.archive.push('.');
            self.archive.push_str(f.extension());
        }
    }

    fn general(&mut self, ui: &mut egui::Ui, app: &mut App) {
        ui.label(tl!("Archive file"));
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut self.archive).desired_width(ui.available_width() - 90.0));
            if r.lost_focus() {
                self.sync_format_from_name();
            }
            if ui.button(tl!("Browse…")).clicked() {
                let p = PathBuf::from(&self.archive);
                app.pick(Pick::AddArchiveName, craft_dialogs::Kind::Save, tl!("Where should the archive go?"), p.parent().map(PathBuf::from), p.file_name().map(|n| n.to_string_lossy().to_string()), false);
            }
        });
        ui.add_space(6.0);
        ui.columns(2, |c| {
            group(&mut c[0], &tl!("Archive type"), |ui| {
                ui.add_enabled_ui(self.fixed_format.is_none(), |ui| {
                    egui::Grid::new("ac-add-fmt").num_columns(2).show(ui, |ui| {
                        for (i, f) in Format::WRITABLE.iter().enumerate() {
                            if ui.radio(self.opts.format == *f, tl!(f.label())).clicked() {
                                self.set_format(*f);
                            }
                            if i % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });
                });
            });
            c[0].add_space(6.0);
            let f = self.opts.format;
            c[0].label(tl!("How hard to compress"));
            c[0].add_enabled_ui(f.supports_levels(), |ui| {
                egui::ComboBox::from_id_salt("ac-add-level").width(200.0).selected_text(tl!(archivecraft_engine::model::format::level_name(self.opts.level))).show_ui(ui, |ui| {
                    for (l, name) in LEVELS {
                        ui.selectable_value(&mut self.opts.level, l, name);
                    }
                });
            });
            c[0].label(tl!("Dictionary size (MB)"));
            c[0].add_enabled_ui(f.supports_dictionary() && self.opts.level > 0, |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.dict_mb).hint_text(tl!("level default")).desired_width(120.0));
            });
            c[0].label(tl!("Split into parts of"));
            c[0].horizontal(|ui| {
                ui.add_enabled_ui(self.fixed_format.is_none(), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.volume_text).hint_text(tl!("e.g. 700m, 4g")).desired_width(110.0));
                    egui::ComboBox::from_id_salt("ac-add-vol").width(80.0).selected_text(tl!("Presets")).show_ui(ui, |ui| {
                        if ui.selectable_label(false, tl!("No split")).clicked() {
                            self.volume_text.clear();
                        }
                        for v in app.prefs.get().compression.volume_presets.clone() {
                            if ui.selectable_label(false, human_size(v, false)).clicked() {
                                self.volume_text = format!("{}m", v / (1024 * 1024));
                            }
                        }
                    });
                });
            });

            c[1].label(tl!("When the archive already exists"));
            egui::ComboBox::from_id_salt("ac-add-update").width(c[1].available_width()).selected_text(tl!(self.opts.update.label())).show_ui(&mut c[1], |ui| {
                for m in UpdateMode::ALL {
                    ui.selectable_value(&mut self.opts.update, m, tl!(m.label()));
                }
            });
            c[1].add_space(6.0);
            group(&mut c[1], &tl!("Packing options"), |ui| {
                ui.checkbox(&mut self.opts.delete_sources, tl!("Remove the originals once packed"));
                ui.checkbox(&mut self.opts.test_after, tl!("Check the archive after packing"));
                ui.add_enabled_ui(f.supports_solid(), |ui| ui.checkbox(&mut self.opts.solid, tl!("One solid block (smaller, slower to pick from)")));
                ui.checkbox(&mut self.recovery, tl!("Add recovery data (PAR2)"));
                ui.add_enabled_ui(self.fixed_format.is_none(), |ui| ui.checkbox(&mut self.per_file, tl!("A separate archive for each item")));
            });
            c[1].add_space(6.0);
            let lock_label = if self.password.is_empty() { tl!("Set a password…") } else { tl!("Password is set — change…") };
            if c[1].add_enabled(f.supports_password(), egui::Button::new(lock_label).min_size(egui::vec2(c[1].available_width(), 26.0))).clicked() {
                self.pw = Some(PasswordForm { first: self.password.clone(), second: self.password.clone(), names: self.opts.encrypt_names, legacy: self.opts.zip_legacy_crypto, show: false });
            }
            if !f.supports_password() {
                c[1].label(egui::RichText::new(tl!("This type cannot hold a password.")).weak());
            }
        });
    }

    fn advanced(&mut self, ui: &mut egui::Ui) {
        group(ui, &tl!("Recovery data"), |ui| {
            ui.add_enabled_ui(self.recovery, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.opts.recovery_percent).range(1..=100).suffix(" %"));
                    ui.label(tl!("of the archive size, written next to it as .par2 files"));
                });
            });
        });
        ui.add_space(6.0);
        group(ui, &tl!("Work"), |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.opts.threads).range(1..=256));
                ui.label(tl!("threads for 7z packing"));
            });
        });
    }

    fn files(&mut self, ui: &mut egui::Ui, app: &mut App) {
        ui.label(tl!("What to pack"));
        let mut remove: Option<usize> = None;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::ScrollArea::vertical().max_height(120.0).auto_shrink([false, true]).show(ui, |ui| {
                if self.sources.is_empty() {
                    ui.label(egui::RichText::new(tl!("Nothing yet: add files or folders, or drop them on the main window.")).weak());
                }
                for (i, s) in self.sources.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.small_button("✕").on_hover_text(tl!("Leave this out")).clicked() {
                            remove = Some(i);
                        }
                        ui.label(s.display().to_string());
                    });
                }
            });
        });
        if let Some(i) = remove {
            self.sources.remove(i);
        }
        ui.horizontal(|ui| {
            if ui.button(tl!("Add files…")).clicked() {
                app.pick(Pick::AddFiles, craft_dialogs::Kind::Open { multiple: true }, tl!("Files to pack"), None, None, false);
            }
            if ui.button(tl!("Add a folder…")).clicked() {
                app.pick(Pick::AddFolder, craft_dialogs::Kind::Folder, tl!("Folder to pack"), None, None, false);
            }
        });
        ui.add_space(6.0);
        ui.label(tl!("Leave out names like (e.g. *.tmp *.log)"));
        ui.text_edit_singleline(&mut self.exclude);
        ui.label(tl!("Store without compressing"));
        ui.text_edit_singleline(&mut self.store);
        ui.label(tl!("Folder names inside the archive"));
        egui::ComboBox::from_id_salt("ac-add-paths").width(300.0).selected_text(tl!(self.opts.path_mode.label())).show_ui(ui, |ui| {
            for m in PathMode::ALL {
                ui.selectable_value(&mut self.opts.path_mode, m, tl!(m.label()));
            }
        });
    }

    fn time(&mut self, ui: &mut egui::Ui) {
        group(ui, &tl!("Times"), |ui| {
            ui.checkbox(&mut self.opts.store_mtime, tl!("Keep each file's last-changed time"));
        });
        ui.add_space(6.0);
        group(ui, &tl!("Which files"), |ui| {
            egui::ComboBox::from_id_salt("ac-add-time").width(280.0).selected_text(tl!(self.opts.time_filter.label())).show_ui(ui, |ui| {
                for t in TimeFilter::ALL {
                    ui.selectable_value(&mut self.opts.time_filter, t, tl!(t.label()));
                }
            });
        });
    }

    fn note(&mut self, ui: &mut egui::Ui, app: &mut App) {
        let ok = self.opts.format.supports_comment();
        ui.horizontal(|ui| {
            ui.label(tl!("Note stored with the archive"));
            if ui.add_enabled(ok, egui::Button::new(tl!("Load from a text file…"))).clicked() {
                app.pick(Pick::CommentFile, craft_dialogs::Kind::Open { multiple: false }, tl!("Text file with the note"), None, None, false);
            }
        });
        ui.add_enabled(ok, egui::TextEdit::multiline(&mut self.comment).desired_rows(10).desired_width(f32::INFINITY));
        if !ok {
            ui.label(egui::RichText::new(tl!("Only ZIP archives can hold a note.")).weak());
        }
    }

    fn password_window(&mut self, ctx: &egui::Context) {
        let Some(form) = &mut self.pw else { return };
        let mut close = None;
        let fmt = self.opts.format;
        egui::Modal::new(egui::Id::unique("ac-add-pw")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.heading(tl!("Lock with a password"));
            ui.add_space(4.0);
            ui.label(tl!("Password"));
            ui.add(egui::TextEdit::singleline(&mut form.first).password(!form.show).desired_width(f32::INFINITY));
            ui.label(tl!("Type it again"));
            ui.add(egui::TextEdit::singleline(&mut form.second).password(!form.show).desired_width(f32::INFINITY));
            ui.checkbox(&mut form.show, tl!("Show the password"));
            ui.add_enabled_ui(fmt.supports_name_encryption(), |ui| ui.checkbox(&mut form.names, tl!("Also hide the file names")));
            ui.add_enabled_ui(fmt == Format::Zip, |ui| ui.checkbox(&mut form.legacy, tl!("Old weak ZIP locking (only for very old tools)")));
            ui.label(egui::RichText::new(if form.legacy { tl!("Weak locking: easy to break. Use only if you must.") } else { tl!("Strong AES-256 locking.") }).weak());
            let mismatch = form.first != form.second;
            if mismatch {
                ui.colored_label(crate::icons::RED, tl!("The two passwords differ."));
            }
            let (ok, cancel) = ok_cancel(ui, &tl!("OK"), !mismatch);
            if ok {
                close = Some(true);
            } else if cancel {
                close = Some(false);
            }
        });
        match close {
            Some(true) => {
                let f = self.pw.take().unwrap_or_default();
                self.password = f.first;
                self.opts.encrypt_names = f.names;
                self.opts.zip_legacy_crypto = f.legacy;
            }
            Some(false) => self.pw = None,
            None => {}
        }
    }

    fn build(&mut self) -> Result<CreateOptions, String> {
        let mut o = self.opts.clone();
        o.password = if o.format.supports_password() && !self.password.is_empty() { Some(self.password.clone()) } else { None };
        o.volume_size = if self.volume_text.trim().is_empty() || self.fixed_format.is_some() { None } else { Some(parse_size(&self.volume_text).ok_or_else(|| tl!("The part size is not a size, e.g. 700m"))?) };
        o.dictionary = if self.dict_mb.trim().is_empty() { None } else { Some(self.dict_mb.trim().parse::<f64>().map_err(|_| tl!("The dictionary size must be a number of megabytes"))?.mul_add(1024.0 * 1024.0, 0.0) as u32) };
        o.exclude = parse_patterns(&self.exclude);
        o.store_patterns = parse_patterns(&self.store);
        o.comment = if o.format.supports_comment() && !self.comment.trim().is_empty() { Some(self.comment.clone()) } else { None };
        o.recovery_percent = if self.recovery { o.recovery_percent.max(1) } else { 0 };
        if self.sources.is_empty() {
            return Err(tl!("Add at least one file or folder to pack."));
        }
        if self.archive.trim().is_empty() {
            return Err(tl!("Give the archive a name."));
        }
        Ok(o)
    }

    pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> Flow {
        let mut flow = Flow::Keep;
        let title = if self.fixed_format.is_some() { tl!("Add to an archive") } else { tl!("Pack into an archive") };
        let mut open = true;
        window(title, "ac-add").open(&mut open).default_width(620.0).show(ctx, |ui| {
            tabs(ui, &mut self.tab, &[tl!("Basics"), tl!("Extras"), tl!("Files"), tl!("Dates"), tl!("Note")]);
            ui.set_min_height(330.0);
            match self.tab {
                0 => self.general(ui, app),
                1 => self.advanced(ui),
                2 => self.files(ui, app),
                3 => self.time(ui),
                _ => self.note(ui, app),
            }
            if let Some(e) = &self.error {
                ui.colored_label(crate::icons::RED, e);
            }
            let (ok, cancel) = ok_cancel(ui, &tl!("Pack"), true);
            if cancel {
                flow = Flow::Close;
            }
            if ok {
                match self.build() {
                    Ok(o) => {
                        app.prefs.update(|p| {
                            p.saved.path_mode = o.path_mode;
                            if self.fixed_format.is_none() {
                                p.compression.format = o.format;
                                p.compression.level = o.level;
                            }
                        });
                        app.start_create(self.sources.clone(), PathBuf::from(self.archive.trim()), o, self.per_file);
                        flow = Flow::Close;
                    }
                    Err(e) => self.error = Some(e),
                }
            }
        });
        self.password_window(ctx);
        if !open { Flow::Close } else { flow }
    }
}
