//! The window's state: where it is (a disk folder or a folder inside an archive), what is
//! chosen, and the jobs it starts.

use crate::dialogs;
use crate::jobs::{Job, JobKind};
use crate::rows::{self, Row, RowRef, Sort};
use crate::settings::Prefs;
use crate::{tf, tl};
use archivecraft_engine::model::{CreateOptions, Ctl, ExtractOptions, Listing, Severity};
use archivecraft_engine::{bench, find, formats, ops, repair, Error};
use craft_settings::{Location, SettingsStore};
use egui::Rect;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

/// How the program was started.
#[derive(Default, Debug, Clone)]
pub struct Launch {
    pub screenshot: Option<PathBuf>,
    pub self_test: Option<PathBuf>,
    pub open: Vec<PathBuf>,
    pub action: Option<LaunchAction>,
}

#[derive(Debug, Clone)]
pub enum LaunchAction {
    AddDialog(Vec<PathBuf>),
    AddZip(Vec<PathBuf>),
    ExtractHere(Vec<PathBuf>),
    ExtractFolder(Vec<PathBuf>),
}

pub struct ArchiveView {
    pub path: PathBuf,
    pub listing: Listing,
    pub folder: String,
    pub password: Option<String>,
}

pub enum Place {
    Disk(PathBuf),
    Archive(ArchiveView),
}

pub type Res<T> = archivecraft_engine::Result<T>;

/// What a finished job hands back.
pub enum Outcome {
    Created(Res<Vec<ops::CreateReport>>, PathBuf),
    Extracted(Vec<(PathBuf, Res<ops::ExtractReport>)>, PathBuf, dialogs::extract::DeleteAfter),
    Tested(Vec<(PathBuf, Res<ops::TestReport>)>),
    Deleted(Res<usize>),
    Converted(Vec<(PathBuf, Res<usize>)>),
    Repaired(Res<repair::RepairReport>),
    Commented(Res<()>),
    Found(Res<usize>, Vec<find::Hit>),
    Benched(Res<bench::BenchResult>),
    Recovery(Res<Vec<PathBuf>>),
}

/// Who asked for a file picker, so the answer goes back to the right place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    OpenArchive,
    AddArchiveName,
    AddFiles,
    AddFolder,
    ExtractDest,
    WizardArchive,
    WizardNewArchive,
    WizardFiles,
    FindFolder,
    RepairOut,
    ConvertOut,
    CommentFile,
    InfoCommentFile,
    PrefsFolder(u8),
    ExternalViewer,
}

/// What was drawn this frame, for the self-test's "hud" step.
#[derive(Default, Clone, Debug)]
pub struct Hud {
    pub menu_bar: bool,
    pub toolbar_buttons: usize,
    pub path_bar: bool,
    pub headers: Vec<String>,
    pub status: String,
    /// A row's right-click menu was drawn this frame.
    pub context_menu: bool,
}

pub struct App {
    pub prefs: SettingsStore<Prefs>,
    pub place: Place,
    pub rows: Vec<Row>,
    pub sort: Sort,
    pub selected: BTreeSet<usize>,
    pub anchor: Option<usize>,
    pub path_text: String,
    pub job: Option<Job<Outcome>>,
    pub status_note: Option<(String, Instant)>,
    pub session_password: Option<String>,
    pub ui_state: dialogs::Open,
    pub files: craft_dialogs::Dialogs<Pick>,
    pub menu: craft_tracked::Tracked<craft_menu::MenuBar>,
    pub menu_key: String,
    pub fired: Vec<String>,
    pub hud: Hud,
    pub row_rects: HashMap<String, Rect>,
    pub drag_out: Option<Vec<String>>,
    pub drop_targets: Vec<(Rect, PathBuf)>,
    pub launch: Launch,
    pub frame_no: u64,
    pub inject: Vec<egui::Event>,
    pub self_test: Option<crate::selftest::SelfTest>,
    pub shot_requested: bool,
    pub shot_done: Arc<std::sync::atomic::AtomicBool>,
    pub last_extract: Option<PathBuf>,
    pub ctx: egui::Context,
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.display().to_string())
}

fn disk_target(r: Row) -> Option<PathBuf> {
    if let RowRef::Disk(p) = r.target { Some(p) } else { None }
}

impl App {
    pub fn new(ctx: &egui::Context, launch: Launch) -> App {
        let scripted = launch.screenshot.is_some() || launch.self_test.is_some();
        let location = if scripted { Location::Memory } else { Location::Auto };
        let (prefs, _report) = SettingsStore::<Prefs>::open(location);
        let wake_ctx = ctx.clone();
        let files = craft_dialogs::Dialogs::new(craft_host::Wake::new(move || wake_ctx.request_repaint()));
        let start = {
            let p = prefs.get();
            let from_prefs = if p.paths.reopen_last_folder { p.paths.last_folder.clone() } else { p.paths.start_folder.clone() };
            from_prefs.filter(|d| d.is_dir()).or_else(|| std::env::current_dir().ok()).or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from("/"))
        };
        let mut app = App {
            prefs,
            place: Place::Disk(start.clone()),
            rows: Vec::new(),
            sort: Sort::default(),
            selected: BTreeSet::new(),
            anchor: None,
            path_text: start.display().to_string(),
            job: None,
            status_note: None,
            session_password: None,
            ui_state: dialogs::Open::default(),
            files,
            menu: craft_tracked::Tracked::new(craft_menu::MenuBar::default()),
            menu_key: String::new(),
            fired: Vec::new(),
            hud: Hud::default(),
            row_rects: HashMap::new(),
            drag_out: None,
            drop_targets: Vec::new(),
            launch: launch.clone(),
            frame_no: 0,
            inject: Vec::new(),
            self_test: launch.self_test.clone().map(crate::selftest::SelfTest::new),
            shot_requested: false,
            shot_done: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            last_extract: None,
            ctx: ctx.clone(),
        };
        app.refresh();
        for p in &launch.open {
            if p.is_dir() {
                app.go_disk(p.clone());
            } else {
                app.open_archive(p.clone());
            }
        }
        if let Some(a) = launch.action {
            app.launch_action(a);
        } else if app.prefs.get().interface.guide_on_start && !scripted {
            app.ui_state.wizard = Some(dialogs::wizard::Wizard::default());
        }
        let installed = archivecraft_engine::integration::Places::for_user().is_some_and(|p| archivecraft_engine::integration::installed(&p) != archivecraft_engine::integration::Targets::default());
        if !scripted && cfg!(all(unix, not(target_os = "macos"))) && !app.prefs.get().integration.offered && !installed {
            app.ui_state.offer = Some(dialogs::naming::IntegrationOffer);
        }
        app
    }

    fn launch_action(&mut self, a: LaunchAction) {
        match a {
            LaunchAction::AddDialog(files) => self.open_add_dialog(files),
            LaunchAction::AddZip(files) => {
                let archive = ops::archive_path_for(&files, archivecraft_engine::Format::Zip);
                let opts = CreateOptions { format: archivecraft_engine::Format::Zip, ..self.default_create_options() };
                self.start_create(files, archive, opts, false);
            }
            LaunchAction::ExtractHere(archives) => {
                let dest = archives.first().and_then(|a| a.parent()).map(Path::to_path_buf).unwrap_or_default();
                let o = self.default_extract_options(dest);
                self.start_extract(archives, o, false, dialogs::extract::DeleteAfter::Never);
            }
            LaunchAction::ExtractFolder(archives) => {
                let dest = archives.first().and_then(|a| a.parent()).map(Path::to_path_buf).unwrap_or_default();
                let o = self.default_extract_options(dest);
                self.start_extract(archives, o, true, dialogs::extract::DeleteAfter::Never);
            }
        }
    }

    // ------------------------------------------------------------ places and rows

    pub fn disk_dir(&self) -> PathBuf {
        match &self.place {
            Place::Disk(d) => d.clone(),
            Place::Archive(v) => v.path.parent().map(Path::to_path_buf).unwrap_or_default(),
        }
    }

    pub fn refresh(&mut self) {
        let prefs = self.prefs.get().list.clone();
        self.rows = match &self.place {
            Place::Disk(d) => match rows::disk_rows(d, &prefs) {
                Ok(r) => r,
                Err(e) => {
                    self.note(tf!("This folder cannot be read: {e}", e = e));
                    vec![rows::parent_row()]
                }
            },
            Place::Archive(v) => rows::archive_rows(&v.listing, &v.folder),
        };
        rows::sort_rows(&mut self.rows, self.sort, prefs.archives_first);
        self.selected.clear();
        self.anchor = None;
        self.path_text = match &self.place {
            Place::Disk(d) => d.display().to_string(),
            Place::Archive(v) => v.path.display().to_string(),
        };
    }

    pub fn go_disk(&mut self, dir: PathBuf) {
        self.place = Place::Disk(dir.clone());
        self.prefs.update(|p| p.paths.last_folder = Some(dir));
        self.refresh();
    }

    pub fn enter_folder(&mut self, folder: String) {
        if let Place::Archive(v) = &mut self.place {
            v.folder = folder;
        }
        self.refresh();
    }

    pub fn note(&mut self, s: impl Into<String>) {
        self.status_note = Some((s.into(), Instant::now()));
    }

    /// Open an archive in the list. Asks for a password when its names are locked.
    pub fn open_archive(&mut self, path: PathBuf) {
        let pw = self.session_password.clone();
        self.open_archive_with(path, pw);
    }

    pub fn open_archive_with(&mut self, path: PathBuf, password: Option<String>) {
        match formats::list(&path, password.as_deref()) {
            Ok(listing) => {
                let show_comment = self.prefs.get().interface.show_comment_on_open && listing.comment.as_deref().is_some_and(|c| !c.trim().is_empty());
                self.prefs.update(|p| p.remember(path.clone()));
                self.place = Place::Archive(ArchiveView { path: path.clone(), listing, folder: String::new(), password });
                self.refresh();
                if show_comment {
                    self.ui_state.info = Some(dialogs::info::InfoDialog::for_archive(self, &path, 1));
                }
            }
            Err(e) if e.is_password() => {
                let wrong = matches!(e, Error::WrongPassword) || password.is_some();
                self.ui_state.unlock = Some(dialogs::progress::PasswordPrompt::new(path.display().to_string(), String::new(), wrong, Some(path)));
            }
            Err(e) => self.problem(&path, e),
        }
    }

    pub fn problem(&mut self, archive: &Path, e: Error) {
        let ctl = Ctl::default();
        ctl.error(&archive.display().to_string(), e.to_string());
        self.ui_state.diags = Some(dialogs::progress::DiagWindow::from_ctl(&ctl, tl!("Problems")));
    }

    pub fn go_up(&mut self) {
        match &self.place {
            Place::Disk(d) => {
                if let Some(p) = d.parent().map(Path::to_path_buf) {
                    let from = d.clone();
                    self.go_disk(p);
                    self.select_target(&RowRef::Disk(from));
                }
            }
            Place::Archive(v) => {
                if v.folder.is_empty() {
                    let archive = v.path.clone();
                    let dir = archive.parent().map(Path::to_path_buf).unwrap_or_default();
                    self.go_disk(dir);
                    self.select_target(&RowRef::Disk(archive));
                } else {
                    let from = v.folder.clone();
                    let up = v.folder.rsplit_once('/').map(|(a, _)| a.to_string()).unwrap_or_default();
                    self.enter_folder(up);
                    self.select_target(&RowRef::Entry(from));
                }
            }
        }
    }

    pub fn select_target(&mut self, t: &RowRef) {
        if let Some(i) = self.rows.iter().position(|r| &r.target == t) {
            self.selected = [i].into();
            self.anchor = Some(i);
        }
    }

    /// Double-click / Enter on a row.
    pub fn activate(&mut self, i: usize) {
        let Some(row) = self.rows.get(i).cloned() else { return };
        match row.target {
            RowRef::Parent => self.go_up(),
            RowRef::Disk(p) if row.is_dir => self.go_disk(p),
            RowRef::Disk(p) if row.is_archive => self.open_archive(p),
            RowRef::Disk(p) => self.view_disk_file(p),
            RowRef::Entry(path) if row.is_dir => self.enter_folder(path),
            RowRef::Entry(path) => self.view_entry(path),
        }
    }

    pub fn selected_rows(&self) -> Vec<Row> {
        self.selected.iter().filter_map(|i| self.rows.get(*i)).filter(|r| r.target != RowRef::Parent).cloned().collect()
    }

    pub fn selected_disk(&self) -> Vec<PathBuf> {
        self.selected_rows().into_iter().filter_map(disk_target).collect()
    }

    pub fn selected_archives(&self) -> Vec<PathBuf> {
        self.selected_rows().into_iter().filter(|r| r.is_archive).filter_map(disk_target).collect()
    }

    pub fn selected_entries(&self) -> Vec<String> {
        self.selected_rows().into_iter().filter_map(|r| if let RowRef::Entry(p) = r.target { Some(p) } else { None }).collect()
    }

    pub fn open_archive_view(&self) -> Option<&ArchiveView> {
        match &self.place {
            Place::Archive(v) => Some(v),
            Place::Disk(_) => None,
        }
    }

    /// The open archive, or the first chosen archive in a disk folder.
    pub fn target_archive(&self) -> Option<PathBuf> {
        match &self.place {
            Place::Archive(v) => Some(v.path.clone()),
            Place::Disk(_) => self.selected_archives().into_iter().next(),
        }
    }

    // ------------------------------------------------------------ option defaults

    pub fn default_create_options(&self) -> CreateOptions {
        let p = self.prefs.get();
        CreateOptions {
            format: p.compression.format,
            level: p.compression.level,
            threads: p.system.threads.max(1),
            store_patterns: archivecraft_engine::model::util::parse_patterns(&p.compression.store_patterns),
            path_mode: p.saved.path_mode,
            ..CreateOptions::default()
        }
    }

    pub fn default_extract_options(&self, dest: PathBuf) -> ExtractOptions {
        let p = self.prefs.get();
        let mut o = ExtractOptions::to(dest);
        o.overwrite = p.security.default_overwrite;
        if p.security.block_types {
            o.blocked = archivecraft_engine::model::util::parse_patterns(&p.security.blocked_patterns);
        }
        o
    }

    fn password_for(&self, archive: &Path) -> Option<String> {
        self.open_archive_view().filter(|v| v.path == archive).and_then(|v| v.password.clone()).or_else(|| self.session_password.clone())
    }

    // ------------------------------------------------------------ jobs

    fn busy(&mut self) -> bool {
        if self.job.is_some() {
            self.note(tl!("Please wait: another task is still running."));
            return true;
        }
        false
    }

    fn spawn(&mut self, kind: JobKind, title: String, work: impl FnOnce(&Ctl) -> Outcome + Send + 'static) {
        self.job = Some(Job::spawn(kind, title, self.ctx.clone(), work));
    }

    pub fn start_create(&mut self, sources: Vec<PathBuf>, archive: PathBuf, opts: CreateOptions, per_file: bool) {
        if self.busy() {
            return;
        }
        let title = tf!("Packing {name}", name = file_name(&archive));
        self.spawn(JobKind::Create, title, move |ctl| {
            let res = if per_file {
                let dir = archive.parent().map(Path::to_path_buf).unwrap_or_default();
                let mut out = Vec::new();
                for s in &sources {
                    let name = ops::archive_path_for(std::slice::from_ref(s), opts.format);
                    let a = dir.join(name.file_name().unwrap_or_default());
                    match ops::create(std::slice::from_ref(s), &a, &opts, ctl) {
                        Ok(r) => out.push(r),
                        Err(e) => return Outcome::Created(Err(e), archive),
                    }
                }
                Ok(out)
            } else {
                ops::create(&sources, &archive, &opts, ctl).map(|r| vec![r])
            };
            Outcome::Created(res, archive)
        });
    }

    pub fn start_extract(&mut self, archives: Vec<PathBuf>, opts: ExtractOptions, own_folder: bool, delete_after: dialogs::extract::DeleteAfter) {
        if archives.is_empty() || self.busy() {
            return;
        }
        let name = if archives.len() == 1 { file_name(&archives[0]) } else { tf!("{n} archives", n = archives.len()) };
        let password = self.password_for(&archives[0]);
        let dest = opts.dest.clone();
        self.last_extract = Some(dest.clone());
        self.spawn(JobKind::Extract, tf!("Unpacking {name}", name = name), move |ctl| {
            let mut out = Vec::new();
            for a in &archives {
                let mut o = opts.clone();
                if own_folder {
                    o.dest = opts.dest.join(ops::folder_for(a).file_name().unwrap_or_default());
                }
                let r = ops::extract(a, &o, password.as_deref(), ctl);
                let stop = matches!(r, Err(Error::Cancelled));
                out.push((a.clone(), r));
                if stop {
                    break;
                }
            }
            Outcome::Extracted(out, dest, delete_after)
        });
    }

    pub fn start_test(&mut self, archives: Vec<PathBuf>) {
        if archives.is_empty() || self.busy() {
            return;
        }
        let title = tf!("Checking {name}", name = file_name(&archives[0]));
        let password = self.password_for(&archives[0]);
        self.spawn(JobKind::Test, title, move |ctl| {
            let mut out = Vec::new();
            for a in &archives {
                let r = ops::test(a, password.as_deref(), ctl);
                out.push((a.clone(), r));
            }
            Outcome::Tested(out)
        });
    }

    pub fn start_delete_entries(&mut self, items: Vec<String>) {
        let Some(archive) = self.open_archive_view().map(|v| v.path.clone()) else { return };
        if self.busy() {
            return;
        }
        let password = self.password_for(&archive);
        self.spawn(JobKind::Delete, tl!("Removing items"), move |ctl| Outcome::Deleted(ops::delete(&archive, &items, password.as_deref(), ctl)));
    }

    pub fn start_comment(&mut self, archive: PathBuf, text: String) {
        if self.busy() {
            return;
        }
        let password = self.password_for(&archive);
        self.spawn(JobKind::Comment, tl!("Saving the note"), move |ctl| Outcome::Commented(ops::set_comment(&archive, &text, password.as_deref(), ctl)));
    }

    pub fn start_convert(&mut self, sources: Vec<PathBuf>, out_dir: PathBuf, opts: CreateOptions, recycle_originals: bool) {
        if self.busy() {
            return;
        }
        let password = self.session_password.clone();
        self.spawn(JobKind::Convert, tl!("Converting archives"), move |ctl| {
            let mut out = Vec::new();
            for s in &sources {
                let stem = file_name(&ops::folder_for(s));
                let dst = out_dir.join(format!("{stem}.{}", opts.format.extension()));
                let r = ops::convert(s, &dst, &opts, password.as_deref(), ctl);
                if r.is_ok() && recycle_originals {
                    let _ = archivecraft_engine::send_to_recycle_bin(&formats::volumes::volume_parts(s));
                }
                out.push((dst, r));
            }
            Outcome::Converted(out)
        });
    }

    pub fn start_repair(&mut self, archive: PathBuf, out: PathBuf, treat_as: Option<archivecraft_engine::Format>) {
        if self.busy() {
            return;
        }
        let password = self.session_password.clone();
        let title = tf!("Repairing {name}", name = file_name(&archive));
        self.spawn(JobKind::Repair, title, move |ctl| Outcome::Repaired(repair::repair(&archive, &out, treat_as, password.as_deref(), ctl)));
    }

    pub fn start_recovery(&mut self, archive: PathBuf, percent: u32) {
        if self.busy() {
            return;
        }
        self.spawn(JobKind::Recovery, tl!("Writing recovery data"), move |ctl| {
            let parts = formats::volumes::volume_parts(&archive);
            Outcome::Recovery(archivecraft_engine::par2::create(&parts, percent, ctl))
        });
    }

    pub fn start_find(&mut self, root: PathBuf, q: find::FindQuery) {
        if self.busy() {
            return;
        }
        self.spawn(JobKind::Find, tl!("Searching"), move |ctl| {
            let mut hits = Vec::new();
            let r = find::find(&root, &q, ctl, &mut |h| hits.push(h));
            Outcome::Found(r, hits)
        });
    }

    pub fn start_bench(&mut self, format: archivecraft_engine::Format, level: u32, size_mb: usize) {
        if self.busy() {
            return;
        }
        let threads = self.prefs.get().system.threads.max(1);
        self.spawn(JobKind::Bench, tl!("Measuring speed"), move |ctl| Outcome::Benched(bench::run(format, level, size_mb, threads, ctl)));
    }

    pub fn poll_job(&mut self) {
        let Some(job) = &mut self.job else { return };
        job.poll_question();
        if job.is_done() && job.asking.is_none() {
            let Some(mut job) = self.job.take() else { return };
            let kind = job.kind;
            let ctl = job.ctl.clone();
            match job.take() {
                Some(out) => self.finish_job(out, &ctl, kind),
                None => {
                    ctl.error("", tl!("The task stopped unexpectedly."));
                    self.show_diags(&ctl);
                }
            }
        }
    }

    fn show_diags(&mut self, ctl: &Ctl) {
        let w = dialogs::progress::DiagWindow::from_ctl(ctl, tl!("Problems found"));
        self.ui_state.last_diags = Some(w.clone());
        self.ui_state.diags = Some(w);
    }

    fn finish_job(&mut self, outcome: Outcome, ctl: &Ctl, kind: JobKind) {
        let mut ok_message: Option<String> = None;
        match outcome {
            Outcome::Created(r, archive) => match r {
                Ok(reports) => {
                    let errors: usize = reports.iter().map(|r| r.test_errors).sum();
                    let added: usize = reports.iter().map(|r| r.added).sum();
                    self.note(tf!("Packed {n} file(s) into {name}", n = added, name = file_name(&archive)));
                    if errors > 0 {
                        ctl.error(&archive.display().to_string(), tf!("the check after packing found {n} problem(s)", n = errors));
                    }
                    self.prefs.update(|p| p.remember(archive.clone()));
                    self.reload_after_change(&archive);
                }
                Err(Error::Cancelled) => self.note(tl!("Packing stopped.")),
                Err(e) => ctl.error(&archive.display().to_string(), e.to_string()),
            },
            Outcome::Extracted(list, dest, delete_after) => {
                let mut files = 0;
                for (a, r) in &list {
                    match r {
                        Ok(rep) => {
                            files += rep.files;
                            if rep.password_used.is_some() && self.session_password.is_none() {
                                self.session_password = rep.password_used.clone();
                            }
                            if rep.errors == 0 && rep.refused == 0 {
                                let parts = formats::volumes::volume_parts(a);
                                match delete_after {
                                    dialogs::extract::DeleteAfter::Always => {
                                        let _ = archivecraft_engine::send_to_recycle_bin(&parts);
                                    }
                                    dialogs::extract::DeleteAfter::Ask => self.ui_state.confirm = Some(dialogs::tools::Confirm::recycle(parts)),
                                    dialogs::extract::DeleteAfter::Never => {}
                                }
                            }
                        }
                        Err(Error::Cancelled) => self.note(tl!("Unpacking stopped.")),
                        Err(e) => ctl.error(&a.display().to_string(), e.to_string()),
                    }
                }
                self.note(tf!("Unpacked {n} file(s) to {dest}", n = files, dest = dest.display()));
                if let Place::Disk(d) = &self.place
                    && dest.starts_with(d)
                {
                    let keep = self.selected_disk();
                    self.refresh();
                    for k in keep {
                        self.select_target(&RowRef::Disk(k));
                    }
                }
            }
            Outcome::Tested(list) => {
                let mut checked = 0;
                for (a, r) in &list {
                    match r {
                        Ok(t) => checked += t.files,
                        Err(Error::Cancelled) => self.note(tl!("Checking stopped.")),
                        Err(e) => ctl.error(&a.display().to_string(), e.to_string()),
                    }
                }
                if ctl.error_count() == 0 && !ctl.is_cancelled() {
                    ok_message = Some(tf!("Everything checked out: {n} file(s), no problems found.", n = checked));
                }
            }
            Outcome::Deleted(r) => match r {
                Ok(n) => {
                    self.note(tf!("Removed {n} item(s).", n = n));
                    self.reload_open();
                }
                Err(e) => ctl.error("", e.to_string()),
            },
            Outcome::Commented(r) => match r {
                Ok(()) => {
                    self.note(tl!("Note saved."));
                    self.reload_open();
                }
                Err(e) => ctl.error("", e.to_string()),
            },
            Outcome::Converted(list) => {
                for (d, r) in &list {
                    if let Err(e) = r {
                        ctl.error(&d.display().to_string(), e.to_string());
                    }
                }
                self.note(tf!("Converted {n} archive(s).", n = list.iter().filter(|(_, r)| r.is_ok()).count()));
                self.refresh_if_disk();
            }
            Outcome::Repaired(r) => match r {
                Ok(rep) => {
                    ok_message = Some(match rep.method {
                        repair::RepairMethod::NothingToDo => tl!("The archive matches its recovery data. Nothing needed fixing."),
                        repair::RepairMethod::RecoveryData => tl!("The archive was rebuilt exactly from its recovery data."),
                        repair::RepairMethod::Salvage { saved, lost } => tf!("Saved {saved} item(s) into a new archive; {lost} could not be read.", saved = saved, lost = lost),
                    });
                    self.refresh_if_disk();
                }
                Err(e) => ctl.error("", e.to_string()),
            },
            Outcome::Recovery(r) => match r {
                Ok(files) => {
                    ok_message = Some(tf!("Recovery data written: {n} file(s) next to the archive.", n = files.len()));
                    self.refresh_if_disk();
                }
                Err(e) => ctl.error("", e.to_string()),
            },
            Outcome::Found(r, hits) => {
                if let Err(e) = r
                    && !matches!(e, Error::Cancelled)
                {
                    ctl.error("", e.to_string());
                }
                self.ui_state.found = Some(dialogs::find::FoundWindow { hits, selected: None });
            }
            Outcome::Benched(r) => match r {
                Ok(b) => {
                    if let Some(w) = &mut self.ui_state.bench {
                        w.results.push(b);
                    }
                }
                Err(e) => ctl.error("", e.to_string()),
            },
        }
        let has_problems = ctl.diags.lock().map(|d| d.iter().any(|x| x.severity >= Severity::Warning)).unwrap_or(false);
        if has_problems {
            self.show_diags(ctl);
        } else if let Some(m) = ok_message {
            self.ui_state.message = Some(dialogs::progress::MessageBox::info(m));
        }
        if let Some(st) = &mut self.self_test {
            st.job_finished(kind, ctl.error_count());
        }
    }

    fn refresh_if_disk(&mut self) {
        if matches!(self.place, Place::Disk(_)) {
            self.refresh();
        }
    }

    pub fn reload_open(&mut self) {
        if let Some(p) = self.open_archive_view().map(|v| v.path.clone()) {
            self.reload_after_change(&p);
        } else {
            self.refresh();
        }
    }

    /// After an archive changed on disk: reload it if it is open, or refresh the folder.
    pub fn reload_after_change(&mut self, archive: &Path) {
        let mut folder = None;
        if let Place::Archive(v) = &mut self.place
            && v.path == archive
        {
            if let Ok(l) = formats::list(&v.path, v.password.as_deref()) {
                v.listing = l;
            }
            folder = Some(v.folder.clone());
        }
        match folder {
            Some(f) => self.enter_folder(f),
            None => {
                if matches!(self.place, Place::Disk(_)) {
                    self.refresh();
                    self.select_target(&RowRef::Disk(archive.to_path_buf()));
                }
            }
        }
    }

    // ------------------------------------------------------------ toolbar actions

    pub fn open_add_dialog(&mut self, files: Vec<PathBuf>) {
        let d = dialogs::add::AddDialog::new(self, files);
        self.ui_state.add = Some(d);
    }

    pub fn act_add(&mut self) {
        let d = match &self.place {
            Place::Disk(_) => {
                let files = self.selected_disk();
                dialogs::add::AddDialog::new(self, files)
            }
            Place::Archive(_) => dialogs::add::AddDialog::to_open_archive(self, Vec::new()),
        };
        self.ui_state.add = Some(d);
    }

    pub fn act_extract(&mut self) {
        let d = match &self.place {
            Place::Disk(_) => {
                let archives = self.selected_archives();
                if archives.is_empty() {
                    self.note(tl!("Choose one or more archives in the list first."));
                    return;
                }
                dialogs::extract::ExtractDialog::new(self, archives, Vec::new(), String::new())
            }
            Place::Archive(v) => {
                let (p, folder) = (v.path.clone(), v.folder.clone());
                let items = self.selected_entries();
                dialogs::extract::ExtractDialog::new(self, vec![p], items, folder)
            }
        };
        self.ui_state.extract = Some(d);
    }

    pub fn act_extract_here(&mut self) {
        let archives: Vec<PathBuf> = match &self.place {
            Place::Disk(_) => self.selected_archives(),
            Place::Archive(v) => vec![v.path.clone()],
        };
        if archives.is_empty() {
            self.note(tl!("Choose one or more archives in the list first."));
            return;
        }
        let dest = archives[0].parent().map(Path::to_path_buf).unwrap_or_default();
        let mut o = self.default_extract_options(dest);
        if let Place::Archive(v) = &self.place {
            o.only = self.selected_entries();
            o.base = v.folder.clone();
        }
        self.start_extract(archives, o, false, dialogs::extract::DeleteAfter::Never);
    }

    pub fn act_test(&mut self) {
        let archives: Vec<PathBuf> = match &self.place {
            Place::Disk(_) => self.selected_archives(),
            Place::Archive(v) => vec![v.path.clone()],
        };
        if archives.is_empty() {
            self.note(tl!("Choose one or more archives in the list first."));
            return;
        }
        self.start_test(archives);
    }

    pub fn act_view(&mut self) {
        match self.selected.iter().next().copied() {
            Some(i) => self.activate(i),
            None => self.note(tl!("Choose a file to look at.")),
        }
    }

    pub fn act_delete(&mut self) {
        match &self.place {
            Place::Disk(_) => {
                let paths = self.selected_disk();
                if !paths.is_empty() {
                    self.ui_state.confirm = Some(dialogs::tools::Confirm::recycle(paths));
                }
            }
            Place::Archive(_) => {
                let items = self.selected_entries();
                if !items.is_empty() {
                    self.ui_state.confirm = Some(dialogs::tools::Confirm::remove_entries(items));
                }
            }
        }
    }

    pub fn act_info(&mut self) {
        match self.target_archive() {
            Some(p) => self.ui_state.info = Some(dialogs::info::InfoDialog::for_archive(self, &p, 0)),
            None => self.note(tl!("Open an archive, or choose one in the list, to see its details.")),
        }
    }

    pub fn act_note(&mut self) {
        if let Some(p) = self.open_archive_view().map(|v| v.path.clone()) {
            self.ui_state.info = Some(dialogs::info::InfoDialog::for_archive(self, &p, 1));
        }
    }

    pub fn act_find(&mut self) {
        self.ui_state.find = Some(dialogs::find::FindDialog::new(self.disk_dir()));
    }

    pub fn act_repair(&mut self) {
        let target = match &self.place {
            Place::Archive(v) => Some(v.path.clone()),
            Place::Disk(_) => self.selected_rows().into_iter().find(|r| !r.is_dir).and_then(disk_target),
        };
        match target {
            Some(p) => self.ui_state.repair = Some(dialogs::tools::RepairDialog::new(p)),
            None => self.note(tl!("Choose the damaged archive in the list first.")),
        }
    }

    pub fn view_disk_file(&mut self, p: PathBuf) {
        let read = std::fs::File::open(&p).and_then(|f| {
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut std::io::Read::take(f, ops::PREVIEW_LIMIT), &mut buf)?;
            Ok(buf)
        });
        match read {
            Ok(data) => self.show_viewer(file_name(&p), data, Some(p)),
            Err(e) => self.note(tf!("Cannot read the file: {e}", e = e)),
        }
    }

    pub fn view_entry(&mut self, path: String) {
        let Some(archive) = self.open_archive_view().map(|v| v.path.clone()) else { return };
        let pw = self.password_for(&archive);
        match ops::read_entry(&archive, &path, pw.as_deref()) {
            Ok(data) => self.show_viewer(path.rsplit('/').next().unwrap_or(&path).to_string(), data, None),
            Err(e) if e.is_password() => {
                self.ui_state.unlock = Some(dialogs::progress::PasswordPrompt::new(archive.display().to_string(), path.clone(), pw.is_some(), None).then_view(path));
            }
            Err(e) => self.problem(&archive, e),
        }
    }

    fn show_viewer(&mut self, name: String, data: Vec<u8>, disk: Option<PathBuf>) {
        use crate::settings::ViewerKind;
        let viewer = self.prefs.get().viewer.clone();
        if matches!(viewer.kind, ViewerKind::SystemApp | ViewerKind::External) {
            let file = match disk {
                Some(p) => p,
                None => {
                    let dir = std::env::temp_dir().join("archivecraft-view");
                    let _ = std::fs::create_dir_all(&dir);
                    let p = dir.join(&name);
                    let _ = std::fs::write(&p, &data);
                    p
                }
            };
            let program = if viewer.kind == ViewerKind::External { viewer.external_program.clone() } else { String::new() };
            if let Err(e) = dialogs::viewer::open_outside(&file, &program) {
                self.note(tf!("Could not open it outside: {e}", e = e));
            }
            return;
        }
        self.ui_state.viewer = Some(dialogs::viewer::Viewer::new(&self.ctx, name, data, viewer, viewer_kind_ask(&self.prefs.get().viewer)));
    }

    pub fn go_bookmark(&mut self, b: &crate::settings::Bookmark) {
        if b.path.is_dir() {
            self.go_disk(b.path.clone());
        } else {
            self.open_archive(b.path.clone());
            if matches!(self.place, Place::Archive(_)) && !b.inner.is_empty() {
                self.enter_folder(b.inner.clone());
            }
        }
    }

    // ------------------------------------------------------------ file pickers

    pub fn pick(&mut self, pick: Pick, kind: craft_dialogs::Kind, title: String, dir: Option<PathBuf>, name: Option<String>, archives_only: bool) {
        let mut d = craft_dialogs::FileDialog::new(kind);
        d.title = Some(title);
        d.directory = dir.or_else(|| Some(self.disk_dir()));
        d.file_name = name;
        if archives_only {
            let mut exts: Vec<String> = ["zip", "7z", "tar", "gz", "tgz", "xz", "txz", "zst", "tzst", "001"].iter().map(|s| s.to_string()).collect();
            if cfg!(feature = "rar") {
                exts.push("rar".into());
            }
            d.filters = vec![craft_dialogs::Filter { name: tl!("Archives"), extensions: exts }];
        }
        self.files.ask(d, pick);
    }

    pub fn pick_open_archive(&mut self) {
        self.pick(Pick::OpenArchive, craft_dialogs::Kind::Open { multiple: false }, tl!("Open an archive"), None, None, true);
    }

    pub fn deliver(&mut self, pick: Pick, answer: craft_dialogs::Answer) {
        let paths: Vec<PathBuf> = match answer {
            craft_dialogs::Answer::Paths(p) => p,
            craft_dialogs::Answer::Saved { path, .. } => vec![path],
            craft_dialogs::Answer::Unavailable(why) => {
                self.note(tf!("The file chooser is not available here: {why}", why = why));
                return;
            }
            _ => return,
        };
        let Some(first) = paths.first().cloned() else { return };
        let u = &mut self.ui_state;
        match pick {
            Pick::OpenArchive => self.open_archive(first),
            Pick::AddArchiveName => {
                if let Some(d) = &mut u.add {
                    d.set_archive(first);
                }
            }
            Pick::AddFiles | Pick::AddFolder => {
                if let Some(d) = &mut u.add {
                    d.add_sources(paths);
                }
            }
            Pick::ExtractDest => {
                if let Some(d) = &mut u.extract {
                    d.dest = first.display().to_string();
                }
            }
            Pick::WizardArchive | Pick::WizardNewArchive | Pick::WizardFiles => {
                if let Some(w) = &mut u.wizard {
                    w.picked(pick, paths);
                }
            }
            Pick::FindFolder => {
                if let Some(f) = &mut u.find {
                    f.root = first.display().to_string();
                }
            }
            Pick::RepairOut => {
                if let Some(r) = &mut u.repair {
                    r.out = first.display().to_string();
                }
            }
            Pick::ConvertOut => {
                if let Some(c) = &mut u.convert {
                    c.out = first.display().to_string();
                }
            }
            Pick::CommentFile => {
                if let Some(d) = &mut u.add {
                    d.comment = std::fs::read_to_string(&first).unwrap_or_default();
                }
            }
            Pick::InfoCommentFile => {
                if let Some(d) = &mut u.info {
                    d.comment = std::fs::read_to_string(&first).unwrap_or_default();
                }
            }
            Pick::PrefsFolder(slot) => {
                if let Some(p) = &mut u.prefs {
                    p.set_folder(slot, first);
                }
            }
            Pick::ExternalViewer => {
                if let Some(p) = &mut u.prefs {
                    p.draft.viewer.external_program = first.display().to_string();
                }
            }
        }
    }
}

fn viewer_kind_ask(v: &crate::settings::Viewer) -> bool {
    v.kind == crate::settings::ViewerKind::Ask
}
