//! How far a solidcore-tess plan's triangles stand off the exact surfaces
//! (M12.1, #940): sampled, per triangle, two ways - against the plan's own
//! surface at the same (u, v), and against the record face's analytic
//! surface by its closed-form distance. A measure, not a proof: see
//! [`Deviation`] for exactly what is and is not covered.
//! Provenance: our own design. Barycentric sampling and linear
//! interpolation of a parametric triangle are textbook (Farin, "Curves and
//! Surfaces for CAGD"); the point-to-plane, -cylinder, -cone, -sphere and
//! -torus distances are closed-form (any analytic-geometry text). The
//! plan's surfaces are evaluated through solidcore's public evaluator
//! (Apache-2.0 port of NVIDIA's SMLib). No HSMWorks material consulted.

use solidcore::kernel::CancelToken;
use solidcore_record::brep::{self, Frame};
use solidcore_tess::{EvalSurface, FacePlan, TessPlan};

use crate::P3;

/// Divisions per triangle side of the barycentric sampling grid: 4 gives
/// 15 points per triangle - the corners, three points on each side
/// (quarters and middle) and three inside.
pub const SAMPLE_DIVISIONS: usize = 4;

/// What [`measure`] found. Every distance in millimetres.
///
/// MEASURED, at each sample point of each triangle:
/// - `parametric`: |S(u, v) - T(u, v)|, the distance between the plan
///   surface's point at the sample's interpolated (u, v) and the triangle's
///   own point there. It bounds, AT THE SAMPLE, both the distance from the
///   triangle to the surface and from the surface to the triangle (each
///   point is on its side), so it never under-reads there; on a strongly
///   non-uniform parameterisation it can over-read (a false refusal, never
///   a false pass). At a triangle corner on a shared edge node it is the
///   gap between the edge's point and its face's surface.
/// - `analytic`: the closed-form distance from the triangle's sample point
///   to the record face's untrimmed analytic surface (plane, cylinder,
///   cone, sphere, torus, the sphere of a ball patch). Independent of the
///   mesher's own surface conversion: a conversion error shows here.
///
/// JUDGED (`gated`, what the tolerance is held against): per face, the
/// `analytic` distance where the record face is analytic - the true
/// distance from the triangle to the surface at the sample - else the
/// `parametric` one. Why not `parametric` everywhere: on a rational
/// spline's circle the parameter does not run at a constant speed along
/// the arc, so the interpolated (u, v) lands a little along the arc from
/// the triangle's point and the parametric distance over-reads the sag by
/// a few per cent (Milling-test.step at 0.005 mm: 5.058e-3 parametric
/// against 4.965e-3 true) - a refusal of a mesh that is inside its
/// tolerance.
///
/// NOT MEASURED: the triangle between the samples (a sampled maximum is a
/// lower bound of the true maximum; the error falls with the sampling
/// density, not proven); the analytic measure on spline and revolved faces
/// (counted in `faces_without_analytic`); the trim (a triangle sitting on
/// the right surface outside its face's boundary is not caught here - the
/// welded-mesh watertightness check is what binds faces to their edges).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Deviation {
    /// Largest `parametric` distance over every sample.
    pub parametric: f64,
    /// Largest `analytic` distance over every sample of an analytic face.
    pub analytic: f64,
    /// Largest judged distance (see above): what the tolerance binds.
    pub gated: f64,
    /// The record face of the largest judged distance.
    pub worst_face: Option<u32>,
    /// Sample points measured.
    pub samples: usize,
    /// Faces with triangles measured against an analytic surface too.
    pub faces_analytic: usize,
    /// Faces with triangles measured by `parametric` alone.
    pub faces_without_analytic: usize,
}

impl Deviation {
    /// The judged distance: what the tolerance is held against.
    #[must_use]
    pub fn largest(&self) -> f64 {
        self.gated
    }
}

/// Why a face's triangles could not be measured.
#[derive(Debug, Clone, PartialEq)]
pub enum Unmeasured {
    /// The face's triangles have no surface to evaluate.
    NoSurface { face: u32 },
    /// The surface has no point at a sample's parameters.
    NoPoint { face: u32, uv: [f64; 2] },
    /// A triangle names a node the face does not have.
    BadIndex { face: u32 },
    /// The caller's time bound.
    Cancelled,
}

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: P3) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: P3) -> Option<P3> {
    let n = norm(a);
    (n.is_finite() && n > 0.0).then(|| [a[0] / n, a[1] / n, a[2] / n])
}

/// The surface point at `(u, v)`, `None` where the surface has none.
fn point(s: &EvalSurface, uv: [f64; 2]) -> Option<P3> {
    let (p, _, _) = s.derivs(uv[0], uv[1])?;
    let p = [p.x, p.y, p.z];
    p.iter().all(|c| c.is_finite()).then_some(p)
}

/// A frame's origin, and its unit z axis and the point's height and radius
/// about it.
fn cylindrical(f: &Frame, p: P3) -> Option<(f64, f64)> {
    let z = unit(cross(unit(f.x_axis)?, unit(f.y_axis)?))?;
    let d = sub(p, f.origin);
    let h = dot(d, z);
    let rho = (dot(d, d) - h * h).max(0.0).sqrt();
    Some((h, rho))
}

/// The distance from `p` to the untrimmed analytic surface of a record
/// face, `None` for a surface with no closed form here (splines,
/// revolutions) or a degenerate frame.
#[must_use]
pub fn analytic_distance(s: &brep::Surface, p: P3) -> Option<f64> {
    use brep::Surface as S;
    let d = match s {
        S::Plane {
            origin,
            u_axis,
            v_axis,
        } => dot(sub(p, *origin), unit(cross(*u_axis, *v_axis))?).abs(),
        S::PlaneThrough {
            origin,
            u_point,
            v_point,
        } => dot(
            sub(p, *origin),
            unit(cross(sub(*u_point, *origin), sub(*v_point, *origin)))?,
        )
        .abs(),
        S::Cylinder { frame, radius } => (cylindrical(frame, p)?.1 - radius).abs(),
        S::Sphere { frame, radius } => (norm(sub(p, frame.origin)) - radius).abs(),
        S::BallPatch { center, radius } => (norm(sub(p, *center)) - radius).abs(),
        S::Torus {
            frame,
            major_radius,
            minor_radius,
            ..
        } => {
            let (h, rho) = cylindrical(frame, p)?;
            ((rho - major_radius).hypot(h) - minor_radius).abs()
        }
        S::Cone {
            frame,
            radius_at_origin,
            half_angle,
        } => {
            // In the (radius, height) half-plane the cone is the half-line
            // from (r0, 0) along (sin a, cos a), cut where the radius
            // reaches 0 (the apex).
            let (h, rho) = cylindrical(frame, p)?;
            let (sa, ca) = half_angle.sin_cos();
            let t = (rho - radius_at_origin) * sa + h * ca;
            if radius_at_origin + t * sa >= 0.0 {
                ((rho - radius_at_origin) * ca - h * sa).abs()
            } else {
                // Past the apex (only reachable with sin a != 0): r0 + h tan a = 0.
                let apex_h = if sa != 0.0 {
                    -radius_at_origin * ca / sa
                } else {
                    0.0
                };
                rho.hypot(h - apex_h)
            }
        }
        S::Spline { .. } | S::RationalSpline { .. } | S::PlacedRevolution { .. } => return None,
    };
    d.is_finite().then_some(d)
}

/// The barycentric weights of the sampling grid.
fn weights() -> Vec<[f64; 3]> {
    let n = SAMPLE_DIVISIONS;
    let mut w = Vec::with_capacity((n + 1) * (n + 2) / 2);
    for i in 0..=n {
        for j in 0..=(n - i) {
            let (a, b) = (i as f64 / n as f64, j as f64 / n as f64);
            w.push([a, b, (1.0 - a - b).max(0.0)]);
        }
    }
    w
}

/// Measures every triangle of `plan` (see [`Deviation`]). `record` is the
/// B-rep record the plan was made from (its face order). `stop` is polled
/// per face.
pub fn measure(
    plan: &TessPlan,
    record: &brep::Brep,
    stop: &CancelToken,
) -> Result<Deviation, Unmeasured> {
    let w = weights();
    let mut out = Deviation::default();
    let mut worst = (0.0_f64, None);
    for f in &plan.faces {
        if stop.is_cancelled() {
            return Err(Unmeasured::Cancelled);
        }
        if f.triangles.is_empty() {
            continue;
        }
        let analytic = record.faces.get(f.face as usize).map(|rf| &rf.surface);
        let has_analytic = analytic.is_some_and(|s| analytic_distance(s, [0.0; 3]).is_some());
        if has_analytic {
            out.faces_analytic += 1;
        } else {
            out.faces_without_analytic += 1;
        }
        let (p, a, n) = face(plan, f, analytic.filter(|_| has_analytic), &w)?;
        out.parametric = out.parametric.max(p);
        out.analytic = out.analytic.max(a);
        out.samples = out.samples.saturating_add(n);
        let g = if has_analytic { a } else { p };
        out.gated = out.gated.max(g);
        if g > worst.0 || worst.1.is_none() {
            worst = (g, Some(f.face));
        }
    }
    out.worst_face = worst.1;
    Ok(out)
}

/// One face: (largest parametric, largest analytic, samples).
fn face(
    plan: &TessPlan,
    f: &FacePlan,
    analytic: Option<&brep::Surface>,
    w: &[[f64; 3]],
) -> Result<(f64, f64, usize), Unmeasured> {
    let s = f
        .surface
        .and_then(|i| plan.surfaces.get(i as usize))
        .ok_or(Unmeasured::NoSurface { face: f.face })?;
    // The nodes as the evaluated mesh has them: a shared edge node's own
    // point, else the surface's.
    let mut nodes: Vec<P3> = Vec::with_capacity(f.uv.len());
    for (k, uv) in f.uv.iter().enumerate() {
        let p = match f.fixed.get(k) {
            Some(q) => [q.x, q.y, q.z],
            None => point(s, *uv).ok_or(Unmeasured::NoPoint {
                face: f.face,
                uv: *uv,
            })?,
        };
        nodes.push(p);
    }
    let (mut pm, mut am, mut n) = (0.0_f64, 0.0_f64, 0usize);
    for t in &f.triangles {
        let get = |i: u32| {
            let i = i as usize;
            Some((*nodes.get(i)?, *f.uv.get(i)?))
        };
        let (Some(c0), Some(c1), Some(c2)) = (get(t[0]), get(t[1]), get(t[2])) else {
            return Err(Unmeasured::BadIndex { face: f.face });
        };
        for b in w {
            let mix = |x: [f64; 3]| b[0] * x[0] + b[1] * x[1] + b[2] * x[2];
            let tp = [0, 1, 2].map(|i| mix([c0.0[i], c1.0[i], c2.0[i]]));
            let uv = [0, 1].map(|i| mix([c0.1[i], c1.1[i], c2.1[i]]));
            let sp = point(s, uv).ok_or(Unmeasured::NoPoint { face: f.face, uv })?;
            pm = pm.max(norm(sub(sp, tp)));
            if let Some(d) = analytic.and_then(|a| analytic_distance(a, tp)) {
                am = am.max(d);
            }
            n = n.saturating_add(1);
        }
    }
    Ok((pm, am, n))
}
