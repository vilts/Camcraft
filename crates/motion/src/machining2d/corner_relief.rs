//! Corner relief (#2400): dogbone and T-bone inside-corner reliefs for
//! closed 2D pocket and slot boundaries, with explicit reachability
//! checks and spur emission.
//!
//! Provenance: clean-room implementation from elementary planar geometry
//! (moving cutter center to distance `r` along bisector for dogbone, or
//! tangent to one wall at distance `r` for T-bone, matching Vectric
//! VCarve / CloudCut router corner-relief semantics); no vendor source
//! or output consulted.

use crate::geo2d::{Loop, Point2, Region};

use super::face::GenError;
use super::waterscan::chord_crosses_path;

/// Minimum deflection angle (radians) for a corner to qualify as an inside corner.
const MIN_CORNER_RAD: f64 = 2.0_f64 * std::f64::consts::PI / 180.0;
/// Maximum distance (mm) from center boundary to vertex before relief is needed
/// (exceeds Clipper polygon-offset arc/miter chord sagitta).
const REACH_SLACK: f64 = 0.02;
/// Tolerance (mm) for matching user per-corner overrides against loop vertices.
const CORNER_MATCH_TOL: f64 = 0.1;
/// Tolerance (mm) on non-incident wall clearance checks along the relief spur.
const WALL_CLEAR_TOL: f64 = 1e-3;
/// Cosine threshold (35 deg) for collapsing micro-fillet tessellation chords.
const FILLET_CHORD_COS: f64 = 0.819_152_044_288_991_8;
/// Polygon vertices per full circle when building relief capsules for region union.
const CAPSULE_STEPS: usize = 64;

/// Global corner relief mode for a closed 2D pocket or square-ended slot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CornerReliefKind {
    #[default]
    None,
    Dogbone,
    TBone,
}

/// Rule selecting which adjacent wall a T-bone spur extends past.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TBoneDirection {
    /// Extend along the longer wall so the shorter adjacent wall takes the notch.
    #[default]
    LongEdge,
    /// Extend along the shorter wall so the longer adjacent wall takes the notch.
    ShortEdge,
    /// Extend predominantly horizontally (along X), notching vertical walls.
    Horizontal,
    /// Extend predominantly vertically (along Y), notching horizontal walls.
    Vertical,
}

/// Per-corner relief override choice matched by vertex XY.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CornerReliefChoice {
    None,
    Dogbone,
    TBoneFirst,
    TBoneSecond,
    TBoneX,
    TBoneY,
}

/// Explicit corner relief choice attached to a specific corner vertex XY.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CornerReliefOverride {
    pub at: Point2,
    pub choice: CornerReliefChoice,
}

/// Complete corner relief configuration for a pocket or slot operation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CornerRelief {
    pub kind: CornerReliefKind,
    pub tbone_direction: TBoneDirection,
    pub overrides: Vec<CornerReliefOverride>,
}

impl CornerRelief {
    #[must_use]
    pub fn is_off(&self) -> bool {
        self.kind == CornerReliefKind::None && self.overrides.is_empty()
    }

    pub fn validate(&self) -> Result<(), GenError> {
        for (i, ov) in self.overrides.iter().enumerate() {
            if !(ov.at.x.is_finite() && ov.at.y.is_finite()) {
                return Err(GenError::Invalid("corner relief override has non-finite coordinates".into()));
            }
            for other in &self.overrides[i + 1..] {
                if dist(ov.at, other.at) <= CORNER_MATCH_TOL * 0.5 {
                    return Err(GenError::Invalid(format!(
                        "duplicate corner relief override near ({:.3}, {:.3})",
                        ov.at.x, ov.at.y
                    )));
                }
            }
        }
        Ok(())
    }
}

/// One detected inside corner on a closed region boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InsideCorner {
    pub at: Point2,
    pub incoming_dir: Point2,
    pub outgoing_dir: Point2,
    pub bisector: Point2,
    pub edge_in_len: f64,
    pub edge_out_len: f64,
}

/// One planned corner relief spur in XY cutter-center coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedRelief {
    pub corner: Point2,
    pub start: Point2,
    pub end: Point2,
    pub radius: f64,
}

/// Validated corner relief plan and relieved machining region.
#[derive(Clone, Debug)]
pub struct CornerReliefPlan {
    pub reliefs: Vec<ResolvedRelief>,
    pub relieved_region: Region,
}

#[inline]
fn dist(a: Point2, b: Point2) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

#[inline]
fn norm(v: Point2) -> Option<(Point2, f64)> {
    let l = v.x.hypot(v.y);
    (l > 1e-9).then(|| (Point2::new(v.x / l, v.y / l), l))
}

fn closest_on_seg(p: Point2, a: Point2, b: Point2) -> (Point2, f64) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    let t = if dd <= 1e-18 { 0.0 } else { (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0) };
    let q = Point2::new(a.x + t * dx, a.y + t * dy);
    (q, dist(p, q))
}

fn seg_seg_dist(a0: Point2, a1: Point2, b0: Point2, b1: Point2) -> f64 {
    if chord_crosses_path(a0, a1, &[b0, b1], false) {
        return 0.0;
    }
    closest_on_seg(a0, b0, b1).1
        .min(closest_on_seg(a1, b0, b1).1)
        .min(closest_on_seg(b0, a0, a1).1)
        .min(closest_on_seg(b1, a0, a1).1)
}

/// Extract clean polygon rings (Region::new keeps CCW outer / CW hole so interior is left).
fn clean_loops(region: &Region) -> Vec<Vec<Point2>> {
    let mut out = Vec::with_capacity(region.loops().len());
    for lp in region.loops() {
        let mut dedup: Vec<Point2> = Vec::with_capacity(lp.points().len());
        for &p in lp.points() {
            if dedup.last().is_none_or(|&q| dist(p, q) > 1e-6) {
                dedup.push(p);
            }
        }
        while dedup.len() >= 3 && dist(dedup[0], dedup[dedup.len() - 1]) <= 1e-6 {
            dedup.pop();
        }
        let mut changed = true;
        while changed && dedup.len() >= 3 {
            changed = false;
            let n = dedup.len();
            let mut merged = Vec::with_capacity(n);
            for i in 0..n {
                let (a, b, c) = (dedup[(i + n - 1) % n], dedup[i], dedup[(i + 1) % n]);
                let (Some((u, _)), Some((w, _))) = (norm(Point2::new(b.x - a.x, b.y - a.y)), norm(Point2::new(c.x - b.x, c.y - b.y))) else {
                    changed = true;
                    continue;
                };
                if (u.x * w.y - u.y * w.x).abs() <= 1e-9 && u.x * w.x + u.y * w.y > 0.0 {
                    changed = true;
                } else { merged.push(b); }
            }
            dedup = merged;
        }
        if dedup.len() >= 3 {
            out.push(dedup);
        }
    }
    out
}

fn corner_at(pts: &[Point2], vi: usize) -> Option<InsideCorner> {
    let n = pts.len();
    let (a, v, b) = (pts[(vi + n - 1) % n], pts[vi], pts[(vi + 1) % n]);
    let (u, l_in) = norm(Point2::new(v.x - a.x, v.y - a.y))?;
    let (w, l_out) = norm(Point2::new(b.x - v.x, b.y - v.y))?;
    let cross = u.x * w.y - u.y * w.x;
    let dot = u.x * w.x + u.y * w.y;
    if cross <= 0.0 || cross.atan2(dot) < MIN_CORNER_RAD {
        return None;
    }
    let (bisector, _) = norm(Point2::new(-u.y - w.y, u.x + w.x))?;
    Some(InsideCorner { at: v, incoming_dir: u, outgoing_dir: w, bisector, edge_in_len: l_in, edge_out_len: l_out })
}

/// Enumerate inside corners of `region` (where interior pocket angle < 180 deg).
#[must_use]
pub fn inside_corners(region: &Region) -> Vec<InsideCorner> {
    clean_loops(region)
        .iter()
        .flat_map(|pts| (0..pts.len()).filter_map(move |vi| corner_at(pts, vi)))
        .collect()
}

fn nearest_on_center(center: &Region, target: Point2) -> Option<(Point2, f64)> {
    let mut best: Option<(Point2, f64)> = None;
    for lp in center.loops() {
        let pts = lp.points();
        let n = pts.len();
        for i in 0..n {
            let (q, d) = closest_on_seg(target, pts[i], pts[(i + 1) % n]);
            if best.is_none_or(|(_, bd)| d < bd) { best = Some((q, d)); }
        }
    }
    best
}

fn choose_direction(corner: &InsideCorner, spec: &CornerRelief, choice_ov: Option<CornerReliefChoice>) -> Option<Point2> {
    let choice = match choice_ov {
        Some(ov) => ov,
        None => match spec.kind {
            CornerReliefKind::None => CornerReliefChoice::None,
            CornerReliefKind::Dogbone => CornerReliefChoice::Dogbone,
            CornerReliefKind::TBone => match spec.tbone_direction {
                TBoneDirection::LongEdge => if corner.edge_in_len >= corner.edge_out_len { CornerReliefChoice::TBoneFirst } else { CornerReliefChoice::TBoneSecond },
                TBoneDirection::ShortEdge => if corner.edge_in_len <= corner.edge_out_len { CornerReliefChoice::TBoneFirst } else { CornerReliefChoice::TBoneSecond },
                TBoneDirection::Horizontal => CornerReliefChoice::TBoneX,
                TBoneDirection::Vertical => CornerReliefChoice::TBoneY,
            },
        },
    };
    let n_in = Point2::new(-corner.incoming_dir.y, corner.incoming_dir.x);
    let n_out = Point2::new(-corner.outgoing_dir.y, corner.outgoing_dir.x);
    match choice {
        CornerReliefChoice::None => None,
        CornerReliefChoice::Dogbone => Some(corner.bisector),
        CornerReliefChoice::TBoneFirst => Some(n_in),
        CornerReliefChoice::TBoneSecond => Some(n_out),
        CornerReliefChoice::TBoneX => Some(if corner.incoming_dir.x.abs() >= corner.outgoing_dir.x.abs() { n_in } else { n_out }),
        CornerReliefChoice::TBoneY => Some(if corner.incoming_dir.y.abs() >= corner.outgoing_dir.y.abs() { n_in } else { n_out }),
    }
}

fn is_fillet_step(pts: &[Point2], ei: usize, short_lim: f64) -> bool {
    let n = pts.len();
    if dist(pts[ei], pts[(ei + 1) % n]) > short_lim {
        return false;
    }
    let prev = (ei + n - 1) % n;
    let next = (ei + 1) % n;
    let (Some((u, _)), Some((w, _))) = (
        norm(Point2::new(pts[ei].x - pts[prev].x, pts[ei].y - pts[prev].y)),
        norm(Point2::new(pts[next].x - pts[ei].x, pts[next].y - pts[ei].y)),
    ) else {
        return false;
    };
    u.x * w.y - u.y * w.x > 0.0 && u.x * w.x + u.y * w.y >= FILLET_CHORD_COS
}

fn incident_edge_span(pts: &[Point2], vi: usize, off: f64) -> (usize, usize) {
    let n = pts.len();
    let short_lim = 0.35 * off;
    let mut back = (vi + n - 1) % n;
    for _ in 0..n.min(8) {
        if !is_fillet_step(pts, back, short_lim) {
            break;
        }
        back = (back + n - 1) % n;
    }
    let mut front = vi;
    for _ in 0..n.min(8) {
        if !is_fillet_step(pts, front, short_lim) {
            break;
        }
        front = (front + 1) % n;
    }
    (back, front)
}

fn edge_in_span(ei: usize, back: usize, front: usize, n: usize) -> bool {
    let mut cur = back;
    for _ in 0..=n {
        if cur == ei { return true; }
        if cur == front { return false; }
        cur = (cur + 1) % n;
    }
    false
}

/// Build a circumscribed CCW polygon enclosing the swept capsule `start..end` of radius `r`.
pub fn capsule_region(start: Point2, end: Point2, r: f64) -> Result<Region, GenError> {
    let poly_r = r / (std::f64::consts::PI / CAPSULE_STEPS as f64).cos();
    let (dx, dy) = (end.x - start.x, end.y - start.y);
    let mut pts = Vec::with_capacity(CAPSULE_STEPS + 2);
    if dx.hypot(dy) <= 1e-6 {
        for k in 0..CAPSULE_STEPS {
            let th = k as f64 * std::f64::consts::TAU / CAPSULE_STEPS as f64;
            pts.push(Point2::new(start.x + poly_r * th.cos(), start.y + poly_r * th.sin()));
        }
    } else {
        let base_ang = dy.atan2(dx);
        let half = CAPSULE_STEPS / 2;
        for k in 0..=half {
            let th = base_ang + std::f64::consts::FRAC_PI_2 + (k as f64 / half as f64) * std::f64::consts::PI;
            pts.push(Point2::new(start.x + poly_r * th.cos(), start.y + poly_r * th.sin()));
        }
        for k in 0..=half {
            let th = base_ang - std::f64::consts::FRAC_PI_2 + (k as f64 / half as f64) * std::f64::consts::PI;
            pts.push(Point2::new(end.x + poly_r * th.cos(), end.y + poly_r * th.sin()));
        }
    }
    Ok(Region::new(vec![Loop::new(pts)?])?)
}

/// Plan and validate corner reliefs for `region` given offset `off = tool_radius + stock_to_leave`.
pub fn plan_corner_reliefs(region: &Region, center: &Region, off: f64, spec: &CornerRelief) -> Result<CornerReliefPlan, GenError> {
    spec.validate()?;
    if spec.is_off() {
        return Ok(CornerReliefPlan { reliefs: Vec::new(), relieved_region: region.clone() });
    }
    if !(off.is_finite() && off > 0.0) {
        return Err(GenError::Invalid("corner relief needs a positive cutter radius plus stock to leave".into()));
    }
    let loops = clean_loops(region);
    let mut matched = vec![false; spec.overrides.len()];
    let mut reliefs: Vec<ResolvedRelief> = Vec::new();
    for (li, pts) in loops.iter().enumerate() {
        for vi in 0..pts.len() {
            let Some(corner) = corner_at(pts, vi) else { continue };
            let mut ov_choice = None;
            for (oi, ov) in spec.overrides.iter().enumerate() {
                if dist(ov.at, corner.at) <= CORNER_MATCH_TOL {
                    matched[oi] = true;
                    ov_choice = Some(ov.choice);
                    break;
                }
            }
            let Some(dir) = choose_direction(&corner, spec, ov_choice) else { continue };
            let Some((start, d_v)) = nearest_on_center(center, corner.at) else {
                return Err(GenError::Invalid(format!(
                    "corner relief cannot reach inside corner at ({:.3}, {:.3}): pocket is narrower than the tool",
                    corner.at.x, corner.at.y
                )));
            };
            if ov_choice.is_none() && (d_v <= off + REACH_SLACK || reliefs.iter().any(|r| dist(corner.at, r.end) <= off + REACH_SLACK)) {
                continue;
            }
            let end = Point2::new(corner.at.x + off * dir.x, corner.at.y + off * dir.y);
            let (span_back, span_front) = incident_edge_span(pts, vi, off);
            for (li2, pts2) in loops.iter().enumerate() {
                let n2 = pts2.len();
                for ei in 0..n2 {
                    if li2 == li && edge_in_span(ei, span_back, span_front, n2) {
                        continue;
                    }
                    if seg_seg_dist(start, end, pts2[ei], pts2[(ei + 1) % n2]) < off - WALL_CLEAR_TOL {
                        return Err(GenError::Invalid(format!(
                            "corner relief cannot reach inside corner at ({:.3}, {:.3}): blocked by pocket wall or narrow feature",
                            corner.at.x, corner.at.y
                        )));
                    }
                }
            }
            reliefs.push(ResolvedRelief { corner: corner.at, start, end, radius: off });
        }
    }
    for (oi, &ok) in matched.iter().enumerate() {
        if !ok {
            let at = spec.overrides[oi].at;
            return Err(GenError::Invalid(format!(
                "corner relief override at ({:.3}, {:.3}) does not match any inside corner of the boundary",
                at.x, at.y
            )));
        }
    }
    let mut relieved = region.clone();
    for r in &reliefs {
        relieved = relieved.union(&capsule_region(r.start, r.end, r.radius)?)?;
    }
    Ok(CornerReliefPlan { reliefs, relieved_region: relieved })
}
