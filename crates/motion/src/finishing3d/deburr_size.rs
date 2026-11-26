//! Deburr break sizing (#1890): the chord width or the depth of a
//! break, read as the cone's setback on each edge, and the pass ladder.
//!
//! Provenance: plane geometry of a symmetric chamfer on a wedge of
//! included angle `theta` with setback `s` on each face - chord
//! `w = 2 s sin(theta / 2)`, depth `d = s cos(theta / 2)` from the edge
//! to the chord (machine-tool handbook geometry). Our own model; no
//! vendor material consulted.

use crate::cl::{MoveKind, PathBuilder};
use crate::machining2d::GenError;
use crate::geometry::Point3;

use super::crease::{add3, scale3, sub3};
use super::deburr::DeburrParams;

/// What the authored size holds the same on every edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BreakMode {
    /// The chord `w` across the break is the same on every edge.
    #[default]
    ConstantWidth,
    /// The depth `d` from the edge to the chord is the same on every edge.
    ConstantDepth,
}

/// The cross-section the break leaves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BreakProfile {
    /// A flat bevel: the sharp cone's own cut.
    #[default]
    Chamfer,
    /// A rounded edge: needs a ball-tipped cutter, refused on a sharp cone.
    Round,
}

/// The smallest wedge `ConstantWidth` sizes, degrees: a sharper wedge
/// would need a setback of more than about twice the chord.
pub const MIN_WIDTH_WEDGE_DEG: f64 = 30.0;
/// The largest wedge `ConstantDepth` sizes, degrees: a flatter wedge
/// would need a setback of more than about four times the depth.
pub const MAX_DEPTH_WEDGE_DEG: f64 = 150.0;
/// The wedge a rim (an open mesh's edge, one face) is sized as: the
/// 90-degree corner each earlier file's width was measured on.
pub const RIM_WEDGE_DEG: f64 = 90.0;
/// The most passes one break may take.
pub const MAX_PASSES: u32 = 100;

/// How the break is sized, shaped and stepped (#1890). The chord width
/// itself stays on the operation (`width`); this holds the rest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeburrBreak {
    pub mode: BreakMode,
    /// The break depth, mm, that `ConstantDepth` holds on every edge.
    pub depth: f64,
    pub profile: BreakProfile,
    /// Passes from the outer (shallow) break in to the final one, >= 1.
    /// Pass `k` of `n` cuts the setback `s * k / n`.
    pub passes: u32,
}

impl Default for DeburrBreak {
    /// One chamfer pass of constant width - each earlier file's ride.
    /// The depth matches the default 0.5 mm chord on a 90-degree edge.
    fn default() -> Self {
        Self { mode: BreakMode::ConstantWidth, depth: 0.25, profile: BreakProfile::Chamfer, passes: 1 }
    }
}

impl DeburrBreak {
    /// The most passes one break may take (the UI's range).
    pub const MAX_PASSES: u32 = MAX_PASSES;

    /// Finite positive depth and a pass count in `1..=MAX_PASSES`.
    /// Each mode and profile is legal here; a Round profile is refused
    /// by the tool it meets, not by the numbers.
    #[must_use]
    pub fn is_sane(&self) -> bool {
        self.depth.is_finite() && self.depth > 0.0 && (1..=MAX_PASSES).contains(&self.passes)
    }

    /// The setback `s` on each face of a wedge of `wedge_deg` degrees, for
    /// the chord `width`: `w / (2 sin(theta / 2))` or `d / cos(theta / 2)`.
    /// `None` when the wedge is outside the mode's range (see the bounds).
    #[must_use]
    pub fn setback(&self, width: f64, wedge_deg: f64) -> Option<f64> {
        if !wedge_deg.is_finite() || wedge_deg <= 0.0 || wedge_deg >= 180.0 {
            return None;
        }
        let half = 0.5 * wedge_deg.to_radians();
        let s = match self.mode {
            BreakMode::ConstantWidth if wedge_deg >= MIN_WIDTH_WEDGE_DEG => width / (2.0 * half.sin()),
            BreakMode::ConstantDepth if wedge_deg <= MAX_DEPTH_WEDGE_DEG => self.depth / half.cos(),
            _ => return None,
        };
        (s.is_finite() && s > 0.0).then_some(s)
    }

    /// The largest setback any edge can take: the bound a declared break
    /// record is held to (`stock_sim_breaks`). NaN when the size is not sane.
    #[must_use]
    pub fn max_setback(&self, width: f64) -> f64 {
        let wedge = match self.mode {
            BreakMode::ConstantWidth => MIN_WIDTH_WEDGE_DEG,
            BreakMode::ConstantDepth => MAX_DEPTH_WEDGE_DEG,
        };
        self.setback(width, wedge).unwrap_or(f64::NAN)
    }

    /// The setbacks of the passes for a final setback `s`, outer to
    /// final: `s * k / n` for `k = 1..=n`. The last one is `s` exactly.
    pub fn pass_setbacks(&self, s: f64) -> impl Iterator<Item = f64> {
        let n = self.passes.max(1);
        (1..=n).map(move |k| if k == n { s } else { s * f64::from(k) / f64::from(n) })
    }
}

/// Ride each run (its edge points and its final drop) once for each
/// pass, outer to final: pass `k` of `n` stands the tip `drop * k / n`
/// under the edge, so each pass lies above the final break and inside
/// the `EdgeBreak` that the run declares.
pub(super) fn emit_runs(b: &mut PathBuilder, runs: &[(Vec<Point3>, f64)], p: &DeburrParams) -> Result<(), GenError> {
    // Validate every pass before emitting any travel. Final-drop clipping
    // alone does not keep the earlier, shallower passes below the ceiling.
    for (run, drop) in runs {
        for dk in p.sizing.pass_setbacks(*drop) {
            if run.iter().any(|q| q.z - dk < p.bottom_z - 1e-9 || q.z - dk > p.top_z + 1e-9) {
                return Err(GenError::Invalid(
                    "deburr: a pass leaves the tip window - widen Window Top Z and Window Bottom Z or use fewer passes".into(),
                ));
            }
        }
    }
    for (run, drop) in runs {
        let Some(&first) = run.first() else { continue };
        for dk in p.sizing.pass_setbacks(*drop) {
            // Leave the previous cut straight up before travelling
            // (#304): the cone ends a run still in the edge it broke, so
            // a diagonal rapid from there would drag it up and across
            // the part.
            retract(b, p.clearance_z);
            b.push(MoveKind::Rapid, Point3::new(first.x, first.y, p.clearance_z), None);
            let tip = |q: Point3| Point3::new(q.x, q.y, q.z - dk);
            b.push(MoveKind::Plunge, tip(first), Some(p.plunge_feed));
            for &pt in &run[1..] {
                b.push(MoveKind::Feed, tip(pt), Some(p.feed));
            }
        }
    }
    retract(b, p.clearance_z);
    Ok(())
}

/// Leave the cut vertically to the clearance plane before any traverse.
/// The window admits a numerical skin: normalize even an ulp above the
/// plane vertically, so the following XY rapid never descends across the
/// part. Meaningful overshoot is refused before emission.
fn retract(b: &mut PathBuilder, clearance_z: f64) {
    let cur = b.at();
    if cur.z != clearance_z {
        b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, clearance_z), None);
    }
}

/// Does the edge climb faster than the cone's flank (see `deburr`)?
/// A vertical edge always does.
pub(super) fn steep(q0: Point3, q1: Point3, slope: f64) -> bool {
    let plan = ((q1.x - q0.x).powi(2) + (q1.y - q0.y).powi(2)).sqrt();
    (q1.z - q0.z).abs() > slope * plan + 1e-9
}

/// Keep the stretch whose TIP (z minus the drop) stands inside the
/// programmed window; None when the whole segment misses it. The
/// clip rides the segment's own line, so a rim leaving the floor is
/// cut where the floor is, not dropped whole.
pub(crate) fn clip(p0: Point3, p1: Point3, p: &DeburrParams, drop: f64) -> Option<(Point3, Point3)> {
    let (lo, hi) = (p.bottom_z + drop, p.top_z + drop);
    let dz = p1.z - p0.z;
    if dz.abs() < 1e-15 * (p0.z.abs() + p1.z.abs() + 1.0) {
        return (p0.z >= lo - 1e-9 && p0.z <= hi + 1e-9).then_some((p0, p1));
    }
    let (mut t0, mut t1) = ((lo - p0.z) / dz, (hi - p0.z) / dz);
    if t0 > t1 {
        std::mem::swap(&mut t0, &mut t1);
    }
    let t0 = t0.clamp(0.0, 1.0);
    let t1 = t1.clamp(0.0, 1.0);
    if t1 <= t0 {
        return None;
    }
    let d = sub3(p1, p0);
    let at = |t: f64| add3(p0, scale3(d, t));
    Some((at(t0), at(t1)))
}
