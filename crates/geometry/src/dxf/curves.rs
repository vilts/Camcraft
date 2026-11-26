//! DXF SPLINE and ELLIPSE entities read and laid as tangent arcs and lines (#1865).
//! Provenance: the SPLINE and ELLIPSE group codes of Autodesk's published DXF Reference
//! (format only); the ellipse in its standard parametric form `c + M cos t + m sin t`; our own
//! implementation. The fit and its bound are in `curve_fit`, the B-spline in `nurbs`.
//!
//! The tolerance is [`CURVE_TOLERANCE_MM`] in millimetres, read in drawing units from the
//! `$INSUNITS` stated before the entity. A drawing without a stated unit is read as inches,
//! the largest unit a caller can state: the bound in drawing units is then the tightest. The
//! tolerance used is kept on the document, and `SketchData` refuses a drawing whose applied
//! unit makes it looser than the stated bound.
use super::Units;
use super::curve_fit::{Budget, Curve, Fit};
use super::entities::{circle_range, common, coordinate, layer, point};
use super::nurbs::{MAX_DEGREE, Nurbs};
use super::pairs::{Record, integer, number};
use super::types::{DxfError, Primitive, State, error};
use crate::geo2d::Point2;
use std::f64::consts::{FRAC_PI_2, TAU};

/// The largest distance between a SPLINE or ELLIPSE and its arcs, in millimetres (both ways).
pub const CURVE_TOLERANCE_MM: f64 = 0.001;
/// An arc of a curve's fit with a larger radius is laid as a line, in millimetres.
pub const CURVE_MAX_RADIUS_MM: f64 = 1000.0;
/// The curve evaluations that one drawing may spend on its curves.
pub(super) const EVALUATION_BUDGET: usize = 50_000_000;

/// Millimetres per drawing unit for the fit: inches when the drawing states no unit.
fn per_unit(units: Units) -> f64 {
    match units {
        Units::Known(unit) => unit.millimeters_per_unit(),
        _ => 25.4,
    }
}

fn fit_settings(state: &mut State, kind: &'static str) -> Fit<'static> {
    let k = per_unit(state.doc.units);
    let tol = CURVE_TOLERANCE_MM / k;
    let used = state.doc.curve_tolerance.map_or(tol, |old| old.max(tol));
    state.doc.curve_tolerance = Some(used);
    Fit {
        kind,
        tol,
        max_radius: CURVE_MAX_RADIUS_MM / k,
    }
}

fn emit(state: &mut State, layer: &str, out: Vec<Primitive>, kind: &str) -> Result<(), DxfError> {
    if out.is_empty() {
        return Err(error(kind, "curve has zero length"));
    }
    for primitive in out {
        state.vertex(kind)?;
        state.push(layer, primitive);
    }
    Ok(())
}

struct Ellipse {
    c: Point2,
    major: (f64, f64),
    minor: (f64, f64),
}

impl Curve for Ellipse {
    fn point(&self, t: f64) -> Point2 {
        let (cos, sin) = (t.cos(), t.sin());
        Point2::new(
            self.c.x + self.major.0 * cos + self.minor.0 * sin,
            self.c.y + self.major.1 * cos + self.minor.1 * sin,
        )
    }

    fn tangent(&self, t: f64, _: f64, _: f64) -> Option<(f64, f64)> {
        let (cos, sin) = (t.cos(), t.sin());
        let dx = -self.major.0 * sin + self.minor.0 * cos;
        let dy = -self.major.1 * sin + self.minor.1 * cos;
        let len = dx.hypot(dy);
        (len > 0.0 && len.is_finite()).then(|| (dx / len, dy / len))
    }

    fn speed(&self, _: f64, _: f64) -> f64 {
        // |C'(t)| <= |M| since |m| <= |M|.
        self.major.0.hypot(self.major.1)
    }
}

pub(super) fn ellipse(record: Record<'_>, state: &mut State) -> Result<(), DxfError> {
    const KIND: &str = "ELLIPSE";
    common(record)?;
    let layer = layer(record)?;
    let c = point(record, 10, 20)?;
    let z = coordinate(record.number(30, Some(0.0))?, KIND)?;
    state.elevation(KIND, z)?;
    let m = point(record, 11, 21)?;
    if record.number(31, Some(0.0))? != 0.0 {
        return Err(error(KIND, "major axis leaves the drawing plane"));
    }
    let a = m.x.hypot(m.y);
    let ratio = record.number(40, None)?;
    if !(ratio > 0.0 && ratio <= 1.0) {
        return Err(error(KIND, "axis ratio must be above 0 and at most 1"));
    }
    circle_range(c, a, KIND)?;
    let t0 = record.number(41, Some(0.0))?;
    let t1 = record.number(42, Some(TAU))?;
    if t0.abs() > 1e6 || t1.abs() > 1e6 {
        return Err(error(KIND, "parameter outside a plain angle range"));
    }
    let span = t1 - t0;
    let full = span.abs() >= TAU || (span != 0.0 && span.rem_euclid(TAU) == 0.0);
    let sweep = if full { TAU } else { span.rem_euclid(TAU) };
    if sweep == 0.0 {
        return Err(error(KIND, "zero-length parameter sweep"));
    }
    let start = t0.rem_euclid(TAU);
    let axis = m.y.atan2(m.x);
    // The circle of radius a is at most a (1 - ratio) from the ellipse, at the minor ends.
    // A ratio of exactly 1 is the circle itself; a near one is a circle only while that
    // distance is within the tolerance, which the document then records.
    let round = ratio == 1.0 || {
        let tol = CURVE_TOLERANCE_MM / per_unit(state.doc.units);
        let near = a * (1.0 - ratio) <= tol;
        if near {
            fit_settings(state, KIND);
        }
        near
    };
    if round {
        let primitive = if full {
            Primitive::Circle {
                center: c,
                radius: a,
            }
        } else {
            Primitive::Arc {
                center: c,
                radius: a,
                start_angle: (axis + start).rem_euclid(TAU),
                sweep_angle: sweep,
            }
        };
        state.vertex(KIND)?;
        state.push(layer, primitive);
        return Ok(());
    }
    let curve = Ellipse {
        c,
        major: (m.x, m.y),
        minor: (-m.y * ratio, m.x * ratio),
    };
    // Pieces of at most a quarter turn, so each starts with a well-shaped biarc.
    let quarters = (sweep / FRAC_PI_2).ceil().max(1.0) as usize;
    let cuts: Vec<f64> = (0..=quarters)
        .map(|i| start + sweep * i as f64 / quarters as f64)
        .collect();
    let fit = fit_settings(state, KIND);
    let mut budget = budget(state);
    let mut out = Vec::new();
    let last = full.then(|| curve.point(start));
    fit.section(&curve, &cuts, last, &mut budget, &mut out)?;
    state.curve_work = budget.evaluations;
    emit(state, layer, out, KIND)
}

fn budget(state: &State) -> Budget {
    Budget {
        evaluations: state.curve_work,
        primitives: state
            .limits
            .max_vertices
            .saturating_sub(state.doc.summary.vertices),
    }
}

/// The SPLINE fields in their file order.
#[derive(Default)]
struct Fields {
    knots: Vec<f64>,
    weights: Vec<f64>,
    points: Vec<Point2>,
    fit: usize,
    x: Option<f64>,
    xy: Option<Point2>,
    z: Option<f64>,
    fit_x: Option<f64>,
    fit_xy: bool,
    fit_z: Option<f64>,
    elevation: Option<f64>,
}

impl Fields {
    fn finish_control(&mut self, kind: &'static str) -> Result<(), DxfError> {
        if self.x.is_some() {
            return Err(error(kind, "control point has no Y coordinate"));
        }
        if let Some(point) = self.xy.take() {
            // Omitted Z is zero for this control point.
            let z = self.z.take().unwrap_or(0.0);
            if self.elevation.is_some_and(|elevation| elevation != z) {
                return Err(error(kind, "control points are not in one Z plane"));
            }
            self.elevation = Some(z);
            self.points.push(point);
        }
        Ok(())
    }

    /// The fit point rule is the control point rule: one Z for each point, an omitted Z
    /// is zero, and all fit and control points are in one Z plane.
    fn finish_fit(&mut self, kind: &'static str) -> Result<(), DxfError> {
        if self.fit_x.is_some() {
            return Err(error(kind, "fit point has no Y coordinate"));
        }
        if std::mem::take(&mut self.fit_xy) {
            // Omitted Z is zero for this fit point.
            let z = self.fit_z.take().unwrap_or(0.0);
            if self.elevation.is_some_and(|elevation| elevation != z) {
                return Err(error(kind, "fit points are not in one Z plane"));
            }
            self.elevation = Some(z);
        }
        Ok(())
    }
}

pub(super) fn spline(record: Record<'_>, state: &mut State) -> Result<(), DxfError> {
    const KIND: &str = "SPLINE";
    common(record)?;
    let layer = layer(record)?;
    let degree = usize::try_from(integer(record.required(71)?, KIND)?)
        .map_err(|_| error(KIND, &format!("degree must be 1 to {MAX_DEGREE}")))?;
    let count = |code: u16| -> Result<usize, DxfError> {
        let n = usize::try_from(record.integer(code, 0)?)
            .map_err(|_| error(KIND, &format!("negative count in group {code}")))?;
        let room = state
            .limits
            .max_vertices
            .saturating_sub(state.doc.summary.vertices);
        if n > room {
            return Err(error(KIND, "point or knot count exceeds limit"));
        }
        Ok(n)
    };
    let (knots, controls, fits) = (count(72)?, count(73)?, count(74)?);
    let mut f = Fields::default();
    let mut pairs = record.pairs();
    while let Some(pair) = pairs.next()? {
        let value = || number(pair.value, KIND);
        match pair.code {
            40 => push(&mut f.knots, value()?, knots, "knot")?,
            41 => push(&mut f.weights, value()?, controls, "weight")?,
            10 => {
                f.finish_control(KIND)?;
                f.finish_fit(KIND)?;
                f.x = Some(coordinate(value()?, KIND)?);
            }
            20 => {
                let x =
                    f.x.take()
                        .ok_or_else(|| error(KIND, "Y before X in a control point"))?;
                let p = Point2::new(x, coordinate(value()?, KIND)?);
                if f.points.len() >= controls {
                    return Err(error(KIND, "more control points than group 73 states"));
                }
                f.xy = Some(p);
            }
            30 => {
                if f.xy.is_none() {
                    return Err(error(KIND, "Z before X and Y in a control point"));
                }
                if f.z.is_some() {
                    return Err(error(KIND, "duplicate Z in a control point"));
                }
                f.z = Some(coordinate(value()?, KIND)?);
            }
            11 => {
                f.finish_control(KIND)?;
                f.finish_fit(KIND)?;
                f.fit_x = Some(coordinate(value()?, KIND)?);
                f.fit += 1;
                if f.fit > fits {
                    return Err(error(KIND, "more fit points than group 74 states"));
                }
            }
            21 => {
                if f.fit_x.take().is_none() {
                    return Err(error(KIND, "Y before X in a fit point"));
                }
                coordinate(value()?, KIND)?;
                f.fit_xy = true;
            }
            31 => {
                if !f.fit_xy {
                    return Err(error(KIND, "Z before X and Y in a fit point"));
                }
                if f.fit_z.is_some() {
                    return Err(error(KIND, "duplicate Z in a fit point"));
                }
                f.fit_z = Some(coordinate(value()?, KIND)?);
            }
            _ => {}
        }
    }
    f.finish_control(KIND)?;
    f.finish_fit(KIND)?;
    if f.fit != fits {
        return Err(error(KIND, "fit point count does not match group 74"));
    }
    if controls == 0 && fits > 0 {
        return Err(error(
            KIND,
            "fit points without control points are unsupported",
        ));
    }
    if f.knots.len() != knots || f.points.len() != controls {
        return Err(error(
            KIND,
            "knot or control point count does not match groups 72 and 73",
        ));
    }
    state.elevation(KIND, f.elevation.unwrap_or(0.0))?;
    for _ in 0..f.points.len() {
        state.vertex(KIND)?;
    }
    let curve = Nurbs::new(degree, f.knots, f.points, f.weights)?;
    let fit = fit_settings(state, KIND);
    let mut budget = budget(state);
    let mut out = Vec::new();
    for cuts in sections(&curve) {
        fit.section(&curve, &cuts, None, &mut budget, &mut out)?;
    }
    state.curve_work = budget.evaluations;
    emit(state, layer, out, KIND)
}

fn push(list: &mut Vec<f64>, value: f64, cap: usize, what: &str) -> Result<(), DxfError> {
    if list.len() >= cap {
        return Err(error(
            "SPLINE",
            &format!("more {what} values than the stated count"),
        ));
    }
    list.push(value);
    Ok(())
}

/// The smooth sections of the spline: its spans, broken where the direction turns at a knot.
fn sections(curve: &Nurbs) -> Vec<Vec<f64>> {
    /// A turn at a knot above this angle (radians) is a corner.
    const KINK: f64 = 1e-5;
    let breaks = curve.breaks();
    let (lo, hi) = curve.domain();
    let mut out = vec![vec![lo]];
    for (i, &u) in breaks.iter().enumerate().skip(1) {
        if let Some(cur) = out.last_mut() {
            cur.push(u);
        }
        if i + 1 == breaks.len() {
            break;
        }
        let left = curve.tangent(u, lo, u);
        let right = curve.tangent(u, u, hi);
        let corner = match (left, right) {
            (Some(l), Some(r)) => (l.0 * r.1 - l.1 * r.0).atan2(l.0 * r.0 + l.1 * r.1).abs() > KINK,
            _ => true,
        };
        if corner {
            out.push(vec![u]);
        }
    }
    out
}
