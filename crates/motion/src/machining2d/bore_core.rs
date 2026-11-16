//! Bore floor reach and the bounded concentric-ring plan (#1566).
//!
//! Provenance: plain torus geometry of a corner-radiused end mill (a
//! cutter of radius `r` whose corner of radius `cr` rounds into a flat
//! bottom of radius `r - cr`), as machine-tool handbooks draw it; the
//! ride's bands are those of `bore.rs`. Our own derivation, written
//! down in `docs/decisions/1556-bore-core-door.md`.

/// Float allowance for the cutter corner fit and minimum radial step.
pub(super) const CORE_TOL: f64 = 1e-6;

/// The radius a cutter cuts at `height` above its tip: the flat bottom
/// (`r - cr`) plus the corner's reach at that height, `sqrt(cr^2 -
/// (cr - h)^2) = sqrt(h (2 cr - h))`; the full radius `r` from `cr` up.
/// `corner_radius` is clamped to `[0, r]` (0 a square end mill, `r` a
/// ball); a negative height reads as the tip.
#[must_use]
pub fn cutting_radius_at(tool_radius: f64, corner_radius: f64, height: f64) -> f64 {
    let r = tool_radius.max(0.0);
    let cr = corner_radius.clamp(0.0, r);
    if height >= cr {
        return r;
    }
    let h = height.max(0.0);
    (r - cr) + (h * (2.0 * cr - h)).max(0.0).sqrt()
}

/// How far the cutter reaches at the plane the core would stand on.
/// A through hole (`through_allowance = Some(a)`, the tip `a` under the
/// floor) is judged at the floor plane, where the cutter cuts at
/// height `a`: a core left there has no floor under it and comes loose.
/// A blind hole (`None`) uses the flat bottom radius. Rings overlap at
/// the floor plane, so no unwarned ridge remains between them.
#[must_use]
pub fn bore_floor_reach(tool_diameter: f64, corner_radius: f64, through_allowance: Option<f64>) -> f64 {
    let r = tool_diameter * 0.5;
    match through_allowance {
        Some(a) => cutting_radius_at(r, corner_radius, a),
        None => cutting_radius_at(r, corner_radius, 0.0),
    }
}

/// Historical single-ring gap across a hole of `hole_radius`: the gap
/// between the dive's disc (radius `reach`) and the ring's inner edge
/// (`rc - reach`, `rc = R - r - leave`): `rc - 2 reach`. Positive is
/// material no pass touches.
#[must_use]
pub fn bore_core_gap(hole_radius: f64, tool_diameter: f64, corner_radius: f64, leave: f64, through_allowance: Option<f64>) -> f64 {
    let rc = hole_radius - (tool_diameter * 0.5 + leave);
    rc - 2.0 * bore_floor_reach(tool_diameter, corner_radius, through_allowance)
}

/// Maximum rings per hole. Refuse a pathological radial plan before allocation.
pub(super) const MAX_BORE_RINGS: usize = 4096;

/// Radial expansion stays below one cutter diameter and overlaps the
/// floor bands. Blind holes use the flat bottom, through holes the
/// corner reach at the model floor. A ball at a blind floor has no band.
pub(super) fn bore_step(tool_diameter: f64, corner_radius: f64, through: Option<f64>) -> f64 {
    0.8 * bore_floor_reach(tool_diameter, corner_radius, through)
}

/// Keep the old single ring when its floor band meets the centre disc.
/// Otherwise walk outward at bounded stepover, ending exactly on the wall.
/// None names an unresolvable floor band or an excessive ring count.
pub(super) fn bore_ring_count(hole_radius: f64, tool_diameter: f64, corner_radius: f64, leave: f64, through: Option<f64>) -> Option<usize> {
    let rc = hole_radius - (tool_diameter * 0.5 + leave);
    let step = bore_step(tool_diameter, corner_radius, through);
    if !rc.is_finite() || rc <= 0.0 || !step.is_finite() || step <= CORE_TOL {
        return None;
    }
    if rc <= 2.0 * bore_floor_reach(tool_diameter, corner_radius, through) {
        return Some(1);
    }
    let count = (rc / step).ceil();
    if !count.is_finite() || count > MAX_BORE_RINGS as f64 {
        return None;
    }
    Some(count as usize)
}

/// The generator and Detect holes share the bounded floor-coverage verdict.
#[must_use]
pub fn bore_leaves_core(hole_radius: f64, tool_diameter: f64, corner_radius: f64, leave: f64, through_allowance: Option<f64>) -> bool {
    bore_ring_count(hole_radius, tool_diameter, corner_radius, leave, through_allowance).is_none()
}

/// Largest hole allowed by the bounded radial plan. A zero floor band
/// cannot clear a flat floor at any width and reports no capacity.
#[must_use]
pub fn max_bore_diameter(tool_diameter: f64, corner_radius: f64, stock_to_leave: f64, through_allowance: Option<f64>) -> f64 {
    let step = bore_step(tool_diameter, corner_radius, through_allowance);
    if !step.is_finite() || step <= CORE_TOL {
        return 0.0;
    }
    tool_diameter + 2.0 * stock_to_leave + 2.0 * MAX_BORE_RINGS as f64 * step
}
