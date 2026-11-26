//! ArchiveCraft's own icons, drawn with shapes at any size. Palette: warm amber parcels with a
//! teal band, set on the Craft theme's text colour for outlines.

use egui::epaint::{CubicBezierShape, PathShape, PathStroke};
use egui::{Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2, pos2, vec2};

pub const AMBER: Color32 = Color32::from_rgb(0xE2, 0xA2, 0x3B);
pub const AMBER_DARK: Color32 = Color32::from_rgb(0xB9, 0x7A, 0x1E);
pub const TEAL: Color32 = Color32::from_rgb(0x2C, 0x8C, 0x99);
pub const GREEN: Color32 = Color32::from_rgb(0x3F, 0xA8, 0x6B);
pub const RED: Color32 = Color32::from_rgb(0xD2, 0x55, 0x3F);
pub const SKY: Color32 = Color32::from_rgb(0x4C, 0x8D, 0xD6);
pub const SLATE: Color32 = Color32::from_rgb(0x7A, 0x86, 0x94);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Add,
    Extract,
    Test,
    View,
    Delete,
    Search,
    Guide,
    Info,
    Repair,
    Note,
    Folder,
    File,
    Archive,
    Up,
    Lock,
}

/// Map a unit square (0..1) point into `r`.
fn at(r: Rect, x: f32, y: f32) -> Pos2 {
    pos2(r.left() + x * r.width(), r.top() + y * r.height())
}

fn line(p: &Painter, r: Rect, pts: &[(f32, f32)], s: Stroke) {
    let v: Vec<Pos2> = pts.iter().map(|(x, y)| at(r, *x, *y)).collect();
    p.add(Shape::line(v, s));
}

fn poly(p: &Painter, r: Rect, pts: &[(f32, f32)], fill: Color32, s: Stroke) {
    let v: Vec<Pos2> = pts.iter().map(|(x, y)| at(r, *x, *y)).collect();
    p.add(Shape::Path(PathShape::convex_polygon(v, fill, PathStroke::from(s))));
}

fn sub(r: Rect, x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
    Rect::from_min_max(at(r, x0, y0), at(r, x1, y1))
}

/// A closed parcel: amber body with a teal band.
fn parcel(p: &Painter, r: Rect, ink: Color32) {
    let w = (r.width() / 22.0).max(1.0);
    let body = sub(r, 0.12, 0.30, 0.88, 0.90);
    p.rect_filled(body, CornerRadius::same((r.width() * 0.06) as u8), AMBER);
    let lid = sub(r, 0.08, 0.20, 0.92, 0.36);
    p.rect_filled(lid, CornerRadius::same((r.width() * 0.05) as u8), AMBER_DARK);
    p.rect_filled(sub(r, 0.44, 0.20, 0.56, 0.90), CornerRadius::ZERO, TEAL);
    p.rect_stroke(body.union(lid), CornerRadius::same((r.width() * 0.06) as u8), Stroke::new(w, ink), StrokeKind::Inside);
}

pub fn paint(p: &Painter, r: Rect, icon: Icon, ink: Color32) {
    let w = (r.width() / 16.0).max(1.0);
    let s = Stroke::new(w, ink);
    match icon {
        Icon::Add => {
            parcel(p, sub(r, 0.0, 0.05, 0.85, 0.95), ink);
            let c = at(r, 0.78, 0.30);
            let rad = r.width() * 0.2;
            p.circle_filled(c, rad, GREEN);
            let t = Stroke::new(w * 1.4, Color32::WHITE);
            p.line_segment([c - vec2(rad * 0.55, 0.0), c + vec2(rad * 0.55, 0.0)], t);
            p.line_segment([c - vec2(0.0, rad * 0.55), c + vec2(0.0, rad * 0.55)], t);
        }
        Icon::Extract => {
            parcel(p, sub(r, 0.0, 0.15, 0.80, 1.0), ink);
            let t = Stroke::new(w * 1.6, SKY);
            line(p, r, &[(0.55, 0.62), (0.88, 0.22)], t);
            line(p, r, &[(0.66, 0.20), (0.90, 0.18), (0.88, 0.42)], t);
        }
        Icon::Test => {
            poly(p, r, &[(0.5, 0.06), (0.88, 0.2), (0.82, 0.62), (0.5, 0.94), (0.18, 0.62), (0.12, 0.2)], TEAL, s);
            line(p, r, &[(0.32, 0.50), (0.46, 0.64), (0.70, 0.36)], Stroke::new(w * 1.8, Color32::WHITE));
        }
        Icon::View => {
            let c = at(r, 0.5, 0.5);
            let a = at(r, 0.06, 0.5);
            let b = at(r, 0.94, 0.5);
            let up = vec2(0.0, r.height() * 0.38);
            for sign in [-1.0, 1.0] {
                p.add(CubicBezierShape::from_points_stroke([a, a + vec2(r.width() * 0.25, 0.0) + up * sign, b - vec2(r.width() * 0.25, 0.0) + up * sign, b], false, Color32::TRANSPARENT, s));
            }
            p.circle_filled(c, r.width() * 0.17, SKY);
            p.circle_filled(c, r.width() * 0.07, ink);
        }
        Icon::Delete => {
            poly(p, r, &[(0.22, 0.30), (0.78, 0.30), (0.72, 0.92), (0.28, 0.92)], SLATE, s);
            p.rect_filled(sub(r, 0.14, 0.18, 0.86, 0.27), CornerRadius::same(2), SLATE);
            p.rect_filled(sub(r, 0.40, 0.08, 0.60, 0.18), CornerRadius::same(2), SLATE);
            for x in [0.40, 0.5, 0.60] {
                line(p, r, &[(x, 0.42), (x, 0.82)], Stroke::new(w, Color32::WHITE));
            }
            line(p, r, &[(0.62, 0.48), (0.92, 0.80)], Stroke::new(w * 1.4, RED));
            line(p, r, &[(0.92, 0.48), (0.62, 0.80)], Stroke::new(w * 1.4, RED));
        }
        Icon::Search => {
            let c = at(r, 0.42, 0.42);
            p.circle_filled(c, r.width() * 0.28, SKY.gamma_multiply(0.35));
            p.circle_stroke(c, r.width() * 0.28, Stroke::new(w * 1.5, ink));
            line(p, r, &[(0.62, 0.62), (0.90, 0.90)], Stroke::new(w * 2.6, AMBER_DARK));
        }
        Icon::Guide => {
            line(p, r, &[(0.5, 0.10), (0.5, 0.95)], Stroke::new(w * 1.6, ink));
            poly(p, r, &[(0.18, 0.18), (0.80, 0.18), (0.94, 0.30), (0.80, 0.42), (0.18, 0.42)], TEAL, s);
            poly(p, r, &[(0.82, 0.50), (0.20, 0.50), (0.06, 0.62), (0.20, 0.74), (0.82, 0.74)], AMBER, s);
        }
        Icon::Info => {
            let c = at(r, 0.5, 0.5);
            p.circle_filled(c, r.width() * 0.44, SKY);
            p.circle_filled(at(r, 0.5, 0.28), r.width() * 0.07, Color32::WHITE);
            p.rect_filled(sub(r, 0.43, 0.42, 0.57, 0.76), CornerRadius::same(2), Color32::WHITE);
        }
        Icon::Repair => {
            let t = Stroke::new(w * 2.6, SLATE);
            line(p, r, &[(0.22, 0.80), (0.62, 0.40)], t);
            p.circle_stroke(at(r, 0.70, 0.30), r.width() * 0.17, Stroke::new(w * 2.2, SLATE));
            p.rect_filled(sub(r, 0.70, 0.08, 0.95, 0.24), CornerRadius::ZERO, Color32::TRANSPARENT);
            p.circle_filled(at(r, 0.30, 0.30), r.width() * 0.16, GREEN);
            line(p, r, &[(0.22, 0.30), (0.29, 0.37), (0.40, 0.23)], Stroke::new(w * 1.2, Color32::WHITE));
        }
        Icon::Note => {
            let b = sub(r, 0.08, 0.12, 0.92, 0.72);
            p.rect_filled(b, CornerRadius::same((r.width() * 0.12) as u8), AMBER);
            p.rect_stroke(b, CornerRadius::same((r.width() * 0.12) as u8), s, StrokeKind::Inside);
            poly(p, r, &[(0.28, 0.70), (0.46, 0.70), (0.24, 0.92)], AMBER, s);
            for y in [0.32, 0.46] {
                line(p, r, &[(0.24, y), (0.76, y)], Stroke::new(w, ink));
            }
        }
        Icon::Folder => {
            poly(p, r, &[(0.06, 0.22), (0.38, 0.22), (0.46, 0.32), (0.94, 0.32), (0.94, 0.86), (0.06, 0.86)], AMBER, Stroke::new(w * 0.8, AMBER_DARK));
            p.rect_filled(sub(r, 0.06, 0.42, 0.94, 0.86), CornerRadius::same(1), Color32::from_rgb(0xF0, 0xBD, 0x5E));
        }
        Icon::File => {
            poly(p, r, &[(0.20, 0.08), (0.62, 0.08), (0.82, 0.28), (0.82, 0.92), (0.20, 0.92)], Color32::from_gray(245), Stroke::new(w * 0.8, SLATE));
            line(p, r, &[(0.62, 0.08), (0.62, 0.28), (0.82, 0.28)], Stroke::new(w * 0.8, SLATE));
        }
        Icon::Archive => parcel(p, r, ink),
        Icon::Up => {
            let t = Stroke::new(w * 1.6, ink);
            line(p, r, &[(0.5, 0.85), (0.5, 0.18)], t);
            line(p, r, &[(0.22, 0.45), (0.5, 0.15), (0.78, 0.45)], t);
        }
        Icon::Lock => {
            p.circle_stroke(at(r, 0.5, 0.38), r.width() * 0.2, Stroke::new(w * 1.4, ink));
            p.rect_filled(sub(r, 0.22, 0.42, 0.78, 0.90), CornerRadius::same(2), AMBER_DARK);
        }
    }
}

/// Allocate `size` and draw the icon there.
pub fn show(ui: &mut egui::Ui, icon: Icon, size: f32) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let ink = ui.visuals().text_color();
    paint(ui.painter(), r, icon, ink);
    resp
}
