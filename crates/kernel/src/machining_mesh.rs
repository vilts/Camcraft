//! The machining mesh (M12.1, #940): a solidcore-imported solid meshed by
//! solidcore-tess at a chord tolerance in millimetres, handed out ONLY when
//! it is whole and measured inside the tolerance. solidcore-tess is a
//! display mesher that names what it could not do and meshes the rest;
//! here every such shortfall is a named [`MeshRefusal`] - a toolpath over
//! a missing or loose face is a gouge. Since #917 the app meshes every
//! STEP model here (through `camcraft_geometry::step`).
//! Provenance: our own seam design over the public API of solidcore-tess
//! (the solidcore workspace's mesher, Apache-2.0, at the commit pinned in
//! Cargo.toml; solidcore is the Apache-2.0 port of NVIDIA's SMLib). The
//! watertightness count (each side used by exactly two triangles once
//! equal positions are welded) is textbook mesh topology. No HSMWorks
//! material consulted.

use std::panic::{AssertUnwindSafe, catch_unwind};

use solidcore::kernel::CancelToken;
use solidcore_tess::{MeshTolerance, PlanStats, TessPlan};

use crate::mesh_deviation::{self, Deviation, Unmeasured};
use crate::mesh_weld::{SNAP_MOVE_MM, sides, turn_outward, weld};
use crate::solidcore_step::StepSolid;
use crate::{P3, mesh_signed_volume};

/// The most triangles one machining mesh may hold: the app's own import
/// budget (`crates/app/src/geometry/import/mod.rs`, `MAX_FACETS`).
pub const MAX_MACHINING_TRIANGLES: usize = 30_000_000;

/// solidcore-tess silently meshes no finer than a millionth of the body's
/// size; a tolerance under that is refused, never quietly coarsened.
pub const MIN_TOL_FRACTION: f64 = 1e-6;

/// Factor applied when replanning a mesh whose certified continuous bound
/// overshoots `tol_mm` between planner sag points (M12.1, #1029).
pub const PLAN_MARGIN: f64 = 0.5;

/// A machining mesh: indexed triangles in millimetres, vertices welded by
/// exact position (so a side is shared by index), outward-facing.
#[derive(Debug, Clone, PartialEq)]
pub struct MachiningMesh {
    /// Welded vertex positions (mm).
    pub positions: Vec<P3>,
    /// Triangles, counter-clockwise seen from outside.
    pub triangles: Vec<[u32; 3]>,
    /// The record face of each triangle.
    pub tri_face: Vec<u32>,
    /// What was asked and what was measured.
    pub report: MeshReport,
}

/// The guarantee a [`MachiningMesh`] comes with.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshReport {
    /// The chord tolerance asked for (mm).
    pub tolerance_mm: f64,
    /// Triangles in the mesh.
    pub triangles: usize,
    /// Record faces, every one with triangles.
    pub faces: usize,
    /// Sides used by one triangle only: 0 in a solid's mesh; a sheet's or a
    /// partial mesh's open sides (`machining_mesh_open`).
    pub open_sides: usize,
    /// The sampled deviation, `largest()` at or under `tolerance_mm`.
    pub deviation: Deviation,
    /// Enclosed volume of the mesh (mm^3), positive; 0 for an open mesh.
    pub volume: f64,
    /// The file wound every face of the solid inward and the mesh was
    /// turned outward (#917): the caller says so.
    pub turned_outward: bool,
    /// What the planner noted that does not touch the guarantee (an edge
    /// kept at a finer sampling, a display normal averaged at a pole, a
    /// face the planner capped but measured within tolerance).
    pub notes: Vec<String>,
}

/// Why no machining mesh was handed out. Each names what failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum MeshRefusal {
    /// The tolerance is not a finite positive length, or is under
    /// [`MIN_TOL_FRACTION`] of the body.
    #[error("machining mesh: tolerance {tol} mm refused ({why})")]
    Tolerance { tol: f64, why: String },
    /// The planner refused the body.
    #[error("machining mesh: the mesher refused the body: {0}")]
    Planner(String),
    /// A face has no triangles (planner or evaluator left it out).
    #[error("machining mesh: face {face} left out: {reason}")]
    FaceLeftOut { face: usize, reason: String },
    /// A face every method refused and that was filled leniently.
    #[error("machining mesh: face {face} filled leniently: {reason}")]
    Lenient { face: usize, reason: String },
    /// An edge that could not be sampled.
    #[error("machining mesh: edge {edge} not sampled: {reason}")]
    EdgeNotSampled { edge: usize, reason: String },
    /// A memory bound of the planner left a face off its tolerance.
    #[error("machining mesh: face {face} capped at {sag:.3e} mm sag, tolerance {tol:e} mm")]
    FaceCapped { face: usize, sag: f64, tol: f64 },
    /// An edge's sampling stopped with its chords off the curve.
    #[error("machining mesh: edge {edge} capped at {sag:.3e} mm sag, tolerance {tol:e} mm")]
    EdgeCapped { edge: usize, sag: f64, tol: f64 },
    /// More triangles than [`MAX_MACHINING_TRIANGLES`].
    #[error("machining mesh: {0} triangles is past the {MAX_MACHINING_TRIANGLES} budget")]
    TooLarge(usize),
    /// Sides used by one triangle (a hole), by more than two, or twice in
    /// the same direction (a triangle turned over against its neighbour).
    #[error(
        "machining mesh: not watertight ({open} open, {over} over-shared, {turned} turned sides)"
    )]
    NotWatertight {
        open: usize,
        over: usize,
        turned: usize,
    },
    /// The mesh encloses no positive volume (inside out or flat).
    #[error("machining mesh: encloses volume {0} mm^3, not a positive one")]
    Volume(f64),
    /// A triangle could not be measured.
    #[error("machining mesh: deviation not measurable: {0:?}")]
    Unmeasured(Unmeasured),
    /// The measured deviation is past the tolerance.
    #[error(
        "machining mesh: face {face:?} stands {measured:.3e} mm off its surface, tolerance {tol:e} mm"
    )]
    Deviation {
        measured: f64,
        tol: f64,
        face: Option<u32>,
    },
    /// The caller's time bound.
    #[error("machining mesh: stopped at its time bound ({0})")]
    TimedOut(&'static str),
}

/// A face the planner or the evaluator left without triangles.
#[derive(Debug, Clone, PartialEq)]
pub struct LeftOut {
    /// The record face (index in the record's face order).
    pub face: usize,
    /// Why, as the mesher says it.
    pub reason: String,
}

/// Every planner shortfall as a refusal or a note (decision per entry in
/// docs/decisions/940-machining-mesh.md). `Err`: the first refusal.
pub fn judge_stats(stats: &PlanStats) -> Result<Vec<String>, MeshRefusal> {
    judge(stats, false).map(|(notes, _)| notes)
}

/// [`judge_stats`]; with `keep_left_out` a face the planner left out is
/// handed back, not refused (the partial mesh of `machining_mesh_open`).
fn judge(stats: &PlanStats, keep_left_out: bool) -> Result<(Vec<String>, Vec<LeftOut>), MeshRefusal> {
    let left_out: Vec<LeftOut> =
        stats.unmeshed.iter().map(|r| LeftOut { face: r.face, reason: r.reason.clone() }).collect();
    if let (false, Some(r)) = (keep_left_out, left_out.first()) {
        return Err(MeshRefusal::FaceLeftOut {
            face: r.face,
            reason: r.reason.clone(),
        });
    }
    if let Some(r) = stats.lenient.first() {
        return Err(MeshRefusal::Lenient {
            face: r.face,
            reason: r.reason.clone(),
        });
    }
    if let Some(r) = stats.unsampled_edges.first() {
        return Err(MeshRefusal::EdgeNotSampled {
            edge: r.edge,
            reason: r.reason.clone(),
        });
    }
    if let Some(o) = stats.over_tolerance.iter().find(|o| !o.within) {
        return Err(MeshRefusal::FaceCapped {
            face: o.face,
            sag: o.sag,
            tol: o.tol,
        });
    }
    if let Some(c) = stats.edges_capped.iter().find(|c| !c.within) {
        return Err(MeshRefusal::EdgeCapped {
            edge: c.edge,
            sag: c.sag,
            tol: c.tol,
        });
    }
    let mut notes = Vec::new();
    for o in &stats.over_tolerance {
        notes.push(format!(
            "face {}: a planner bound limited it, measured {:.3e} mm within {:e} mm",
            o.face, o.sag, o.tol
        ));
    }
    for c in &stats.edges_capped {
        notes.push(format!(
            "edge {}: sampling stopped at {:?} with its chords within {:e} mm ({:.3e} mm)",
            c.edge, c.cap, c.tol, c.sag
        ));
    }
    for k in &stats.kept_strict {
        notes.push(format!(
            "edge {}: kept at its finer sampling ({})",
            k.edge, k.reason
        ));
    }
    for p in &stats.averaged_poles {
        notes.push(format!(
            "pole {p:?}: display normal averaged (positions unchanged)"
        ));
    }
    Ok((notes, left_out))
}

/// The machining mesh of one imported solid at `tol_mm` (see the module
/// docs). Bounds: the triangle budget, the planner's own memory bounds
/// (each one that leaves the tolerance is a refusal), and `stop`, polled
/// before and after planning and per face while measuring. The planner
/// itself has no cancellation hook at the pinned commit: it runs to its
/// end inside its memory bounds. A panic in the mesher becomes
/// [`MeshRefusal::Planner`].
pub fn machining_mesh(
    solid: &StepSolid,
    tol_mm: f64,
    stop: &CancelToken,
) -> Result<MachiningMesh, MeshRefusal> {
    let record = &solid.record;
    let planned = planned(record, tol_mm, stop, false)?;
    let notes = planned.notes;
    let (mesh, _) = evaluate(&planned.plan, record.faces.len(), false)?;
    let (positions, triangles, tri_face) = weld(&mesh);
    let (open, over, turned) = sides(&triangles);
    if open > 0 || over > 0 || turned > 0 {
        return Err(MeshRefusal::NotWatertight { open, over, turned });
    }
    let tris: Vec<[usize; 3]> = triangles.iter().map(|t| t.map(|i| i as usize)).collect();
    let mut volume = mesh_signed_volume(&positions, &tris).unwrap_or(f64::NAN);
    let mut triangles = triangles;
    // An inward solid of one shell is turned outward and said by the
    // caller from `turned_outward` (#917); a void solid, zero or NaN stays
    // a refusal (`turn_outward`).
    let turned_outward = turn_outward(&mut triangles, volume).map(|v| volume = v).is_some();
    if !(volume.is_finite() && volume > 0.0) {
        return Err(MeshRefusal::Volume(volume));
    }
    let report = MeshReport {
        tolerance_mm: tol_mm,
        triangles: triangles.len(),
        faces: record.faces.len(),
        open_sides: open,
        deviation: planned.deviation,
        volume,
        turned_outward,
        notes,
    };
    Ok(MachiningMesh {
        positions,
        triangles,
        tri_face,
        report,
    })
}

/// A plan measured inside its tolerance, with the planner's notes and the
/// faces it left out (always none unless asked to keep them).
pub(crate) struct Planned {
    pub plan: TessPlan,
    pub deviation: Deviation,
    pub notes: Vec<String>,
    pub left_out: Vec<LeftOut>,
}

/// The plan of `record` at `tol_mm`, its deviation measured (see
/// [`machining_mesh`]). `keep_left_out`: a face the planner leaves out is
/// handed back in [`Planned::left_out`], not refused.
pub(crate) fn planned(
    record: &solidcore_record::brep::Brep,
    tol_mm: f64,
    stop: &CancelToken,
    keep_left_out: bool,
) -> Result<Planned, MeshRefusal> {
    let size = body_size(record);
    if !(tol_mm.is_finite() && tol_mm > 0.0) {
        return Err(MeshRefusal::Tolerance {
            tol: tol_mm,
            why: "not a finite positive length".into(),
        });
    }
    if !size.is_finite() || tol_mm < size * MIN_TOL_FRACTION {
        return Err(MeshRefusal::Tolerance {
            tol: tol_mm,
            why: format!("under a millionth of the body's {size:.3e} mm size"),
        });
    }
    let mut chord = tol_mm;
    loop {
        if stop.is_cancelled() {
            return Err(MeshRefusal::TimedOut("before planning"));
        }
        let tol = MeshTolerance::chord(chord);
        let planned = catch_unwind(AssertUnwindSafe(|| {
            let planned = solidcore_tess::plan(record, &tol, size)?;
            // The display planner can exhaust its face refinement budget.
            // Try the constrained mesh path; all acceptance checks still apply.
            if planned.1.over_tolerance.iter().any(|o| !o.within) && !stop.is_cancelled() {
                solidcore_tess::mesh(record, &solidcore_tess::MeshSpec::chord(chord), size, None)
            } else {
                Ok(planned)
            }
        }))
        .map_err(|_| MeshRefusal::Planner("the planner panicked".into()))?;
        let (mut plan, stats) = planned.map_err(|e| MeshRefusal::Planner(e.to_string()))?;
        if stop.is_cancelled() {
            return Err(MeshRefusal::TimedOut("planning"));
        }
        let (mut notes, left_out) = judge(&stats, keep_left_out)?;
        // A runaway strip is divided over its boundary nodes (#2844); the
        // measure below judges the plan as it then stands.
        notes.extend(crate::mesh_redivide::redivide(&mut plan, record, tol_mm, stop));
        let planned_tris: usize = plan.faces.iter().map(|f| f.triangles.len()).sum();
        if planned_tris > MAX_MACHINING_TRIANGLES {
            return Err(MeshRefusal::TooLarge(planned_tris));
        }
        let deviation = mesh_deviation::measure_within(&plan, record, tol_mm, stop).map_err(|e| match e {
            Unmeasured::Cancelled => MeshRefusal::TimedOut("measuring"),
            other => MeshRefusal::Unmeasured(other),
        })?;
        // A NaN deviation is refused too.
        // The judged figure adds the snap's largest move (sqrt(3) windows,
        // one in each axis): the weld after this check may move a vertex
        // that far, and the measured bound must still hold for it.
        let judged = deviation.largest() + SNAP_MOVE_MM;
        if judged.is_nan() || judged > tol_mm {
            // The display planner checks sag at centroids and midpoints only;
            // continuous bounds between samples can overshoot near seams. Try
            // one finer plan with `PLAN_MARGIN`, keeping the requested tolerance.
            let finer = (chord * PLAN_MARGIN).max(size * MIN_TOL_FRACTION);
            if chord == tol_mm && finer < chord && deviation.largest().is_finite() {
                chord = finer;
                continue;
            }
            return Err(MeshRefusal::Deviation {
                measured: deviation.largest(),
                tol: tol_mm,
                face: deviation.worst_face,
            });
        }
        if chord < tol_mm {
            notes.push(format!(
                "replanned at {chord:e} mm chord to meet the requested {tol_mm:e} mm measured tolerance"
            ));
        }
        return Ok(Planned { plan, deviation, notes, left_out });
    }
}

/// The body's box diagonal from its vertices (the bench's size, mm).
fn body_size(record: &solidcore_record::brep::Brep) -> f64 {
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for v in &record.vertices {
        for (i, c) in v.point.into_iter().enumerate() {
            lo[i] = lo[i].min(c);
            hi[i] = hi[i].max(c);
        }
    }
    let d = (0..3).map(|i| (hi[i] - lo[i]).powi(2)).sum::<f64>().sqrt();
    if record.vertices.is_empty() { 0.0 } else { d }
}

/// The plan evaluated, every face with triangles, none refused; with
/// `keep_left_out` the faces without triangles are handed back instead.
pub(crate) fn evaluate(
    plan: &TessPlan,
    faces: usize,
    keep_left_out: bool,
) -> Result<(solidcore_tess::Mesh, Vec<LeftOut>), MeshRefusal> {
    let (mesh, refused) = catch_unwind(AssertUnwindSafe(|| plan.evaluate()))
        .map_err(|_| MeshRefusal::Planner("the evaluator panicked".into()))?;
    let mut left_out: Vec<LeftOut> =
        refused.iter().map(|r| LeftOut { face: r.face, reason: r.reason.clone() }).collect();
    let mut has = vec![false; faces];
    for f in &mesh.tri_face {
        if let Some(h) = has.get_mut(*f as usize) {
            *h = true;
        }
    }
    for (face, _) in has.iter().enumerate().filter(|(_, h)| !**h) {
        if !left_out.iter().any(|l| l.face == face) {
            left_out.push(LeftOut { face, reason: "no triangles in the evaluated mesh".into() });
        }
    }
    if let (false, Some(r)) = (keep_left_out, left_out.first()) {
        return Err(MeshRefusal::FaceLeftOut { face: r.face, reason: r.reason.clone() });
    }
    left_out.sort_by_key(|l| l.face);
    Ok((mesh, left_out))
}
