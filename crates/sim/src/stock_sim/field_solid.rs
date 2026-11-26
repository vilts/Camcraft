//! The tool solids themselves: one pose's distance, read in the
//! solid's own half-plane, plus the small vector arithmetic the sweep
//! walks need. Split from `field_cut.rs` by the 400-line law - the
//! layer, the window, and the move vocabulary live there; the
//! geometry that says "how far is this point from a tool standing
//! here" lives here.
//!
//! Provenance: point-to-primitive distances from the published
//! catalogue (segment, box-orthant, and the revolved profile read as
//! a planar polygon; the bull nose's rounded rectangle in
//! `field_bull`), and the solids of RS-274 world machining
//! this codebase's dexel atoms already sweep. Nothing from vendor
//! binaries.
//!
//! ## The law here
//!
//! - **The half-plane is exact, both signs.** A solid of revolution
//!   about the axis puts its nearest surface point to any query in
//!   the query's own half-plane: rotating the pair to a common plane
//!   preserves distances and lands the foot on the profile, so the
//!   2-D profile distance IS the 3-D distance. For the cone's inside
//!   values the flank line's perpendicular may under-read near the
//!   axis (its foot can slide off the truncated segment), which can
//!   only SHRINK the claimed removal: the conservative direction,
//!   chosen not born.

use super::field_cut::SdfTool;
use super::kernel::StockError;

use crate::geometry::Point3;

/// Which tool, pre-validated.
pub(crate) enum ToolKind {
    Ball(f64),
    Shanked { r: f64, shank: f64 },
    Flat { r: f64, flank: f64 },
    Taper { r: f64, cot: f64 },
    Drill { r: f64, cot: f64, flank: f64 },
    Bull { r: f64, corner: f64, flank: f64 },
    /// The dovetail head's frustum (#1444), widest at the tip.
    Dove { r: f64, cot: f64, flank: f64 },
    /// The turning tool's footprint (M13.1 owed; the blade's, #21):
    /// the annular-sector sweep about the lathe axis, `field_cut`'s
    /// `Sweep::Lathe`.
    Lathe { foot: super::field_blade::Footprint, axis: super::field_lathe::LatheAxis },
}

/// The tool's parameters, refused by name when nonsense.
pub(crate) fn tool_kind(tool: &SdfTool) -> Result<ToolKind, StockError> {
    let radius = |r: &f64| -> Result<(), StockError> {
        if !r.is_finite() || *r <= 0.0 {
            return Err(StockError::Invalid(format!(
                "field subtract: tool radius {r} is not a positive finite number"
            )));
        }
        Ok(())
    };
    let length = |what: &str, v: &f64| -> Result<(), StockError> {
        if !v.is_finite() || *v <= 0.0 {
            return Err(StockError::Invalid(format!(
                "field subtract: {what} {v} is not a positive finite number"
            )));
        }
        Ok(())
    };
    Ok(match tool {
        SdfTool::Ball { r } => {
            radius(r)?;
            ToolKind::Ball(*r)
        }
        SdfTool::Shanked { r, shank } => {
            radius(r)?;
            length("shank length", shank)?;
            ToolKind::Shanked { r: *r, shank: *shank }
        }
        SdfTool::Flat { r, flank } => {
            radius(r)?;
            length("flank length", flank)?;
            ToolKind::Flat { r: *r, flank: *flank }
        }
        SdfTool::Taper { r, cot } => {
            radius(r)?;
            length("cone slope", cot)?;
            ToolKind::Taper { r: *r, cot: *cot }
        }
        SdfTool::Drill { r, cot, flank } => {
            radius(r)?;
            length("cone slope", cot)?;
            length("flank length", flank)?;
            ToolKind::Drill { r: *r, cot: *cot, flank: (*flank).max(*r * *cot) }
        }
        SdfTool::BullNose { r, corner, flank } => {
            radius(r)?;
            length("corner radius", corner)?;
            length("flank length", flank)?;
            if corner > r || flank < corner {
                return Err(StockError::Invalid(format!(
                    "field subtract: bull-nose corner {corner} must not exceed the radius {r} or the flank {flank}"
                )));
            }
            ToolKind::Bull { r: *r, corner: *corner, flank: *flank }
        }
        SdfTool::Dovetail { r, cot, flank } => {
            radius(r)?;
            length("dovetail flank slope", cot)?;
            length("flank length", flank)?;
            if r - flank * cot <= 0.0 {
                return Err(StockError::Invalid(format!(
                    "field subtract: a dovetail of radius {r} narrows to nothing within its flank {flank} (slope {cot})"
                )));
            }
            ToolKind::Dove { r: *r, cot: *cot, flank: *flank }
        }
        SdfTool::Lathe { profile, axis } => {
            let foot = profile.footprint()?;
            // Re-stated through the gate: a hand-built axis is
            // refused by name, never cut as a NaN spindle.
            let axis = super::field_lathe::LatheAxis::new(axis.origin(), axis.dir())?;
            ToolKind::Lathe { foot, axis }
        }
    })
}

/// The distance from `q` to the parallelogram `o + s e1 + t e2`,
/// `s, t` in [0, 1] (the shanked ball's swept core): the plane's
/// perpendicular where the foot lands inside, else the nearest of the
/// four edges - exact both ways; a degenerate (parallel-sided) one is
/// its edges alone.
pub(crate) fn dist_pt_para(q: Point3, o: Point3, e1: Vec3, e2: Vec3) -> f64 {
    let n = cross(e1, e2);
    let nn = dot(n, n);
    let w = sub(q, o);
    if nn > 1e-24 * (dot(e1, e1) * dot(e2, e2)) && nn > 0.0 {
        let s = dot(cross(w, e2), n) / nn;
        let t = dot(cross(e1, w), n) / nn;
        if (0.0..=1.0).contains(&s) && (0.0..=1.0).contains(&t) {
            return dot(w, n).abs() / nn.sqrt();
        }
    }
    let seg = |a: Point3, d: Vec3| {
        let dd = dot(d, d);
        let v = sub(q, a);
        let t = if dd > 0.0 { (dot(v, d) / dd).clamp(0.0, 1.0) } else { 0.0 };
        let r = sub(q, add(a, scale(d, t)));
        dot(r, r).sqrt()
    };
    let (o1, o2) = (add(o, e1), add(o, e2));
    seg(o, e1).min(seg(o, e2)).min(seg(o1, e2)).min(seg(o2, e1))
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// The exact minimizer of `t -> pose_dist` for an upright flat tool
/// on the two moves 2.5-D paths are made of (M7.3's per-cell cost):
/// on a level move `zl` is fixed and the distance only grows with the
/// radial offset, so the minimum sits at the XY foot of the path; on
/// a plunge `rho` is fixed and the box distance is convex in `zl`
/// with its minimum at mid-flank. `None` sends every other move to
/// the golden section.
pub(crate) fn upright_flat_foot(ax: [f64; 3], a: Point3, d: [f64; 3], q: Point3, kind: &RevKind) -> Option<f64> {
    let RevKind::Flat { flank, .. } = kind else {
        return None;
    };
    if ax != [0.0, 0.0, 1.0] {
        return None;
    }
    if d[2] == 0.0 {
        let dd = d[0] * d[0] + d[1] * d[1];
        let t = ((q.x - a.x) * d[0] + (q.y - a.y) * d[1]) / dd;
        return t.is_finite().then(|| t.clamp(0.0, 1.0));
    }
    if d[0] == 0.0 && d[1] == 0.0 {
        let t = (q.z - a.z - 0.5 * flank) / d[2];
        return t.is_finite().then(|| t.clamp(0.0, 1.0));
    }
    None
}

/// The revolved profiles, in their own frame: radius, and the
/// along-axis extent from the tip anchor.
pub(crate) enum RevKind {
    /// cylinder: radius `r`, from 0 to `flank` along the axis
    Flat { r: f64, flank: f64 },
    /// truncated cone: apex at the anchor, up to radius `r`
    Taper { r: f64, cot: f64 },
    /// twist drill (#2441): cone up to `r * cot`, then cylinder of `r` up to `flank`
    Drill { r: f64, cot: f64, flank: f64 },
    /// bull nose (M13.1): cylinder of `r` to `flank`, its bottom edge
    /// rounded by `corner` (`field_bull`'s profile)
    Bull { r: f64, corner: f64, flank: f64 },
    /// dovetail head (#1444): a frustum of radius `r` at the anchor that
    /// narrows by `cot` per unit height, up to `flank`
    Dove { r: f64, cot: f64, flank: f64 },
}

/// The distance from `q` to the tool solid sitting at anchor `t` with
/// axis `ax`: the profile read in the solid's own (rho, along-axis)
/// half-plane (the law this module's header states).
pub(crate) fn pose_dist(ax: [f64; 3], t: Point3, q: Point3, kind: &RevKind) -> f64 {
    let y = sub(q, t);
    let zl = dot(y, ax);
    let rho = (dot(y, y) - zl * zl).max(0.0).sqrt();
    match kind {
        RevKind::Flat { r, flank } => {
            let (e1, e2, e3) = (rho - r, -zl, zl - *flank);
            if e1.max(e2).max(e3) > 0.0 {
                e1.max(0.0).hypot(e2.max(0.0)).hypot(e3.max(0.0))
            } else {
                e1.max(e2).max(e3)
            }
        }
        RevKind::Taper { r, cot } => {
            let h = r * cot;
            let g1 = (rho * cot - zl) / (cot * cot + 1.0).sqrt();
            if g1 <= 0.0 && zl <= h {
                -g1.abs().min(h - zl) // inside: nearest face, flank or top
            } else {
                min3(
                    dist_pt_seg2(rho, zl, 0.0, 0.0, *r, h), // the flank segment
                    dist_pt_seg2(rho, zl, 0.0, h, *r, h),   // the top face
                    rho.hypot(zl - zl.clamp(0.0, h)),       // the axis, revolved to points
                )
            }
        }
        RevKind::Drill { r, cot, flank } => drill_dist(rho, zl, *r, *cot, *flank),
        RevKind::Bull { r, corner, flank } => super::field_bull::bull_dist(rho, zl, *r, *corner, *flank),
        RevKind::Dove { r, cot, flank } => dove_dist(rho, zl, *r, *cot, *flank),
    }
}

/// Exact Euclidean SDF to the dovetail frustum's meridian (#1444): the
/// convex quadrilateral (0, 0), (r, 0), (r - flank cot, flank),
/// (0, flank) in the (rho, along-axis) half-plane. A convex profile
/// that holds its axis revolves into a convex solid whose nearest point
/// lies in the query's own meridian, so the 2-D distance is the 3-D one.
fn dove_dist(rho: f64, zl: f64, r: f64, cot: f64, flank: f64) -> f64 {
    let top = r - flank * cot;
    // Signed distance to the flank line, positive outside.
    let side = (rho - r + zl * cot) / (1.0 + cot * cot).sqrt();
    if zl >= 0.0 && zl <= flank && side <= 0.0 {
        return side.max(-zl).max(zl - flank);
    }
    min3(
        dist_pt_seg2(rho, zl, 0.0, 0.0, r, 0.0),
        dist_pt_seg2(rho, zl, r, 0.0, top, flank),
        dist_pt_seg2(rho, zl, 0.0, flank, top, flank),
    )
}

/// Exact Euclidean SDF to a twist drill profile: cone from apex `(0, 0)`
/// to `(r, r * cot)`, followed by cylindrical flute of `r` up to `flank`.
fn drill_dist(rho: f64, zl: f64, r: f64, cot: f64, flank: f64) -> f64 {
    let h = r * cot;
    let f = flank.max(h);
    let len = r.hypot(h);
    let d_cone = (rho * h - zl * r) / len;
    if rho <= r && zl <= f && d_cone <= 0.0 {
        return d_cone.max(rho - r).max(zl - f);
    }
    let l2 = r * r + h * h;
    let tc = if l2 > 0.0 { ((rho * r + zl * h) / l2).clamp(0.0, 1.0) } else { 0.0 };
    let dist_cone = (rho - tc * r).hypot(zl - tc * h);
    let dist_cyl = (rho - r).max(0.0).hypot(zl - zl.clamp(h, f));
    dist_cone.min(dist_cyl)
}

/// An UPRIGHT flat pose's exact distance `v` (anchor `foot`, axis +Z)
/// with its ties settled the column engine's way (M7.3): a point ON
/// the flank (`rho == r`) strictly above the floor is removed - the
/// column's reach law is inclusive of its disc - while a point ON the
/// floor is not (the column's top IS the floor: steel to that plane).
/// Elsewhere the old conservative hair stays: outside reads a
/// rounding-scale more than exact, so no other tie ever claims.
pub(crate) fn flat_tie(foot: Point3, q: Point3, kind: &RevKind, v: f64) -> f64 {
    if let RevKind::Flat { r, flank } = kind {
        let zl = q.z - foot.z;
        let rho = (q.x - foot.x).hypot(q.y - foot.y);
        if rho <= *r && zl > 0.0 && zl < *flank {
            return v.min(-f64::MIN_POSITIVE);
        }
    }
    v + 1e-12 + 1e-15 * v.abs()
}

/// Distance in the (rho, z) half-plane from a point to a segment.
pub(crate) fn dist_pt_seg2(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let (dx, dy) = (bx - ax, by - ay);
    let dd = dx * dx + dy * dy;
    let t = if dd > 0.0 {
        (((px - ax) * dx + (py - ay) * dy) / dd).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (px - (ax + dx * t)).hypot(py - (ay + dy * t))
}

fn min3(a: f64, b: f64, c: f64) -> f64 {
    a.min(b).min(c)
}

pub(crate) type Vec3 = [f64; 3];

pub(crate) fn sub(a: Point3, b: Point3) -> Vec3 {
    [a.x - b.x, a.y - b.y, a.z - b.z]
}

pub(crate) fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn add(p: Point3, v: Vec3) -> Point3 {
    Point3::new(p.x + v[0], p.y + v[1], p.z + v[2])
}

pub(crate) fn scale(v: Vec3, s: f64) -> Vec3 {
    [v[0] * s, v[1] * s, v[2] * s]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    #[test]
    fn a_stationary_pose_reads_its_profile_exactly() {
        // from == to: the sweep is one pose, and the flat's box read
        // must say what the (rho, z) half-plane says.
        let ax = [0.0, 0.0, 1.0];
        let d = pose_dist(ax, p(0.0, 0.0, 0.0), p(0.0, 0.3, 1.0), &RevKind::Flat { r: 1.0, flank: 5.0 });
        assert!((d + 0.7).abs() < 1e-12, "inside a cylinder: min(1 - 0.3, 1, 4) deep: {d}");
        let e = pose_dist(ax, p(0.0, 0.0, 0.0), p(0.0, 0.0, -2.0), &RevKind::Flat { r: 1.0, flank: 5.0 });
        assert_eq!(e, 2.0, "below the disc face it is the vertical excess");
        // The 45-degree cone, a point on its flank line: distance 0.
        let f = pose_dist(ax, p(0.0, 0.0, 0.0), p(2.0, 0.0, 2.0), &RevKind::Taper { r: 4.0, cot: 1.0 });
        assert!(f.abs() < 1e-12, "on the flank: {f}");
        // Twist drill (45-degree half angle, cone height 3, shank to z 10).
        let drill = RevKind::Drill { r: 3.0, cot: 1.0, flank: 10.0 };
        assert!(pose_dist(ax, p(0.0, 0.0, 0.0), p(2.0, 0.0, 2.0), &drill).abs() < 1e-12, "on drill cone");
        assert!(pose_dist(ax, p(0.0, 0.0, 0.0), p(3.0, 0.0, 6.0), &drill).abs() < 1e-12, "on drill flute");
        assert!((pose_dist(ax, p(0.0, 0.0, 0.0), p(4.0, 0.0, 6.0), &drill) - 1.0).abs() < 1e-12, "outside flute");
        assert!((pose_dist(ax, p(0.0, 0.0, 0.0), p(0.0, 0.0, -2.0), &drill) - 2.0).abs() < 1e-12, "below tip");
        assert!(pose_dist(ax, p(0.0, 0.0, 0.0), p(2.5, 0.0, 6.0), &drill) < -0.49, "inside flute");
    }

    #[test]
    fn the_parallelogram_distance_is_exact_inside_and_at_the_edges() {
        let (o, e1, e2) = (p(0.0, 0.0, 0.0), [4.0, 0.0, 0.0], [0.0, 0.0, 10.0]);
        assert!((dist_pt_para(p(2.0, 3.0, 5.0), o, e1, e2) - 3.0).abs() < 1e-12, "the face");
        assert!((dist_pt_para(p(7.0, 4.0, 5.0), o, e1, e2) - 5.0).abs() < 1e-12, "an edge");
        assert!((dist_pt_para(p(-3.0, 0.0, -4.0), o, e1, e2) - 5.0).abs() < 1e-12, "a corner");
        // Degenerate: e1 parallel to e2 is the long segment.
        let d = dist_pt_para(p(0.0, 2.0, 7.0), o, [0.0, 0.0, 1.0], e2);
        assert!((d - 2.0).abs() < 1e-12, "a plunge's core is a segment: {d}");
    }

    #[test]
    fn a_tilted_pose_rotates_the_profile_with_the_axis() {
        // The same world point under a 45-degree lean: the profile
        // lives in the axis frame, so the radial excess grows.
        let lean = [std::f64::consts::FRAC_1_SQRT_2, 0.0, std::f64::consts::FRAC_1_SQRT_2];
        let d = pose_dist(lean, p(0.0, 0.0, 0.0), p(0.0, 0.0, 5.0), &RevKind::Flat { r: 1.0, flank: 10.0 });
        let rho = 5.0 * std::f64::consts::FRAC_1_SQRT_2; // 3.54 radial
        assert!((d - (rho - 1.0)).abs() < 1e-9, "{d}");
    }
}
