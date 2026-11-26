//! Preferences, with a category list on the left. Changes apply on OK.

use super::{group, ok_cancel, Flow};
use crate::settings::{ListKind, Prefs, ThemeMode, ViewerKind};
use crate::state::{App, Pick};
use crate::{tf, tl};
use archivecraft_engine::integration::{self, Places, Targets};
use archivecraft_engine::model::util::{human_size, parse_size};
use archivecraft_engine::model::{format::LEVELS, Format, Overwrite};
use std::path::PathBuf;

pub struct PrefsDialog {
    pub draft: Prefs,
    page: usize,
    presets_text: String,
    defaults: Vec<String>,
    integration_note: Option<String>,
}

impl PrefsDialog {
    pub fn new(app: &App) -> Self {
        let draft = app.prefs.get().clone();
        let presets_text = draft.compression.volume_presets.iter().map(|v| format!("{}m", v / (1024 * 1024))).collect::<Vec<_>>().join(" ");
        let defaults = Places::for_user().map(|p| integration::current_defaults(&p)).unwrap_or_default();
        PrefsDialog { draft, page: 0, presets_text, defaults, integration_note: None }
    }

    pub fn set_folder(&mut self, slot: u8, p: PathBuf) {
        match slot {
            0 => self.draft.paths.start_folder = Some(p),
            1 => self.draft.paths.archives_folder = Some(p),
            _ => self.draft.paths.extract_folder = Some(p),
        }
    }

    fn folder_row(ui: &mut egui::Ui, app: &mut App, value: &mut Option<PathBuf>, slot: u8, title: &str) {
        group(ui, title, |ui| {
            ui.horizontal(|ui| {
                let mut s = value.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
                if ui.add(egui::TextEdit::singleline(&mut s).hint_text(tl!("(not set)")).desired_width(ui.available_width() - 160.0)).changed() {
                    *value = if s.trim().is_empty() { None } else { Some(PathBuf::from(s)) };
                }
                if ui.button(tl!("Browse…")).clicked() {
                    app.pick(Pick::PrefsFolder(slot), craft_dialogs::Kind::Folder, title.to_string(), value.clone(), None, false);
                }
                if ui.button(tl!("Clear")).clicked() {
                    *value = None;
                }
            });
        });
    }

    fn page_ui(&mut self, ui: &mut egui::Ui, app: &mut App) {
        let d = &mut self.draft;
        match self.page {
            0 => {
                ui.columns(2, |c| {
                    group(&mut c[0], &tl!("Light or dark"), |ui| {
                        ui.radio_value(&mut d.interface.theme, ThemeMode::System, tl!("Follow the system"));
                        ui.radio_value(&mut d.interface.theme, ThemeMode::Light, tl!("Light"));
                        ui.radio_value(&mut d.interface.theme, ThemeMode::Dark, tl!("Dark"));
                    });
                    c[0].add_space(4.0);
                    group(&mut c[0], &tl!("Toolbar"), |ui| {
                        ui.checkbox(&mut d.interface.large_buttons, tl!("Large buttons"));
                        ui.checkbox(&mut d.interface.button_labels, tl!("Words under the buttons"));
                    });
                    group(&mut c[1], &tl!("Also"), |ui| {
                        ui.checkbox(&mut d.interface.guide_on_start, tl!("Open the guide at start"));
                        ui.checkbox(&mut d.interface.show_comment_on_open, tl!("Show an archive's note when opening it"));
                        ui.checkbox(&mut d.interface.full_path_in_title, tl!("Whole path in the window title"));
                        ui.checkbox(&mut d.interface.keep_history, tl!("Remember recently opened archives"));
                    });
                });
            }
            1 => {
                group(ui, &tl!("Speed"), |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut d.system.threads).range(1..=256));
                        ui.label(tl!("threads"));
                    });
                });
                group(ui, &tl!("Problems log"), |ui| {
                    ui.checkbox(&mut d.system.log_problems_to_file, tl!("Also write problems to a log file"));
                    ui.add_enabled_ui(d.system.log_problems_to_file, |ui| {
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut d.system.log_limit_kb).range(10..=100_000));
                            ui.label(tl!("KB at most"));
                        });
                    });
                });
            }
            2 => {
                group(ui, &tl!("New archives start as"), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for f in Format::WRITABLE {
                            ui.radio_value(&mut d.compression.format, f, tl!(f.label()));
                        }
                    });
                    egui::ComboBox::from_id_salt("ac-pref-level").selected_text(tl!(archivecraft_engine::model::format::level_name(d.compression.level))).show_ui(ui, |ui| {
                        for (l, n) in LEVELS {
                            ui.selectable_value(&mut d.compression.level, l, n);
                        }
                    });
                });
                group(ui, &tl!("Part sizes offered for splitting"), |ui| {
                    ui.text_edit_singleline(&mut self.presets_text);
                    ui.label(egui::RichText::new(tl!("Separate with spaces, e.g. 100m 700m 4g")).weak());
                });
                group(ui, &tl!("Recovery data"), |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut d.compression.recovery_percent).range(1..=100).suffix(" %"));
                        ui.label(tl!("suggested size"));
                    });
                });
                group(ui, &tl!("Store without compressing"), |ui| {
                    ui.text_edit_singleline(&mut d.compression.store_patterns);
                });
            }
            3 => {
                let mut start = d.paths.start_folder.clone();
                Self::folder_row(ui, app, &mut start, 0, &tl!("Folder to start in"));
                let d = &mut self.draft;
                d.paths.start_folder = start;
                ui.checkbox(&mut d.paths.reopen_last_folder, tl!("Start where I left off instead"));
                let mut a = d.paths.archives_folder.clone();
                Self::folder_row(ui, app, &mut a, 1, &tl!("Put new archives in"));
                self.draft.paths.archives_folder = a;
                let mut e = self.draft.paths.extract_folder.clone();
                Self::folder_row(ui, app, &mut e, 2, &tl!("Unpack into"));
                let d = &mut self.draft;
                d.paths.extract_folder = e;
                ui.checkbox(&mut d.paths.add_archive_name, tl!("Inside it, a folder named after the archive"));
            }
            4 => {
                ui.columns(2, |c| {
                    group(&mut c[0], &tl!("Style"), |ui| {
                        ui.radio_value(&mut d.list.kind, ListKind::Details, tl!("Columns"));
                        ui.radio_value(&mut d.list.kind, ListKind::Names, tl!("Names only"));
                        ui.checkbox(&mut d.list.stripes, tl!("Stripes"));
                    });
                    group(&mut c[1], &tl!("Opening"), |ui| {
                        ui.radio_value(&mut d.list.single_click_opens, true, tl!("One click opens folders"));
                        ui.radio_value(&mut d.list.single_click_opens, false, tl!("Double-click opens"));
                    });
                });
                group(ui, &tl!("Items"), |ui| {
                    ui.checkbox(&mut d.list.archives_first, tl!("Archives before other files"));
                    ui.checkbox(&mut d.list.exact_sizes, tl!("Sizes in exact bytes"));
                    ui.checkbox(&mut d.list.show_seconds, tl!("Times with seconds"));
                    ui.checkbox(&mut d.list.show_hidden, tl!("Show hidden files"));
                });
            }
            5 => {
                group(ui, &tl!("Looking at a file uses"), |ui| {
                    ui.radio_value(&mut d.viewer.kind, ViewerKind::Internal, tl!("ArchiveCraft's own viewer"));
                    ui.radio_value(&mut d.viewer.kind, ViewerKind::SystemApp, tl!("The program the system picks"));
                    ui.radio_value(&mut d.viewer.kind, ViewerKind::External, tl!("This program:"));
                    ui.radio_value(&mut d.viewer.kind, ViewerKind::Ask, tl!("Ask me (built-in viewer for now)"));
                });
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut d.viewer.external_program).desired_width(ui.available_width() - 90.0));
                    if ui.button(tl!("Browse…")).clicked() {
                        app.pick(Pick::ExternalViewer, craft_dialogs::Kind::Open { multiple: false }, tl!("Viewer program"), None, None, false);
                    }
                });
                let d = &mut self.draft;
                group(ui, &tl!("Own viewer"), |ui| {
                    ui.checkbox(&mut d.viewer.detect_encoding, tl!("Work out the text encoding"));
                    ui.checkbox(&mut d.viewer.word_wrap, tl!("Wrap long lines"));
                });
            }
            6 => {
                group(ui, &tl!("Never unpack these file types"), |ui| {
                    ui.checkbox(&mut d.security.block_types, tl!("Block these:"));
                    ui.add_enabled(d.security.block_types, egui::TextEdit::singleline(&mut d.security.blocked_patterns).desired_width(f32::INFINITY));
                });
                group(ui, &tl!("When a file is already there"), |ui| {
                    for o in Overwrite::ALL {
                        ui.radio_value(&mut d.security.default_overwrite, o, tl!(o.label()));
                    }
                });
                ui.label(egui::RichText::new(tl!("Always on: names that would leave the destination folder are refused, and links are never followed.")).weak());
            }
            _ => self.integration_page(ui),
        }
    }

    fn integration_page(&mut self, ui: &mut egui::Ui) {
        let Some(places) = Places::for_user() else {
            ui.label(tl!("Your user folders could not be found."));
            return;
        };
        if !cfg!(all(unix, not(target_os = "macos"))) {
            ui.label(tl!("Right-click actions are available on Linux in this version; Windows and macOS will follow."));
            return;
        }
        let now = integration::installed(&places);
        let t = &mut self.draft.integration.targets;
        group(ui, &tl!("Right-click actions in"), |ui| {
            ui.checkbox(&mut t.desktop_entry, tl!("The app menu entry (and \"Open with\")"));
            ui.checkbox(&mut t.nautilus, tl!("Files (Nautilus)"));
            ui.checkbox(&mut t.dolphin, tl!("Dolphin"));
            ui.checkbox(&mut t.nemo, tl!("Nemo"));
            ui.checkbox(&mut t.thunar, tl!("Thunar"));
            ui.label(egui::RichText::new(tl!("Adds: Pack into an archive…, Pack into a ZIP right away, Unpack here, Unpack into its own folder.")).weak());
            ui.label(tf!("Installed now: {list}", list = describe(now)));
        });
        group(ui, &tl!("Open these archive types with ArchiveCraft"), |ui| {
            ui.horizontal_wrapped(|ui| {
                for (label, _, _) in integration::ASSOCIATIONS.iter().filter(|(l, _, _)| *l != "RAR" || cfg!(feature = "rar")) {
                    let mut on = self.defaults.iter().any(|d| d == label);
                    if ui.checkbox(&mut on, *label).changed() {
                        if on {
                            self.defaults.push(label.to_string());
                        } else {
                            self.defaults.retain(|d| d != label);
                        }
                    }
                }
            });
        });
        if let Some(n) = &self.integration_note {
            ui.label(n);
        }
    }

    fn apply(&mut self, app: &mut App) {
        let presets: Vec<u64> = self.presets_text.split_whitespace().filter_map(parse_size).collect();
        if !presets.is_empty() {
            self.draft.compression.volume_presets = presets;
        }
        if let Some(places) = Places::for_user()
            && cfg!(all(unix, not(target_os = "macos")))
        {
            let want = self.draft.integration.targets;
            if want != integration::installed(&places) {
                let _ = integration::uninstall(&places);
                if want != Targets::default()
                    && let Ok(exe) = std::env::current_exe()
                {
                    let _ = integration::install(&exe, &places, want);
                }
            }
            let _ = integration::set_defaults(&places, &self.defaults);
        }
        let d = self.draft.clone();
        app.prefs.update(|p| *p = d);
        app.refresh();
    }

    pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> Flow {
        let mut flow = Flow::Keep;
        let pages = [tl!("Look"), tl!("Work"), tl!("Packing"), tl!("Folders"), tl!("File list"), tl!("Viewer"), tl!("Safety"), tl!("File manager")];
        egui::Modal::new(egui::Id::unique("ac-prefs")).show(ctx, |ui| {
            ui.set_width(640.0);
            ui.heading(tl!("Preferences"));
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(120.0);
                    for (i, p) in pages.iter().enumerate() {
                        ui.selectable_value(&mut self.page, i, p.as_str());
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_min_height(340.0);
                    ui.set_width(490.0);
                    self.page_ui(ui, app);
                });
            });
            let (ok, cancel) = ok_cancel(ui, &tl!("OK"), true);
            if ok {
                self.apply(app);
                flow = Flow::Close;
            } else if cancel {
                flow = Flow::Close;
            }
        });
        let _ = human_size(0, false);
        flow
    }
}

fn describe(t: Targets) -> String {
    let mut v = Vec::new();
    if t.desktop_entry {
        v.push("menu entry");
    }
    if t.nautilus {
        v.push("Nautilus");
    }
    if t.dolphin {
        v.push("Dolphin");
    }
    if t.nemo {
        v.push("Nemo");
    }
    if t.thunar {
        v.push("Thunar");
    }
    if v.is_empty() { tl!("nothing") } else { v.join(", ") }
}
