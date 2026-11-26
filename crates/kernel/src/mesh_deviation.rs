//! How far a solidcore-tess plan's triangles stand off the exact surfaces
//! (M12.1, #940, #1029): sampled at the 15 barycentric points per triangle AND
//! certified continuously between samples by [`crate::mesh_bound`].
//! Provenance: our own design. Barycentric sampling is textbook (Farin, "Curves
//! and Surfaces for CAGD"); closed-form analytic distances are standard
//! analytic geometry; continuous upper bounds and Gauss-Newton projection use
//! solidcore's public evaluator (`EvalSurface`, `uv::project_with`) and
//! [`crate::mesh_bound`]. No HSMWorks material consulted.

use solidcore::eval::plan::uv;
use solidcore::kernel::CancelToken;
use solidcore_record::brep;
use solidcore_tess::{EvalSurface, FacePlan, TessPlan};

use crate::P3;
pub use crate::mesh_distance::analytic_distance;
use crate::mesh_bound::{analytic_is_exact, analytic_tri_bound, norm, parametric_eps, project_sample, sub};

/// Divisions per triangle side of the barycentric sampling grid: 4 gives
/// 15 points per triangle - the corners, three points on each side
/// (quarters and middle) and three inside.
pub const SAMPLE_DIVISIONS: usize = 4;

/// Maximum recursive 4-way sub-triangle refinement depth beyond `N = 4`
/// for [`measure`] (`N = 4 -> 8 -> 16 -> 32`).
const MAX_REFINE_DEPTH: u8 = 3;

/// [`measure_within`] refines a sub-triangle until its bound is inside the
/// tolerance (#917): at most this deep (`N = 4 * 2^12`), and at most
/// [`MAX_REFINE_SAMPLES`] new samples for one plan. Past either cap the
/// bound found so far is the judged one: never smaller than the truth.
const MAX_ADAPTIVE_DEPTH: u8 = 12;

/// The most samples [`measure_within`] adds by refinement for one plan.
pub const MAX_REFINE_SAMPLES: usize = 1 << 21;

/// What [`measure`] / [`measure_within`] found. Every distance in millimetres.
///
/// MEASURED:
/// - `parametric`: `|S(u, v) - T(u, v)|` at each sample's interpolated `(u, v)`.
/// - `analytic`: closed-form continuous upper bound to the untrimmed analytic
///   surface over every triangle of an analytic face.
/// - `projected`: certified continuous upper bound to the orthogonally
///   projected footpoint on spline and revolved faces (`faces_without_analytic`).
///
/// JUDGED (`gated`, held against `largest()`): `analytic` on analytic faces,
/// `projected` elsewhere, proven everywhere inside each triangle.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Deviation {
    /// Largest `parametric` distance over evaluated samples.
    pub parametric: f64,
    /// Largest continuous `analytic` distance bound over analytic faces.
    pub analytic: f64,
    /// Largest continuous `projected` distance bound over non-analytic faces.
    pub projected: f64,
    /// Largest judged distance bound (see above): what the tolerance binds.
    pub gated: f64,
    /// The record face of the largest judged distance bound.
    pub worst_face: Option<u32>,
    /// Sample points measured.
    pub samples: usize,
    /// Faces with triangles measured against an analytic surface.
    pub faces_analytic: usize,
    /// Faces with triangles measured by projection onto the plan surface.
    pub faces_without_analytic: usize,
}

impl Deviation {
    /// The judged continuous deviation upper bound: what the tolerance is held against.
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
    /// The surface has no point at a sample's parameters, or projection failed.
    NoPoint { face: u32, uv: [f64; 2] },
    /// A triangle names a node the face does not have.
    BadIndex { face: u32 },
    /// The caller's time bound.
    Cancelled,
}

#[derive(Clone, Copy)]
struct SamplePt {
    p: P3,
    q: [f64; 2],
    d: f64,
    exact: bool,
}

fn point(s: &EvalSurface, uv_in: [f64; 2]) -> Option<P3> {
    let (p, _, _) = s.derivs(uv_in[0], uv_in[1])?;
    let p = [p.x, p.y, p.z];
    p.iter().all(|c| c.is_finite()).then_some(p)
}
/// Measures every triangle of `plan` (see [`Deviation`]), adaptively tightening
/// between-sample continuous bounds up to `N = 32`.
pub fn measure(plan: &TessPlan, record: &brep::Brep, stop: &CancelToken) -> Result<Deviation, Unmeasured> {
    measure_inner(plan, record, None, stop)
}

/// Measures every triangle of `plan`, refining sub-triangles only where needed
/// to decide whether the continuous deviation stays within `tol_mm`.
pub fn measure_within(plan: &TessPlan, record: &brep::Brep, tol_mm: f64, stop: &CancelToken) -> Result<Deviation, Unmeasured> {
    measure_inner(plan, record, Some(tol_mm), stop)
}

fn measure_inner(plan: &TessPlan, record: &brep::Brep, tol_mm: Option<f64>, stop: &CancelToken) -> Result<Deviation, Unmeasured> {
    let mut out = Deviation::default();
    let mut worst = (0.0_f64, None);
    let mut budget = MAX_REFINE_SAMPLES;
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
        let s = f
            .surface
            .and_then(|i| plan.surfaces.get(i as usize))
            .ok_or(Unmeasured::NoSurface { face: f.face })?;
        let (p, a, r, g, n) = face(f, s, analytic.filter(|_| has_analytic), tol_mm, &mut budget, stop)?;
        out.parametric = out.parametric.max(p);
        out.analytic = out.analytic.max(a);
        out.projected = out.projected.max(r);
        out.samples = out.samples.saturating_add(n);
        out.gated = out.gated.max(g);
        if g > worst.0 || worst.1.is_none() {
            worst = (g, Some(f.face));
        }
    }
    out.worst_face = worst.1;
    Ok(out)
}

enum FaceBoundCtx<'a> {
    Analytic {
        surf: &'a brep::Surface,
        face: u32,
    },
    Parametric {
        s: &'a EvalSurface,
        dom: &'a uv::Domain,
        starts: &'a uv::Starts,
        face: u32,
    },
}

fn eval_mid(
    ctx: &FaceBoundCtx<'_>,
    a: SamplePt,
    b: SamplePt,
    n: &mut usize,
) -> Result<SamplePt, Unmeasured> {
    let p = [0, 1, 2].map(|k| 0.5 * (a.p[k] + b.p[k]));
    *n = n.saturating_add(1);
    match ctx {
        FaceBoundCtx::Analytic { surf, face } => {
            let d = analytic_distance(surf, p).ok_or(Unmeasured::NoPoint { face: *face, uv: [0.0; 2] })?;
            Ok(SamplePt { p, q: [0.0; 2], d, exact: true })
        }
        FaceBoundCtx::Parametric { s, dom, starts, face } => {
            let hint = [0, 1].map(|k| 0.5 * (a.q[k] + b.q[k]));
            let (q, d) = project_sample(s, dom, starts, p, Some(hint))
                .ok_or(Unmeasured::NoPoint { face: *face, uv: hint })?;
            Ok(SamplePt { p, q, d, exact: true })
        }
    }
}

fn ensure_projected(ctx: &FaceBoundCtx<'_>, tri: &mut [SamplePt; 3]) -> Result<(), Unmeasured> {
    let FaceBoundCtx::Parametric { s, dom, starts, face } = ctx else {
        return Ok(());
    };
    let q0 = tri[0].q;
    for (idx, pt) in tri.iter_mut().enumerate() {
        if !pt.exact {
            let (mut pq, pd) = project_sample(s, dom, starts, pt.p, Some(pt.q))
                .ok_or(Unmeasured::NoPoint { face: *face, uv: pt.q })?;
            if idx > 0 {
                pq = [uv::unwrap(pq[0], q0[0], dom.period[0]), uv::unwrap(pq[1], q0[1], dom.period[1])];
            }
            pt.q = pq;
            pt.d = pd;
            pt.exact = true;
        }
    }
    Ok(())
}

fn tri_metrics(
    ctx: &FaceBoundCtx<'_>,
    tri: [SamplePt; 3],
) -> Result<(f64, f64, bool, f64), Unmeasured> {
    let dmax = tri[0].d.max(tri[1].d).max(tri[2].d);
    // The distance to a surface is 1-Lipschitz: no point of the triangle is
    // farther from a corner than its longest side, so `dmax + side` bounds
    // it too. It shrinks with each split where the parametric bound cannot
    // (a sliver at a pole spans the whole u range in parameters, #917).
    let side = [(0, 1), (1, 2), (2, 0)]
        .map(|(a, b)| norm(sub(tri[a].p, tri[b].p)))
        .into_iter()
        .fold(0.0_f64, f64::max);
    match ctx {
        FaceBoundCtx::Analytic { surf, face } => {
            let b = analytic_tri_bound(surf, tri.map(|v| v.p), tri.map(|v| v.d))
                .ok_or(Unmeasured::NoPoint { face: *face, uv: tri[0].q })?
                .min(dmax + side);
            Ok((b, (b - dmax).max(0.0), !analytic_is_exact(surf), dmax))
        }
        FaceBoundCtx::Parametric { s, dom, face, .. } => {
            let eps = parametric_eps(s, dom, tri.map(|v| v.q))
                .ok_or(Unmeasured::NoPoint { face: *face, uv: tri[0].q })?
                .min(side);
            Ok((dmax + eps, eps, true, dmax))
        }
    }
}

fn subtri_bound(
    ctx: &FaceBoundCtx<'_>,
    mut tri: [SamplePt; 3],
    depth: u8,
    tol_mm: Option<f64>,
    running_gate: &mut f64,
    (n, budget): (&mut usize, &mut usize),
    stop: &CancelToken,
) -> Result<f64, Unmeasured> {
    let (mut bound, mut eps, can_refine, mut dmax) = tri_metrics(ctx, tri)?;
    if bound > *running_gate && tri.iter().any(|v| !v.exact) {
        ensure_projected(ctx, &mut tri)?;
        (bound, eps, _, dmax) = tri_metrics(ctx, tri)?;
    }
    let cap = if tol_mm.is_some() { MAX_ADAPTIVE_DEPTH } else { MAX_REFINE_DEPTH };
    let refine = bound.is_finite()
        && depth < cap
        && *budget >= 3
        && can_refine
        && bound > *running_gate
        && match tol_mm {
            Some(tol) => dmax <= tol && bound > tol,
            None => eps > (0.15 * (*running_gate).max(dmax)).max(1e-4),
        };
    if !refine {
        *running_gate = running_gate.max(bound);
        return Ok(bound);
    }
    if stop.is_cancelled() {
        return Err(Unmeasured::Cancelled);
    }
    *budget -= 3;
    let (m01, m12, m20) = (eval_mid(ctx, tri[0], tri[1], n)?, eval_mid(ctx, tri[1], tri[2], n)?, eval_mid(ctx, tri[2], tri[0], n)?);
    let mut worst = 0.0_f64;
    for ch in [[tri[0], m01, m20], [tri[1], m12, m01], [tri[2], m20, m12], [m01, m12, m20]] {
        worst = worst.max(subtri_bound(ctx, ch, depth + 1, tol_mm, running_gate, (&mut *n, &mut *budget), stop)?);
    }
    Ok(worst)
}

fn face(
    f: &FacePlan,
    s: &EvalSurface,
    analytic: Option<&brep::Surface>,
    tol_mm: Option<f64>,
    budget: &mut usize,
    stop: &CancelToken,
) -> Result<(f64, f64, f64, f64, usize), Unmeasured> {
    let (dom, starts) = (uv::domain(s), uv::Starts::default());
    let mut nodes: Vec<(P3, [f64; 2], f64)> = Vec::with_capacity(f.uv.len());
    for (k, uv_raw) in f.uv.iter().enumerate() {
        let node = match (f.fixed.get(k), analytic) {
            (Some(q), Some(a)) => {
                let p = [q.x, q.y, q.z];
                let d = analytic_distance(a, p).ok_or(Unmeasured::NoPoint { face: f.face, uv: *uv_raw })?;
                (p, *uv_raw, d)
            }
            (Some(q), None) => {
                let p = [q.x, q.y, q.z];
                let (pq, pd) = project_sample(s, &dom, &starts, p, Some(*uv_raw))
                    .ok_or(Unmeasured::NoPoint { face: f.face, uv: *uv_raw })?;
                (p, pq, pd)
            }
            (None, _) => {
                let p = point(s, *uv_raw).ok_or(Unmeasured::NoPoint { face: f.face, uv: *uv_raw })?;
                let d = match analytic {
                    Some(a) => analytic_distance(a, p).ok_or(Unmeasured::NoPoint { face: f.face, uv: *uv_raw })?,
                    None => 0.0,
                };
                (p, *uv_raw, d)
            }
        };
        nodes.push(node);
    }
    let ctx = match analytic {
        Some(a) => FaceBoundCtx::Analytic { surf: a, face: f.face },
        None => FaceBoundCtx::Parametric { s, dom: &dom, starts: &starts, face: f.face },
    };
    let (mut pm, mut am, mut rm, mut n, mut running_gate) = (0.0_f64, 0.0_f64, 0.0_f64, 0usize, 0.0_f64);
    let exact_analytic = analytic.is_some_and(analytic_is_exact);
    let zero_pt = SamplePt { p: [0.0; 3], q: [0.0; 2], d: 0.0, exact: true };
    for (ti, t) in f.triangles.iter().enumerate() {
        if ti & 0xff == 0 && stop.is_cancelled() {
            return Err(Unmeasured::Cancelled);
        }
        let get = |i: u32| Some((*nodes.get(i as usize)?, *f.uv.get(i as usize)?));
        let (Some(c0), Some(c1), Some(c2)) = (get(t[0]), get(t[1]), get(t[2])) else {
            return Err(Unmeasured::BadIndex { face: f.face });
        };
        let q0 = c0.0.1;
        let q1 = [uv::unwrap(c1.0.1[0], q0[0], dom.period[0]), uv::unwrap(c1.0.1[1], q0[1], dom.period[1])];
        let q2 = [uv::unwrap(c2.0.1[0], q0[0], dom.period[0]), uv::unwrap(c2.0.1[1], q0[1], dom.period[1])];
        // A corner at a pole takes its free parameter from the others
        // (#3034), so no hint inside the triangle crosses the surface.
        let settled = if analytic.is_none() { crate::mesh_pole::settle(s, dom.period, [q0, q1, q2]) } else { None };
        let ([q0, q1, q2], same_uv) = match settled {
            Some(q) => (q, false),
            None => ([q0, q1, q2], q0 == c0.1 && q1 == c1.1 && q2 == c2.1),
        };
        let mut grid = [[zero_pt; 5]; 5];
        for (i, row) in grid.iter_mut().enumerate().take(SAMPLE_DIVISIONS + 1) {
            for (j, cell) in row.iter_mut().enumerate().take(SAMPLE_DIVISIONS - i + 1) {
                let b = [i as f64 / 4.0, j as f64 / 4.0, (4 - i - j) as f64 / 4.0];
                let mix = |x: [f64; 3]| b[0] * x[0] + b[1] * x[1] + b[2] * x[2];
                let tp = [0, 1, 2].map(|k| mix([c0.0.0[k], c1.0.0[k], c2.0.0[k]]));
                let uv_raw = [0, 1].map(|k| mix([c0.1[k], c1.1[k], c2.1[k]]));
                let sp = point(s, uv_raw).ok_or(Unmeasured::NoPoint { face: f.face, uv: uv_raw })?;
                let d_par = norm(sub(sp, tp));
                pm = pm.max(d_par);
                *cell = match (analytic, (i, j)) {
                    (Some(a), _) => {
                        let d = analytic_distance(a, tp).ok_or(Unmeasured::NoPoint { face: f.face, uv: uv_raw })?;
                        am = am.max(d);
                        SamplePt { p: tp, q: [0.0; 2], d, exact: true }
                    }
                    (None, (4, 0)) => SamplePt { p: tp, q: q0, d: c0.0.2, exact: true },
                    (None, (0, 4)) => SamplePt { p: tp, q: q1, d: c1.0.2, exact: true },
                    (None, (0, 0)) => SamplePt { p: tp, q: q2, d: c2.0.2, exact: true },
                    (None, _) => {
                        let hint = [0, 1].map(|k| b[0] * q0[k] + b[1] * q1[k] + b[2] * q2[k]);
                        let d_hint = if same_uv { d_par } else { norm(sub(point(s, hint).ok_or(Unmeasured::NoPoint { face: f.face, uv: hint })?, tp)) };
                        SamplePt { p: tp, q: hint, d: d_hint, exact: d_hint == 0.0 }
                    }
                };
                n = n.saturating_add(1);
            }
        }
        if exact_analytic && let Some(a) = analytic {
            let cs = [grid[4][0], grid[0][4], grid[0][0]];
            let b = analytic_tri_bound(a, cs.map(|v| v.p), cs.map(|v| v.d))
                .ok_or(Unmeasured::NoPoint { face: f.face, uv: [0.0; 2] })?;
            am = am.max(b);
            running_gate = running_gate.max(b);
            continue;
        }
        let mut acc = |tri| -> Result<(), Unmeasured> {
            let b = subtri_bound(&ctx, tri, 0, tol_mm, &mut running_gate, (&mut n, &mut *budget), stop)?;
            if analytic.is_some() { am = am.max(b); } else { rm = rm.max(b); }
            Ok(())
        };
        for i in 0..SAMPLE_DIVISIONS {
            for j in 0..(SAMPLE_DIVISIONS - i) {
                acc([grid[i][j], grid[i + 1][j], grid[i][j + 1]])?;
                if i + j + 2 <= SAMPLE_DIVISIONS {
                    acc([grid[i + 1][j], grid[i + 1][j + 1], grid[i][j + 1]])?;
                }
            }
        }
    }
    Ok((pm, am, rm, if analytic.is_some() { am } else { rm }, n))
}
