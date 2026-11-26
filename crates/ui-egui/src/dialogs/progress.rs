//! One progress window for every job (packing, checking, unpacking…), the "file already there"
//! question shown over it, password prompts, the problems list and short messages.

use super::{ok_cancel, Flow};
use crate::state::App;
use crate::{icons, tf, tl};
use archivecraft_engine::model::util::{format_time, group_digits};
use archivecraft_engine::model::{Answer, Ctl, Diag, Question, Severity};
use std::path::PathBuf;

fn hms(secs: u64) -> String {
    format!("{:02}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60)
}

pub fn show_job(app: &mut App, ctx: &egui::Context) {
    let Some(job) = &mut app.job else { return };
    if !job.background {
        let p = job.ctl.progress.lock().map(|p| p.clone()).unwrap_or_default();
        let paused = job.ctl.is_paused();
        let mut cancel = false;
        let mut pause = None;
        let mut background = false;
        egui::Window::new(job.title.clone())
            .id(egui::Id::unique("ac-progress"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.set_width(380.0);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new(&p.archive).weak());
                    ui.label(if paused { tl!("paused") } else { p.stage.to_lowercase() });
                    ui.add(egui::Label::new(&p.current).truncate());
                    ui.add(egui::ProgressBar::new(p.item_fraction()).show_percentage());
                });
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    egui::Grid::new("ac-prog").num_columns(2).show(ui, |ui| {
                        ui.label(tl!("Time so far"));
                        ui.label(hms(job.ctl.started.elapsed().as_secs()));
                        ui.end_row();
                        if let Some(r) = p.running_ratio() {
                            ui.label(tl!("Size so far"));
                            ui.label(format!("{r}%"));
                            ui.end_row();
                        }
                        ui.label(tl!("Done"));
                        ui.label(if p.total_items > 0 { tf!("{a} of {b} items", a = p.done_items, b = p.total_items) } else { tf!("{a} items", a = p.done_items) });
                        ui.end_row();
                    });
                    ui.add(egui::ProgressBar::new(p.fraction()).show_percentage());
                });
                ui.horizontal(|ui| {
                    background = ui.button(tl!("Keep working in the background")).clicked();
                    if ui.button(if paused { tl!("Go on") } else { tl!("Pause") }).clicked() {
                        pause = Some(!paused);
                    }
                    cancel = ui.button(tl!("Stop")).clicked();
                });
            });
        if let Some(pz) = pause {
            job.ctl.set_paused(pz);
        }
        if cancel {
            job.ctl.cancel();
            job.answer(Answer::Cancel);
        }
        if background {
            job.background = true;
        }
    }
    let Some((q, _)) = &job.asking else { return };
    let q = q.clone();
    let mut answer: Option<Answer> = None;
    match q {
        Question::Replace { path, old_size, old_time, new_size, new_time } => {
            egui::Modal::new(egui::Id::unique("ac-replace")).show(ctx, |ui| {
                ui.set_width(430.0);
                ui.heading(tl!("This file is already there"));
                ui.add(egui::Label::new(egui::RichText::new(&path).monospace()).wrap());
                ui.add_space(6.0);
                ui.columns(2, |c| {
                    for (i, (title, size, time)) in [(tl!("Already there"), old_size, old_time), (tl!("From the archive"), Some(new_size), new_time)].into_iter().enumerate() {
                        egui::Frame::group(c[i].style()).show(&mut c[i], |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                icons::show(ui, icons::Icon::File, 28.0);
                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new(title).strong());
                                    ui.label(tf!("{n} bytes", n = size.map(group_digits).unwrap_or_default()));
                                    ui.label(tf!("changed {t}", t = format_time(time, true)));
                                });
                            });
                        });
                    }
                });
                ui.add_space(6.0);
                egui::Grid::new("ac-replace-btns").num_columns(4).show(ui, |ui| {
                    let mut b = |ui: &mut egui::Ui, l: String, a: Answer| {
                        if ui.add_sized([100.0, 24.0], egui::Button::new(l)).clicked() {
                            answer = Some(a);
                        }
                    };
                    b(ui, tl!("Replace"), Answer::Yes);
                    b(ui, tl!("Replace all"), Answer::YesToAll);
                    b(ui, tl!("Keep both"), Answer::Rename);
                    b(ui, tl!("Keep both, all"), Answer::RenameAll);
                    ui.end_row();
                    b(ui, tl!("Skip"), Answer::No);
                    b(ui, tl!("Skip all"), Answer::NoToAll);
                    b(ui, tl!("Stop"), Answer::Cancel);
                    ui.end_row();
                });
            });
        }
        Question::Password { archive, item, wrong } => {
            let prompt = app.ui_state.unlock.get_or_insert_with(|| PasswordPrompt::new(archive.clone(), item.clone(), wrong, None).for_job());
            let _ = prompt;
        }
    }
    if let Some(a) = answer
        && let Some(j) = &mut app.job
    {
        if a == Answer::Cancel {
            j.ctl.cancel();
        }
        j.answer(a);
    }
}

/// Asks for a password: for a running job, to open an archive with locked names, to look at a
/// locked item, or to set the session password.
pub struct PasswordPrompt {
    archive: String,
    item: String,
    wrong: bool,
    text: String,
    show: bool,
    remember: bool,
    open_after: Option<PathBuf>,
    view_after: Option<String>,
    job: bool,
}

impl PasswordPrompt {
    pub fn new(archive: String, item: String, wrong: bool, open_after: Option<PathBuf>) -> Self {
        PasswordPrompt { archive, item, wrong, text: String::new(), show: false, remember: true, open_after, view_after: None, job: false }
    }
    pub fn then_view(mut self, item: String) -> Self {
        self.view_after = Some(item);
        self
    }
    fn for_job(mut self) -> Self {
        self.job = true;
        self
    }

    pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> Flow {
        let mut result: Option<Option<String>> = None;
        egui::Modal::new(egui::Id::unique(("ac-pw", self.job))).show(ctx, |ui| {
            ui.set_width(360.0);
            if self.archive.is_empty() {
                ui.heading(tl!("Password for this session"));
                ui.label(tl!("Tried first on every locked archive until ArchiveCraft closes. It is never saved to disk."));
            } else {
                ui.heading(tl!("This is locked"));
                if self.item.is_empty() {
                    ui.label(tf!("Enter the password for {a}", a = self.archive));
                } else {
                    ui.label(tf!("Enter the password for \"{i}\" in {a}", i = self.item, a = self.archive));
                }
            }
            if self.wrong {
                ui.colored_label(icons::RED, tl!("That password did not work. Try again."));
            }
            let r = ui.add(egui::TextEdit::singleline(&mut self.text).password(!self.show).desired_width(f32::INFINITY));
            r.request_focus();
            ui.checkbox(&mut self.show, tl!("Show the password"));
            if !self.archive.is_empty() {
                ui.checkbox(&mut self.remember, tl!("Use it for other archives until I close ArchiveCraft"));
            }
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let (ok, cancel) = ok_cancel(ui, &tl!("OK"), !self.text.is_empty());
            if (ok || enter) && !self.text.is_empty() {
                result = Some(Some(self.text.clone()));
            } else if cancel {
                result = Some(None);
            }
        });
        let Some(res) = result else { return Flow::Keep };
        if self.job
            && let Some(j) = &mut app.job {
                j.answer(match &res {
                    Some(p) => Answer::Password(p.clone()),
                    None => Answer::Cancel,
                });
            }
        if let Some(p) = res {
            if self.remember || self.archive.is_empty() {
                app.session_password = Some(p.clone());
            }
            if let Some(path) = self.open_after.take() {
                app.open_archive_with(path, Some(p.clone()));
            } else if let Some(item) = self.view_after.take() {
                if let crate::state::Place::Archive(v) = &mut app.place {
                    v.password = Some(p);
                }
                app.view_entry(item);
            }
        }
        Flow::Close
    }
}

/// The list of problems a job found: one window instead of many pop-ups.
#[derive(Clone)]
pub struct DiagWindow {
    title: String,
    items: Vec<Diag>,
}

impl DiagWindow {
    pub fn from_ctl(ctl: &Ctl, title: String) -> Self {
        DiagWindow { title, items: ctl.diags.lock().map(|d| d.clone()).unwrap_or_default() }
    }

    pub fn show(&mut self, app: &mut App, ctx: &egui::Context) -> Flow {
        let mut flow = Flow::Keep;
        let mut open = true;
        super::window(self.title.clone(), "ac-diags").open(&mut open).default_size([560.0, 320.0]).show(ctx, |ui| {
            egui_extras::TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .max_scroll_height(260.0)
                .column(egui_extras::Column::initial(330.0).clip(true))
                .column(egui_extras::Column::remainder().clip(true))
                .header(20.0, |mut h| {
                    h.col(|ui| {
                        ui.strong(tl!("What happened"));
                    });
                    h.col(|ui| {
                        ui.strong(tl!("Archive"));
                    });
                })
                .body(|body| {
                    body.rows(20.0, self.items.len(), |mut r| {
                        let d = &self.items[r.index()];
                        r.col(|ui| {
                            let (c, s) = match d.severity {
                                Severity::Info => (icons::SKY, "i"),
                                Severity::Warning => (icons::AMBER_DARK, "!"),
                                Severity::Error => (icons::RED, "×"),
                            };
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 7.5, c);
                            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, s, egui::FontId::proportional(11.0), egui::Color32::WHITE);
                            ui.add(egui::Label::new(&d.message).truncate()).on_hover_text(&d.message);
                        });
                        r.col(|ui| {
                            ui.add(egui::Label::new(&d.archive).truncate());
                        });
                    });
                });
            ui.separator();
            ui.horizontal(|ui| {
                let errors = self.items.iter().filter(|d| d.severity == Severity::Error).count();
                ui.label(tf!("{e} problem(s), {n} note(s) in total", e = errors, n = self.items.len()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tl!("Close")).clicked() {
                        flow = Flow::Close;
                    }
                    if ui.button(tl!("Copy all")).clicked() {
                        let text = self.items.iter().map(|d| format!("{:?}\t{}\t{}", d.severity, d.message, d.archive)).collect::<Vec<_>>().join("\n");
                        ui.ctx().copy_text(text);
                    }
                    if let Some(j) = &app.job
                        && ui.button(tl!("Stop the task")).clicked()
                    {
                        j.ctl.cancel();
                    }
                });
            });
        });
        if !open { Flow::Close } else { flow }
    }
}

/// A short message with one button.
pub struct MessageBox {
    text: String,
}

impl MessageBox {
    pub fn info(text: String) -> Self {
        MessageBox { text }
    }

    pub fn show(&mut self, _app: &mut App, ctx: &egui::Context) -> Flow {
        let mut flow = Flow::Keep;
        egui::Modal::new(egui::Id::unique("ac-msg")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.horizontal(|ui| {
                icons::show(ui, icons::Icon::Info, 30.0);
                ui.add(egui::Label::new(&self.text).wrap());
            });
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tl!("OK")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape)) {
                    flow = Flow::Close;
                }
            });
        });
        flow
    }
}
