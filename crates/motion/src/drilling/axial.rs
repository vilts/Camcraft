//! Axial drilling on the lathe (M13.5 A): the turret's simplest hole,
//! a drill held stationary on the spindle axis while the part spins.
//! The cycle vocabulary is the mill's (`cycle::drill` makes the
//! chain and the `Cycle` records); what this file adds is the
//! lathe's two laws around it - the DOORS a lathe-mounted drill op
//! must pass before a move is computed, and the VERDICT that judges
//! the finished chain against the modeled bore in the meridian,
//! where the vertical height-field laws (`Mode::Revolve` stands
//! them down) cannot see a hole that runs down the spindle.
//!
//! Doors, by name: a hole off the spindle axis (that is the live
//! tooling's radial drill, or an end-face hole wanting a C axis -
//! never this cycle); a hole bottoming past the bar's far end; a
//! drill wider than the modeled bore. Verdict: the hole recognition
//! re-reads the part (the hole list a user typed is never the
//! judge), and per cycle the drill must find a modeled hole on its
//! axis, be no wider than it, bottom no deeper than its floor (plus
//! the through exit), and bottom no shallower than it - rest by name,
//! not silence.
//!
//! Provenance: axial drilling from the turret as the lathe's own
//! hole-making cycle is machine-tool handbook practice; the doors and
//! the meridian verdict are ours, specced clean-room from the law
//! that a lathe's drill stands on the spindle axis and nowhere else.
//! No vendor material consulted.

use super::hole::Hole;
use super::recognize::recognize;

use crate::cl::{CycleKind, Toolpath};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::multiaxis::axis;
use crate::verify::Warning;

/// How far off the fitted spindle axis a hole centre may stand and
/// still be "on the axis": a twentieth of a millimetre - over the
/// circle-fit noise of recognition, under any real eccentricity.
pub const AXIAL_TOL: f64 = 0.05;

/// A cosine this close to one reads the fitted axis as the spindle's
/// Z (the lathe's only turning line).
const Z_COS: f64 = 0.999;

/// The part's spindle line as the lathe sees it: the fitted axis of
/// revolution, refused by name when the model states no turning or
/// turns about anything but Z.
fn spindle_line(facets: &[Triangle]) -> Result<axis::Axis, GenError> {
    let ax = axis::fit(facets).map_err(|e| {
        GenError::Invalid(format!("axial drilling needs a turned part on the spindle: {e}"))
    })?;
    if ax.dir.z.abs() < Z_COS {
        return Err(GenError::Invalid(format!(
            "the part's axis of revolution runs ({:.3}, {:.3}, {:.3}); a lathe's spindle is Z - stand the part on the spindle first",
            ax.dir.x, ax.dir.y, ax.dir.z
        )));
    }
    Ok(ax)
}

/// Distance from `(x, y)` to the axis line, measured in the plane
/// transverse to Z (the axis is Z to within `Z_COS`, so the
/// transverse offset is the honest eccentricity).
fn off_axis(ax: &axis::Axis, x: f64, y: f64) -> f64 {
    ((x - ax.origin.x).powi(2) + (y - ax.origin.y).powi(2)).sqrt()
}

/// The doors a lathe-mounted drill op passes before the mill's
/// cycle kernel runs: every hole on the spindle axis, no bottom past
/// the bar's far end, the drill no wider than the bore it is told to
/// make. `bottom_z` is the op's depth override, when authored.
pub fn axial_doors(
    facets: &[Triangle],
    holes: &[Hole],
    tool_d: f64,
    bottom_z: Option<f64>,
    through_clear: f64,
) -> Result<(), GenError> {
    if !(tool_d.is_finite() && tool_d > 0.0) {
        return Err(GenError::Invalid("axial drill diameter not usable".into()));
    }
    if holes.is_empty() {
        return Err(GenError::Empty("no axial holes to drill".into()));
    }
    let ax = spindle_line(facets)?;
    let bar_end = facets
        .iter()
        .flat_map(|t| [t.a.z, t.b.z, t.c.z])
        .fold(f64::INFINITY, f64::min);
    if !bar_end.is_finite() {
        return Err(GenError::Invalid("the bar has no far end to measure a hole against".into()));
    }
    for h in holes {
        // #677: the spot each refusal below marks - the hole's mouth,
        // or its floor where the floor is what is wrong.
        let mouth = Point3::new(h.xy.x, h.xy.y, h.entry_z);
        let e = off_axis(&ax, h.xy.x, h.xy.y);
        if !(e.is_finite() && e <= AXIAL_TOL) {
            return Err(GenError::invalid_at(
                format!(
                    "hole at ({}, {}) stands {e:.3} mm off the spindle axis: a lathe's turret drills on the axis only - a radial hole is the live tooling's cross drill (M5.2 F2), an off-axis end hole needs a C axis (M13.5 B)",
                    h.xy.x, h.xy.y
                ),
                mouth,
            ));
        }
        let floor = bottom_z.unwrap_or(h.floor_z - if h.through { through_clear } else { 0.0 });
        if floor < bar_end - through_clear - 1e-9 {
            return Err(GenError::invalid_at(
                format!(
                    "hole at ({}, {}) bottoms at z = {floor}, past the bar's far end at z = {bar_end}: the drill would leave the steel through the chuck side",
                    h.xy.x, h.xy.y
                ),
                Point3::new(h.xy.x, h.xy.y, floor),
            ));
        }
        if tool_d > 2.0 * h.radius + 2.0 * super::hole::FIT_TOL {
            return Err(GenError::invalid_at(
                format!(
                    "the drill is {tool_d} mm across, the bore at ({}, {}) is modeled {} mm: a wider drill cuts past the modeled wall",
                    h.xy.x,
                    h.xy.y,
                    2.0 * h.radius
                ),
                mouth,
            ));
        }
    }
    Ok(())
}

/// The meridian verdict of an axial-drilled chain: the part is
/// re-read for its holes, and every cycle must name one on its own
/// centre. Gouge (by name, at the cycle's first move) where the
/// drill is wider than the modeled bore, deeper than its floor plus
/// the through exit, or stands where the model has no hole at all;
/// rest ([`Warning::Envelope`]) where a blind hole's floor is left
/// above the modeled one by more than `tol`. A model the recognition
/// cannot read reports as a gouge at the first cut - a judge that
/// cannot measure never reports silence.
pub fn axial_warnings(facets: &[Triangle], path: &Toolpath, tool_d: f64, tol: f64, through_clear: f64) -> Vec<Warning> {
    let first_cut = || Warning::Gouge {
        at: path
            .moves
            .iter()
            .position(|m| m.kind != crate::cl::MoveKind::Rapid)
            .unwrap_or(0),
    };
    if facets.is_empty() || !(tool_d.is_finite() && tool_d > 0.0) || path.cycles.is_empty() {
        return vec![first_cut()];
    }
    let found = recognize(facets);
    let mut warns = Vec::new();
    for c in &path.cycles {
        let at = c.first_move.min(path.moves.len().saturating_sub(1));
        let Some(h) = found
            .iter()
            .find(|h| ((h.xy.x - c.xy.x).powi(2) + (h.xy.y - c.xy.y).powi(2)).sqrt() <= AXIAL_TOL)
        else {
            warns.push(Warning::Gouge { at });
            continue;
        };
        let floor = h.floor_z - if h.through { through_clear } else { 0.0 };
        if tool_d * 0.5 > h.radius + tol || c.bottom_z < floor - tol {
            warns.push(Warning::Gouge { at });
            continue;
        }
        // A spot dimple is shallow by promise; every other cycle owes
        // the floor.
        if c.kind != CycleKind::Spot && !h.through && c.bottom_z > h.floor_z + tol {
            warns.push(Warning::Envelope { at, gouge_mm: 0.0, rest_mm: c.bottom_z - h.floor_z });
        }
    }
    warns
}
