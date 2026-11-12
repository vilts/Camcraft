//! Shared helpers of the STEP bench (step_agreement.rs, step_corrupt.rs):
//! the truck-stepio reading of a file as crates/app does it today, the
//! solidcore mesher over the same file, mesh measures, and a time bound
//! that cancels the import it bounds.
//! Provenance: our own bench design (M12.1 retirement gate). The truck
//! calls are the ones crates/app/src/geometry/import/step.rs makes
//! (ruststep parse, `Table::from_data_section`, `to_compressed_shell`,
//! `robust_triangulation`), at the same chord target; mesh volume is the
//! divergence theorem, mesh area the sum of triangle areas.

#![allow(dead_code)] // each bench binary uses its own part

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc;
use std::time::Duration;

use camcraft_kernel::{P3, mesh_signed_volume};
use solidcore::kernel::CancelToken;
use truck_meshalgo::prelude::*;
use truck_stepio::r#in::Table;

/// No single file may take longer than this (release). At the bound the
/// import's token is cancelled; a case that still does not answer in
/// [`GRACE`] fails as a hang.
pub const CASE_TIMEOUT: Duration = Duration::from_secs(60);
/// How long a cancelled case gets to come back with its named error.
pub const GRACE: Duration = Duration::from_secs(30);

/// The coarsest chord a STEP mesh is made at, in millimetres: the app's
/// `MAX_CHORD` (half the sim's verdict tolerance, #689).
pub const CHORD_MM: f64 = 0.005;
/// truck's least meshing tolerance in the file's unit (the app's `MIN_TOL`).
pub const TRUCK_MIN_TOL: f64 = 1e-5;

/// Runs `f` on its own thread with a token it must poll. At `limit` the
/// token is cancelled; the test fails if `f` still has not answered after
/// [`GRACE`] (a hang is a finding, never a silent wait).
pub fn bounded_cancel<T: Send + 'static>(
    name: &str,
    limit: Duration,
    f: impl FnOnce(&CancelToken) -> T + Send + 'static,
) -> T {
    let token = CancelToken::new();
    let inner = token.clone();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f(&inner));
    });
    if let Ok(v) = rx.recv_timeout(limit) {
        return v;
    }
    token.cancel();
    match rx.recv_timeout(GRACE) {
        Ok(v) => v,
        Err(e) => panic!("{name}: no answer {GRACE:?} after its cancel ({e}) - a hang or a panic"),
    }
}

/// A triangle mesh's numbers, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshMeasure {
    pub volume: f64,
    pub area: f64,
    pub min: P3,
    pub max: P3,
    pub triangles: usize,
    /// Triangle sides used by exactly one triangle once vertices at the
    /// same position are taken as one: 0 for a watertight mesh.
    pub open_edges: usize,
}

/// Measures a triangle soup after scaling it by `k`. `None` for a mesh
/// with an index out of range or a coordinate that is not finite.
pub fn mesh_measure(positions: &[P3], tris: &[[usize; 3]], k: f64) -> Option<MeshMeasure> {
    let pos: Vec<P3> = positions.iter().map(|p| [p[0] * k, p[1] * k, p[2] * k]).collect();
    let volume = mesh_signed_volume(&pos, tris)?;
    let mut area = 0.0_f64;
    let (mut min, mut max) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    // Vertices welded by exact position: the id of the first one seen there.
    let mut weld: HashMap<[u64; 3], usize> = HashMap::new();
    let mut sides: HashMap<(usize, usize), usize> = HashMap::new();
    for t in tris {
        // `as_slice`: truck's prelude brings a `get` of its own onto `Vec`.
        let at = |i: usize| pos.as_slice().get(i).copied();
        let (a, b, c): (P3, P3, P3) = (at(t[0])?, at(t[1])?, at(t[2])?);
        let (u, v) = (sub(b, a), sub(c, a));
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        area += 0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let mut ids = [0usize; 3];
        for (slot, p) in ids.iter_mut().zip([a, b, c]) {
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
            // +0.0 and -0.0 are one position.
            let key = [0, 1, 2].map(|i| (p[i] + 0.0).to_bits());
            let next = weld.len();
            *slot = *weld.entry(key).or_insert(next);
        }
        for (i, j) in [(0, 1), (1, 2), (2, 0)] {
            let side = (ids[i].min(ids[j]), ids[i].max(ids[j]));
            if side.0 != side.1 {
                *sides.entry(side).or_default() += 1;
            }
        }
    }
    let open_edges = sides.values().filter(|n| **n == 1).count();
    (area.is_finite() && min.iter().chain(&max).all(|c| c.is_finite())).then_some(MeshMeasure {
        volume,
        area,
        min,
        max,
        triangles: tris.len(),
        open_edges,
    })
}

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// What truck-stepio makes of a file, as the app's importer reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct TruckStep {
    /// Shells in the file's table (the app meshes each one).
    pub shells: usize,
    /// All shells' meshes together, scaled to millimetres. The volume is
    /// the sum of each shell's absolute mesh volume.
    pub mesh: MeshMeasure,
    /// The chord the meshes were made at, in millimetres.
    pub chord_mm: f64,
}

/// Reads `bytes` with ruststep + truck-stepio and meshes every shell at
/// the app's chord target. `mm_per_unit` is the file's length unit (the
/// bench table states it; truck meshes in the file's unit). Every refusal
/// and every truck panic is an `Err` with its reason.
pub fn truck_read(bytes: &[u8], mm_per_unit: f64) -> Result<TruckStep, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "not valid UTF-8".to_string())?;
    let read = catch_unwind(AssertUnwindSafe(|| truck_read_text(text, mm_per_unit)));
    read.unwrap_or_else(|_| Err("truck panicked on this file".into()))
}

fn truck_read_text(text: &str, k: f64) -> Result<TruckStep, String> {
    let exchange = ruststep::parser::parse(text).map_err(|e| format!("syntax error: {e}"))?;
    let section = exchange.data.first().ok_or("no DATA section")?;
    let table = Table::from_data_section(section);
    if table.shell.is_empty() {
        return Err("no shells found".into());
    }
    let mut ids: Vec<&u64> = table.shell.keys().collect();
    ids.sort_unstable();
    let (mut volume, mut area, mut triangles, mut open_edges) = (0.0_f64, 0.0_f64, 0usize, 0usize);
    let (mut min, mut max) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    let mut chord_mm = 0.0_f64;
    for id in ids {
        let Some(shell) = table.shell.get(id) else {
            continue;
        };
        let cshell = table
            .to_compressed_shell(shell)
            .map_err(|e| format!("shell conversion failed: {e}"))?;
        let extent = cshell
            .vertices
            .iter()
            .flat_map(|p| [p.x.abs(), p.y.abs(), p.z.abs()])
            .fold(0.0_f64, f64::max);
        if !extent.is_finite() {
            return Err("shell vertices are not finite".into());
        }
        // The app's target: 0.1% of the extent, never coarser than the mm
        // cap, never under truck's floor (all in the file's unit).
        let tol = (extent * 1e-3).min(CHORD_MM / k).max(TRUCK_MIN_TOL);
        chord_mm = chord_mm.max(tol * k);
        let meshed = cshell.robust_triangulation(tol).to_polygon();
        let positions: Vec<P3> = meshed.positions().iter().map(|p| [p.x, p.y, p.z]).collect();
        let mut tris: Vec<[usize; 3]> = Vec::new();
        for face in meshed.face_iter() {
            let Some(a) = face.first() else { continue };
            for w in face.get(1..).unwrap_or_default().windows(2) {
                if let [b, c] = w {
                    tris.push([a.pos, b.pos, c.pos]);
                }
            }
        }
        let m = mesh_measure(&positions, &tris, k).ok_or("a shell's mesh is not finite")?;
        volume += m.volume.abs();
        area += m.area;
        triangles += m.triangles;
        open_edges += m.open_edges;
        for i in 0..3 {
            min[i] = min[i].min(m.min[i]);
            max[i] = max[i].max(m.max[i]);
        }
    }
    Ok(TruckStep {
        shells: table.shell.len(),
        mesh: MeshMeasure {
            volume,
            area,
            min,
            max,
            triangles,
            open_edges,
        },
        chord_mm,
    })
}

/// What solidcore's mesher makes of a file's solids.
#[derive(Debug, Clone, PartialEq)]
pub struct SolidcoreMesh {
    /// All solids' meshes together (mm); volume summed per solid.
    pub mesh: MeshMeasure,
    /// Faces the planner or the evaluator left out, with why.
    pub refused_faces: Vec<String>,
    /// Faces and edges the planner says left the chord tolerance.
    pub over_tolerance: usize,
}

/// Meshes every solid record of `bytes` with solidcore-tess at
/// [`CHORD_MM`] (the record is already in millimetres).
pub fn solidcore_mesh(bytes: &[u8]) -> Result<SolidcoreMesh, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "not valid UTF-8".to_string())?;
    let run = catch_unwind(AssertUnwindSafe(|| solidcore_mesh_text(text)));
    run.unwrap_or_else(|_| Err("solidcore-tess panicked on this file".into()))
}

fn solidcore_mesh_text(text: &str) -> Result<SolidcoreMesh, String> {
    let ex = solidcore_step::p21::parse(text).map_err(|e| e.to_string())?;
    let read = solidcore_step::topology::read_solids(&ex);
    if let Some(f) = read.failures.first() {
        return Err(format!("solid #{} not read: {}", f.item, f.message));
    }
    let mut out = SolidcoreMesh {
        mesh: MeshMeasure {
            volume: 0.0,
            area: 0.0,
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
            triangles: 0,
            open_edges: 0,
        },
        refused_faces: Vec::new(),
        over_tolerance: 0,
    };
    for s in &read.solids {
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for v in &s.brep.vertices {
            for (i, c) in v.point.into_iter().enumerate() {
                lo[i] = lo[i].min(c);
                hi[i] = hi[i].max(c);
            }
        }
        let size = (0..3).map(|i| (hi[i] - lo[i]).powi(2)).sum::<f64>().sqrt();
        let tol = solidcore_tess::MeshTolerance::chord(CHORD_MM);
        let (plan, stats) = solidcore_tess::plan(&s.brep, &tol, size).map_err(|e| e.to_string())?;
        let (mesh, refused) = plan.evaluate();
        for r in stats.unmeshed.iter().chain(&stats.lenient).chain(&refused) {
            out.refused_faces.push(format!("face {}: {}", r.face, r.reason));
        }
        out.over_tolerance += stats.over_tolerance.len() + stats.edges_capped.len();
        let positions: Vec<P3> = mesh.positions.iter().map(|p| [p.x, p.y, p.z]).collect();
        let tris: Vec<[usize; 3]> = mesh
            .triangles
            .iter()
            .map(|t| [t[0] as usize, t[1] as usize, t[2] as usize])
            .collect();
        let m = mesh_measure(&positions, &tris, 1.0).ok_or("a solid's mesh is not finite")?;
        out.mesh.volume += m.volume.abs();
        out.mesh.area += m.area;
        out.mesh.triangles += m.triangles;
        out.mesh.open_edges += m.open_edges;
        for i in 0..3 {
            out.mesh.min[i] = out.mesh.min[i].min(m.min[i]);
            out.mesh.max[i] = out.mesh.max[i].max(m.max[i]);
        }
    }
    Ok(out)
}
