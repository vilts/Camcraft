//! #2844 at the kernel: the seam divides a runaway face over its boundary
//! nodes only where that division measures inside the tolerance. A wide
//! twisted face needs its inside nodes: its boundary division stands
//! millimetres off the surface, so it keeps the planner's triangles and
//! the mesh says nothing about a new division. (The face that does take
//! the boundary division, a thread flank, is pinned where its fixture is
//! made: `crates/app/tests/import_step_thread.rs`.)
//! Fixture (our own, testdata/models/): `twisted_flank.step`, a closed
//! solid over the 40 x 40 mm patch z = 3xy/80.
//! Provenance: our own test design.
//!
//! Gate map (M12.1): covers camcraft_kernel::mesh_redivide,
//! camcraft_kernel::machining_mesh.

use std::path::PathBuf;

use camcraft_kernel::machining_mesh::machining_mesh;
use camcraft_kernel::mesh_redivide::{REDIVIDE_MIN_TRIANGLES, REDIVIDE_RATIO};
use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_step::import_step;
use solidcore::kernel::CancelToken;

/// Fine enough that the planner gives the twisted patch a runaway count.
const TOL: f64 = 1e-3;

#[test]
fn a_wide_twisted_face_keeps_the_planner_division() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/twisted_flank.step");
    let bytes = std::fs::read(path).expect("twisted_flank.step");
    let stop = CancelToken::new();
    let mut import = import_step(&mut SolidcoreKernel::default(), &bytes, &stop).expect("the solid imports");
    let solid = import.solids.remove(0);
    let mesh = machining_mesh(&solid, TOL, &stop).expect("the solid meshes inside the tolerance");
    let mut per_face = vec![0_usize; mesh.report.faces];
    mesh.tri_face.iter().for_each(|f| per_face[*f as usize] += 1);
    let most = per_face.iter().copied().max().unwrap_or(0);
    // The patch is a candidate (so the measure, not the count, kept it):
    // a runaway count on a boundary of four straight sides.
    assert!(most >= REDIVIDE_MIN_TRIANGLES * 2, "the twisted patch holds {most} triangles: {per_face:?}");
    assert!(most / REDIVIDE_RATIO >= 64, "far more triangles than its boundary needs: {most}");
    assert!(mesh.report.notes.iter().all(|n| !n.contains("divided over")), "{:?}", mesh.report.notes);
    assert!(mesh.report.deviation.largest() <= TOL, "{:?}", mesh.report.deviation);
}
