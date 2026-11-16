//! The cross drill's holder on one station (issue #119): a move whose
//! pose lies along an XY axis and which slides along its own hole line,
//! across it, or in Z - never swinging round the bar - sweeps a
//! rectangular tube about that line. The material read in that tube
//! (`cross_entry::tube_reach`: a box at its face, a turned round at its
//! circumscribed reading) is what the holder's face must stand outside,
//! so a box stock no longer refuses a holder that clears its face but
//! not its corner circle. Any other move (the turret swinging between
//! quarter-turn stations, a pose off the XY axes) keeps the corner
//! radius in the holder's Z band (`drill_reach::cross_holder`).
//!
//! Provenance: a cylinder about a line lies inside the square tube of
//! its radius; a straight slide's swept footprint is the tube widened
//! by the slide - textbook geometry. Our own design notes; no vendor
//! material consulted.

use crate::geometry::{Point3, Triangle};

use super::super::cross_entry::{Line, StockMesh, Tube, tube_reach};

const TOL: f64 = 1e-6;

/// The part and the stock (when known) in the spindle frame, with the
/// widest vertex radius about the spindle - the circumscribed reading -
/// and any model excess-driven slab growth (`grow`).
pub(super) struct Material {
    mesh: StockMesh,
    raw: f64,
    grow: f64,
}

pub(super) fn material(part: &[Triangle], stock: Option<&[Triangle]>, (ox, oy): (f64, f64), excess: f64) -> Material {
    let shift = |p: Point3| Point3::new(p.x - ox, p.y - oy, p.z);
    let frame = |s: &[Triangle]| -> Vec<Triangle> {
        s.iter().map(|t| Triangle::new(shift(t.a), shift(t.b), shift(t.c))).collect()
    };
    let (p, s) = (frame(part), stock.map(frame).unwrap_or_default());
    let raw = p
        .iter()
        .chain(&s)
        .flat_map(|t| [t.a, t.b, t.c])
        .filter(|v| v.is_finite())
        .map(|v| v.x.hypot(v.y))
        .fold(0.0_f64, f64::max);
    let grow = if p.is_empty() && s.is_empty() { 0.0 } else { super::super::fidelity_law::band_growth(excess) };
    let mut mesh = StockMesh::new(p);
    mesh.extend(StockMesh::new(s));
    Material { mesh, raw: raw + grow, grow }
}

/// A holder on one station: its pose `axis` (tip toward shank) a unit
/// XY axis, the tip sliding from `from` to `to` along that line, across
/// it, or in Z, but not both along and across (that is a swing round
/// the bar). `Some((tube, face))`: the tube the holder (radius `hr`,
/// face `reach` behind the tip) sweeps, and the outward coordinate of
/// its face at its innermost. `None`: not an on-station move.
pub(super) fn on_station(from: Point3, to: Point3, axis: [f64; 3], reach: f64, hr: f64) -> Option<(Tube, f64)> {
    let [x, y, z] = axis;
    let unit = |a: f64, b: f64| (a.abs() - 1.0).abs() <= 1e-9 && b.abs() <= 1e-9 && z.abs() <= 1e-9;
    let (a, out) = if unit(x, y) {
        (0, x.signum())
    } else if unit(y, x) {
        (1, y.signum())
    } else {
        return None;
    };
    let w = |p: Point3| if a == 0 { p.y } else { p.x };
    let line = Line { a, out, t: 0.5 * (w(from) + w(to)), z: 0.5 * (from.z + to.z) };
    let (du, dw) = ((line.u(to) - line.u(from)).abs(), (w(to) - w(from)).abs());
    if !(du <= TOL || dw <= TOL) {
        return None;
    }
    let tube = Tube { line, hw: hr + 0.5 * dw, hz: hr + 0.5 * (to.z - from.z).abs() };
    Some((tube, line.u(from).min(line.u(to)) + reach))
}

/// The outermost outward coordinate of material in `tube`, `None` when
/// the tube meets none.
pub(super) fn reach_in(m: &Material, tube: Tube) -> Option<f64> {
    tube_reach(&m.mesh, m.raw, tube, m.grow)
}
