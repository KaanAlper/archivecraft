//! Look inside a file without unpacking it: text (with encoding detection), pictures, or a
//! hexadecimal dump for anything else.

use super::{window, Flow};
use crate::settings::Viewer as ViewerPrefs;
use crate::state::App;
use crate::{tf, tl};
use std::path::Path;

enum Body {
    Text(String, &'static str),
    Picture(egui::TextureHandle, [usize; 2]),
    Bytes(String),
}

pub struct Viewer {
    name: String,
    size: usize,
    body: Body,
    wrap: bool,
    hex: bool,
    raw: Vec<u8>,
    ask: bool,
}

/// Decode text: UTF-8 (with or without BOM), UTF-16 with a BOM, else Latin-1 when it looks like
/// text at all. `None` for binary data.
pub fn decode_text(data: &[u8], detect: bool) -> Option<(String, &'static str)> {
    if let Some(rest) = data.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return Some((String::from_utf8_lossy(rest).into_owned(), "UTF-8"));
    }
    if detect && data.len() >= 2 && (data.starts_with(&[0xFF, 0xFE]) || data.starts_with(&[0xFE, 0xFF])) {
        let le = data[0] == 0xFF;
        let units: Vec<u16> = data[2..].as_chunks::<2>().0.iter().map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
        return Some((String::from_utf16_lossy(&units), if le { "UTF-16 LE" } else { "UTF-16 BE" }));
    }
    let sample = &data[..data.len().min(8192)];
    let controls = sample.iter().filter(|b| **b < 9 || (**b > 13 && **b < 32)).count();
    if sample.contains(&0) || controls * 20 > sample.len().max(1) {
        return None;
    }
    match std::str::from_utf8(data) {
        Ok(s) => Some((s.to_string(), "UTF-8")),
        Err(_) if detect => Some((data.iter().map(|b| *b as char).collect(), "Latin-1")),
        Err(_) => Some((String::from_utf8_lossy(data).into_owned(), "UTF-8")),
    }
}

pub fn hex_dump(data: &[u8], limit: usize) -> String {
    let mut out = String::new();
    for (i, chunk) in data[..data.len().min(limit)].chunks(16).enumerate() {
        out.push_str(&format!("{:08X}  ", i * 16));
        for j in 0..16 {
            match chunk.get(j) {
                Some(b) => out.push_str(&format!("{b:02X} ")),
                None => out.push_str("   "),
            }
        }
        out.push(' ');
        out.extend(chunk.iter().map(|b| if b.is_ascii_graphic() || *b == b' ' { *b as char } else { '.' }));
        out.push('\n');
    }
    out
}

impl Viewer {
    pub fn new(ctx: &egui::Context, name: String, data: Vec<u8>, prefs: ViewerPrefs, ask: bool) -> Viewer {
        let body = if let Ok(img) = image::load_from_memory(&data) {
            let rgba = img.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let tex = ctx.load_texture(format!("ac-view-{name}"), egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()), egui::TextureOptions::LINEAR);
            Body::Picture(tex, size)
        } else if let Some((t, enc)) = decode_text(&data, prefs.detect_encoding) {
            Body::Text(t, enc)
        } else {
            Body::Bytes(hex_dump(&data, 64 * 1024))
        };
        Viewer { name, size: data.len(), body, wrap: prefs.word_wrap, hex: false, raw: data, ask }
    }

    pub fn show(&mut self, _app: &mut App, ctx: &egui::Context) -> Flow {
        let mut flow = Flow::Keep;
        let mut open = true;
        window(tf!("Looking at {n}", n = self.name), "ac-viewer").open(&mut open).default_size([720.0, 520.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(tf!("{n} bytes", n = archivecraft_engine::model::util::group_digits(self.size as u64)));
                if let Body::Text(_, enc) = &self.body {
                    ui.separator();
                    ui.label(*enc);
                    ui.checkbox(&mut self.wrap, tl!("Wrap long lines"));
                }
                ui.checkbox(&mut self.hex, tl!("Show as bytes"));
                if self.ask {
                    ui.label(egui::RichText::new(tl!("(The viewer setting is \"ask\": this is the built-in one.)")).weak());
                }
            });
            ui.separator();
            egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                if self.hex {
                    let mut s = hex_dump(&self.raw, 64 * 1024);
                    ui.add(egui::TextEdit::multiline(&mut s).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY).interactive(true));
                    return;
                }
                match &mut self.body {
                    Body::Text(t, _) => {
                        let mut te = egui::TextEdit::multiline(t).font(egui::TextStyle::Monospace).desired_width(if self.wrap { ui.available_width() } else { f32::INFINITY });
                        if !self.wrap {
                            te = te.desired_width(10_000.0);
                        }
                        ui.add(te);
                    }
                    Body::Picture(tex, size) => {
                        let avail = ui.available_size();
                        let scale = (avail.x / size[0] as f32).min(avail.y / size[1] as f32).min(1.0);
                        ui.image((tex.id(), egui::vec2(size[0] as f32 * scale, size[1] as f32 * scale)));
                    }
                    Body::Bytes(s) => {
                        ui.add(egui::TextEdit::multiline(s).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY));
                    }
                }
            });
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                flow = Flow::Close;
            }
        });
        if !open { Flow::Close } else { flow }
    }
}

/// Open a file or folder with another program: the given one, or the system's choice.
pub fn open_outside(path: &Path, program: &str) -> std::io::Result<()> {
    use std::process::Command;
    if !program.trim().is_empty() {
        Command::new(program.trim()).arg(path).spawn()?;
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    Command::new("explorer").arg(path).spawn()?;
    #[cfg(target_os = "macos")]
    Command::new("open").arg(path).spawn()?;
    #[cfg(all(unix, not(target_os = "macos")))]
    Command::new("xdg-open").arg(path).spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_detection() {
        assert_eq!(decode_text(b"hello\nworld", true).unwrap().1, "UTF-8");
        assert_eq!(decode_text(&[0xFF, 0xFE, b'h', 0, b'i', 0], true).unwrap().0, "hi");
        assert_eq!(decode_text(&[b'c', b'a', b'f', 0xE9], true).unwrap().0, "café");
        assert!(decode_text(&[0, 1, 2, 3, 4, 5], true).is_none());
        assert!(hex_dump(b"AB", 100).starts_with("00000000  41 42"));
    }
}
