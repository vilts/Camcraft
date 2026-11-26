//! The viewport bar's glyph drawing, separate from its layout and actions (#2508).
//! Provenance: original geometric glyphs from our viewport bar; no vendor assets.

use super::{NAV_PX, Nav};
use crate::viewport::ProjectionMode;

pub(super) fn draw(
    ui: &egui::Ui,
    kind: Nav,
    r: egui::Rect,
    s: egui::Stroke,
    triad: [egui::Color32; 3],
    projection: ProjectionMode,
) {
    let (a, b) = (r.min, r.max);
    let m = NAV_PX * 0.22;
    match kind {
        Nav::Frame => {
            // Four corner brackets around the button's inset square.
            let i0 = a + egui::vec2(m * 0.45, m * 0.45);
            let i1 = b - egui::vec2(m * 0.45, m * 0.45);
            for (p, dx, dy) in [
                (i0, 1.0f32, 1.0f32),
                (egui::pos2(i1.x, i0.y), -1.0, 1.0),
                (egui::pos2(i0.x, i1.y), 1.0, -1.0),
                (i1, -1.0, -1.0),
            ] {
                let hx = egui::vec2(dx * m, 0.0);
                let hy = egui::vec2(0.0, dy * m);
                ui.painter().line_segment([p + hx, p + hx + hy * 0.62], s);
                ui.painter().line_segment([p + hy, p + hx * 0.62 + hy], s);
            }
        }
        Nav::Iso => {
            // The cube from the house diagonal: hexagon silhouette,
            // three visible faces' inner edges meeting at the center.
            let c = r.center();
            let rr = NAV_PX * 0.30;
            let hex: Vec<egui::Pos2> = (0..6)
                .map(|k| {
                    let t = std::f64::consts::FRAC_PI_3 * f64::from(k) + 0.261_799;
                    c + egui::vec2(
                        (t.cos() * f64::from(rr)) as f32,
                        (t.sin() * f64::from(rr)) as f32,
                    )
                })
                .collect();
            ui.painter().add(egui::Shape::convex_polygon(
                hex.clone(),
                egui::Color32::TRANSPARENT,
                s,
            ));
            for k in [0usize, 2, 4] {
                ui.painter().line_segment([c, hex[k]], s);
            }
        }
        Nav::Wire => {
            let q0 = a + egui::vec2(m * 0.62, m * 0.62);
            let q1 = b - egui::vec2(m * 0.62, m * 0.62);
            ui.painter().rect_stroke(
                egui::Rect::from_min_max(q0, q1),
                1.0,
                s,
                egui::StrokeKind::Inside,
            );
            ui.painter().line_segment([q0, q1], s);
            ui.painter()
                .line_segment([egui::pos2(q1.x, q0.y), egui::pos2(q0.x, q1.y)], s);
        }
        Nav::Display => {
            // Three slider rows, each with its knob: the settings menu.
            let (x0, x1) = (a.x + m, b.x - m);
            for (k, t) in [(0.0f32, 0.3f32), (1.0, 0.7), (2.0, 0.45)] {
                let y = a.y + m + k * (NAV_PX - 2.0 * m) * 0.5;
                ui.painter().line_segment([egui::pos2(x0, y), egui::pos2(x1, y)], s);
                ui.painter().circle_filled(egui::pos2(x0 + (x1 - x0) * t, y), 2.2, s.color);
            }
        }
        Nav::Paths => {
            // A zigzag toolpath.
            let p0 = egui::pos2(a.x + m * 0.4, r.center().y);
            let step = (r.width() - m * 0.8) / 4.0;
            let pts: Vec<egui::Pos2> = (0..5)
                .map(|k| {
                    p0 + egui::vec2(
                        step * k as f32,
                        if k % 2 == 0 { 0.0 } else { -r.height() * 0.20 },
                    )
                })
                .collect();
            ui.painter().add(egui::Shape::line(pts, s));
        }
        Nav::Axes => {
            // The origin triad: an isometric triad, equal arms 120 deg
            // apart, Z up, X and Y 30 deg below the horizontal; the box
            // spans y -l..l/2, so the origin sits l/4 below the centre.
            let l = NAV_PX * 0.32;
            let (c30, s30) = (l * 0.866, l * 0.5);
            let o = r.center() + egui::vec2(0.0, l * 0.25);
            let [x, y, z] = triad;
            for (dir, col) in [
                (egui::vec2(c30, s30), x),
                (egui::vec2(-c30, s30), y),
                (egui::vec2(0.0, -l), z),
            ] {
                ui.painter()
                    .line_segment([o, o + dir], egui::Stroke::new(1.4, col));
            }
        }
        Nav::PickThrough => {
            // A pick ray crossing a bar: the ray runs through, the bar
            // is the near surface it passes.
            let c = r.center();
            let half = r.width() * 0.5 - m;
            ui.painter()
                .line_segment([c - egui::vec2(half, 0.0), c + egui::vec2(half, 0.0)], s);
            ui.painter()
                .line_segment([c - egui::vec2(m * 0.4, m), c + egui::vec2(m * 0.4, m)], s);
        }
        Nav::Move => {
            // An original part-and-handles glyph: three axes, unlike Pan's four arrows.
            let c = r.center() + egui::vec2(-2.0, 2.0);
            for d in [
                egui::vec2(9.0, 0.0),
                egui::vec2(0.0, -9.0),
                egui::vec2(-5.0, 5.0),
            ] {
                ui.painter().line_segment([c, c + d], s);
                ui.painter().circle_filled(c + d, 1.5, s.color);
            }
            ui.painter().rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(7.0, 7.0)),
                0.0,
                s,
                egui::StrokeKind::Inside,
            );
        }
        Nav::Rotate => super::super::turn_handles::glyph(ui, r, s),
        Nav::Projection => {
            // The two doors on one viewer: converging bars for
            // perspective, a plain square for orthographic.
            let c = r.center();
            let half = r.width() * 0.5 - m;
            if matches!(projection, ProjectionMode::Orthographic) {
                ui.painter().rect_stroke(
                    egui::Rect::from_center_size(c, egui::vec2(half * 1.7, half * 1.7)),
                    1.0,
                    s,
                    egui::StrokeKind::Inside,
                );
            } else {
                let top = c.y - half * 0.8;
                let bot = c.y + half * 0.8;
                let tip_top = half;
                let tip_bot = half * 0.4;
                ui.painter().line_segment(
                    [
                        egui::pos2(c.x - tip_top, top),
                        egui::pos2(c.x - tip_bot, bot),
                    ],
                    s,
                );
                ui.painter().line_segment(
                    [
                        egui::pos2(c.x + tip_top, top),
                        egui::pos2(c.x + tip_bot, bot),
                    ],
                    s,
                );
                ui.painter().line_segment(
                    [
                        egui::pos2(c.x - tip_top, top),
                        egui::pos2(c.x + tip_top, top),
                    ],
                    s,
                );
                ui.painter().line_segment(
                    [
                        egui::pos2(c.x - tip_bot, bot),
                        egui::pos2(c.x + tip_bot, bot),
                    ],
                    s,
                );
            }
        }
        Nav::Orbit => {
            // The orbit ring with the eye-satellite riding it.
            let c = r.center();
            let rad = NAV_PX * 0.27;
            ui.painter().circle_stroke(c, rad, s);
            let a = std::f32::consts::FRAC_PI_4;
            let dot = c + egui::vec2(a.cos() * rad, -a.sin() * rad);
            ui.painter().circle_filled(dot, 2.4, s.color);
            ui.painter()
                .arrow(dot, egui::vec2(a.sin(), a.cos()) * 5.0, s);
        }
        Nav::Pan => {
            // A pair of rails bowed out: the move arrows' move.
            let c = r.center();
            let l = NAV_PX * 0.24;
            for (origin, vec) in [
                (c, egui::vec2(l, 0.0)),
                (c, egui::vec2(-l, 0.0)),
                (c, egui::vec2(0.0, l)),
                (c, egui::vec2(0.0, -l)),
            ] {
                ui.painter().arrow(origin, vec, s);
            }
        }
        Nav::Zoom => {
            // A magnifier: ring plus the handle it hangs from.
            let c = r.center() - egui::vec2(2.5, 2.5);
            ui.painter().circle_stroke(c, NAV_PX * 0.21, s);
            ui.painter().line_segment(
                [
                    c + egui::vec2(NAV_PX * 0.15, NAV_PX * 0.15),
                    c + egui::vec2(NAV_PX * 0.30, NAV_PX * 0.30),
                ],
                s,
            );
        }
    }
}
