//! The windows the main window opens. Each one is taken out of `Open` while it is drawn, gets
//! the whole `App` to act on, and says whether it stays open.

pub mod add;
pub mod extract;
pub mod find;
pub mod info;
pub mod naming;
pub mod prefs;
pub mod progress;
pub mod tools;
pub mod viewer;
pub mod wizard;

use crate::state::App;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Keep,
    Close,
}

#[derive(Default)]
pub struct Open {
    pub add: Option<add::AddDialog>,
    pub extract: Option<extract::ExtractDialog>,
    pub info: Option<info::InfoDialog>,
    pub viewers: Vec<viewer::Viewer>,
    pub find: Option<find::FindDialog>,
    pub found: Option<find::FoundWindow>,
    pub wizard: Option<wizard::Wizard>,
    pub prefs: Option<prefs::PrefsDialog>,
    pub repair: Option<tools::RepairDialog>,
    pub convert: Option<tools::ConvertDialog>,
    pub recovery: Option<tools::RecoveryDialog>,
    pub bench: Option<tools::BenchWindow>,
    pub bookmarks: Option<tools::BookmarksWindow>,
    pub confirm: Option<tools::Confirm>,
    pub pattern: Option<tools::PatternPrompt>,
    pub name_prompt: Option<naming::NamePrompt>,
    pub offer: Option<naming::IntegrationOffer>,
    pub unlock: Option<progress::PasswordPrompt>,
    pub session_pw: Option<progress::PasswordPrompt>,
    pub message: Option<progress::MessageBox>,
    pub diags: Option<progress::DiagWindow>,
    pub last_diags: Option<progress::DiagWindow>,
}

impl Open {
    /// Windows that take the keyboard from the main list.
    pub fn any_modal(&self) -> bool {
        self.add.is_some()
            || self.extract.is_some()
            || self.confirm.is_some()
            || self.unlock.is_some()
            || self.session_pw.is_some()
            || self.message.is_some()
            || self.wizard.is_some()
            || self.prefs.is_some()
            || self.pattern.is_some()
            || self.name_prompt.is_some()
            || self.offer.is_some()
            || self.find.is_some()
            || self.repair.is_some()
            || self.convert.is_some()
            || self.recovery.is_some()
            || self.info.as_ref().is_some_and(|i| i.tab == 1)
    }

    /// Any dialog or message window currently visible on screen.
    pub fn any_open(&self) -> bool {
        self.any_modal()
            || self.info.is_some()
            || !self.viewers.is_empty()
            || self.found.is_some()
            || self.bench.is_some()
            || self.bookmarks.is_some()
            || self.diags.is_some()
    }
}

macro_rules! run {
    ($app:ident, $ctx:ident, $field:ident) => {
        if let Some(mut d) = $app.ui_state.$field.take() {
            if d.show($app, $ctx) == Flow::Keep && $app.ui_state.$field.is_none() {
                $app.ui_state.$field = Some(d);
            }
        }
    };
}

/// Draw every open window, then the job's progress on top.
pub fn show_all(app: &mut App, ctx: &egui::Context) {
    let mut viewers = std::mem::take(&mut app.ui_state.viewers);
    viewers.retain_mut(|v| v.show(app, ctx) == Flow::Keep);
    if !app.ui_state.viewers.is_empty() {
        viewers.extend(std::mem::take(&mut app.ui_state.viewers));
    }
    app.ui_state.viewers = viewers;
    run!(app, ctx, found);
    run!(app, ctx, info);
    run!(app, ctx, bench);
    run!(app, ctx, bookmarks);
    run!(app, ctx, find);
    run!(app, ctx, repair);
    run!(app, ctx, convert);
    run!(app, ctx, recovery);
    run!(app, ctx, add);
    run!(app, ctx, extract);
    run!(app, ctx, wizard);
    run!(app, ctx, prefs);
    run!(app, ctx, pattern);
    run!(app, ctx, name_prompt);
    run!(app, ctx, offer);
    run!(app, ctx, confirm);
    run!(app, ctx, unlock);
    run!(app, ctx, session_pw);
    progress::show_job(app, ctx);
    run!(app, ctx, diags);
    run!(app, ctx, message);
}

/// A dialog window in the app's style.
pub fn window(title: impl Into<egui::WidgetText>, id: &str) -> egui::Window<'static> {
    egui::Window::new(title).id(egui::Id::unique(id)).collapsible(false).resizable(true).pivot(egui::Align2::CENTER_CENTER).default_pos(egui::pos2(480.0, 330.0))
}

/// A row of tabs; returns the chosen index.
pub fn tabs(ui: &mut egui::Ui, current: &mut usize, labels: &[String]) {
    ui.horizontal(|ui| {
        for (i, l) in labels.iter().enumerate() {
            ui.selectable_value(current, i, l.as_str());
        }
    });
    ui.separator();
}

/// A titled group box.
pub fn group<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::group(ui.style())
        .show(ui, |ui| {
            ui.label(egui::RichText::new(title).strong());
            ui.add_space(2.0);
            add(ui)
        })
        .inner
}

/// OK / Cancel buttons at the bottom right; returns (ok, cancel).
pub fn ok_cancel(ui: &mut egui::Ui, ok_label: &str, ok_enabled: bool) -> (bool, bool) {
    let mut out = (false, false);
    let btn_size = egui::vec2(88.0, 26.0);
    ui.separator();
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        out.1 = ui.add(egui::Button::new(crate::tl!("Cancel")).min_size(btn_size)).clicked();
        out.0 = ui.add_enabled(ok_enabled, egui::Button::new(ok_label).min_size(btn_size)).clicked();
    });
    out
}

/// Single-line text edit filling the remaining width to the left of `right_controls`.
/// Controls are added right-to-left (rightmost first) so the row never overflows `available_width`.
pub fn text_row(
    ui: &mut egui::Ui,
    text: &mut String,
    hint: Option<&str>,
    right_controls: impl FnOnce(&mut egui::Ui, &str),
) -> egui::Response {
    let row_size = egui::vec2(ui.available_width().max(0.0), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(row_size, egui::Layout::right_to_left(egui::Align::Center), |ui| {
        right_controls(ui, text.as_str());
        let mut edit = egui::TextEdit::singleline(text).desired_width(ui.available_width().max(0.0));
        if let Some(h) = hint {
            edit = edit.hint_text(h);
        }
        ui.add(edit)
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Launch;
    use std::path::PathBuf;

    fn run_dialog_frames<F: FnMut(&mut App, &egui::Context)>(name: &str, id_str: &str, mut draw: F) -> Option<String> {
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx, Launch::default());
        let id = egui::Id::unique(id_str);
        let mut sizes = Vec::new();
        for _ in 0..10 {
            let mut input = egui::RawInput::default();
            input.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0)));
            let mut out = ctx.run_ui(input, |ui| {
                let ctx = ui.ctx().clone();
                draw(&mut app, &ctx);
            });
            out.textures_delta.clear();
            let sz = ctx.memory(|m| m.area_rect(id)).map(|r| r.size());
            sizes.push(sz);
        }
        if let (Some(Some(f)), Some(Some(l))) = (sizes.get(2), sizes.last()) {
            if (l.x - f.x).abs() >= 2.0 || (l.y - f.y).abs() >= 2.0 {
                return Some(format!("{name} grew! frame2={f:?} frame9={l:?}"));
            }
        }
        None
    }

    #[test]
    fn windows_do_not_grow_unbounded() {
        let mut failures = Vec::new();
        for tab in 0..5 {
            let name = format!("add-tab-{tab}");
            if let Some(e) = run_dialog_frames(&name, "ac-add", |app, ctx| {
                let mut d = add::AddDialog::new(app, vec![PathBuf::from("/tmp/a.txt")]);
                d.tab = tab;
                d.show(app, ctx);
            }) {
                failures.push(e);
            }
        }
        for tab in 0..3 {
            let name = format!("extract-tab-{tab}");
            if let Some(e) = run_dialog_frames(&name, "ac-extract", |app, ctx| {
                let mut d = extract::ExtractDialog::new(app, vec![PathBuf::from("/tmp/a.zip")], vec![], String::new());
                d.tab = tab;
                d.show(app, ctx);
            }) {
                failures.push(e);
            }
        }
        if let Some(e) = run_dialog_frames("find", "ac-find", |app, ctx| {
            let mut d = find::FindDialog::new(PathBuf::from("/tmp"));
            d.show(app, ctx);
        }) {
            failures.push(e);
        }
        if let Some(e) = run_dialog_frames("found", "ac-found", |app, ctx| {
            let mut d = find::FoundWindow { hits: vec![], selected: None };
            d.show(app, ctx);
        }) {
            failures.push(e);
        }
        if let Some(e) = run_dialog_frames("repair", "ac-repair", |app, ctx| {
            let mut d = tools::RepairDialog::new(PathBuf::from("/tmp/a.zip"));
            d.show(app, ctx);
        }) {
            failures.push(e);
        }
        if let Some(e) = run_dialog_frames("convert", "ac-convert", |app, ctx| {
            let mut d = tools::ConvertDialog::new(vec![PathBuf::from("/tmp/a.zip")], PathBuf::from("/tmp"));
            d.show(app, ctx);
        }) {
            failures.push(e);
        }
        if let Some(e) = run_dialog_frames("bench", "ac-bench", |app, ctx| {
            let mut d = tools::BenchWindow::default();
            d.show(app, ctx);
        }) {
            failures.push(e);
        }
        if let Some(e) = run_dialog_frames("bookmarks", "ac-bookmarks", |app, ctx| {
            let mut d = tools::BookmarksWindow::default();
            d.show(app, ctx);
        }) {
            failures.push(e);
        }
        if let Some(e) = run_dialog_frames("viewer", "ac-viewer", |app, ctx| {
            let mut d = viewer::Viewer::new(ctx, "test.txt".into(), b"hello world\nline2\n".to_vec(), app.prefs.get().viewer.clone(), false);
            d.show(app, ctx);
        }) {
            failures.push(e);
        }
        for page in 0..8 {
            let name = format!("prefs-page-{page}");
            if let Some(e) = run_dialog_frames(&name, "ac-prefs", |app, ctx| {
                let mut d = prefs::PrefsDialog::new(app);
                d.page = page;
                d.show(app, ctx);
            }) {
                failures.push(e);
            }
        }
        if let Some(e) = run_dialog_frames("wizard", "ac-wizard", |app, ctx| {
            let mut d = wizard::Wizard::default();
            d.show(app, ctx);
        }) {
            failures.push(e);
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
