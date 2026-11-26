//! Morph finishing (M2.5 D2): the ruled loft between the ladder's
//! successive levels.
//!
//! Provenance: "morph between sections" - generating cut guides by
//! interpolating successive section curves - is public CAM practice,
//! described in machine-tool handbooks and vendor documentation at
//! the level of behavior only. The construction here (area-rank
//! pairing, centroid-phase seam, column-grounded march of the
//! interpolated loop) is our own derivation, written from geometry,
//! not from any vendor's code, output forensics, or documentation
//! text.
//!
//! ## The law
//!
//! The waterline ladder holds its cusp promise by spacing levels at
//! `Δz = 2√(r·tol)` - a promise that rides the `cos²α` wall and
//! breaks in the steep band (the ramp and blend strategies' ground).
//! Morph attacks the same cusp from the other side: between every
//! consecutive pair of rings it rides ONE interpolated guide, the
//! loop `t = 1/2` of the ruling from ring `z_i` to ring `z_{i+1}` -
//! its XY the arc-length midpoint of the two tangent loops, its Z
//! not interpolated at all but GROUNDED by the column law
//! (`plunge_support`, the march families' own currency). Halving
//! the spacing quarters the cusp, so a wall ridden by morph shows
//! the ladder's promise at a quarter of the Z budget spent.
//!
//! The interpolation is a GUIDE, never a promise: where the column
//! refuses a sample (a canopy, an under-hang) the march breaks the
//! run at the hop like every other march family, and the refusal is
//! a note. Where the two levels' loops are not the same feature -
//! centroids a tool-diameter apart, a hole closing, an island
//! dying - nothing is paired and no loft is written; the rings
//! alone carry that level change, exactly as the ladder did before
//! this file existed. A single-level ladder (top = bottom) lofts
//! nothing and morph is waterline byte for byte.
//!
//! ## Pairing
//!
//! Components of consecutive levels match by AREA RANK (both loop
//! sets are sorted by |area| descending, the same order the ring
//! walker emits in) + a CENTROID PROXIMITY gate: the pair must sit
//! within `2r` XY of one another, or it is a feature appearing or
//! dying, not a wall continuing. Loops are resampled to a common
//! arc-length count with the seam at each loop's furthest +X point
//! from the centroid - deterministic, and aligned between levels
//! because every level's scan uses the same direction set.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{Loop, Point2};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::ladder::levels;
use super::march::emit_runs;
use super::params::{
    Finish3DParams, dirs, refinement_floor, scallop_stepdown, validate,
};
use super::rings::emit_rings;
use super::scan::level_region;
use super::shadow::march_loop;
use super::support::{build_sups, plunge_support};

/// The level's cut loops: outer (CCW) loops only - the wavefront law
/// rings.rs states in full - ordered by |area| descending, oriented
/// by the climb flag, and dropped below the march's minimum of three
/// vertices. The same enumeration the ring walker walks; here it is
/// kept so consecutive levels can be paired.
fn level_loops(
    facets: &[Triangle],
    r: f64,
    z_tip: f64,
    ds: &[(f64, f64)],
    climb: bool,
) -> Result<Vec<Vec<Point2>>, GenError> {
    let region = level_region(facets, r, z_tip + r, ds)?;
    let mut loops: Vec<&Loop> = region.loops().iter().filter(|l| l.is_ccw()).collect();
    loops.sort_by(|a, b| b.area().abs().total_cmp(&a.area().abs()));
    Ok(loops
        .into_iter()
        .filter(|l| l.points().len() >= 3)
        .map(|l| {
            let s = l.points();
            if climb {
                s.to_vec()
            } else {
                s.iter().rev().copied().collect()
            }
        })
        .collect())
}

/// The signed-area centroid of a closed polygon, or `None` when the
/// area is a rounding artifact - a loop with no signed area has no
/// fact to be paired by.
fn centroid(pts: &[Point2]) -> Option<Point2> {
    let (mut a2, mut cx, mut cy) = (0.0f64, 0.0f64, 0.0f64);
    let n = pts.len();
    for i in 0..n {
        let (p, q) = (pts[i], pts[(i + 1) % n]);
        let cross = p.x * q.y - q.x * p.y;
        a2 += cross;
        cx += (p.x + q.x) * cross;
        cy += (p.y + q.y) * cross;
    }
    if a2.abs() < 1e-12 {
        return None;
    }
    let z = Some(Point2::new(cx / (3.0 * a2), cy / (3.0 * a2)));
    z.filter(|c| c.x.is_finite() && c.y.is_finite())
}

/// The vertex furthest +X from the centroid (ties: lower y, then
/// lower index) - the seam every resample starts its arc-length
/// walk from, so two levels of the same feature pick the same
/// phase point.
fn seam(pts: &[Point2], c: Point2) -> usize {
    let mut best = 0usize;
    let mut bx = f64::NEG_INFINITY;
    let mut by = f64::INFINITY;
    for (i, p) in pts.iter().enumerate() {
        let (dx, dy) = (p.x - c.x, p.y - c.y);
        if dx > bx || (dx == bx && (dy < by || (dy == by && i == best))) {
            bx = dx;
            by = dy;
            best = i;
        }
    }
    best
}

/// Resample a closed loop to exactly `n` points, walking from the
/// seam by cumulative chord length (the traveler's sum, as
/// `march_loop` names it). Degenerate (zero-length) loops return
/// nothing; the pairing step reads that as "no loft".
fn resample(pts: &[Point2], n: usize) -> Vec<Point2> {
    let k = seam(pts, centroid(pts).unwrap_or(pts[0]));
    let m = pts.len();
    let mut cum = Vec::with_capacity(m + 1);
    cum.push(0.0f64);
    for i in 0..m {
        let (a, b) = (pts[(k + i) % m], pts[(k + i + 1) % m]);
        cum.push(cum[i] + (b.x - a.x).hypot(b.y - a.y));
    }
    let len = cum[m];
    if !(len.is_finite() && len > 0.0) {
        return Vec::new();
    }
    (0..n)
        .map(|s| {
            let t = len * s as f64 / n as f64;
            let j = match cum.binary_search_by(|c| c.total_cmp(&t)) {
                Ok(j) => j.min(m - 1),
                Err(j) => j.saturating_sub(1).min(m - 1),
            };
            let (t0, t1) = (cum[j], cum[j + 1]);
            let u = if t1 > t0 {
                ((t - t0) / (t1 - t0)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let (a, b) = (pts[(k + j) % m], pts[(k + j + 1) % m]);
            Point2::new(a.x + (b.x - a.x) * u, a.y + (b.y - a.y) * u)
        })
        .collect()
}

/// Morph finishing (M2.5 D2): the waterline ladder, then one
/// column-grounded loft per paired level gap. The ring part is
/// `emit_rings`' ordinary walk - Morph's pattern check is neither
/// steep nor band - so a single-level ladder is the waterline path
/// byte for byte.
pub fn morph(facets: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    validate(p, facets)?;
    if !p.waterlines {
        return Err(GenError::Invalid(
            "Morph 3D finish lofts between waterline rings - enable waterlines or pick a shallow-first pattern".into(),
        ));
    }
    let r = p.tool_diameter * 0.5;
    let sups = build_sups(facets, r);
    let zs = levels(p, r, facets)?;
    let ds = dirs(r, p.tolerance);
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    let mut last = Some(p.clearance_z);
    let mut emitted = emit_rings(&mut b, p, r, facets, &zs, &ds, &mut last)?;
    // The loft family. Grounded like every march: the XY is the
    // interpolation's, the Z is the column's, and a sample under a
    // canopy is a trimmed note, never a quiet chord through air.
    let delta0 = scallop_stepdown(r, p.tolerance).max(1e-6);
    let trimmed = std::cell::Cell::new(0usize);
    let sup = |q: Point2| {
        let (c, cut) = plunge_support(q.x, q.y, r, p.bottom_z, p.clearance_z + r, &sups);
        if cut {
            trimmed.set(trimmed.get() + 1);
        }
        c
    };
    let mut runs: Vec<Vec<(Point2, f64)>> = Vec::new();
    let mut prev: Vec<Vec<Point2>> = Vec::new();
    for (gi, &z_tip) in zs.iter().enumerate() {
        let loops = level_loops(facets, r, z_tip, &ds, p.climb)?;
        if gi > 0 {
            for i in 0..prev.len().min(loops.len()) {
                let (a, c) = (&prev[i], &loops[i]);
                // Area rank paired; the centroid gate says whether
                // this rank is the same feature or two different
                // ones that happen to be first of their lists.
                let (Some(ca), Some(cb)) = (centroid(a), centroid(c)) else {
                    continue;
                };
                if (cb.x - ca.x).hypot(cb.y - ca.y) > 2.0 * r {
                    continue;
                }
                let n = a.len().max(c.len()).clamp(64, 4096);
                let (ra, rb) = (resample(a, n), resample(c, n));
                if ra.len() != n || rb.len() != n {
                    continue; // a degenerate level's loop lofts nothing
                }
                let poly: Vec<Point2> = ra
                    .iter()
                    .zip(&rb)
                    .map(|(pa, pb)| {
                        Point2::new((pa.x + pb.x) * 0.5, (pa.y + pb.y) * 0.5)
                    })
                    .collect();
                runs.extend(march_loop(
                    &poly,
                    &sup,
                    delta0,
                    p.tolerance,
                    p.tolerance * 0.25,
                    refinement_floor(delta0, p.min_step),
                ));
            }
        }
        prev = loops;
    }
    if trimmed.get() > 0 {
        b.note(format!(
            "morph: {} loft samples refused - their plunge column from clearance crosses the solid; what hides there stands as rest, reported not cut",
            trimmed.get()
        ));
    }
    if !runs.is_empty() {
        emit_runs(&mut b, p, r, &sups, &runs, true);
        emitted = true;
    }
    if !emitted {
        return Err(GenError::Empty(
            "no surface reaches within one ball radius of the requested depths".into(),
        ));
    }
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}
