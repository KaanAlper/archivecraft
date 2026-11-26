//! `--self-test <file.json>`: open a sample archive the way a person would, double-click into a
//! folder with real (injected) mouse input, unpack and check it through the same jobs the toolbar
//! starts, confirm the window's information is drawn, then write the results and close.

use crate::jobs::JobKind;
use crate::state::{App, Place};
use archivecraft_engine::model::{CreateOptions, Ctl, Overwrite};
use archivecraft_engine::ops;
use std::path::PathBuf;

#[derive(serde::Serialize)]
struct Step {
    step: String,
    passed: bool,
    detail: String,
}

pub struct SelfTest {
    out: PathBuf,
    phase: u32,
    wait: u32,
    dir: Option<tempfile::TempDir>,
    archive: PathBuf,
    steps: Vec<Step>,
    done: Vec<(JobKind, usize)>,
    click_at: Option<egui::Pos2>,
}

impl SelfTest {
    pub fn new(out: PathBuf) -> Self {
        SelfTest { out, phase: 0, wait: 0, dir: None, archive: PathBuf::new(), steps: Vec::new(), done: Vec::new(), click_at: None }
    }

    pub fn job_finished(&mut self, kind: JobKind, errors: usize) {
        self.done.push((kind, errors));
    }

    fn push(&mut self, step: &str, passed: bool, detail: String) {
        self.steps.push(Step { step: step.into(), passed, detail });
    }

    fn click(app: &mut App, pos: egui::Pos2) {
        let m = egui::Modifiers::NONE;
        app.inject.push(egui::Event::PointerMoved(pos));
        app.inject.push(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: m });
        app.inject.push(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: false, modifiers: m });
    }

    fn finish(&mut self, ctx: &egui::Context) {
        let body = serde_json::json!({ "version": 1, "steps": self.steps });
        if let Some(p) = self.out.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        let _ = std::fs::write(&self.out, serde_json::to_string_pretty(&body).unwrap_or_default());
        self.phase = 99;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    pub fn step(&mut self, app: &mut App, ctx: &egui::Context) {
        if self.phase == 99 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if app.frame_no > 3000 {
            self.push("timeout", false, format!("stopped in phase {} after {} frames", self.phase, app.frame_no));
            self.finish(ctx);
            return;
        }
        self.wait += 1;
        match self.phase {
            // Make a sample archive and open it, as Archive ▸ Open would.
            0 if app.frame_no >= 5 => {
                let dir = match tempfile::tempdir() {
                    Ok(d) => d,
                    Err(e) => {
                        self.push("move", false, format!("no temporary folder: {e}"));
                        self.finish(ctx);
                        return;
                    }
                };
                let src = dir.path().join("sample");
                let _ = std::fs::create_dir_all(src.join("docs/deep"));
                let _ = std::fs::write(src.join("docs/notes.txt"), "Notes for the self-test.\n".repeat(40));
                let _ = std::fs::write(src.join("docs/deep/data.bin"), (0..20_000u32).map(|i| (i * 7 % 251) as u8).collect::<Vec<_>>());
                let _ = std::fs::write(src.join("readme.txt"), "Read me.\n");
                self.archive = dir.path().join("selftest.zip");
                let r = ops::create(&[src.join("docs"), src.join("readme.txt")], &self.archive, &CreateOptions::default(), &Ctl::default());
                self.dir = Some(dir);
                if let Err(e) = r {
                    self.push("move", false, format!("could not make the sample archive: {e}"));
                    self.finish(ctx);
                    return;
                }
                app.open_archive(self.archive.clone());
                self.phase = 1;
                self.wait = 0;
            }
            // Double-click the "docs" row with real mouse events.
            1 if self.wait > 3 => match app.row_rects.get("docs").copied() {
                Some(r) => {
                    let pos = r.left_center() + egui::vec2(60.0, 0.0);
                    self.click_at = Some(pos);
                    Self::click(app, pos);
                    self.phase = 2;
                    self.wait = 0;
                }
                None if self.wait > 60 => {
                    self.push("move", false, "the \"docs\" row was never drawn".into());
                    self.phase = 4;
                }
                None => {}
            },
            2 if self.wait >= 2 => {
                if let Some(p) = self.click_at {
                    Self::click(app, p);
                }
                self.phase = 3;
                self.wait = 0;
            }
            3 if self.wait >= 4 => {
                let folder = match &app.place {
                    Place::Archive(v) => v.folder.clone(),
                    Place::Disk(_) => "(left the archive)".into(),
                };
                let names: Vec<String> = app.rows.iter().map(|r| r.name.clone()).collect();
                let ok = folder == "docs" && names.iter().any(|n| n == "notes.txt");
                self.push("move", ok, format!("double-click on \"docs\" opened folder \"{folder}\" showing {names:?}"));
                // Right-click a file row, as a person would.
                if let Some(r) = app.row_rects.get("notes.txt").copied() {
                    let pos = r.left_center() + egui::vec2(60.0, 0.0);
                    let m = egui::Modifiers::NONE;
                    app.inject.push(egui::Event::PointerMoved(pos));
                    app.inject.push(egui::Event::PointerButton { pos, button: egui::PointerButton::Secondary, pressed: true, modifiers: m });
                    app.inject.push(egui::Event::PointerButton { pos, button: egui::PointerButton::Secondary, pressed: false, modifiers: m });
                }
                self.phase = 30;
                self.wait = 0;
            }
            30 if self.wait >= 12 => {
                let open = app.hud.context_menu;
                let chosen = app.selected_rows().iter().map(|r| r.name.clone()).collect::<Vec<_>>();
                self.push("right-click", open, format!("right-click on \"notes.txt\": menu shown {open}, chosen {chosen:?}"));
                // Keep a picture of the open menu next to the results.
                let shot = self.out.with_file_name(format!("{}-right-click.png", self.out.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()));
                ctx.request_screenshot(move |img| {
                    let (w, h) = (img.size[0] as u32, img.size[1] as u32);
                    let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
                    if let Some(buf) = image::RgbaImage::from_raw(w, h, bytes) {
                        let _ = buf.save(&shot);
                    }
                });
                // Close it again with Escape.
                app.inject.push(egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE });
                app.inject.push(egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: false, repeat: false, modifiers: egui::Modifiers::NONE });
                self.phase = 4;
                self.wait = 0;
            }
            // Unpack everything through the same job the Extract window starts.
            4 => {
                let dest = self.dir.as_ref().map(|d| d.path().join("unpacked")).unwrap_or_default();
                let mut o = app.default_extract_options(dest);
                o.overwrite = Overwrite::Always;
                app.start_extract(vec![self.archive.clone()], o, false, crate::dialogs::extract::DeleteAfter::Never);
                self.phase = 5;
                self.wait = 0;
            }
            5 if self.done.iter().any(|(k, _)| *k == JobKind::Extract) => {
                app.ui_state.message = None;
                app.start_test(vec![self.archive.clone()]);
                self.phase = 6;
            }
            6 if self.done.iter().any(|(k, _)| *k == JobKind::Test) => {
                app.ui_state.message = None;
                let base = self.dir.as_ref().map(|d| d.path().to_path_buf()).unwrap_or_default();
                let same = |a: &str, b: &str| std::fs::read(base.join(a)).ok().is_some() && std::fs::read(base.join(a)).ok() == std::fs::read(base.join(b)).ok();
                let a = same("sample/docs/notes.txt", "unpacked/docs/notes.txt");
                let b = same("sample/docs/deep/data.bin", "unpacked/docs/deep/data.bin");
                let c = same("sample/readme.txt", "unpacked/readme.txt");
                let test_errors: usize = self.done.iter().filter(|(k, _)| *k == JobKind::Test).map(|(_, e)| *e).sum();
                let extract_errors: usize = self.done.iter().filter(|(k, _)| *k == JobKind::Extract).map(|(_, e)| *e).sum();
                let ok = a && b && c && test_errors == 0 && extract_errors == 0;
                self.push("act", ok, format!("unpacked 3 files: identical {}/3; unpack problems {extract_errors}; check found {test_errors} problem(s)", [a, b, c].iter().filter(|x| **x).count()));
                self.phase = 7;
                self.wait = 0;
            }
            7 if self.wait > 3 => {
                let h = &app.hud;
                let want = ["Name", "Size", "Packed", "Type", "Modified"];
                let cols = want.iter().all(|w| h.headers.iter().any(|x| x == w));
                let ok = h.menu_bar && h.toolbar_buttons >= 9 && h.path_bar && cols && !h.status.is_empty();
                self.push(
                    "hud",
                    ok,
                    format!("menu bar {}, {} toolbar buttons, path bar {}, columns {:?}, status \"{}\"", h.menu_bar, h.toolbar_buttons, h.path_bar, h.headers, h.status),
                );
                self.finish(ctx);
            }
            _ => {}
        }
    }
}
