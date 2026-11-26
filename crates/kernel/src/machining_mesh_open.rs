//! Machining meshes with open sides (#917): the mesh of a sheet (a shell of
//! a SHELL_BASED_SURFACE_MODEL, open by the file's own word), and the
//! partial mesh of a solid whose faces the mesher left out, each such face
//! handed back by index with the mesher's reason. Neither is a solid: a
//! caller lands a partial mesh only as an incomplete model with every
//! left-out face named (#1610), never as a whole one. Every other shortfall
//! (a lenient fill, an unsampled edge, a capped face, a deviation past the
//! tolerance, an over-shared or turned side) stays the refusal of
//! [`machining_mesh`](crate::machining_mesh::machining_mesh).
//! Provenance: our own seam design over the public API of solidcore-tess
//! (Apache-2.0); open and over-shared sides by textbook mesh topology. No
//! HSMWorks material consulted.

use solidcore::kernel::CancelToken;

use crate::machining_mesh::{LeftOut, MachiningMesh, MeshRefusal, MeshReport, evaluate, planned};
use crate::mesh_weld::{sides, weld};
use crate::solidcore_step::StepSolid;

/// A solid meshed with faces left out: open sides where they were.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenMesh {
    /// The faces that have triangles, measured inside the tolerance; its
    /// report's `volume` is 0 (an open mesh encloses none).
    pub mesh: MachiningMesh,
    /// Every face without triangles, by record index, in index order.
    pub left_out: Vec<LeftOut>,
}

/// The mesh of an open shell's record (a sheet) at `tol_mm`: open sides are
/// the sheet's own boundary and allowed; a face left out, an over-shared or
/// a turned side is refused as in [`machining_mesh`](crate::machining_mesh::machining_mesh).
pub fn sheet_mesh(
    record: &solidcore_record::brep::Brep,
    tol_mm: f64,
    stop: &CancelToken,
) -> Result<MachiningMesh, MeshRefusal> {
    let planned = planned(record, tol_mm, stop, false)?;
    let (mesh, _) = evaluate(&planned.plan, record.faces.len(), false)?;
    open_mesh(record, tol_mm, planned.deviation, planned.notes, &mesh)
}

/// The partial mesh of `solid` at `tol_mm`: as [`machining_mesh`](crate::machining_mesh::machining_mesh),
/// but a face the planner or the evaluator leaves out is handed back in
/// [`OpenMesh::left_out`], not refused. A caller asks for it only after the
/// strict mesh refused a face left out.
pub fn partial_mesh(solid: &StepSolid, tol_mm: f64, stop: &CancelToken) -> Result<OpenMesh, MeshRefusal> {
    let record = &solid.record;
    let planned = planned(record, tol_mm, stop, true)?;
    let (mesh, mut left_out) = evaluate(&planned.plan, record.faces.len(), true)?;
    for l in planned.left_out {
        if !left_out.iter().any(|k| k.face == l.face) {
            left_out.push(l);
        }
    }
    left_out.sort_by_key(|l| l.face);
    if left_out.len() >= record.faces.len() {
        return Err(MeshRefusal::FaceLeftOut {
            face: 0,
            reason: "every face of the solid was left out".into(),
        });
    }
    let mesh = open_mesh(record, tol_mm, planned.deviation, planned.notes, &mesh)?;
    Ok(OpenMesh { mesh, left_out })
}

/// The welded mesh with open sides allowed, over-shared and turned sides
/// refused.
fn open_mesh(
    record: &solidcore_record::brep::Brep,
    tol_mm: f64,
    deviation: crate::mesh_deviation::Deviation,
    notes: Vec<String>,
    mesh: &solidcore_tess::Mesh,
) -> Result<MachiningMesh, MeshRefusal> {
    let (positions, triangles, tri_face) = weld(mesh);
    let (open, over, turned) = sides(&triangles);
    if over > 0 || turned > 0 {
        return Err(MeshRefusal::NotWatertight { open, over, turned });
    }
    let report = MeshReport {
        tolerance_mm: tol_mm,
        triangles: triangles.len(),
        faces: record.faces.len(),
        open_sides: open,
        deviation,
        volume: 0.0,
        turned_outward: false,
        notes,
    };
    Ok(MachiningMesh {
        positions,
        triangles,
        tri_face,
        report,
    })
}
