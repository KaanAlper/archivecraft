//! Drawing the main window: Craft menu bar, toolbar, path bar, file list, status bar; and its
//! keys, context menu, drops from outside and drags out of an archive.

use crate::dialogs;
use crate::icons::{self, Icon};
use crate::rows::{self, Row, RowRef, Sort, SortKey};
use crate::settings::{ListKind, ThemeMode};
use crate::state::{App, Place};
use crate::{tf, tl};
use archivecraft_engine::model::util::{format_time, human_size};
use craft_ui::contract::ShowContainer;
use egui::{Color32, Key, Modifiers, Rect, RichText, Sense, Ui, Vec2};
use std::path::PathBuf;

impl App {
    // ------------------------------------------------------------ menu and commands

    fn menu_rows(&self) -> Vec<craft_menu::Row> {
        use craft_menu::{Chord, Row};
        fn sc(r: Row, c: &str) -> Row {
            match Chord::parse(c) {
                Ok(ch) => r.shortcut(ch),
                Err(_) => r,
            }
        }
        let (a, c, t, b, s, h) = (tl!("Archive"), tl!("Actions"), tl!("Tools"), tl!("Bookmarks"), tl!("Settings"), tl!("Help"));
        let recent = tl!("Recently opened");
        let prefs = self.prefs.get();
        let mut rows = vec![sc(Row::new(&[&a], "archive.open", tl!("Open archive…")), "Ctrl+O"), Row::separator(&[&a])];
        if prefs.saved.recent_archives.is_empty() {
            rows.push(Row::new(&[&a, &recent], "archive.recent.none", tl!("(nothing yet)")));
        }
        for (i, p) in prefs.saved.recent_archives.iter().enumerate() {
            rows.push(Row::new(&[&a, &recent], format!("archive.recent.{i}").as_str(), p.display().to_string()));
        }
        rows.extend([
            sc(Row::new(&[&a], "archive.password", tl!("Password for this session…")), "Ctrl+P"),
            Row::separator(&[&a]),
            sc(Row::new(&[&a], "select.all", tl!("Select everything")), "Ctrl+A"),
            Row::new(&[&a], "select.pattern", tl!("Select by name pattern…")),
            Row::new(&[&a], "select.unpattern", tl!("Unselect by name pattern…")),
            Row::new(&[&a], "select.invert", tl!("Swap selection")),
            Row::separator(&[&a]),
            sc(Row::new(&[&a], "archive.copy_paths", tl!("Copy full paths")), "Ctrl+Shift+C"),
            sc(Row::new(&[&a], "view.refresh", tl!("Reload list")), "F5"),
            sc(Row::new(&[&c], "act.add", tl!("Add files to an archive…")), "Ctrl+Shift+A"),
            sc(Row::new(&[&c], "act.extract", tl!("Unpack to a folder…")), "Ctrl+E"),
            sc(Row::new(&[&c], "act.extract_here", tl!("Unpack here")), "Ctrl+Shift+E"),
            sc(Row::new(&[&c], "act.test", tl!("Check archive")), "Ctrl+T"),
            Row::separator(&[&c]),
            sc(Row::new(&[&c], "act.view", tl!("Look inside a file")), "F3"),
            sc(Row::new(&[&c], "act.delete", tl!("Remove")), "Delete"),
            Row::new(&[&c], "act.note", tl!("Archive note…")),
            Row::new(&[&c], "act.recovery", tl!("Add recovery data…")),
            Row::new(&[&t], "tool.guide", tl!("Step-by-step guide…")),
            Row::separator(&[&t]),
            Row::new(&[&t], "tool.convert", tl!("Convert archives…")),
            Row::new(&[&t], "tool.repair", tl!("Repair an archive…")),
            Row::separator(&[&t]),
            sc(Row::new(&[&t], "act.find", tl!("Search…")), "Ctrl+F"),
            sc(Row::new(&[&t], "act.info", tl!("Archive details")), "Ctrl+I"),
            Row::new(&[&t], "tool.bench", tl!("Speed test…")),
            sc(Row::new(&[&b], "bookmark.add", tl!("Bookmark this place")), "Ctrl+D"),
            Row::new(&[&b], "bookmark.manage", tl!("Manage bookmarks…")),
            Row::separator(&[&b]),
        ]);
        for (i, bm) in prefs.saved.bookmarks.iter().enumerate() {
            rows.push(Row::new(&[&b], format!("bookmark.go.{i}").as_str(), bm.name.clone()));
        }
        let look = tl!("Look");
        let style = tl!("List style");
        rows.extend([
            sc(Row::new(&[&s], "prefs.open", tl!("Preferences…")), "Ctrl+,"),
            Row::new(&[&s, &look], "theme.system", tl!("Follow the system")),
            Row::new(&[&s, &look], "theme.light", tl!("Light")),
            Row::new(&[&s, &look], "theme.dark", tl!("Dark")),
            Row::new(&[&s, &style], "list.details", tl!("Columns")),
            Row::new(&[&s, &style], "list.names", tl!("Names only")),
            Row::separator(&[&s]),
            Row::new(&[&s], "integration.install", tl!("Add ArchiveCraft to my file manager's right-click menu")),
            Row::new(&[&s], "integration.remove", tl!("Take ArchiveCraft out of the right-click menu")),
            Row::separator(&[&s]),
            Row::new(&[&s], "history.clear", tl!("Forget recently opened")),
            Row::new(&[&s], "diags.show", tl!("Last problems list…")),
            Row::new(&[&h], "help.keys", tl!("Keyboard keys")),
            Row::new(&[&h], "help.about", tl!("About ArchiveCraft")),
        ]);
        rows
    }

    fn rebuild_menu_if_needed(&mut self) {
        let p = self.prefs.get();
        let key = format!("{:?}{:?}", p.saved.recent_archives, p.saved.bookmarks.iter().map(|b| &b.name).collect::<Vec<_>>());
        if key == self.menu_key {
            return;
        }
        self.menu_key = key;
        let rows = self.menu_rows();
        let tops = [tl!("Archive"), tl!("Actions"), tl!("Tools"), tl!("Bookmarks"), tl!("Settings"), tl!("Help")];
        let tops_ref: Vec<&str> = tops.iter().map(String::as_str).collect();
        let mut bar = craft_menu::from_items(&tops_ref, &rows);
        bar.tag_menu(&tops[5], craft_menu::MenuRole::Help);
        let laid = craft_menu::layout(&bar, craft_menu::Target::current(), &craft_menu::LayoutOptions::new("ArchiveCraft")).bar;
        self.menu = craft_tracked::Tracked::new(laid);
    }

    pub fn run_command(&mut self, id: &str) {
        match id {
            "archive.open" => self.pick_open_archive(),
            "archive.password" => self.ui_state.session_pw = Some(dialogs::progress::PasswordPrompt::new(String::new(), String::new(), false, None)),
            "select.all" => self.selected = (0..self.rows.len()).filter(|i| self.rows[*i].target != RowRef::Parent).collect(),
            "select.pattern" => self.ui_state.pattern = Some(dialogs::tools::PatternPrompt::new(true)),
            "select.unpattern" => self.ui_state.pattern = Some(dialogs::tools::PatternPrompt::new(false)),
            "select.invert" => self.selected = (0..self.rows.len()).filter(|i| self.rows[*i].target != RowRef::Parent && !self.selected.contains(i)).collect(),
            "archive.copy_paths" => {
                let base = self.open_archive_view().map(|v| v.path.display().to_string());
                let text = self
                    .selected_rows()
                    .iter()
                    .map(|r| match &r.target {
                        RowRef::Disk(p) => p.display().to_string(),
                        RowRef::Entry(e) => base.as_ref().map(|b| format!("{b}/{e}")).unwrap_or_else(|| e.clone()),
                        RowRef::Parent => String::new(),
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                self.ctx.copy_text(text);
                self.note(tl!("Paths copied."));
            }
            "archive.copy_names" => {
                let text = self.selected_rows().iter().map(|r| r.name.clone()).collect::<Vec<_>>().join("\n");
                self.ctx.copy_text(text);
                self.note(tl!("Names copied."));
            }
            "act.new_folder" => {
                if matches!(self.place, Place::Disk(_)) {
                    self.ui_state.name_prompt = Some(dialogs::naming::NamePrompt::new_folder(self.disk_dir()));
                }
            }
            "act.rename" => {
                let sel = self.selected_disk();
                if let [one] = sel.as_slice() {
                    self.ui_state.name_prompt = Some(dialogs::naming::NamePrompt::change_name(one.clone()));
                } else {
                    self.note(tl!("Choose one file or folder on disk to give it a new name."));
                }
            }
            "view.refresh" => self.reload_open(),
            "act.add" => self.act_add(),
            "act.extract" => self.act_extract(),
            "act.extract_here" => self.act_extract_here(),
            "act.test" => self.act_test(),
            "act.view" => self.act_view(),
            "act.delete" => self.act_delete(),
            "act.info" => self.act_info(),
            "act.note" => self.act_note(),
            "act.find" => self.act_find(),
            "act.recovery" => match self.target_archive() {
                Some(p) => self.ui_state.recovery = Some(dialogs::tools::RecoveryDialog { archive: p, percent: self.prefs.get().compression.recovery_percent.max(1) }),
                None => self.note(tl!("Choose an archive first.")),
            },
            "tool.guide" => self.ui_state.wizard = Some(dialogs::wizard::Wizard::default()),
            "tool.convert" => {
                let sources: Vec<PathBuf> = match &self.place {
                    Place::Archive(v) => vec![v.path.clone()],
                    Place::Disk(_) => self.selected_archives(),
                };
                self.ui_state.convert = Some(dialogs::tools::ConvertDialog::new(sources, self.disk_dir()));
            }
            "tool.repair" => self.act_repair(),
            "tool.bench" => self.ui_state.bench = Some(dialogs::tools::BenchWindow::default()),
            "bookmark.add" => {
                let (path, inner) = match &self.place {
                    Place::Disk(d) => (d.clone(), String::new()),
                    Place::Archive(v) => (v.path.clone(), v.folder.clone()),
                };
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.display().to_string());
                let name = if inner.is_empty() { name } else { format!("{name} / {inner}") };
                self.prefs.update(|p| p.saved.bookmarks.push(crate::settings::Bookmark { name, path, inner }));
                self.note(tl!("Bookmarked."));
            }
            "bookmark.manage" => self.ui_state.bookmarks = Some(dialogs::tools::BookmarksWindow::default()),
            "prefs.open" => self.ui_state.prefs = Some(dialogs::prefs::PrefsDialog::new(self)),
            "theme.system" => {
                self.prefs.update(|p| p.interface.theme = ThemeMode::System);
            }
            "theme.light" => {
                self.prefs.update(|p| p.interface.theme = ThemeMode::Light);
            }
            "theme.dark" => {
                self.prefs.update(|p| p.interface.theme = ThemeMode::Dark);
            }
            "list.details" => {
                self.prefs.update(|p| p.list.kind = ListKind::Details);
            }
            "list.names" => {
                self.prefs.update(|p| p.list.kind = ListKind::Names);
            }
            "integration.install" | "integration.remove" => self.file_manager_menu(id == "integration.install"),
            order if order.starts_with("sort.") => {
                if let Some(k) = order.strip_prefix("sort.").and_then(|n| n.parse::<usize>().ok()).and_then(|i| rows::SortKey::MENU.get(i).copied()) {
                    self.sort = Sort { key: k, ascending: if self.sort.key == k { !self.sort.ascending } else { true } };
                    let keep: Vec<RowRef> = self.selected_rows().into_iter().map(|r| r.target).collect();
                    let archives_first = self.prefs.get().list.archives_first;
                    rows::sort_rows(&mut self.rows, self.sort, archives_first);
                    self.selected = self.rows.iter().enumerate().filter(|(_, r)| keep.contains(&r.target)).map(|(i, _)| i).collect();
                }
            }
            "history.clear" => {
                self.prefs.update(|p| p.saved.recent_archives.clear());
            }
            "diags.show" => match self.ui_state.last_diags.clone() {
                Some(d) => self.ui_state.diags = Some(d),
                None => self.note(tl!("No problems so far.")),
            },
            "help.keys" => self.ui_state.message = Some(dialogs::progress::MessageBox::info(dialogs::tools::keys_help())),
            "help.about" => self.ui_state.message = Some(dialogs::progress::MessageBox::info(dialogs::tools::about_text())),
            other => {
                if let Some(i) = other.strip_prefix("archive.recent.").and_then(|s| s.parse::<usize>().ok()) {
                    if let Some(p) = self.prefs.get().saved.recent_archives.get(i).cloned() {
                        self.open_archive(p);
                    }
                } else if let Some(i) = other.strip_prefix("bookmark.go.").and_then(|s| s.parse::<usize>().ok())
                    && let Some(b) = self.prefs.get().saved.bookmarks.get(i).cloned()
                {
                    self.go_bookmark(&b);
                }
            }
        }
    }

    // ------------------------------------------------------------ chrome

    /// Put the right-click actions into (or take them out of) the file managers of this user.
    pub(crate) fn file_manager_menu(&mut self, install: bool) {
        use archivecraft_engine::integration::{self, Places, Targets};
        if !cfg!(all(unix, not(target_os = "macos"))) {
            self.ui_state.message = Some(dialogs::progress::MessageBox::info(tl!("Right-click actions in the file manager are available on Linux in this version; Windows and macOS will follow.")));
            return;
        }
        let Some(places) = Places::for_user() else {
            self.note(tl!("Your user folders could not be found."));
            return;
        };
        let result = if install {
            std::env::current_exe().map_err(|e| e.to_string()).and_then(|exe| integration::install(&exe, &places, integration::detect()).map_err(|e| e.to_string()))
        } else {
            integration::uninstall(&places).map_err(|e| e.to_string())
        };
        let text = match result {
            Ok(_) if install => {
                self.prefs.update(|p| {
                    p.integration.targets = integration::detect();
                    p.integration.offered = true;
                });
                tl!("Done. Right-click a file or folder in your file manager and open the ArchiveCraft submenu: pack into an archive, pack into a ZIP right away, and for archives unpack here or into their own folder. Close and reopen the file manager once if it does not show yet.")
            }
            Ok(_) => {
                self.prefs.update(|p| {
                    p.integration.targets = Targets::default();
                    p.integration.offered = true;
                });
                tl!("The ArchiveCraft right-click actions were removed.")
            }
            Err(e) => tf!("That did not work: {e}", e = e),
        };
        self.ui_state.message = Some(dialogs::progress::MessageBox::info(text));
    }

    pub fn root_env(&self) -> craft_ui::runtime::env::RootEnv {
        use craft_theme::selection::ModeChoice;
        let mode = match self.prefs.get().interface.theme {
            ThemeMode::System => ModeChoice::System,
            ThemeMode::Light => ModeChoice::Light,
            ThemeMode::Dark => ModeChoice::Dark,
        };
        // Our own accent: a teal that pairs with the amber parcels of the icons.
        craft_ui::runtime::env::RootEnv::default().mode(mode).accent(Some(craft_theme::Rgba::rgb(0x2C, 0x8C, 0x99)))
    }

    fn draw_menu(&mut self, ui: &mut Ui) {
        self.rebuild_menu_if_needed();
        let keyboard = !self.ui_state.any_modal() && self.job.as_ref().is_none_or(|j| j.background);
        let menu = &self.menu;
        egui::Panel::top("ac-menu").show(ui, |ui| {
            craft_ui::layout_components::Stack::new().show(ui, |ui, _| {
                craft_menu::MenuBarUi::new(menu).keyboard(keyboard).show(ui);
            });
        });
        self.hud.menu_bar = true;
    }

    fn tool_button(&mut self, ui: &mut Ui, icon: Icon, label: &str, tip: &str, enabled: bool) -> bool {
        let (large, labels) = {
            let i = &self.prefs.get().interface;
            (i.large_buttons, i.button_labels)
        };
        let isz = if large { 30.0 } else { 20.0 };
        let size = if labels { Vec2::new(if large { 64.0 } else { 56.0 }, isz + 24.0) } else { Vec2::splat(isz + 10.0) };
        let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
        let v = ui.visuals().clone();
        if enabled && resp.hovered() {
            ui.painter().rect_filled(rect, egui::CornerRadius::same(6), v.widgets.hovered.weak_bg_fill);
        }
        let icon_rect = Rect::from_center_size(egui::pos2(rect.center().x, rect.top() + 4.0 + isz / 2.0), Vec2::splat(isz));
        let ink = if enabled { v.text_color() } else { v.weak_text_color() };
        icons::paint(ui.painter(), icon_rect, icon, ink);
        if !enabled {
            ui.painter().rect_filled(icon_rect.expand(1.0), egui::CornerRadius::ZERO, v.panel_fill.gamma_multiply(0.55));
        }
        if labels {
            ui.painter().text(egui::pos2(rect.center().x, rect.bottom() - 3.0), egui::Align2::CENTER_BOTTOM, label, egui::FontId::proportional(12.5), ink);
        }
        self.hud.toolbar_buttons += 1;
        let resp = resp.on_hover_text(tip);
        enabled && resp.clicked()
    }

    fn draw_toolbar(&mut self, ui: &mut Ui) {
        egui::Panel::top("ac-toolbar").show(ui, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let free = self.job.is_none();
                if self.tool_button(ui, Icon::Add, &tl!("Add"), &tl!("Pack the chosen files into an archive"), free) {
                    self.act_add();
                }
                if self.tool_button(ui, Icon::Extract, &tl!("Extract"), &tl!("Unpack to a folder you choose"), free) {
                    self.act_extract();
                }
                if self.tool_button(ui, Icon::Test, &tl!("Test"), &tl!("Read everything and compare checksums"), free) {
                    self.act_test();
                }
                if self.tool_button(ui, Icon::View, &tl!("View"), &tl!("Look inside a file without unpacking"), true) {
                    self.act_view();
                }
                if self.tool_button(ui, Icon::Delete, &tl!("Delete"), &tl!("Remove the chosen items"), free) {
                    self.act_delete();
                }
                if self.tool_button(ui, Icon::Search, &tl!("Search"), &tl!("Find files by name or contents"), free) {
                    self.act_find();
                }
                if self.tool_button(ui, Icon::Guide, &tl!("Guide"), &tl!("Step-by-step help for common jobs"), true) {
                    self.run_command("tool.guide");
                }
                if self.tool_button(ui, Icon::Info, &tl!("Info"), &tl!("Totals, ratio and type of the archive"), true) {
                    self.act_info();
                }
                ui.separator();
                if self.tool_button(ui, Icon::Repair, &tl!("Repair"), &tl!("Fix a damaged archive"), free) {
                    self.act_repair();
                }
                let note = self.open_archive_view().is_some_and(|v| v.listing.format.supports_comment());
                if note && self.tool_button(ui, Icon::Note, &tl!("Note"), &tl!("Read or write the archive's note"), free) {
                    self.act_note();
                }
            });
            ui.add_space(2.0);
        });
    }

    fn draw_path_bar(&mut self, ui: &mut Ui) {
        let mut go_up = false;
        let mut go_to: Option<PathBuf> = None;
        egui::Panel::top("ac-path").show(ui, |ui| {
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                let up = ui.add_sized([28.0, 22.0], egui::Button::new("")).on_hover_text(tl!("Up one level (Backspace)"));
                icons::paint(ui.painter(), up.rect.shrink(5.0), Icon::Up, ui.visuals().text_color());
                go_up = up.clicked();
                ui.separator();
                match &self.place {
                    Place::Disk(_) => {
                        icons::show(ui, Icon::Folder, 18.0);
                        let r = ui.add(egui::TextEdit::singleline(&mut self.path_text).desired_width(f32::INFINITY).id_salt("ac-path-text"));
                        if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            go_to = Some(PathBuf::from(self.path_text.trim()));
                        }
                    }
                    Place::Archive(v) => {
                        icons::show(ui, Icon::Archive, 18.0);
                        let name = v.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        let folder = if v.folder.is_empty() { String::new() } else { format!(" / {}", v.folder.replace('/', " / ")) };
                        let desc = tf!("{name}{folder} — {kind}, {size} bytes unpacked", name = name, folder = folder, kind = tl!(v.listing.format.describe()), size = human_size(v.listing.total_size(), true));
                        egui::Frame::new().inner_margin(egui::Margin::symmetric(6, 3)).fill(ui.visuals().extreme_bg_color).corner_radius(4).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(desc);
                        });
                    }
                }
            });
            ui.add_space(3.0);
        });
        self.hud.path_bar = true;
        if go_up {
            self.go_up();
        }
        if let Some(p) = go_to {
            if p.is_dir() {
                self.go_disk(p);
            } else if p.is_file() {
                self.open_archive(p);
            } else {
                self.note(tl!("There is no such folder."));
            }
        }
    }

    fn draw_status(&mut self, ui: &mut Ui) {
        let exact = self.prefs.get().list.exact_sizes;
        let files = self.rows.iter().filter(|r| !r.is_dir).count();
        let folders = self.rows.iter().filter(|r| r.is_dir && r.target != RowRef::Parent).count();
        let total: u64 = self.rows.iter().filter(|r| !r.is_dir).filter_map(|r| r.size).sum();
        let sel = self.selected_rows();
        let text = if sel.is_empty() {
            tf!("{f} file(s), {d} folder(s), {s} in total", f = files, d = folders, s = human_size(total, exact))
        } else {
            let ss: u64 = sel.iter().filter_map(|r| r.size).sum();
            tf!("Chosen: {n} item(s), {s} — of {f} file(s), {t}", n = sel.len(), s = human_size(ss, exact), f = files, t = human_size(total, exact))
        };
        self.hud.status = text.clone();
        let note = self.status_note.as_ref().filter(|(_, at)| at.elapsed().as_secs() < 8).map(|(n, _)| n.clone());
        let mut show_job = false;
        egui::Panel::bottom("ac-status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(text);
                if let Some(n) = note {
                    ui.separator();
                    ui.label(RichText::new(n).color(ui.visuals().hyperlink_color));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(j) = &self.job
                        && j.background
                    {
                        let p = j.ctl.progress.lock().map(|p| p.fraction()).unwrap_or(0.0);
                        show_job = ui.small_button(tl!("Show")).clicked();
                        ui.add(egui::ProgressBar::new(p).desired_width(120.0).show_percentage());
                        ui.label(&j.title);
                    }
                    if let Place::Archive(v) = &self.place {
                        if v.listing.any_encrypted() {
                            icons::show(ui, Icon::Lock, 14.0).on_hover_text(tl!("Some items are locked with a password"));
                        }
                        ui.label(tl!(v.listing.format.label()));
                    }
                });
            });
        });
        if show_job && let Some(j) = &mut self.job {
            j.background = false;
        }
    }

    fn header_cell(&mut self, ui: &mut Ui, label: &str, key: SortKey) {
        let resp = ui.add(egui::Button::new(RichText::new(label).strong()).frame(false));
        if self.sort.key == key {
            // A small painted triangle: no font glyph needed.
            let c = egui::pos2(resp.rect.right() + 8.0, resp.rect.center().y);
            let (w, h) = (4.0, 3.5);
            let pts = if self.sort.ascending {
                vec![egui::pos2(c.x - w, c.y + h), egui::pos2(c.x + w, c.y + h), egui::pos2(c.x, c.y - h)]
            } else {
                vec![egui::pos2(c.x - w, c.y - h), egui::pos2(c.x + w, c.y - h), egui::pos2(c.x, c.y + h)]
            };
            ui.painter().add(egui::Shape::convex_polygon(pts, ui.visuals().weak_text_color(), egui::Stroke::NONE));
        }
        if resp.clicked() {
            self.sort = if self.sort.key == key { Sort { key, ascending: !self.sort.ascending } } else { Sort { key, ascending: true } };
            let keep: Vec<RowRef> = self.selected_rows().into_iter().map(|r| r.target).collect();
            let archives_first = self.prefs.get().list.archives_first;
            rows::sort_rows(&mut self.rows, self.sort, archives_first);
            self.selected = self.rows.iter().enumerate().filter(|(_, r)| keep.contains(&r.target)).map(|(i, _)| i).collect();
        }
        self.hud.headers.push(label.to_string());
    }

    pub fn click_row(&mut self, i: usize, mods: Modifiers) {
        if mods.shift
            && let Some(a) = self.anchor
        {
            if !mods.command {
                self.selected.clear();
            }
            self.selected.extend(a.min(i)..=a.max(i));
        } else if mods.command {
            if !self.selected.remove(&i) {
                self.selected.insert(i);
            }
            self.anchor = Some(i);
        } else {
            self.selected = [i].into();
            self.anchor = Some(i);
        }
    }

    fn row_icon(row: &Row) -> Icon {
        if row.target == RowRef::Parent {
            Icon::Up
        } else if row.is_dir {
            Icon::Folder
        } else if row.is_archive {
            Icon::Archive
        } else {
            Icon::File
        }
    }

    fn draw_list(&mut self, ui: &mut Ui) {
        use egui_extras::{Column, TableBuilder};
        let prefs = self.prefs.get().list.clone();
        let in_archive = matches!(self.place, Place::Archive(_));
        let mut clicked: Option<(usize, Modifiers)> = None;
        let mut activated: Option<usize> = None;
        let mut drag_start = false;
        let mut context: Option<usize> = None;
        let mods = ui.input(|i| i.modifiers);
        self.row_rects.clear();
        self.hud.headers.clear();
        let rows = self.rows.clone();
        let selected = self.selected.clone();
        let sort = self.sort;
        let one_disk_item = !in_archive && self.selected_disk().len() == 1;
        let list_kind = prefs.kind;
        let mut chosen: Option<String> = None;
        let mut shown = false;
        let menu = |ui: &mut Ui, chosen: &mut Option<String>| {
            ui.set_min_width(250.0);
            let mut pick = |ui: &mut Ui, id: &str, label: String, keys: &str, enabled: bool| {
                if menu_row(ui, &label, keys, None, enabled) {
                    *chosen = Some(id.to_string());
                    ui.close();
                }
            };
            pick(ui, "select.all", tl!("Select everything"), "Ctrl+A", true);
            pick(ui, "archive.copy_names", tl!("Copy names"), "Ctrl+C", true);
            pick(ui, "archive.copy_paths", tl!("Copy full paths"), "Ctrl+Shift+C", true);
            ui.separator();
            let add = if in_archive { tl!("Add files to this archive…") } else { tl!("Pack into an archive…") };
            pick(ui, "act.add", add, "Ctrl+Shift+A", true);
            let del = if in_archive { tl!("Remove from archive") } else { tl!("Move to the recycle bin") };
            pick(ui, "act.delete", del, "Del", true);
            pick(ui, "act.view", tl!("Look inside"), "F3", true);
            ui.separator();
            pick(ui, "act.new_folder", tl!("New folder…"), "", !in_archive);
            pick(ui, "act.rename", tl!("Change name…"), "F2", one_disk_item);
            ui.menu_button(tl!("List style"), |ui| {
                ui.set_min_width(160.0);
                if menu_row(ui, &tl!("Columns"), "", Some(list_kind == ListKind::Details), true) {
                    *chosen = Some("list.details".into());
                    ui.close();
                }
                if menu_row(ui, &tl!("Names only"), "", Some(list_kind == ListKind::Names), true) {
                    *chosen = Some("list.names".into());
                    ui.close();
                }
            });
            ui.menu_button(tl!("Order by"), |ui| {
                ui.set_min_width(170.0);
                for (i, k) in rows::SortKey::MENU.iter().enumerate() {
                    if *k == rows::SortKey::Packed && !in_archive {
                        continue;
                    }
                    if menu_row(ui, &tl!(k.label()), "", Some(sort.key == *k), true) {
                        *chosen = Some(format!("sort.{i}"));
                        ui.close();
                    }
                }
            });
        };
        // Right-click on empty space opens the same menu (registered first, so rows win where they are).
        let background = ui.interact(ui.max_rect(), ui.scope_id().with("ac-list-bg"), Sense::click());
        if background.clicked() {
            self.selected.clear();
        }
        if prefs.kind == ListKind::Names {
            self.hud.headers = vec![tl!("Name")];
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                for (i, row) in rows.iter().enumerate() {
                    let resp = ui
                        .horizontal(|ui| {
                            icons::show(ui, Self::row_icon(row), 16.0);
                            ui.add(egui::Button::selectable(selected.contains(&i), &row.name).frame_when_inactive(false))
                        })
                        .inner;
                    self.row_rects.insert(row.name.clone(), resp.rect);
                    if resp.clicked() {
                        clicked = Some((i, mods));
                    }
                    if resp.double_clicked() {
                        activated = Some(i);
                    }
                    if resp.secondary_clicked() && row.target != RowRef::Parent {
                        context = Some(i);
                    }
                    if row.target != RowRef::Parent {
                        resp.context_menu(|ui| {
                            shown = true;
                            menu(ui, &mut chosen);
                        });
                    }
                }
            });
        } else {
            let (exact, seconds) = (prefs.exact_sizes, prefs.show_seconds);
            let mut table = TableBuilder::new(ui)
                .striped(prefs.stripes)
                .resizable(true)
                .sense(Sense::click_and_drag())
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::initial(220.0).at_least(120.0).clip(true))
                .column(Column::initial(82.0).at_least(50.0))
                .column(Column::initial(82.0).at_least(50.0))
                .column(Column::initial(110.0).at_least(60.0).clip(true))
                .column(Column::initial(125.0).at_least(80.0));
            if in_archive {
                table = table.column(Column::initial(80.0).at_least(60.0));
            }
            table = table.column(Column::remainder());
            let mut rects: Vec<(String, Rect)> = Vec::new();
            table
                .header(24.0, |mut h| {
                    h.col(|ui| self.header_cell(ui, &tl!("Name"), SortKey::Name));
                    h.col(|ui| self.header_cell(ui, &tl!("Size"), SortKey::Size));
                    h.col(|ui| self.header_cell(ui, &tl!("Packed"), SortKey::Packed));
                    h.col(|ui| self.header_cell(ui, &tl!("Type"), SortKey::Kind));
                    h.col(|ui| self.header_cell(ui, &tl!("Modified"), SortKey::Modified));
                    if in_archive {
                        h.col(|ui| self.header_cell(ui, &tl!("Checksum"), SortKey::Crc));
                    }
                    h.col(|_| {});
                })
                .body(|body| {
                    body.rows(22.0, rows.len(), |mut tr| {
                        let i = tr.index();
                        let row = &rows[i];
                        tr.set_selected(selected.contains(&i));
                        tr.col(|ui| {
                            icons::show(ui, Self::row_icon(row), 16.0);
                            ui.add_space(4.0);
                            let t = if row.locked { RichText::new(&row.name).italics() } else { RichText::new(&row.name) };
                            ui.add(egui::Label::new(t).selectable(false).truncate());
                            if row.locked {
                                icons::show(ui, Icon::Lock, 12.0);
                            }
                        });
                        let right = |ui: &mut Ui, s: String| {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.add(egui::Label::new(s).selectable(false)));
                        };
                        tr.col(|ui| {
                            if let Some(s) = row.size.filter(|_| row.target != RowRef::Parent) {
                                right(ui, human_size(s, exact));
                            }
                        });
                        tr.col(|ui| {
                            if let Some(s) = row.packed {
                                right(ui, human_size(s, exact));
                            }
                        });
                        tr.col(|ui| {
                            ui.add(egui::Label::new(&row.kind).selectable(false).truncate());
                        });
                        tr.col(|ui| {
                            ui.add(egui::Label::new(format_time(row.modified, seconds)).selectable(false));
                        });
                        if in_archive {
                            tr.col(|ui| {
                                if let Some(c) = row.crc {
                                    ui.add(egui::Label::new(RichText::new(format!("{c:08X}")).monospace()).selectable(false));
                                }
                            });
                        }
                        tr.col(|_| {});
                        let resp = tr.response();
                        rects.push((row.name.clone(), resp.rect));
                        if resp.clicked() {
                            clicked = Some((i, mods));
                            if prefs.single_click_opens && row.is_dir {
                                activated = Some(i);
                            }
                        }
                        if resp.double_clicked() {
                            activated = Some(i);
                        }
                        if resp.drag_started() && row.target != RowRef::Parent {
                            if !selected.contains(&i) {
                                clicked = Some((i, Modifiers::NONE));
                            }
                            drag_start = true;
                        }
                        if resp.secondary_clicked() && row.target != RowRef::Parent {
                            context = Some(i);
                        }
                        if row.target != RowRef::Parent {
                            resp.context_menu(|ui| {
                            shown = true;
                            menu(ui, &mut chosen);
                        });
                        }
                    });
                });
            self.row_rects.extend(rects);
        }
        if let Some((i, m)) = clicked {
            self.click_row(i, m);
        }
        if let Some(i) = context
            && !self.selected.contains(&i)
        {
            self.selected = [i].into();
            self.anchor = Some(i);
        }
        background.context_menu(|ui| {
            shown = true;
            menu(ui, &mut chosen);
        });
        self.hud.context_menu = shown;
        if let Some(c) = chosen {
            self.run_command(&c);
        }
        if drag_start && in_archive {
            let items = self.selected_entries();
            if !items.is_empty() {
                self.drag_out = Some(items);
            }
        }
        if let Some(i) = activated {
            self.activate(i);
        }
    }

    fn keys(&mut self, ctx: &egui::Context) {
        if self.ui_state.any_modal() || ctx.egui_wants_keyboard_input() || craft_ui::menu::bar::is_open(ctx) {
            return;
        }
        let mut cmds: Vec<&str> = Vec::new();
        ctx.input_mut(|i| {
            let (ctrl, cs) = (Modifiers::COMMAND, Modifiers::COMMAND | Modifiers::SHIFT);
            let table: [(Modifiers, Key, &str); 19] = [
                (ctrl, Key::C, "archive.copy_names"),
                (Modifiers::NONE, Key::F2, "act.rename"),
                (cs, Key::E, "act.extract_here"),
                (cs, Key::A, "act.add"),
                (cs, Key::C, "archive.copy_paths"),
                (ctrl, Key::O, "archive.open"),
                (ctrl, Key::A, "select.all"),
                (ctrl, Key::E, "act.extract"),
                (ctrl, Key::T, "act.test"),
                (ctrl, Key::I, "act.info"),
                (ctrl, Key::F, "act.find"),
                (ctrl, Key::D, "bookmark.add"),
                (ctrl, Key::P, "archive.password"),
                (ctrl, Key::Comma, "prefs.open"),
                (Modifiers::NONE, Key::F3, "act.view"),
                (Modifiers::NONE, Key::F5, "view.refresh"),
                (Modifiers::NONE, Key::Delete, "act.delete"),
                (Modifiers::NONE, Key::Plus, "select.pattern"),
                (Modifiers::NONE, Key::Minus, "select.unpattern"),
            ];
            for (m, k, c) in table {
                if i.consume_key(m, k) {
                    cmds.push(c);
                }
            }
        });
        for c in cmds {
            self.run_command(c);
        }
        let (enter, back, up, down, home, end, shift) = ctx.input(|i| {
            (i.key_pressed(Key::Enter), i.key_pressed(Key::Backspace), i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown), i.key_pressed(Key::Home), i.key_pressed(Key::End), i.modifiers.shift)
        });
        let n = self.rows.len();
        if n > 0 {
            let cur = self.anchor.unwrap_or(0);
            let to = if up {
                Some(cur.saturating_sub(1))
            } else if down {
                Some((cur + 1).min(n - 1))
            } else if home {
                Some(0)
            } else if end {
                Some(n - 1)
            } else {
                None
            };
            if let Some(t) = to {
                if shift {
                    self.click_row(t, Modifiers::SHIFT);
                } else {
                    self.selected = [t].into();
                    self.anchor = Some(t);
                }
            }
        }
        if enter && let Some(i) = self.selected.iter().next().copied() {
            self.activate(i);
        }
        if back {
            self.go_up();
        }
    }

    /// Files dropped onto the window from outside.
    fn drops(&mut self, ctx: &egui::Context) {
        let (hover, dropped): (usize, Vec<PathBuf>) = ctx.input(|i| (i.raw.hovered_files.len(), i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect()));
        if hover > 0 {
            let screen = ctx.content_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("ac-drop")));
            painter.rect_filled(screen, egui::CornerRadius::ZERO, Color32::from_black_alpha(110));
            let msg = match &self.place {
                Place::Archive(v) => tf!("Drop to add to {name}", name = v.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()),
                Place::Disk(_) => tl!("Drop an archive to open it, or files to pack them"),
            };
            painter.text(screen.center(), egui::Align2::CENTER_CENTER, msg, egui::FontId::proportional(22.0), Color32::WHITE);
        }
        if dropped.is_empty() {
            return;
        }
        if matches!(self.place, Place::Archive(_)) {
            let d = dialogs::add::AddDialog::to_open_archive(self, dropped);
            self.ui_state.add = Some(d);
        } else if dropped.len() == 1 && dropped[0].is_file() && archivecraft_engine::model::format::looks_like_archive(&dropped[0].to_string_lossy()) {
            self.open_archive(dropped[0].clone());
        } else if dropped.len() == 1 && dropped[0].is_dir() {
            self.go_disk(dropped[0].clone());
        } else {
            self.open_add_dialog(dropped);
        }
    }

    /// Dragging items out of an open archive: a bar of places inside the window to drop them on
    /// (every system), and on Windows and macOS a real drag to the file manager once the pointer
    /// leaves the window.
    fn drag_out_bar(&mut self, ui: &mut Ui, frame: &eframe::Frame) {
        let Some(items) = self.drag_out.clone() else { return };
        let released = ui.input(|i| i.pointer.any_released());
        let pos = ui.ctx().pointer_latest_pos();
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        if ui.input(|i| i.pointer.hover_pos().is_none() && i.pointer.primary_down()) {
            self.drag_out = None;
            crate::dragout::start(self, frame, items);
            return;
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let _ = frame;
        let mut places: Vec<(String, PathBuf)> = Vec::new();
        if let Some(dir) = self.open_archive_view().and_then(|v| v.path.parent().map(PathBuf::from)) {
            places.push((tl!("Next to the archive"), dir));
        }
        for (label, d) in [(tl!("Desktop"), dirs::desktop_dir()), (tl!("Downloads"), dirs::download_dir()), (tl!("Home"), dirs::home_dir()), (tl!("Last unpack folder"), self.last_extract.clone())] {
            if let Some(d) = d {
                places.push((label, d));
            }
        }
        for b in self.prefs.get().saved.bookmarks.iter().filter(|b| b.path.is_dir()) {
            places.push((b.name.clone(), b.path.clone()));
        }
        let mut targets = Vec::new();
        egui::Panel::bottom("ac-dragbar").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(tf!("Drop {n} item(s) on a place to unpack them there:", n = items.len())).strong());
                for (label, path) in &places {
                    let r = ui.add(egui::Button::new(label.as_str()).min_size(Vec2::new(0.0, 30.0)));
                    if pos.is_some_and(|p| r.rect.contains(p)) {
                        ui.painter().rect_stroke(r.rect, egui::CornerRadius::same(4), egui::Stroke::new(2.0, icons::TEAL), egui::StrokeKind::Outside);
                    }
                    targets.push((r.rect, path.clone()));
                }
            });
        });
        self.drop_targets = targets;
        if released {
            let target = pos.and_then(|p| self.drop_targets.iter().find(|(r, _)| r.contains(p)).map(|(_, d)| d.clone()));
            self.drag_out = None;
            if let (Some(dest), Some(v)) = (target, self.open_archive_view()) {
                let (archive, folder) = (v.path.clone(), v.folder.clone());
                let mut o = self.default_extract_options(dest);
                o.only = items;
                o.base = folder;
                self.start_extract(vec![archive], o, false, dialogs::extract::DeleteAfter::Never);
            }
        }
    }

    fn title(&self) -> String {
        let full = self.prefs.get().interface.full_path_in_title;
        let p = match &self.place {
            Place::Disk(d) => d.clone(),
            Place::Archive(v) => v.path.clone(),
        };
        let shown = if full { p.display().to_string() } else { p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.display().to_string()) };
        format!("{shown} — ArchiveCraft")
    }

    fn screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.launch.screenshot.clone() else { return };
        if self.frame_no >= 120 && !self.shot_requested {
            self.shot_requested = true;
            let done = self.shot_done.clone();
            ctx.request_screenshot(move |img| {
                let (w, h) = (img.size[0] as u32, img.size[1] as u32);
                let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Some(buf) = image::RgbaImage::from_raw(w, h, bytes) {
                    let _ = buf.save(&path);
                }
                done.store(true, std::sync::atomic::Ordering::SeqCst);
            });
        }
        if self.shot_done.load(std::sync::atomic::Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        raw.events.append(&mut self.inject);
        let env = self.root_env();
        let fired = &mut self.fired;
        craft_ui::shell::embed::before_input(ctx, |out| {
            fired.extend(out.fired.iter().map(|f| f.command.id.clone()));
            craft_ui::runtime::frame::FrameInput::new(env)
        });
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_job();
        let now = web_time::Instant::now();
        let tick = self.prefs.tick(now);
        if let Some(due) = tick.next_deadline {
            ctx.request_repaint_after(due.saturating_duration_since(now));
        }
        let prefs = &mut self.prefs;
        craft_ui::shell::embed::logic(ctx, || {
            let _ = prefs.flush();
            craft_ui::shell::embed::CloseDecision::Close
        });
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        craft_ui::shell::embed::before_ui(&ctx, frame);
        self.frame_no += 1;
        for id in std::mem::take(&mut self.fired) {
            self.run_command(&id);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));
        self.hud.toolbar_buttons = 0;
        self.draw_menu(ui);
        self.draw_toolbar(ui);
        self.draw_path_bar(ui);
        self.draw_status(ui);
        self.drag_out_bar(ui, frame);
        egui::CentralPanel::default().show(ui, |ui| self.draw_list(ui));
        self.keys(&ctx);
        self.drops(&ctx);
        dialogs::show_all(self, &ctx);
        self.files.show_pending(Some(&*frame));
        for (pick, answer) in self.files.take_answers() {
            self.deliver(pick, answer);
        }
        if self.job.is_some() || self.launch.screenshot.is_some() || self.self_test.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
        if let Some(mut st) = self.self_test.take() {
            st.step(self, &ctx);
            self.self_test = Some(st);
        }
        self.screenshot(&ctx);
    }
}

/// One row of a right-click menu: optional check mark, label, shortcut on the right.
fn menu_row(ui: &mut Ui, label: &str, keys: &str, check: Option<bool>, enabled: bool) -> bool {
    let text = if check.is_some() { format!("     {label}") } else { label.to_string() };
    let mut b = egui::Button::new(text).frame(false).min_size(Vec2::new(ui.available_width(), 0.0));
    if !keys.is_empty() {
        b = b.shortcut_text(keys);
    }
    let r = ui.add_enabled(enabled, b);
    if check == Some(true) {
        let c = egui::pos2(r.rect.left() + 10.0, r.rect.center().y);
        let stroke = egui::Stroke::new(1.8, ui.visuals().text_color());
        ui.painter().line(vec![c + Vec2::new(-4.0, 0.0), c + Vec2::new(-1.0, 3.5), c + Vec2::new(5.0, -4.0)], stroke);
    }
    r.clicked()
}
