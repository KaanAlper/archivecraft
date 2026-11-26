//! Windows and macOS: drag items out of an open archive into the file manager. The items are
//! unpacked to a temporary folder first, then handed to the system's drag. (winit has no way to
//! start such a drag on Linux; there the in-window drop bar is used instead.)

use crate::state::App;
use crate::tl;
use archivecraft_engine::model::{Ctl, ExtractOptions, Overwrite};
use archivecraft_engine::ops;

fn preview_png() -> Vec<u8> {
    let img = image::RgbaImage::from_fn(32, 32, |x, y| {
        let band = (13..19).contains(&x);
        if !(4..28).contains(&x) || !(6..28).contains(&y) {
            image::Rgba([0, 0, 0, 0])
        } else if band {
            image::Rgba([0x2C, 0x8C, 0x99, 255])
        } else {
            image::Rgba([0xE2, 0xA2, 0x3B, 255])
        }
    });
    let mut out = std::io::Cursor::new(Vec::new());
    let _ = img.write_to(&mut out, image::ImageFormat::Png);
    out.into_inner()
}

pub fn start(app: &mut App, frame: &eframe::Frame, items: Vec<String>) {
    let Some(v) = app.open_archive_view() else { return };
    let (archive, folder, pw) = (v.path.clone(), v.folder.clone(), v.password.clone().or(app.session_password.clone()));
    let dir = std::env::temp_dir().join(format!("archivecraft-drag-{}-{}", std::process::id(), app.frame_no));
    let mut o = ExtractOptions::to(&dir);
    o.overwrite = Overwrite::Always;
    o.only = items.clone();
    o.base = folder.clone();
    if let Err(e) = ops::extract(&archive, &o, pw.as_deref(), &Ctl::default()) {
        app.note(e.to_string());
        return;
    }
    let base = folder.trim_matches('/');
    let paths: Vec<std::path::PathBuf> = items
        .iter()
        .map(|i| {
            let rel = if !base.is_empty() { i.strip_prefix(&format!("{base}/")).unwrap_or(i) } else { i.as_str() };
            dir.join(rel)
        })
        .filter(|p| p.exists())
        .collect();
    if paths.is_empty() {
        return;
    }
    if let Err(e) = drag::start_drag(frame, drag::DragItem::Files(paths), drag::Image::Raw(preview_png()), |_, _| {}, drag::Options::default()) {
        app.note(format!("{}: {e:?}", tl!("Dragging out did not start")));
    }
}
