//! ArchiveCraft: the desktop window. Everything it shows lives in `archivecraft-ui-egui`.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() -> std::process::ExitCode {
    let launch = match archivecraft_ui_egui::parse_args(std::env::args().skip(1)) {
        Ok(l) => l,
        Err(msg) => {
            eprintln!("{msg}");
            return std::process::ExitCode::from(2);
        }
    };
    match archivecraft_ui_egui::run(launch) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("archivecraft: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
