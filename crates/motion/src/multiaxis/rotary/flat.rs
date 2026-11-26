//! Provenance: axial contour stations and radial retract/index links from general rotary milling practice.

use crate::cl::{Move, MoveKind, Toolpath};
use crate::finishing3d::{self, Finish3DParams};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::{GenError, ARC_FIT_TOL};

use super::super::rotary_links;

/// Plan rings with the fitted axis as local Z. Top and bottom locate
/// ball centers along that axis; clearance and retract are tip radii.
pub(super) fn plan(local: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    if !(p.top_z.is_finite() && p.bottom_z.is_finite() && p.top_z >= p.bottom_z) {
        return Err(GenError::Invalid(
            "Rotary Contour needs finite axial stations with bottom at or below top".into(),
        ));
    }
    let rho = local
        .iter()
        .flat_map(|t| [t.a, t.b, t.c])
        .map(|v| v.x.hypot(v.y))
        .fold(0.0_f64, f64::max);
    let hop = p.retract_z.unwrap_or(p.clearance_z);
    for (name, value) in [("clearance", p.clearance_z), ("retract", hop)] {
        if !(value.is_finite() && value > rho + ARC_FIT_TOL) {
            return Err(GenError::Invalid(format!(
                "Rotary Contour {name} radius {value:.3} mm does not clear the work's {rho:.3} mm radius - raise it"
            )));
        }
    }
    let r = p.tool_diameter * 0.5;
    let mut q = *p;
    q.top_z = p.top_z - r;
    q.bottom_z = p.bottom_z - r;
    // This axial approach belongs only to the waterline planner. Its
    // links are discarded below; the emitted program uses radial air.
    let top = local
        .iter()
        .flat_map(|t| [t.a, t.b, t.c])
        .map(|v| v.z)
        .fold(p.top_z, f64::max);
    q.clearance_z = top + p.clearance_z + r;
    q.retract_z = None;
    q.ramp = false;
    q.entry_ramp = false;
    let base = finishing3d::waterline(local, &q)?;
    // Relink in the existing laid-frame convention: local X is the
    // rotary axis, and ball centers stand r above the planner tips.
    let lay = |v: Point3| Point3::new(v.z + r, v.x, v.y - r);
    let unlay = |v: Point3| Point3::new(v.y, v.z + r, v.x - r);
    let mut moves = Vec::new();
    for (i, m) in base.moves.iter().enumerate() {
        let lift = m.kind == MoveKind::Feed
            && m.arc.is_none()
            && m.from.x == m.to.x
            && m.from.y == m.to.y
            && m.to.z > m.from.z
            && base
                .moves
                .get(i + 1)
                .is_some_and(|n| n.kind == MoveKind::Rapid);
        let kind = if lift { MoveKind::Rapid } else { m.kind };
        for (a, b) in m.segments3(ARC_FIT_TOL) {
            moves.push(Move::new(kind, lay(a), lay(b), m.feed));
        }
        if kind == MoveKind::Feed
            && base
                .moves
                .get(i + 1)
                .is_none_or(|n| n.kind != MoveKind::Feed)
        {
            // A ring's last chord can look like the raster's upward
            // lift after rotation. An entry marker keeps that chord.
            moves.push(Move::new(MoveKind::Plunge, lay(m.to), lay(m.to), m.feed));
        }
    }
    let mut laid = Toolpath::new(moves);
    laid.notes = base.notes;
    let restore = |path: &Toolpath| {
        let mut out = Toolpath::new(
            path.moves
                .iter()
                .map(|m| Move::new(m.kind, unlay(m.from), unlay(m.to), m.feed))
                .collect(),
        );
        out.notes = path.notes.clone();
        out
    };
    rotary_links::radial_relink(
        &laid,
        r,
        p.clearance_z,
        hop.min(p.clearance_z),
        rho,
        p.feed,
        p.plunge_feed,
        None,
    )
    .map(|path| restore(&path))
    .map_err(|e| e.map_location(unlay).map_ghost(restore))
}
