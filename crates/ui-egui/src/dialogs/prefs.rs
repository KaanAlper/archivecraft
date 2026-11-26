//! Preferences, with a category list on the left. Changes apply on OK.

use super::{group, ok_cancel, Flow};
use crate::settings::{ColumnSet, ListKind, Prefs, ThemeMode, UnderlineMode, ViewerKind, WipeTemp};
use crate::state::{App, Pick};
use crate::{tf, tl};
use archivecraft_engine::integration::{self, Places, Targets};
use archivecraft_engine::model::util::{human_size, parse_size};
use archivecraft_engine::model::{format::LEVELS, Format, Overwrite};
use std::path::PathBuf;

pub struct PrefsDialog {
    pub draft: Prefs,
    pub(crate) page: usize,
    presets_text: String,
    defaults: Vec<String>,
    integration_note: Option<String>,
    columns_open: bool,
    cols_disk: ColumnSet,
    cols_archive: ColumnSet,
    font_open: bool,
    font_size: f32,
    font_mono: bool,
    font_bold: bool,
    ctx_items_open: bool,
    ctx_items_backup: Option<crate::settings::ContextItems>,
}

impl PrefsDialog {
    pub fn new(app: &App) -> Self {
        let draft = app.prefs.get().clone();
        let presets_text = draft.compression.volume_presets.iter().map(|v| format!("{}m", v / (1024 * 1024))).collect::<Vec<_>>().join(" ");
        let defaults = Places::for_user().map(|p| integration::current_defaults(&p)).unwrap_or_default();
        let (cols_disk, cols_archive, font_size, font_mono, font_bold) = (
            draft.list.disk_cols.clone(),
            draft.list.archive_cols.clone(),
            draft.list.font_size,
            draft.list.font_mono,
            draft.list.font_bold,
        );
        PrefsDialog { draft, page: 0, presets_text, defaults, integration_note: None, columns_open: false, cols_disk, cols_archive, font_open: false, font_size, font_mono, font_bold, ctx_items_open: false, ctx_items_backup: None }
    }

    pub fn set_folder(&mut self, slot: u8, p: PathBuf) {
        match slot {
            0 => self.draft.paths.start_folder = Some(p),
            1 => self.draft.paths.archives_folder = Some(p),
            2 => self.draft.paths.extract_folder = Some(p),
            _ => self.draft.paths.temp_folder = Some(p),
        }
    }

    fn folder_row(ui: &mut egui::Ui, app: &mut App, value: &mut Option<PathBuf>, slot: u8, title: &str, extra: impl FnOnce(&mut egui::Ui)) {
        group(ui, title, |ui| {
            let mut s = value.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
            let hint = tl!("(not set)");
            let changed = super::text_row(ui, &mut s, Some(&hint), |ui, _| {
                if ui.button(tl!("Browse…")).clicked() {
                    app.pick(Pick::PrefsFolder(slot), craft_dialogs::Kind::Folder, title.to_string(), value.clone(), None, false);
                }
            })
            .changed();
            if changed {
                *value = if s.trim().is_empty() { None } else { Some(PathBuf::from(s)) };
            }
            extra(ui);
        });
    }

    fn page_ui(&mut self, ui: &mut egui::Ui, app: &mut App) {
        let d = &mut self.draft;
        match self.page {
            0 => {
                ui.columns(2, |c| {
                    group(&mut c[0], &tl!("Color mode"), |ui| {
                        ui.radio_value(&mut d.interface.theme, ThemeMode::System, tl!("System default"));
                        ui.radio_value(&mut d.interface.theme, ThemeMode::Light, tl!("Light"));
                        ui.radio_value(&mut d.interface.theme, ThemeMode::Dark, tl!("Dark"));
                    });
                    c[0].add_space(4.0);
                    group(&mut c[0], &tl!("History"), |ui| {
                        ui.checkbox(&mut d.interface.keep_history, tl!("Keep archives history"));
                        ui.checkbox(&mut d.interface.dialog_history, tl!("Allow history in dialogs"));
                    });
                    c[0].add_space(4.0);
                    group(&mut c[0], &tl!("Toolbar"), |ui| {
                        ui.checkbox(&mut d.interface.large_buttons, tl!("Large buttons"));
                        ui.checkbox(&mut d.interface.button_labels, tl!("Show buttons text"));
                    });
                    group(&mut c[1], &tl!("Miscellaneous"), |ui| {
                        ui.checkbox(&mut d.interface.guide_on_start, tl!("Activate Guide on start"));
                        ui.checkbox(&mut d.interface.show_comment_on_open, tl!("Show archive comment"));
                        ui.checkbox(&mut d.interface.wrap_comment, tl!("Word wrap in comment"));
                        ui.checkbox(&mut d.interface.always_on_top, tl!("Always on top"));
                        ui.checkbox(&mut d.interface.full_path_in_title, tl!("Full paths in title bar"));
                    });
                });
            }
            1 => {
                ui.columns(2, |c| {
                    group(&mut c[0], &tl!("Performance"), |ui| {
                        ui.set_min_height(100.0);
                        ui.checkbox(&mut d.system.low_priority, tl!("Low priority"));
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut d.system.threads).range(1..=256));
                            ui.label(tl!("threads"));
                        });
                        #[cfg(target_os = "windows")]
                        ui.checkbox(&mut d.system.large_pages, tl!("Large memory pages"));
                    });
                    group(&mut c[1], &tl!("Logging"), |ui| {
                        ui.set_min_height(100.0);
                        ui.checkbox(&mut d.system.log_problems_to_file, tl!("Log errors to file"));
                        ui.add_enabled_ui(d.system.log_problems_to_file, |ui| {
                            ui.checkbox(&mut d.system.limit_log_size, tl!("Limit log file size to"));
                            ui.add_enabled_ui(d.system.limit_log_size, |ui| {
                                ui.horizontal(|ui| {
                                    ui.add_space(18.0);
                                    ui.add(egui::DragValue::new(&mut d.system.log_limit_kb).range(10..=100_000));
                                    ui.label(tl!("kilobytes"));
                                });
                            });
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
                let mut temp = d.paths.temp_folder.clone();
                let mut temp_rem = d.paths.temp_removable_only;
                Self::folder_row(ui, app, &mut temp, 3, &tl!("Folder for temporary files"), |ui| {
                    ui.checkbox(&mut temp_rem, tl!("Use only for removable disks"));
                });
                self.draft.paths.temp_folder = temp;
                self.draft.paths.temp_removable_only = temp_rem;

                let mut start = self.draft.paths.start_folder.clone();
                let mut reopen = self.draft.paths.reopen_last_folder;
                Self::folder_row(ui, app, &mut start, 0, &tl!("Start-up folder"), |ui| {
                    ui.checkbox(&mut reopen, tl!("Restore last working folder on start-up"));
                });
                self.draft.paths.start_folder = start;
                self.draft.paths.reopen_last_folder = reopen;

                let mut a = self.draft.paths.archives_folder.clone();
                Self::folder_row(ui, app, &mut a, 1, &tl!("Default folder for archives"), |_| {});
                self.draft.paths.archives_folder = a;

                let mut e = self.draft.paths.extract_folder.clone();
                let mut add_name = self.draft.paths.add_archive_name;
                let mut rem_red = self.draft.paths.remove_redundant;
                Self::folder_row(ui, app, &mut e, 2, &tl!("Default folder for extracted files"), |ui| {
                    ui.checkbox(&mut add_name, tl!("Append archive name to path"));
                    ui.checkbox(&mut rem_red, tl!("Remove redundant folders from extraction path"));
                });
                self.draft.paths.extract_folder = e;
                self.draft.paths.add_archive_name = add_name;
                self.draft.paths.remove_redundant = rem_red;
            }
            4 => {
                let mut open_cols = false;
                let mut open_font = false;
                ui.columns(2, |c| {
                    group(&mut c[0], &tl!("List type"), |ui| {
                        ui.radio_value(&mut d.list.kind, ListKind::Names, tl!("Names only"));
                        ui.radio_value(&mut d.list.kind, ListKind::Details, tl!("Columns"));
                    });
                    c[0].add_space(4.0);
                    group(&mut c[0], &tl!("List style"), |ui| {
                        ui.checkbox(&mut d.list.stripes, tl!("Stripes"));
                    });
                    group(&mut c[1], &tl!("Selection"), |ui| {
                        ui.radio_value(&mut d.list.single_click_opens, true, tl!("Single click to open an item"));
                        ui.add_enabled_ui(d.list.single_click_opens, |ui| {
                            ui.horizontal(|ui| {
                                ui.add_space(18.0);
                                ui.vertical(|ui| {
                                    ui.radio_value(&mut d.list.underline, UnderlineMode::Never, tl!("Do not underline names"));
                                    ui.radio_value(&mut d.list.underline, UnderlineMode::Hover, tl!("Underline current name"));
                                    ui.radio_value(&mut d.list.underline, UnderlineMode::Always, tl!("Underline all names"));
                                });
                            });
                        });
                        ui.radio_value(&mut d.list.single_click_opens, false, tl!("Double click to open an item"));
                    });
                });
                ui.add_space(4.0);
                group(ui, &tl!("Files"), |ui| {
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width(340.0);
                            ui.checkbox(&mut d.list.archives_first, tl!("Archives before other files"));
                            ui.checkbox(&mut d.list.exact_sizes, tl!("Sizes in exact bytes"));
                            ui.checkbox(&mut d.list.show_seconds, tl!("Times with seconds"));
                            ui.checkbox(&mut d.list.show_hidden, tl!("Show hidden files"));
                        });
                        ui.vertical(|ui| {
                            let btn = egui::vec2(104.0, 26.0);
                            if ui.add(egui::Button::new(tl!("Columns…")).min_size(btn)).clicked() {
                                open_cols = true;
                            }
                            ui.add_space(4.0);
                            if ui.add(egui::Button::new(tl!("Set font…")).min_size(btn)).clicked() {
                                open_font = true;
                            }
                        });
                    });
                });
                if open_cols {
                    self.cols_disk = self.draft.list.disk_cols.clone();
                    self.cols_archive = self.draft.list.archive_cols.clone();
                    self.columns_open = true;
                }
                if open_font {
                    self.font_size = self.draft.list.font_size;
                    self.font_mono = self.draft.list.font_mono;
                    self.font_bold = self.draft.list.font_bold;
                    self.font_open = true;
                }
            }
            5 => {
                ui.columns(2, |c| {
                    group(&mut c[0], &tl!("Viewer type"), |ui| {
                        ui.set_min_height(100.0);
                        ui.radio_value(&mut d.viewer.kind, ViewerKind::Internal, tl!("Internal viewer"));
                        ui.radio_value(&mut d.viewer.kind, ViewerKind::External, tl!("External viewer"));
                        ui.radio_value(&mut d.viewer.kind, ViewerKind::SystemApp, tl!("Associated program"));
                        ui.radio_value(&mut d.viewer.kind, ViewerKind::Ask, tl!("Ask"));
                    });
                    group(&mut c[1], &tl!("Internal viewer"), |ui| {
                        ui.set_min_height(100.0);
                        ui.checkbox(&mut d.viewer.detect_encoding, tl!("Autodetect encoding"));
                        ui.checkbox(&mut d.viewer.reuse_window, tl!("Reuse existing window"));
                        ui.checkbox(&mut d.viewer.word_wrap, tl!("Word wrap"));
                    });
                });
                ui.add_space(6.0);
                ui.label(tl!("Unpack everything for"));
                ui.add(egui::TextEdit::singleline(&mut d.viewer.unpack_all_for).desired_width(f32::INFINITY));
                ui.add_space(4.0);
                ui.label(tl!("Ignore modifications for"));
                ui.add(egui::TextEdit::singleline(&mut d.viewer.ignore_modifications_for).desired_width(f32::INFINITY));
                ui.add_space(4.0);
                ui.label(tl!("External viewer name"));
                super::text_row(ui, &mut d.viewer.external_program, None, |ui, _| {
                    if ui.button(tl!("Browse…")).clicked() {
                        app.pick(Pick::ExternalViewer, craft_dialogs::Kind::Open { multiple: false }, tl!("Viewer program"), None, None, false);
                    }
                });
            }
            6 => {
                group(ui, &tl!("Prohibited file types"), |ui| {
                    ui.checkbox(&mut d.security.block_types, tl!("File types to exclude from extracting"));
                    ui.add_enabled(d.security.block_types, egui::TextEdit::singleline(&mut d.security.blocked_patterns).desired_width(f32::INFINITY));
                });
                ui.add_space(4.0);
                ui.columns(2, |c| {
                    group(&mut c[0], &tl!("When a file is already there"), |ui| {
                        ui.set_min_height(110.0);
                        for o in Overwrite::ALL {
                            ui.radio_value(&mut d.security.default_overwrite, o, tl!(o.label()));
                        }
                    });
                    group(&mut c[1], &tl!("Wipe temporary files"), |ui| {
                        ui.set_min_height(110.0);
                        ui.radio_value(&mut d.security.wipe_temp, WipeTemp::Never, tl!("Never"));
                        ui.radio_value(&mut d.security.wipe_temp, WipeTemp::Always, tl!("Always"));
                        ui.radio_value(&mut d.security.wipe_temp, WipeTemp::EncryptedOnly, tl!("Encrypted only"));
                    });
                });
                ui.add_space(4.0);
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
        let mut open_ctx_items = false;
        ui.columns(2, |c| {
            group(&mut c[0], &tl!("Associate ArchiveCraft with"), |ui| {
                ui.set_min_height(260.0);
                let labels: Vec<&str> = integration::ASSOCIATIONS.iter().filter(|(l, _, _)| *l != "RAR" || cfg!(feature = "rar")).map(|(l, _, _)| *l).collect();
                egui::Grid::new("ac-assoc-grid").num_columns(2).spacing([18.0, 4.0]).show(ui, |ui| {
                    for (i, label) in labels.iter().enumerate() {
                        let mut on = self.defaults.iter().any(|d| d == label);
                        if ui.checkbox(&mut on, *label).changed() {
                            if on {
                                self.defaults.push((*label).to_string());
                            } else {
                                self.defaults.retain(|d| d != label);
                            }
                        }
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
                ui.add_space(6.0);
                if ui.add(egui::Button::new(tl!("Toggle all")).min_size(egui::vec2(ui.available_width(), 26.0))).clicked() {
                    if labels.iter().all(|l| self.defaults.iter().any(|d| d == l)) {
                        self.defaults.clear();
                    } else {
                        self.defaults = labels.iter().map(|l| (*l).to_string()).collect();
                    }
                }
            });
            group(&mut c[1], &tl!("Shortcuts"), |ui| {
                let ip = &mut self.draft.integration;
                ui.checkbox(&mut ip.desktop_shortcut, tl!("Add ArchiveCraft to Desktop"));
                ui.checkbox(&mut ip.targets.desktop_entry, tl!("Add ArchiveCraft to App Menu"));
            });
            c[1].add_space(4.0);
            group(&mut c[1], &tl!("Shell integration"), |ui| {
                let ip = &mut self.draft.integration;
                #[cfg(target_os = "linux")]
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut ip.targets.dolphin, tl!("Dolphin"));
                    ui.checkbox(&mut ip.targets.nautilus, tl!("Nautilus"));
                    ui.checkbox(&mut ip.targets.nemo, tl!("Nemo"));
                    ui.checkbox(&mut ip.targets.thunar, tl!("Thunar"));
                });
                ui.checkbox(&mut ip.cascaded_menus, tl!("Cascaded context menus"));
                ui.checkbox(&mut ip.menu_icons, tl!("Icons in context menus"));
                ui.add_space(2.0);
                if ui.add(egui::Button::new(tl!("Context menu items…")).min_size(egui::vec2(ui.available_width(), 26.0))).clicked() {
                    open_ctx_items = true;
                }
                ui.label(egui::RichText::new(tf!("Installed: {list}", list = describe(now))).weak());
            });
        });
        if open_ctx_items {
            self.ctx_items_backup = Some(self.draft.integration.items.clone());
            self.ctx_items_open = true;
        }
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
            let ip = &self.draft.integration;
            let want = ip.targets;
            let opts = integration::InstallOptions {
                desktop_shortcut: ip.desktop_shortcut,
                cascaded_menus: ip.cascaded_menus,
                menu_icons: ip.menu_icons,
                open: ip.items.open,
                extract_dialog: ip.items.extract_dialog,
                extract_here: ip.items.extract_here,
                extract_folder: ip.items.extract_folder || ip.items.extract_each_folder,
                add_dialog: ip.items.add_dialog,
                add_zip: ip.items.add_zip,
                test_archive: ip.items.test_archive,
                convert_archive: ip.items.convert_archive,
                always_pack_for: ip.items.always_pack_for.clone(),
            };
            let prev = app.prefs.get().integration.clone();
            if want != integration::installed(&places) || *ip != prev {
                let _ = integration::uninstall(&places);
                if (want != Targets::default() || opts.desktop_shortcut)
                    && let Ok(exe) = std::env::current_exe()
                {
                    let _ = integration::install_with_options(&exe, &places, want, &opts);
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
        let pages = [tl!("Look"), tl!("System"), tl!("Packing"), tl!("Paths"), tl!("File list"), tl!("Viewer"), tl!("Security"), tl!("Integration")];
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
                let (_, sep_rect) = ui.allocate_space(egui::vec2(2.0, 340.0));
                ui.painter().vline(sep_rect.center().x, sep_rect.y_range(), ui.visuals().widgets.noninteractive.bg_stroke);
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
        if self.columns_open {
            self.columns_modal(ctx);
        }
        if self.font_open {
            self.font_modal(ctx);
        }
        if self.ctx_items_open {
            self.ctx_items_modal(ctx);
        }
        flow
    }

    fn ctx_items_modal(&mut self, ctx: &egui::Context) {
        egui::Modal::new(egui::Id::unique("ac-prefs-ctx")).show(ctx, |ui| {
            ui.set_width(580.0);
            ui.heading(tl!("Context menu items"));
            let items = &mut self.draft.integration.items;
            ui.columns(2, |c| {
                group(&mut c[0], &tl!("Choose items to display in context menus"), |ui| {
                    ui.set_min_height(260.0);
                    ui.checkbox(&mut items.extract_dialog, tl!("Extract files…"));
                    ui.checkbox(&mut items.extract_here, tl!("Extract Here"));
                    ui.checkbox(&mut items.extract_folder, tl!("Unpack into its own folder"));
                    ui.checkbox(&mut items.extract_each_folder, tl!("Extract each archive to separate folder"));
                    #[cfg(target_os = "windows")]
                    {
                        ui.checkbox(&mut items.open_sfx, tl!("Open with ArchiveCraft (for SFX archives)"));
                        ui.checkbox(&mut items.open, tl!("Open with ArchiveCraft (for usual archives)"));
                    }
                    #[cfg(not(target_os = "windows"))]
                    ui.checkbox(&mut items.open, tl!("Open with ArchiveCraft"));
                    ui.checkbox(&mut items.add_dialog, tl!("Pack into an archive…"));
                    ui.checkbox(&mut items.add_zip, tl!("Pack into a ZIP right away"));
                    ui.checkbox(&mut items.test_archive, tl!("Test archive"));
                    ui.checkbox(&mut items.convert_archive, tl!("Convert archive…"));
                });
                #[cfg(target_os = "windows")]
                {
                    group(&mut c[1], &tl!("\"New\" submenu items"), |ui| {
                        ui.checkbox(&mut items.new_archive, tl!("ArchiveCraft archive"));
                        ui.checkbox(&mut items.new_zip, tl!("ArchiveCraft ZIP archive"));
                    });
                    c[1].add_space(4.0);
                }
                group(&mut c[1], &tl!("Drag and drop"), |ui| {
                    ui.checkbox(&mut items.dnd_pack, tl!("Display archiving items"));
                    ui.checkbox(&mut items.dnd_extract, tl!("Display extraction items"));
                });
                #[cfg(target_os = "windows")]
                {
                    c[1].add_space(4.0);
                    group(&mut c[1], &tl!("Where to check for SFX archives"), |ui| {
                        ui.checkbox(&mut items.sfx_local, tl!("Local disks"));
                        ui.checkbox(&mut items.sfx_network, tl!("Network disks"));
                        ui.checkbox(&mut items.sfx_other, tl!("Other disks"));
                        ui.checkbox(&mut items.sfx_cloud, tl!("Cloud files"));
                    });
                }
                c[1].add_space(4.0);
                c[1].label(tl!("Always display archiving items for"));
                c[1].add(egui::TextEdit::singleline(&mut items.always_pack_for).desired_width(f32::INFINITY));
            });
            ui.separator();
            let btn = egui::vec2(88.0, 26.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(tl!("Cancel")).min_size(btn)).clicked() {
                    if let Some(prev) = self.ctx_items_backup.take() {
                        self.draft.integration.items = prev;
                    }
                    self.ctx_items_open = false;
                }
                if ui.add(egui::Button::new(tl!("OK")).min_size(btn)).clicked() {
                    self.ctx_items_backup = None;
                    self.ctx_items_open = false;
                }
            });
        });
    }

    fn columns_modal(&mut self, ctx: &egui::Context) {
        egui::Modal::new(egui::Id::unique("ac-prefs-cols")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading(tl!("Columns"));
            ui.columns(2, |c| {
                group(&mut c[0], &tl!("Outside of archives"), |ui| {
                    let mut name_on = true;
                    ui.add_enabled(false, egui::Checkbox::new(&mut name_on, tl!("Name")));
                    ui.checkbox(&mut self.cols_disk.size, tl!("Size"));
                    ui.checkbox(&mut self.cols_disk.packed, tl!("Packed"));
                    ui.checkbox(&mut self.cols_disk.kind, tl!("Type"));
                    ui.checkbox(&mut self.cols_disk.modified, tl!("Modified"));
                });
                group(&mut c[1], &tl!("Inside of archives"), |ui| {
                    let mut name_on = true;
                    ui.add_enabled(false, egui::Checkbox::new(&mut name_on, tl!("Name")));
                    ui.checkbox(&mut self.cols_archive.size, tl!("Size"));
                    ui.checkbox(&mut self.cols_archive.packed, tl!("Packed"));
                    ui.checkbox(&mut self.cols_archive.kind, tl!("Type"));
                    ui.checkbox(&mut self.cols_archive.modified, tl!("Modified"));
                    ui.checkbox(&mut self.cols_archive.crc, tl!("Checksum"));
                });
            });
            ui.separator();
            let btn = egui::vec2(88.0, 26.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(tl!("Default")).min_size(btn)).clicked() {
                    self.cols_disk = ColumnSet::disk_default();
                    self.cols_archive = ColumnSet::archive_default();
                }
                if ui.add(egui::Button::new(tl!("Cancel")).min_size(btn)).clicked() {
                    self.columns_open = false;
                }
                if ui.add(egui::Button::new(tl!("OK")).min_size(btn)).clicked() {
                    self.draft.list.disk_cols = self.cols_disk.clone();
                    self.draft.list.archive_cols = self.cols_archive.clone();
                    self.columns_open = false;
                }
            });
        });
    }

    fn font_modal(&mut self, ctx: &egui::Context) {
        egui::Modal::new(egui::Id::unique("ac-prefs-font")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading(tl!("File list font"));
            ui.columns(2, |c| {
                group(&mut c[0], &tl!("Font"), |ui| {
                    ui.radio_value(&mut self.font_mono, false, tl!("Proportional"));
                    ui.radio_value(&mut self.font_mono, true, tl!("Monospace"));
                    ui.checkbox(&mut self.font_bold, tl!("Bold"));
                });
                group(&mut c[1], &tl!("Size"), |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut self.font_size).range(10.0..=22.0).speed(0.5));
                        ui.label("pt");
                    });
                    ui.horizontal_wrapped(|ui| {
                        for sz in [11.0, 12.0, 13.5, 15.0, 17.0, 19.0] {
                            ui.selectable_value(&mut self.font_size, sz, format!("{sz}"));
                        }
                    });
                });
            });
            group(ui, &tl!("Preview"), |ui| {
                ui.set_width(ui.available_width());
                let mut rt = egui::RichText::new("AaBbGgSs — kaan-dots.zip   18,829,890 bytes").size(self.font_size);
                if self.font_mono {
                    rt = rt.monospace();
                }
                if self.font_bold {
                    rt = rt.strong();
                }
                ui.label(rt);
            });
            ui.separator();
            let btn = egui::vec2(88.0, 26.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(tl!("Default")).min_size(btn)).clicked() {
                    self.font_size = 13.5;
                    self.font_mono = false;
                    self.font_bold = false;
                }
                if ui.add(egui::Button::new(tl!("Cancel")).min_size(btn)).clicked() {
                    self.font_open = false;
                }
                if ui.add(egui::Button::new(tl!("OK")).min_size(btn)).clicked() {
                    self.draft.list.font_size = self.font_size;
                    self.draft.list.font_mono = self.font_mono;
                    self.draft.list.font_bold = self.font_bold;
                    self.font_open = false;
                }
            });
        });
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
