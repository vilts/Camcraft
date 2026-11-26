//! The B-spline curves that a STEP file's edges and swept surfaces stand on,
//! checked before the reader builds them (#917; the law of #1545): a curve
//! with a weight that is not positive has no shape there, and a curve whose
//! interior knot repeats more often than its degree may jump at that knot.
//! Such a curve must be split into separate edges (a curve subdivision)
//! before a mesh can follow it, so the file is refused by name.
//! Provenance: ISO 10303-42 (B_SPLINE_CURVE_WITH_KNOTS, RATIONAL_B_SPLINE_CURVE,
//! EDGE_CURVE, SURFACE_CURVE, SURFACE_OF_LINEAR_EXTRUSION,
//! SURFACE_OF_REVOLUTION) and the B-spline continuity rule of Piegl and
//! Tiller, "The NURBS Book", 2.2 (a knot of multiplicity k makes the curve
//! C^(p-k) there, so k > p allows a break). Read through the public p21 API
//! of solidcore-step. No HSMWorks material consulted.

use solidcore_step::p21::{Entity, Exchange, Param};

/// Two control points closer than this (relative to their size, at least
/// one file unit) are one point: a repeated knot between them is no break.
const SAME_POINT: f64 = 1e-9;

/// `Err`: the first edge or swept surface whose B-spline curve has a weight
/// that is not positive or breaks at an interior knot, named with the
/// entity ids and the advice.
pub(crate) fn check(ex: &Exchange) -> Result<(), String> {
    for (id, e) in &ex.entities {
        let (role, curve) = if let Some(p) = e.record("EDGE_CURVE") {
            ("edge", p.get(3).and_then(Param::as_ref_id).map(|c| geometry_of(ex, c)))
        } else if let Some(p) = e
            .record("SURFACE_OF_LINEAR_EXTRUSION")
            .or_else(|| e.record("SURFACE_OF_REVOLUTION"))
        {
            ("swept surface", p.get(1).and_then(Param::as_ref_id))
        } else {
            continue;
        };
        let Some(c) = curve else { continue };
        let Some(spline) = ex.get(c).and_then(Spline::of) else {
            continue;
        };
        if let Some(fault) = spline.fault(ex) {
            return Err(format!(
                "{role} #{id}: its B-spline curve #{c} {fault} - the curve needs a subdivision into separate edges at that point; re-export the model"
            ));
        }
    }
    Ok(())
}

/// The 3D curve under a SURFACE_CURVE, SEAM_CURVE or INTERSECTION_CURVE
/// wrapper, else `c`.
fn geometry_of(ex: &Exchange, c: u64) -> u64 {
    ex.get(c)
        .and_then(|e| {
            ["SURFACE_CURVE", "SEAM_CURVE", "INTERSECTION_CURVE"]
                .into_iter()
                .find_map(|n| e.record(n))
        })
        .and_then(|p| p.get(1))
        .and_then(Param::as_ref_id)
        .unwrap_or(c)
}

/// The parts of a B-spline curve entity that the checks read.
struct Spline<'a> {
    degree: usize,
    points: &'a [Param],
    mults: &'a [Param],
    knots: &'a [Param],
    weights: Option<&'a [Param]>,
}

impl<'a> Spline<'a> {
    /// A simple B_SPLINE_CURVE_WITH_KNOTS, or a complex instance with the
    /// B_SPLINE_CURVE and B_SPLINE_CURVE_WITH_KNOTS records.
    fn of(e: &'a Entity) -> Option<Self> {
        let list = |p: &'a [Param], i: usize| p.get(i).and_then(Param::as_list);
        let (head, knotted) = if e.is_complex() {
            (e.record("B_SPLINE_CURVE")?, e.record("B_SPLINE_CURVE_WITH_KNOTS")?)
        } else {
            let p = e.record("B_SPLINE_CURVE_WITH_KNOTS")?;
            // name, degree, points, form, closed, self_intersect | mults, knots, spec
            (p.get(1..)?, p.get(6..)?)
        };
        let degree = usize::try_from(head.first()?.as_i64()?).ok()?;
        let weights = e.record("RATIONAL_B_SPLINE_CURVE").and_then(|p| list(p, 0));
        Some(Spline {
            degree,
            points: list(head, 1)?,
            mults: list(knotted, 0)?,
            knots: list(knotted, 1)?,
            weights,
        })
    }

    /// What is wrong with the curve, worded after "its B-spline curve #c".
    fn fault(&self, ex: &Exchange) -> Option<String> {
        if let Some(w) = self.weights.into_iter().flatten().find(|w| !w.as_f64().is_some_and(|w| w > 0.0)) {
            let w = w.as_f64().map_or_else(|| "that is not a number".to_owned(), |w| format!("{w}"));
            return Some(format!("has a weight {w} that is not positive"));
        }
        let last = self.mults.len().checked_sub(1)?;
        let mut start = 0usize;
        for (i, m) in self.mults.iter().enumerate() {
            let m = usize::try_from(m.as_i64()?).ok()?;
            if i > 0 && i < last && m > self.degree {
                // Left of the knot the curve ends at point start-1; right of
                // it, it begins at point start+m-degree-1 (The NURBS Book).
                let left = point(ex, self.points.get(start.checked_sub(1)?)?)?;
                let right = point(ex, self.points.get(start + m - self.degree - 1)?)?;
                let jump = dist(left, right);
                let size = left.iter().chain(&right).fold(1.0_f64, |a, c| a.max(c.abs()));
                if jump > SAME_POINT * size {
                    let t = self.knots.get(i).and_then(Param::as_f64).unwrap_or(f64::NAN);
                    return Some(format!(
                        "breaks at knot {t}: the knot repeats {m} times, above the degree {}, and the curve jumps {jump:.4} file units there",
                        self.degree
                    ));
                }
            }
            start = start.saturating_add(m);
        }
        None
    }
}

/// The coordinates of a CARTESIAN_POINT reference.
fn point(ex: &Exchange, p: &Param) -> Option<[f64; 3]> {
    let c = ex.get(p.as_ref_id()?)?.record("CARTESIAN_POINT")?.get(1)?.as_list()?;
    let at = |i: usize| c.get(i).and_then(Param::as_f64).unwrap_or(0.0);
    (!c.is_empty()).then(|| [at(0), at(1), at(2)])
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}
