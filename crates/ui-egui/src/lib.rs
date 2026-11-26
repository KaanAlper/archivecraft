//! ArchiveCraft's window, on egui and the Craft UI libraries (craft-ui runtime and theme,
//! craft-menu menu bar, craft-dialogs file pickers, craft-settings preferences, craft-i18n text).

pub mod i18n;

mod app;
mod chrome;
mod dialogs;
#[cfg(any(target_os = "windows", target_os = "macos"))]
mod dragout;
mod icons;
mod jobs;
mod rows;
mod selftest;
mod settings;
mod state;

pub use app::{Launch, LaunchAction};
use archivecraft_engine::integration::{ARG_ADD_DIALOG, ARG_ADD_ZIP, ARG_EXTRACT_FOLDER, ARG_EXTRACT_HERE};
use std::path::PathBuf;
use std::sync::Arc;

/// Read the command line: `--screenshot <png>`, `--self-test <json>`, the right-click switches,
/// and paths to open.
pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Launch, String> {
    let mut l = Launch::default();
    let mut it = args.into_iter().peekable();
    let mut action: Option<&str> = None;
    let mut paths = Vec::new();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--screenshot" => l.screenshot = Some(PathBuf::from(it.next().ok_or("--screenshot needs a file name")?)),
            "--self-test" => l.self_test = Some(PathBuf::from(it.next().ok_or("--self-test needs a file name")?)),
            x if x == ARG_ADD_DIALOG => action = Some(ARG_ADD_DIALOG),
            x if x == ARG_ADD_ZIP => action = Some(ARG_ADD_ZIP),
            x if x == ARG_EXTRACT_HERE => action = Some(ARG_EXTRACT_HERE),
            x if x == ARG_EXTRACT_FOLDER => action = Some(ARG_EXTRACT_FOLDER),
            "--help" | "-h" => {
                return Err("archivecraft [--screenshot out.png] [--self-test out.json] [--add|--add-zip|--extract-here|--extract-folder] [paths…]".into());
            }
            _ => paths.push(PathBuf::from(a)),
        }
    }
    let paths: Vec<PathBuf> = paths.into_iter().map(|p| std::fs::canonicalize(&p).unwrap_or(p)).collect();
    l.action = match action {
        Some(a) if a == ARG_ADD_DIALOG => Some(LaunchAction::AddDialog(paths)),
        Some(a) if a == ARG_ADD_ZIP => Some(LaunchAction::AddZip(paths)),
        Some(a) if a == ARG_EXTRACT_HERE => Some(LaunchAction::ExtractHere(paths)),
        Some(_) => Some(LaunchAction::ExtractFolder(paths)),
        None => {
            l.open = paths;
            None
        }
    };
    Ok(l)
}

fn native_options(launch: &Launch) -> eframe::NativeOptions {
    let scripted = launch.screenshot.is_some() || launch.self_test.is_some();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("ArchiveCraft")
        .with_app_id("archivecraft")
        .with_inner_size([1000.0, 640.0])
        .with_min_inner_size([640.0, 400.0])
        .with_drag_and_drop(true);
    if scripted {
        // Checks must not take the focus from whatever the person is doing.
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }
    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions { viewport, renderer: eframe::Renderer::Glow, run_and_return: true, ..Default::default() };
    #[cfg(target_os = "macos")]
    if scripted {
        options.event_loop_builder = Some(Box::new(|b| {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            b.with_activate_ignoring_other_apps(false);
        }));
    }
    options
}

/// Open the window and run until it closes.
pub fn run(launch: Launch) -> eframe::Result {
    let options = native_options(&launch);
    eframe::run_native(
        "ArchiveCraft",
        options,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            let system = craft_ui::runtime::env::SystemReader::start(move || ctx.request_repaint());
            let shell = craft_host::target::ShellInfo::default().backend(craft_host::target::Backend::from_env());
            let spec = craft_ui::InstallSpec::new(shell, Arc::new(craft_window::system::SystemWindows)).system(system).app_id("archivecraft".to_string());
            let fonts = craft_ui::fonts::UiFonts::new(&craft_ui::fonts::I18nScripts);
            craft_ui::shell::embed::install(&cc.egui_ctx, spec, &fonts).map_err(|e| format!("the Craft runtime could not start: {e}"))?;
            Ok(Box::new(state::App::new(&cc.egui_ctx, launch)))
        }),
    )
}
