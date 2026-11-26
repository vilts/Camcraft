//! One flow pass of the Rotary Flow kernel (#2824): the
//! iso-parametric curve between an ALIGNED pair of unwrapped guide
//! chains. At each normalized arc-length place the pass blends the
//! pair's cylindrical coordinates (theta, v, rho) - the blend the
//! unwrap exists for: a straight blend of two helices in the
//! development is a helix on the flank, while a straight blend of
//! their 3D points is a chord that cuts the flank - pushes the
//! station's ball center out along its radial until every facet
//! keeps one ball radius of clear (the `closest_pt_tri` oracle, so
//! steep helical flanks never gouge), and chords the pushed-out
//! curve within the tolerance.
//!
//! The push-out MARCHES before it bisects: the clearance along a
//! radial is monotone on a star-shaped band, but a neighbouring
//! flute can overhang, so the first clear radius the march finds
//! wins and the bisection only refines inside its step. A station
//! the ball cannot clear within five radii of room is refused by
//! name, at the station's own spot - never posted.
//!
//! Provenance: iso-parametric flow-line machining on developed
//! cylindrical surfaces and gouge-free radial ball offsets are
//! published 4-axis CAM practice; the closest-point oracle is the
//! codebase's own finishing law. No vendor material consulted.

use crate::finishing3d::closest_pt_tri;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::axis::Axis;
use super::rotary_flow_guard::seg_clearance;
use super::rotary_flow_unwrap::{lerp_at, Basis, Unwrapped};

/// The oracle's float allowance - the finishing kinds' own TOUCH_EPS.
pub(super) const TOUCH: f64 = 1e-7;
/// The farthest a station may be pushed out along its radial, in
/// ball radii, before the pass is refused.
pub(super) const PUSH_CAP: f64 = 5.0;
/// Bisection depth of the chord walk: 2^12 spans inside one knot
/// span is beyond any honest tessellation; past it the span is
/// accepted and said in the notes, never silently.
const CHORD_DEPTH: u32 = 12;
/// Most chord stations one pass may state.
pub(super) const CHORD_CAP: usize = 16384;

/// One pass's answer: the chorded ball centers, and whether the walk
/// had to accept a span at the depth cap.
pub(super) struct PassOut {
    pub centers: Vec<Point3>,
    pub depth_capped: bool,
}

/// The world point of a cylindrical station about the axis.
pub(super) fn station(ax: &Axis, b: &Basis, th: f64, v: f64, rho: f64) -> Point3 {
    let (s, c) = th.sin_cos();
    let rad = Point3::new(
        b.e1.x * c + b.e2.x * s,
        b.e1.y * c + b.e2.y * s,
        b.e1.z * c + b.e2.z * s,
    );
    Point3::new(
        ax.origin.x + rad.x * rho + ax.dir.x * v,
        ax.origin.y + rad.y * rho + ax.dir.y * v,
        ax.origin.z + rad.z * rho + ax.dir.z * v,
    )
}

/// The (station, azimuth, radius) of a world point about the axis;
/// the radius is `None` on the axis line itself.
pub(super) fn cyl_of(ax: &Axis, b: &Basis, q: Point3) -> (f64, f64, Option<f64>) {
    let d = q - ax.origin;
    let v = d.dot(ax.dir);
    let w = d - Point3::new(ax.dir.x * v, ax.dir.y * v, ax.dir.z * v);
    let rho = w.dot(w).sqrt();
    (
        v,
        w.dot(b.e2).atan2(w.dot(b.e1)),
        (rho > 1e-12).then_some(rho),
    )
}

/// Each facet's axis-aligned box: the cheap reach screen of the
/// drive-curve walk (curve.rs's own law), spelled for the push-out's
/// swept corridor.
pub(super) fn boxes_of(facets: &[Triangle]) -> Vec<[f64; 6]> {
    facets
        .iter()
        .map(|f| {
            [
                f.a.x.min(f.b.x.min(f.c.x)),
                f.a.y.min(f.b.y.min(f.c.y)),
                f.a.z.min(f.b.z.min(f.c.z)),
                f.a.x.max(f.b.x.max(f.c.x)),
                f.a.y.max(f.b.y.max(f.c.y)),
                f.a.z.max(f.b.z.max(f.c.z)),
            ]
        })
        .collect()
}

/// The facet indices whose boxes come within `reach` of the corridor
/// between world points `lo` and `hi` (the ball's whole push range).
fn near(boxes: &[[f64; 6]], lo: Point3, hi: Point3, reach: f64) -> Vec<usize> {
    let (x0, x1) = (lo.x.min(hi.x) - reach, lo.x.max(hi.x) + reach);
    let (y0, y1) = (lo.y.min(hi.y) - reach, lo.y.max(hi.y) + reach);
    let (z0, z1) = (lo.z.min(hi.z) - reach, lo.z.max(hi.z) + reach);
    boxes
        .iter()
        .enumerate()
        .filter(|(_, b)| x1 >= b[0] && x0 <= b[3] && y1 >= b[1] && y0 <= b[4] && z1 >= b[2] && z0 <= b[5])
        .map(|(i, _)| i)
        .collect()
}

/// The smallest distance from `q` to the mesh over the screened set
/// `idx` (infinite in empty air - an air station is clear by default).
fn mesh_dist(q: Point3, facets: &[Triangle], idx: &[usize]) -> f64 {
    idx.iter()
        .filter_map(|&i| facets.get(i))
        .map(|f| {
            let d = q - closest_pt_tri(q, &[f.a, f.b, f.c]);
            d.dot(d).sqrt()
        })
        .fold(f64::INFINITY, f64::min)
}

/// Why a station has no ball center.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PushFail {
    /// No radius within [`PUSH_CAP`] radii of the blended one clears.
    Blocked,
    /// The radius cannot advance: the coordinates are so large that a
    /// march step is lost in the float spacing.
    Precision,
}

/// The ball center at station `(th, v, rho)` pushed out along its
/// radial until every facet keeps `r + margin` around it (the margin
/// is the chord allowance that the swept segments between stations
/// may spend). An error when no radius within [`PUSH_CAP`] radii of
/// the blended one clears, or when the radius cannot advance - the
/// caller refuses by name at the station's own spot.
#[allow(clippy::too_many_arguments)] // one push-out's flat read
pub(super) fn ball_center(
    ax: &Axis,
    b: &Basis,
    facets: &[Triangle],
    boxes: &[[f64; 6]],
    th: f64,
    v: f64,
    rho: f64,
    r: f64,
    margin: f64,
) -> Result<Point3, PushFail> {
    let need = r + margin;
    // The march: quarter-radius steps to the cap, a fixed count. Each
    // step must move the radius, or the march stops by name - a step
    // lost in the float spacing would repeat forever.
    const STEPS: usize = (PUSH_CAP * 4.0) as usize;
    let step = 0.25 * r;
    let cap = rho + PUSH_CAP * r;
    if !(cap.is_finite() && cap > rho && rho + step > rho) {
        return Err(PushFail::Precision);
    }
    // The screen is the ball's whole push corridor: any facet that
    // can come within `need` of a center on it has a box within
    // `need` of the corridor's bounds.
    let idx = near(
        boxes,
        station(ax, b, th, v, rho),
        station(ax, b, th, v, cap),
        need + 1e-6,
    );
    let clear = |rr: f64| -> bool {
        let q = station(ax, b, th, v, rr);
        mesh_dist(q, facets, &idx) >= need - TOUCH
    };
    if clear(rho) {
        return Ok(station(ax, b, th, v, rho));
    }
    // The FIRST clear radius wins, then the bisection refines inside
    // its step.
    let mut lo = rho;
    let mut hi = None;
    for _ in 0..STEPS {
        let x = (lo + step).min(cap);
        if x.is_nan() || x <= lo {
            return Err(PushFail::Precision);
        }
        if clear(x) {
            hi = Some(x);
            break;
        }
        lo = x;
        if lo >= cap {
            break;
        }
    }
    let Some(mut hi) = hi else {
        return Err(PushFail::Blocked);
    };
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        if clear(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Ok(station(ax, b, th, v, hi))
}

/// The blended cylindrical coordinates at cross fraction `t` and
/// normalized place `x` between the aligned pair.
fn blend_track(a: &Unwrapped, b: &Unwrapped, t: f64, x: f64) -> (f64, f64, f64) {
    let one = |ya: &[f64], ua: &[f64], yb: &[f64], ub: &[f64]| -> f64 {
        let va = lerp_at(ua, ya, x);
        let vb = lerp_at(ub, yb, x);
        va + (vb - va) * t
    };
    (
        one(&a.th, &a.u, &b.th, &b.u),
        one(&a.v, &a.u, &b.v, &b.u),
        one(&a.rho, &a.u, &b.rho, &b.u),
    )
}
/// The pass at cross fraction `t` between the ALIGNED pair `(a, b)`:
/// iso-parametric stations blended in the development, each pushed
/// out until the oracle clears, chorded so consecutive centers stay
/// within `tol` of the curve between them. The union of the two
/// chains' arc-length knots seeds the walk, so every blend kink is a
/// chord endpoint. Refuses, by name and at the station's own spot, a
/// station the push-out cannot clear, and a curve that outruns the
/// chord cap.
#[allow(clippy::too_many_arguments)] // one pass's flat read
pub(super) fn flow_pass(
    ax: &Axis,
    basis: &Basis,
    facets: &[Triangle],
    boxes: &[[f64; 6]],
    a: &Unwrapped,
    b: &Unwrapped,
    t: f64,
    r: f64,
    tol: f64,
) -> Result<PassOut, GenError> {
    let at = |x: f64| -> Result<Point3, GenError> {
        let (th, v, rho) = blend_track(a, b, t, x);
        ball_center(ax, basis, facets, boxes, th, v, rho, r, tol).map_err(|why| {
            let msg = match why {
                PushFail::Blocked => format!(
                    "Rotary Flow: the ball cannot clear the part within {PUSH_CAP} radii of the \
                     flow station at cross {t:.2} - the band is narrower than the tool there; pick \
                     flow lines on exposed ground, or use a smaller ball"
                ),
                PushFail::Precision => format!(
                    "Rotary Flow: the flow station at cross {t:.2} stands so far from the axis \
                     that a {:.3} mm push-out step is lost in the float spacing - move the work \
                     and the axis near the origin",
                    0.25 * r
                ),
            };
            GenError::invalid_at(msg, station(ax, basis, th, v, rho))
        })
    };
    // The knot union: every place either chain states a vertex.
    let mut knots: Vec<f64> = a.u.iter().chain(b.u.iter()).copied().collect();
    knots.sort_by(|x, y| x.total_cmp(y));
    knots.dedup_by(|x, y| (*x - *y).abs() < 1e-12);
    let mut first = Vec::with_capacity(knots.len());
    for &x in &knots {
        first.push(at(x)?);
    }
    let Some(&p0) = first.first() else {
        return Err(GenError::Empty(
            "Rotary Flow: a guide pair left no arc-length knot to walk".into(),
        ));
    };
    let walk = Walk {
        at: &at,
        facets,
        boxes,
        r,
        tol,
    };
    let mut out = PassOut {
        centers: vec![p0],
        depth_capped: false,
    };
    for span in 0..knots.len() - 1 {
        walk.flatten(knots[span], first[span], knots[span + 1], first[span + 1], 0, &mut out)?;
    }
    Ok(out)
}

/// One pass's chord walk: the station oracle and the swept-ball law.
struct Walk<'a> {
    at: &'a dyn Fn(f64) -> Result<Point3, GenError>,
    facets: &'a [Triangle],
    boxes: &'a [[f64; 6]],
    r: f64,
    tol: f64,
}

impl Walk<'_> {
    /// The swept-ball clearance of the chord `p0 -> p1`.
    fn sweep(&self, p0: Point3, p1: Point3) -> f64 {
        seg_clearance(p0, p1, self.facets, self.boxes, self.r + self.tol + 1e-6)
    }

    /// Walk one span: accept its endpoint when the midpoint station
    /// sits within `tol` of the chord AND the chord's swept ball keeps
    /// `r + tol / 2` off the part, else bisect. At the depth cap a
    /// span whose sweep still clears the bare ball is accepted and
    /// flagged (said in the notes); a span that does not is refused
    /// at its own spot - never posted.
    fn flatten(
        &self,
        x0: f64,
        p0: Point3,
        x1: f64,
        p1: Point3,
        depth: u32,
        out: &mut PassOut,
    ) -> Result<(), GenError> {
        if out.centers.len() >= CHORD_CAP {
            return Err(GenError::Invalid(format!(
                "Rotary Flow: a flow pass needs more than {CHORD_CAP} chord stations - the picks \
                 or the tolerance are wrong for this band"
            )));
        }
        if depth >= CHORD_DEPTH {
            let got = self.sweep(p0, p1);
            if got.is_nan() || got < self.r - TOUCH {
                return Err(GenError::invalid_at(
                    format!(
                        "Rotary Flow: a flow chord sweeps the ball {:.4} mm off the part, inside \
                         its {:.3} mm radius, at the chord depth cap - the band is narrower than \
                         the tool there; use a smaller ball",
                        got.max(0.0),
                        self.r
                    ),
                    p1,
                ));
            }
            out.depth_capped = true;
            out.centers.push(p1);
            return Ok(());
        }
        let xm = 0.5 * (x0 + x1);
        let pm = (self.at)(xm)?;
        let mid = Point3::new(
            0.5 * (p0.x + p1.x),
            0.5 * (p0.y + p1.y),
            0.5 * (p0.z + p1.z),
        );
        let d = pm - mid;
        if d.dot(d).sqrt() <= self.tol && self.sweep(p0, p1) >= self.r + 0.5 * self.tol {
            out.centers.push(p1);
            return Ok(());
        }
        self.flatten(x0, p0, xm, pm, depth + 1, out)?;
        self.flatten(xm, pm, x1, p1, depth + 1, out)
    }
}
