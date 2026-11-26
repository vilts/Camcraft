//! Provenance: regression from our SolidCraft plate fixture reported in #3444.
//! Gate map (M12.1): covers camcraft_kernel::machining_mesh.

use camcraft_kernel::machining_mesh::machining_mesh;
use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_step::import_step;
use solidcore::kernel::CancelToken;

#[test]
fn hole_on_pocket_wall_meshes_within_tolerance() {
    let bytes = include_bytes!("../../../testdata/models/hole_on_pocket_wall.step");
    let mut kernel = SolidcoreKernel::default();
    let mut imported = import_step(&mut kernel, bytes, &CancelToken::new())
        .expect("the hole on the pocket wall must import");
    assert_eq!(imported.solids.len(), 1, "the plate has one solid");
    let solid = imported.solids.remove(0);
    let mesh = machining_mesh(&solid, 0.005, &CancelToken::new())
        .expect("the hole on the pocket wall must mesh within 0.005 mm");
    assert_eq!(mesh.report.open_sides, 0);
    assert!(mesh.report.deviation.largest() <= 0.005, "{:?}", mesh.report.deviation);
    assert_eq!(mesh.report.faces, 13);
    assert!(mesh.report.volume > 0.0);
    let expected_volume = 140_000.0 - 117.0 * std::f64::consts::PI;
    assert!((mesh.report.volume - expected_volume).abs() < 1.0, "{:?}", mesh.report);
}

#[test]
fn hole_on_pocket_wall_solidcraft_meshes_within_tolerance() {
    let bytes = include_bytes!("../../../testdata/models/hole_on_pocket_wall.sc");
    let imported = camcraft_kernel::solidcraft::import_solidcraft(
        &mut SolidcoreKernel::default(),
        bytes,
        &CancelToken::new(),
    )
    .expect("the hole on the pocket wall must import from Solidcraft");
    let mesh = machining_mesh(&imported.solid, 0.005, &CancelToken::new())
        .expect("the Solidcraft hole on the pocket wall must mesh within 0.005 mm");
    assert_eq!(mesh.report.open_sides, 0);
    assert!(mesh.report.deviation.largest() <= 0.005, "{:?}", mesh.report.deviation);
    assert_eq!(mesh.report.faces, 13);
    assert!(mesh.report.volume > 0.0);
    let expected_volume = 140_000.0 - 117.0 * std::f64::consts::PI;
    assert!((mesh.report.volume - expected_volume).abs() < 1.0, "{:?}", mesh.report);
}
