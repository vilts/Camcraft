//! The cylindrical unwrap of the Rotary Flow kernel (#2824, the
//! first #2804 slice). Each picked guide chain, read about the STATED
//! axis (the #2772 seam - never fitted here), becomes one continuous
//! cylindrical track: the axial station `v`, the radius `rho`, and
//! the azimuth `th` UNWOUND against the previous point, so a
//! multi-turn helix reads 0, 90, ... 1080 degrees on, never a jump
//! back. The unwinder is the radian twin of the spindle C law's
//! `cl::rotary::unwrap_near` (which speaks degrees of a 360-turn,
//! issue #33); the law is one line either way, and each twin stays
//! with its own units.
//!
//! A pair of tracks is then ALIGNED by the nearest whole turn. The
//! unwrap makes each chain continuous on its own, but the branch the
//! second chain started on is arbitrary: the same two helices answer
//! for a flute's own width and for that width plus a full lap. The
//! band the user picked is the one under half a turn wide, so the
//! second chain is shifted by the whole turn that lands the matched
//! arc-length offset nearest zero - and the development (s =
//! `R_ref` * theta) reads the flute's own width.
//!
//! Provenance: cylindrical unwrapping of rotary toolpaths (developed
//! surfaces, iso-parametric flow lines) is published 4-axis CAM
//! practice; written from those sources and our own design notes -
//! no vendor material consulted.

use crate::finishing3d::closest_pt_tri;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::axis::Axis;

/// One full turn, in radians: the unwrap's period.
pub(super) const TAU: f64 = std::f64::consts::TAU;

/// World up, spelled once (Point3::new is not a const fn).
const UP: Point3 = Point3 {
    x: 0.0,
    y: 0.0,
    z: 1.0,
};

/// The azimuth basis about the stated axis: the axis.rs basis law,
/// verbatim (any vector off the axis, then the right-handed pair it
/// completes), spelled for a GIVEN axis rather than a fitted one.
pub(super) struct Basis {
    pub e1: Point3,
    pub e2: Point3,
}

/// The basis about `ax`; refused by name when the axis direction is
/// too degenerate to complete a frame (the caller already refused a
/// non-finite or non-unit one).
pub(super) fn basis(ax: &Axis) -> Result<Basis, GenError> {
    let u = ax.dir;
    let any = if u.z.abs() < 0.9 {
        UP
    } else {
        Point3::new(1.0, 0.0, 0.0)
    };
    let Some(e1) = super::linalg::unit(u.cross(any)) else {
        return Err(GenError::Invalid(
            "the stated axis is not a direction - no frame can be unwrapped about it".into(),
        ));
    };
    Ok(Basis { e1, e2: u.cross(e1) })
}

/// `c` moved by whole turns to the value nearest `prev` - the radian
/// twin of `cl::unwrap_near` (issue #33's degree law). Documented
/// against it; kept local because the cl twin speaks degrees.
pub(super) fn unwrap_near(c: f64, prev: f64) -> f64 {
    c + TAU * ((prev - c) / TAU).round()
}

/// One guide chain as a cylindrical track about the axis: each
/// point's azimuth `th` (CONTINUOUS - unwound against the previous
/// point), axial station `v`, radius `rho`, and its place `u` in the
/// chain's normalized chord length.
#[derive(Clone)]
pub(super) struct Unwrapped {
    pub th: Vec<f64>,
    pub v: Vec<f64>,
    pub rho: Vec<f64>,
    pub u: Vec<f64>,
}

/// The chain `pts` (validated finite, distinct and off the axis) as
/// one continuous track about the stated axis.
pub(super) fn unwrap(ax: &Axis, b: &Basis, pts: &[Point3]) -> Unwrapped {
    let mut th = Vec::with_capacity(pts.len());
    let mut v = Vec::with_capacity(pts.len());
    let mut rho = Vec::with_capacity(pts.len());
    let mut u = Vec::with_capacity(pts.len());
    let mut walk = 0.0f64;
    let mut total = 0.0f64;
    for w in pts.windows(2) {
        let d = w[0] - w[1];
        total += d.dot(d).sqrt();
    }
    let mut prev: Option<f64> = None;
    for (k, q) in pts.iter().enumerate() {
        if k > 0 {
            let d = *q - pts[k - 1];
            walk += d.dot(d).sqrt();
        }
        let d = *q - ax.origin;
        let s = d.dot(ax.dir);
        let w = d - Point3::new(ax.dir.x * s, ax.dir.y * s, ax.dir.z * s);
        let raw = w.dot(b.e2).atan2(w.dot(b.e1));
        let t = prev.map_or(raw, |p| unwrap_near(raw, p));
        prev = Some(t);
        th.push(t);
        v.push(s);
        rho.push(w.dot(w).sqrt());
        u.push(if total > 0.0 {
            walk / total
        } else {
            k as f64 / (pts.len().max(2) - 1) as f64
        });
    }
    Unwrapped { th, v, rho, u }
}

/// A linear read of the track `y` at the normalized place `x`,
/// between its own `knots` (ascending, in `[0, 1]`; empty tracks
/// never reach here - the caller validated two distinct points).
pub(super) fn lerp_at(knots: &[f64], y: &[f64], x: f64) -> f64 {
    let n = knots.len().min(y.len());
    if n == 0 {
        return 0.0;
    }
    if x <= knots[0] {
        return y[0];
    }
    for i in 1..n {
        if x <= knots[i] {
            let span = knots[i] - knots[i - 1];
            let f = if span > 0.0 {
                ((x - knots[i - 1]) / span).clamp(0.0, 1.0)
            } else {
                0.0
            };
            return y[i - 1] + (y[i] - y[i - 1]) * f;
        }
    }
    y[n - 1]
}

/// Align `b` to `a` by the nearest whole turn: the mean matched-place
/// offset decides the shift, and the band is the one under half a
/// turn wide. Returns the shift applied (a whole multiple of `TAU`).
pub(super) fn align_pair(a: &Unwrapped, b: &mut Unwrapped, samples: usize) -> f64 {
    let n = samples.max(2);
    let mut mean = 0.0f64;
    for i in 0..n {
        let x = i as f64 / (n - 1) as f64;
        mean += lerp_at(&b.u, &b.th, x) - lerp_at(&a.u, &a.th, x);
    }
    mean /= n as f64;
    let k = -(mean / TAU).round();
    if k != 0.0 {
        for th in &mut b.th {
            *th += k * TAU;
        }
    }
    k * TAU
}

/// The largest developed cross width between the aligned pair at
/// matched places: the offset vector `(R_ref * dtheta, dv)` whose
/// length the flow passes divide. `R_ref` is the band's outermost
/// radius, where a developed stepover reads widest - ground inside it
/// gets finer, never coarser.
///
/// The read runs over the union of both tracks' own knots and the
/// fixed samples: the offset is piecewise linear in the place, so its
/// largest length stands at a knot of one track or the other - a
/// fixed grid alone can step over the widest place of the band.
pub(super) fn cross_width(a: &Unwrapped, b: &Unwrapped, r_ref: f64, samples: usize) -> f64 {
    let n = samples.max(2);
    let fixed = (0..n).map(|i| i as f64 / (n - 1) as f64);
    let mut w = 0.0f64;
    for x in fixed.chain(a.u.iter().copied()).chain(b.u.iter().copied()) {
        let (dth, dv) = (
            lerp_at(&b.u, &b.th, x) - lerp_at(&a.u, &a.th, x),
            lerp_at(&b.u, &b.v, x) - lerp_at(&a.u, &a.v, x),
        );
        w = w.max((r_ref * dth).hypot(dv));
    }
    w
}

/// Do the two chains advance about the axis in OPPOSITE senses? Both
/// tracks must wind the same way for an iso-parametric blend to span
/// a band instead of folding it through itself. Tracks that barely
/// wind (under half a radian end to end) are exempt: their sense is
/// noise, and a near-axial band blends fine either way.
pub(super) fn opposite_sense(a: &Unwrapped, b: &Unwrapped) -> bool {
    let span = |t: &Unwrapped| match (t.th.first(), t.th.last()) {
        (Some(&f), Some(&l)) => l - f,
        _ => 0.0,
    };
    let (da, db) = (span(a), span(b));
    da.abs() > 0.5 && db.abs() > 0.5 && da * db < 0.0
}

/// The largest vertex radius of `facets` about the stated axis (0.0
/// for a mesh that sits on the line - the chain checks refuse such
/// picks before this matters).
pub(super) fn mesh_radius(ax: &Axis, facets: &[Triangle]) -> f64 {
    let mut rho = 0.0f64;
    for t in facets {
        for q in [t.a, t.b, t.c] {
            let d = q - ax.origin;
            let s = d.dot(ax.dir);
            let w = d - Point3::new(ax.dir.x * s, ax.dir.y * s, ax.dir.z * s);
            let rr = w.dot(w).sqrt();
            if rr.is_finite() {
                rho = rho.max(rr);
            }
        }
    }
    rho
}

/// The mesh's bounding diagonal - cheap and finite-safe (the
/// drive-curve walk's own law, curve.rs's own numbers).
pub(super) fn extent(facets: &[Triangle]) -> f64 {
    let mut d = 0.0f64;
    for f in facets {
        for (a, b) in [(f.a, f.b), (f.b, f.c), (f.c, f.a)] {
            let e = a - b;
            let l = e.dot(e);
            if l.is_finite() {
                d = d.max(l);
            }
        }
    }
    d.sqrt()
}

/// One chain as the walkable points it claims: finite, its
/// coincidental repeats collapsed. A closed lap is one legal guide
/// chain, and its closing point STAYS: the unwrap carries it on to
/// the first point's azimuth plus a whole turn, so the closing span
/// is walked like any other (dropping it lost a whole strip, #3630).
pub(super) fn clean_chain(ch: &[Point3], ci: usize, eps2: f64) -> Result<Vec<Point3>, GenError> {
    if !ch.iter().all(|q| q.is_finite()) {
        return Err(GenError::Invalid(format!(
            "Rotary Flow: guide chain {} carries a non-finite point",
            ci + 1
        )));
    }
    let mut pts: Vec<Point3> = Vec::with_capacity(ch.len());
    for q in ch {
        if pts.last().is_none_or(|l| {
            let d = *l - *q;
            d.dot(d) > eps2
        }) {
            pts.push(*q);
        }
    }
    if pts.len() < 2 {
        return Err(GenError::Invalid(format!(
            "Rotary Flow: guide chain {} stands on fewer than two distinct points - nothing to \
             walk",
            ci + 1
        )));
    }
    Ok(pts)
}

/// Does the point lie ON the model (within the corner-lattice
/// dust)? The box screen first - a facet whose bounds stand farther
/// away cannot own the point.
pub(super) fn on_model(q: Point3, facets: &[Triangle], boxes: &[[f64; 6]], eps2: f64) -> bool {
    for (i, b) in boxes.iter().enumerate() {
        let (dx, dy, dz) = (
            (b[0] - q.x).max(q.x - b[3]).max(0.0),
            (b[1] - q.y).max(q.y - b[4]).max(0.0),
            (b[2] - q.z).max(q.z - b[5]).max(0.0),
        );
        if dx * dx + dy * dy + dz * dz > eps2 {
            continue;
        }
        let f = &facets[i];
        let d = q - closest_pt_tri(q, &[f.a, f.b, f.c]);
        if d.dot(d) <= eps2 {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwrap_near_keeps_a_multi_turn_track_continuous() {
        // The radian twin of the spindle C law (cl::unwrap_near
        // speaks degrees): 0.75 turns unwound against -0.5 turns
        // reads -0.25 turns, never a jump back.
        let th = unwrap_near(0.75 * TAU, -0.5 * TAU);
        assert!((th - (-0.25 * TAU)).abs() < 1e-12, "{th}");
    }

    #[test]
    fn lerp_at_reads_between_its_own_knots() {
        let knots = [0.0, 0.4, 1.0];
        let y = [2.0, 4.0, 10.0];
        assert!((lerp_at(&knots, &y, 0.2) - 3.0).abs() < 1e-12);
        assert!((lerp_at(&knots, &y, 0.7) - 7.0).abs() < 1e-12);
        assert!((lerp_at(&knots, &y, -1.0) - 2.0).abs() < 1e-12);
        assert!((lerp_at(&knots, &y, 2.0) - 10.0).abs() < 1e-12);
    }
}
