//! The machining mesh (M12.1, #940): a solidcore-imported solid meshed by
//! solidcore-tess at a chord tolerance in millimetres, handed out ONLY when
//! it is whole and measured inside the tolerance. solidcore-tess is a
//! display mesher that names what it could not do and meshes the rest;
//! here every such shortfall is a named [`MeshRefusal`] - a toolpath over
//! a missing or loose face is a gouge. BENCH / SEAM ONLY: the app still
//! meshes STEP through truck (#917 switches it).
//! Provenance: our own seam design over the public API of solidcore-tess
//! (the solidcore workspace's mesher, Apache-2.0, at the commit pinned in
//! Cargo.toml; solidcore is the Apache-2.0 port of NVIDIA's SMLib). The
//! watertightness count (each side used by exactly two triangles once
//! equal positions are welded) is textbook mesh topology. No HSMWorks
//! material consulted.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use solidcore::kernel::CancelToken;
use solidcore_tess::{MeshTolerance, PlanStats, TessPlan};

use crate::mesh_deviation::{self, Deviation, Unmeasured};
use crate::solidcore_step::StepSolid;
use crate::{P3, mesh_signed_volume};

/// The most triangles one machining mesh may hold: the app's own import
/// budget (`crates/app/src/geometry/import/mod.rs`, `MAX_FACETS`).
pub const MAX_MACHINING_TRIANGLES: usize = 30_000_000;

/// solidcore-tess silently meshes no finer than a millionth of the body's
/// size; a tolerance under that is refused, never quietly coarsened.
pub const MIN_TOL_FRACTION: f64 = 1e-6;

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
    /// Sides used by one triangle only: always 0 in a handed-out mesh.
    pub open_sides: usize,
    /// The sampled deviation, `largest()` at or under `tolerance_mm`.
    pub deviation: Deviation,
    /// Enclosed volume of the mesh (mm^3), positive.
    pub volume: f64,
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

/// Every planner shortfall as a refusal or a note (decision per entry in
/// docs/decisions/940-machining-mesh.md). `Err`: the first refusal.
pub fn judge_stats(stats: &PlanStats) -> Result<Vec<String>, MeshRefusal> {
    if let Some(r) = stats.unmeshed.first() {
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
    Ok(notes)
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
    if stop.is_cancelled() {
        return Err(MeshRefusal::TimedOut("before planning"));
    }
    let tol = MeshTolerance::chord(tol_mm);
    let planned = catch_unwind(AssertUnwindSafe(|| {
        solidcore_tess::plan(record, &tol, size)
    }))
    .map_err(|_| MeshRefusal::Planner("the planner panicked".into()))?;
    let (plan, stats) = planned.map_err(|e| MeshRefusal::Planner(e.to_string()))?;
    if stop.is_cancelled() {
        return Err(MeshRefusal::TimedOut("planning"));
    }
    let notes = judge_stats(&stats)?;
    let planned_tris: usize = plan.faces.iter().map(|f| f.triangles.len()).sum();
    if planned_tris > MAX_MACHINING_TRIANGLES {
        return Err(MeshRefusal::TooLarge(planned_tris));
    }
    let deviation = mesh_deviation::measure(&plan, record, stop).map_err(|e| match e {
        Unmeasured::Cancelled => MeshRefusal::TimedOut("measuring"),
        other => MeshRefusal::Unmeasured(other),
    })?;
    // A NaN deviation is refused too.
    if deviation.largest().is_nan() || deviation.largest() > tol_mm {
        return Err(MeshRefusal::Deviation {
            measured: deviation.largest(),
            tol: tol_mm,
            face: deviation.worst_face,
        });
    }
    let mesh = evaluate(&plan, record.faces.len())?;
    let (positions, triangles, tri_face) = weld(&mesh);
    let (open, over, turned) = sides(&triangles);
    if open > 0 || over > 0 || turned > 0 {
        return Err(MeshRefusal::NotWatertight { open, over, turned });
    }
    let tris: Vec<[usize; 3]> = triangles.iter().map(|t| t.map(|i| i as usize)).collect();
    let volume = mesh_signed_volume(&positions, &tris).unwrap_or(f64::NAN);
    if !(volume.is_finite() && volume > 0.0) {
        return Err(MeshRefusal::Volume(volume));
    }
    let report = MeshReport {
        tolerance_mm: tol_mm,
        triangles: triangles.len(),
        faces: record.faces.len(),
        open_sides: open,
        deviation,
        volume,
        notes,
    };
    Ok(MachiningMesh {
        positions,
        triangles,
        tri_face,
        report,
    })
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

/// The plan evaluated, every face with triangles, none refused.
fn evaluate(plan: &TessPlan, faces: usize) -> Result<solidcore_tess::Mesh, MeshRefusal> {
    let (mesh, refused) = catch_unwind(AssertUnwindSafe(|| plan.evaluate()))
        .map_err(|_| MeshRefusal::Planner("the evaluator panicked".into()))?;
    if let Some(r) = refused.first() {
        return Err(MeshRefusal::FaceLeftOut {
            face: r.face,
            reason: r.reason.clone(),
        });
    }
    let mut has = vec![false; faces];
    for f in &mesh.tri_face {
        if let Some(h) = has.get_mut(*f as usize) {
            *h = true;
        }
    }
    if let Some(face) = has.iter().position(|h| !h) {
        return Err(MeshRefusal::FaceLeftOut {
            face,
            reason: "no triangles in the evaluated mesh".into(),
        });
    }
    Ok(mesh)
}

/// Vertices welded by exact position (+0.0 and -0.0 are one); triangles
/// that collapse onto a repeated vertex are dropped (they carry no area).
fn weld(mesh: &solidcore_tess::Mesh) -> (Vec<P3>, Vec<[u32; 3]>, Vec<u32>) {
    let mut ids: HashMap<[u64; 3], u32> = HashMap::new();
    let mut positions: Vec<P3> = Vec::new();
    let mut map: Vec<u32> = Vec::with_capacity(mesh.positions.len());
    for p in &mesh.positions {
        let q = [p.x, p.y, p.z];
        let key = q.map(|c| (c + 0.0).to_bits());
        let next = u32::try_from(positions.len()).unwrap_or(u32::MAX);
        let id = *ids.entry(key).or_insert_with(|| {
            positions.push(q);
            next
        });
        map.push(id);
    }
    let mut tris = Vec::with_capacity(mesh.triangles.len());
    let mut faces = Vec::with_capacity(mesh.triangles.len());
    for (t, f) in mesh.triangles.iter().zip(&mesh.tri_face) {
        let w = t.map(|i| map.get(i as usize).copied().unwrap_or(u32::MAX));
        if w.contains(&u32::MAX) || w[0] == w[1] || w[1] == w[2] || w[2] == w[0] {
            continue;
        }
        tris.push(w);
        faces.push(*f);
    }
    (positions, tris, faces)
}

/// (sides used once, sides used more than twice, sides used twice in the
/// same direction). Each use counts +1 running low-to-high index, -1 back.
fn sides(tris: &[[u32; 3]]) -> (usize, usize, usize) {
    let mut count: HashMap<(u32, u32), (u32, i32)> = HashMap::new();
    for t in tris {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let e = count.entry((a.min(b), a.max(b))).or_default();
            e.0 = e.0.saturating_add(1);
            e.1 += if a < b { 1 } else { -1 };
        }
    }
    let open = count.values().filter(|n| n.0 == 1).count();
    let over = count.values().filter(|n| n.0 > 2).count();
    let turned = count.values().filter(|n| n.0 == 2 && n.1 != 0).count();
    (open, over, turned)
}
