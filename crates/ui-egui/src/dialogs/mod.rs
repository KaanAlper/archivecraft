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
    pub viewer: Option<viewer::Viewer>,
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
    run!(app, ctx, viewer);
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
    ui.separator();
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        out.1 = ui.button(crate::tl!("Cancel")).clicked();
        out.0 = ui.add_enabled(ok_enabled, egui::Button::new(ok_label)).clicked();
    });
    out
}
