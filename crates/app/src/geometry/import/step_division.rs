//! The parameter grid a B-spline or NURBS STEP face is meshed on, split in
//! each direction only where that direction needs it (#1602).
//!
//! Provenance: adaptive tensor-product subdivision with a flatness test per
//! parameter direction, as in Piegl & Tiller, "The NURBS Book" (2nd ed.,
//! ch. 5 and the tessellation notes of ch. 12: a patch is flat in one
//! direction when its iso-curves in that direction stand within the
//! tolerance of their chords), and truck's published `parameter_division`
//! (ricosjp, Apache-2.0), whose acceptance test - the surface against the
//! bilinear patch of a cell's corners at a jittered interior point - every
//! cell of our grid still passes. Nothing from any CAM vendor.
//!
//! Why: truck's division marks BOTH parameter intervals of a cell whose
//! test fails, whichever direction caused it. Every sweep then halves
//! every interval of the other direction too, so a face curved in v only
//! (a thread flank: a ruled helical strip, straight across u, fourteen
//! turns along v) still gets 2^sweeps intervals across u. On the #1602
//! models that turned a strip that needs ~2,000 x 2 cells into ~2,000 x
//! 1,000: 2 M facets on one face, the shell's whole budget, and minutes of
//! import.
//!
//! How: per cell, one interior point is compared three ways. Against the
//! chord of its iso-v curve (the cell's u direction) and of its iso-u
//! curve (v): a far u chord splits the cell's u interval, a far v chord
//! its v interval. Only when both chords hold and the bilinear test still
//! fails (a twist) are both split, as truck does. The loop ends when no
//! cell splits, so every cell meets truck's own bilinear test, and the
//! grid is never coarser than the tolerance asks; it is only not finer
//! where nothing is curved.

use truck_stepio::r#in::alias::{EuclideanSpace, HashGen, MetricSpace, ParametricSurface, Point3};

/// How many sweeps the division may take. Each sweep halves a flagged
/// interval, so 64 sweeps reach below f64 resolution of any range: a
/// continuous face is divided long before (14 turns of a thread at 5 um
/// take 11). A face that is still splitting then has a cell that never
/// meets the tolerance - a jump - and is reported as not converged, never
/// returned as a grid: a jump split only across its own direction grows
/// one column a sweep, so the evaluation meter alone would let it through
/// as a wall that is not there.
pub(super) const MAX_SWEEPS: usize = 64;

/// Divide `(urange, vrange)` of `surface` into a grid whose every cell
/// stands within `tol` of the surface (truck's bilinear test), splitting a
/// direction only where it is curved. `tol` must be positive and finite;
/// anything else returns the range's corners alone. `None` when the grid
/// is still splitting after [`MAX_SWEEPS`] (the surface jumps).
pub(super) fn parameter_division<S>(
    surface: &S,
    (urange, vrange): ((f64, f64), (f64, f64)),
    tol: f64,
) -> Option<(Vec<f64>, Vec<f64>)>
where
    S: ParametricSurface<Point = Point3>,
{
    let mut udiv = vec![urange.0, urange.1];
    let mut vdiv = vec![vrange.0, vrange.1];
    if !(tol.is_finite() && tol > 0.0) {
        return Some((udiv, vdiv));
    }
    let tol2 = tol * tol;
    for _ in 0..MAX_SWEEPS {
        let (usplit, vsplit) = sweep(surface, &udiv, &vdiv, tol2);
        if !usplit.contains(&true) && !vsplit.contains(&true) {
            return Some((udiv, vdiv));
        }
        udiv = halve(&udiv, &usplit);
        vdiv = halve(&vdiv, &vsplit);
    }
    None
}

/// One sweep: which u intervals and which v intervals to halve.
fn sweep<S>(surface: &S, udiv: &[f64], vdiv: &[f64], tol2: f64) -> (Vec<bool>, Vec<bool>)
where
    S: ParametricSurface<Point = Point3>,
{
    let mut usplit = vec![false; udiv.len().saturating_sub(1)];
    let mut vsplit = vec![false; vdiv.len().saturating_sub(1)];
    for (u, ub) in udiv.windows(2).zip(usplit.iter_mut()) {
        for (v, vb) in vdiv.windows(2).zip(vsplit.iter_mut()) {
            if *ub && *vb {
                continue;
            }
            let (du, dv) = cell(surface, (u[0], u[1]), (v[0], v[1]), tol2);
            *ub |= du;
            *vb |= dv;
        }
    }
    (usplit, vsplit)
}

/// Whether one cell must split its u interval, its v interval.
fn cell<S>(surface: &S, (u0, u1): (f64, f64), (v0, v1): (f64, f64), tol2: f64) -> (bool, bool)
where
    S: ParametricSurface<Point = Point3>,
{
    // truck's jitter: an interior point 0.4..0.6 across the cell, drawn
    // from the surface at the centre, so a periodic feature cannot hide
    // between probes placed always at the middle.
    let centre = surface.subs((u0 + u1) / 2.0, (v0 + v1) / 2.0);
    let t = 0.5 + (0.2 * HashGen::hash1(centre) - 0.1);
    let (u, v) = (u0 + (u1 - u0) * t, v0 + (v1 - v0) * t);
    let p = surface.subs(u, v);
    let far = |q: Point3| p.distance2(q) > tol2;
    let lerp = |a: Point3, b: Point3| Point3::from_vec(a.to_vec() * (1.0 - t) + b.to_vec() * t);
    // The iso-v curve through p against its chord: curvature across u.
    let ufar = far(lerp(surface.subs(u0, v), surface.subs(u1, v)));
    // The iso-u curve through p against its chord: curvature along v.
    let vfar = far(lerp(surface.subs(u, v0), surface.subs(u, v1)));
    if ufar || vfar {
        return (ufar, vfar);
    }
    // Both chords hold: truck's own bilinear test catches a twist.
    let (p00, p01) = (surface.subs(u0, v0), surface.subs(u0, v1));
    let (p10, p11) = (surface.subs(u1, v0), surface.subs(u1, v1));
    let bilinear = Point3::from_vec(
        p00.to_vec() * (1.0 - t) * (1.0 - t)
            + p01.to_vec() * (1.0 - t) * t
            + p10.to_vec() * t * (1.0 - t)
            + p11.to_vec() * t * t,
    );
    let twist = far(bilinear);
    (twist, twist)
}

/// `div` with every flagged interval halved.
fn halve(div: &[f64], split: &[bool]) -> Vec<f64> {
    let mut out = Vec::with_capacity(div.len() * 2);
    if let Some(first) = div.first() {
        out.push(*first);
    }
    for (w, s) in div.windows(2).zip(split) {
        if *s {
            out.push((w[0] + w[1]) / 2.0);
        }
        out.push(w[1]);
    }
    out
}

#[cfg(test)]
#[path = "step_division_tests.rs"]
mod tests;

// #1545: curve division keeps truck's probes and order, with a work
// budget and a refusal instead of accepting an unresolved leaf at depth 100.
#[derive(Clone, Debug)]
pub(super) struct CurveGuard {
    pub inner: truck_stepio::r#in::alias::Curve3D,
    pub meter: std::sync::Arc<super::step_guard::Meter>,
    fallback: Point3,
}

impl CurveGuard {
    #[cfg(test)]
    pub(super) fn new(inner: truck_stepio::r#in::alias::Curve3D, cap: u64) -> Self {
        Self::in_pass(inner, cap, std::sync::Arc::default())
    }

    /// An edge curve whose meter shares `pass` with its shell's faces (#1599).
    pub(super) fn in_pass(inner: truck_stepio::r#in::alias::Curve3D, cap: u64, pass: std::sync::Arc<super::step_meter::Pass>) -> Self {
        let meter = std::sync::Arc::new(super::step_guard::Meter::in_pass(cap, pass));
        Self { inner, meter, fallback: Point3::new(0.0, 0.0, 0.0) }
    }

    fn divide(&self, range: (f64, f64), tol: f64) -> Option<(Vec<f64>, Vec<Point3>)> {
        use truck_stepio::r#in::alias::ParametricCurve;
        let at = |t| {
            if !self.meter.spend_many(1) { return None; }
            let p = self.inner.subs(t);
            (p.x.is_finite() && p.y.is_finite() && p.z.is_finite()).then_some(p)
        };
        fn split(
            at: &impl Fn(f64) -> Option<Point3>,
            (a, b): (f64, f64),
            (pa, pb): (Point3, Point3),
            tol2: f64,
            depth: usize,
        ) -> Option<(Vec<f64>, Vec<Point3>)> {
            let p = 0.5 + (0.2 * HashGen::hash1(pa.midpoint(pb)) - 0.1);
            let t = a * (1.0 - p) + b * p;
            let mid = pa + (pb - pa) * p;
            if at(t)?.distance2(mid) < tol2 {
                return Some((vec![a, b], vec![pa, pb]));
            }
            let m = (a + b) / 2.0;
            if depth == 0 || m == a || m == b { return None; }
            let pm = at(m)?;
            let (mut ts, mut ps) = split(at, (a, m), (pa, pm), tol2, depth - 1)?;
            let _ = (ts.pop(), ps.pop());
            let (right_t, right_p) = split(at, (m, b), (pm, pb), tol2, depth - 1)?;
            ts.extend(right_t);
            ps.extend(right_p);
            Some((ts, ps))
        }
        if !(tol.is_finite() && tol > 0.0 && range.0.is_finite() && range.1.is_finite()) {
            return None;
        }
        split(&at, range, (at(range.0)?, at(range.1)?), tol * tol, 100)
    }
}

impl truck_stepio::r#in::alias::ParametricCurve for CurveGuard {
    type Point = Point3;
    type Vector = truck_stepio::r#in::alias::Vector3;
    fn subs(&self, t: f64) -> Point3 { self.inner.subs(t) }
    fn der(&self, t: f64) -> Self::Vector { self.inner.der(t) }
    fn der2(&self, t: f64) -> Self::Vector { self.inner.der2(t) }
    fn parameter_range(&self) -> truck_stepio::r#in::alias::ParameterRange { self.inner.parameter_range() }
    fn period(&self) -> Option<f64> { self.inner.period() }
}
impl truck_stepio::r#in::alias::BoundedCurve for CurveGuard {}
impl truck_stepio::r#in::alias::ParameterDivision1D for CurveGuard {
    type Point = Point3;
    fn parameter_division(&self, range: (f64, f64), tol: f64) -> (Vec<f64>, Vec<Point3>) {
        use truck_stepio::r#in::alias::Curve3D;
        let split = match &self.inner {
            Curve3D::BSplineCurve(b) if jumps(b.knot_vec(), b.control_points(), range) => None,
            Curve3D::NurbsCurve(n) if {
                let b = n.non_rationalized();
                let points: Vec<_> = b.control_points().iter().map(|p| Point3::new(p.x / p.w, p.y / p.w, p.z / p.w)).collect();
                jumps(b.knot_vec(), &points, range)
            } => None,
            Curve3D::BSplineCurve(_) | Curve3D::NurbsCurve(_) | Curve3D::PCurve(_) => self.divide(range, tol),
            other => return other.parameter_division(range, tol),
        };
        match split {
            Some(div) => div,
            None => {
                self.meter.exhaust();
                (vec![range.0, range.1], vec![self.fallback; 2])
            }
        }
    }
}

/// A full-multiplicity knot with unequal one-sided control points jumps.
/// Equal endpoints still describe a continuous composite curve and stay legal.
fn jumps(knots: &truck_stepio::r#in::alias::KnotVec, points: &[Point3], range: (f64, f64)) -> bool {
    let Some(degree) = knots.len().checked_sub(points.len()).and_then(|n| n.checked_sub(1)) else { return true; };
    let (values, mults) = knots.to_single_multi();
    let mut start = 0usize;
    for (i, &mult) in mults.iter().enumerate() {
        if i > 0 && i + 1 < values.len() && mult > degree
            && values[i] > range.0.min(range.1) && values[i] < range.0.max(range.1)
            && (mult != degree + 1 || start == 0 || points.get(start - 1) != points.get(start)) {
            return true;
        }
        start = start.saturating_add(mult);
    }
    false
}

/// Rebuild only the swept curve's division adapter; truck keeps its own
/// extrusion/revolution rules, including transforms and reversed parameters.
pub(super) fn swept_division(
    swept: &truck_stepio::r#in::alias::SweptCurve,
    meter: &std::sync::Arc<super::step_guard::Meter>,
    range: ((f64, f64), (f64, f64)),
    tol: f64,
) -> (Vec<f64>, Vec<f64>) {
    use truck_stepio::r#in::alias::{ExtrudedCurve, ParameterDivision2D, RevolutedCurve, SweptCurve};
    let wrap = |inner, fallback| CurveGuard { inner, meter: std::sync::Arc::clone(meter), fallback };
    match swept {
        SweptCurve::ExtrudedCurve(e) => ExtrudedCurve::by_extrusion(wrap(e.entity_curve().clone(), Point3::new(0.0, 0.0, 0.0)), e.extruding_vector()).parameter_division(range, tol),
        // A refused generator collapses onto the axis, giving truck no
        // fictitious radius from which to allocate an angular grid.
        SweptCurve::RevolutedCurve(p) => p.map_ref(|r| RevolutedCurve::by_revolution(wrap(r.entity_curve().clone(), r.origin()), r.origin(), r.axis())).parameter_division(range, tol),
    }
}
