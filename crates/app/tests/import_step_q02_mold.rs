//! #3034: the recorded QA part `q02-3d-mold.step` (a mould base with an
//! ellipsoid boss whose B-spline face ends in a pole) imports as a closed
//! solid and slices closed at the reported height. Before the fix the
//! import refused it: "face Some(10) stands 5.089e-1 mm off its surface".
//!
//! Fixture: `testdata/models/q02_3d_mold.step`, the recorded QA file of
//! issue #3034, byte for byte (347101 bytes). Our own part, made in
//! SolidCraft for QA; no outside material.
//!
//! Gate map (M4.4): covers app::geometry::import::step,
//! app::geometry::import (parse_model), camcraft_geometry::geometry::slice,
//! app::operation::generate, camcraft_motion::finishing3d.

use std::path::Path;

use camcraft_app::geometry::import::{self, Format};
use camcraft_app::geometry::{Point3, slice};

/// SolidCraft's volume of the part, mm^3 (from the QA run of #3034).
const SOLIDCRAFT_VOLUME: f64 = 97165.69;

#[test]
fn the_recorded_q02_mould_imports_closed_and_slices_at_the_reported_height() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/q02_3d_mold.step");
    let bytes = std::fs::read(path).expect("fixture");
    let mesh = import::parse_model(&bytes, Format::Step)
        .unwrap_or_else(|e| panic!("the q02 mould must import: {e}"));
    // Signed volume (divergence theorem): closed and wound outward.
    let o = Point3::new(0.0, 0.0, 0.0);
    let volume: f64 = mesh
        .facets
        .iter()
        .map(|t| (t.a - o).dot((t.b - o).cross(t.c - o)) / 6.0)
        .sum();
    assert!(
        (volume - SOLIDCRAFT_VOLUME).abs() < 10.0,
        "volume {volume} vs SolidCraft {SOLIDCRAFT_VOLUME}"
    );
    // z = 19.6875 is the height the issue reports; the others cross the
    // boss flank and its cap. Areas measured on the fixed tree.
    for (z, want) in [
        (18.5, 1468.32),
        (19.6875, 1382.27),
        (20.0, 1371.70),
        (25.0, 932.50),
        (29.0, 225.77),
    ] {
        let region = slice::slice(&mesh.facets, z).unwrap_or_else(|e| panic!("z = {z}: {e}"));
        let area = region.area();
        assert!(
            (area - want).abs() < 0.5,
            "z = {z}: section {area} vs {want}"
        );
    }
}

#[test]
#[ignore = "heavy lane: 3D generation on the recorded q02 STEP mesh"]
fn the_recorded_q02_mould_generates_a_3d_flat_finish() {
    use camcraft_app::cl::MoveKind;
    use camcraft_app::commands::{
        AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock,
    };
    use camcraft_app::finishing3d::{FinishPattern, GapLink};
    use camcraft_app::operation::{Height, OpParams, SrcRef};
    use camcraft_app::schema::Location;
    use camcraft_app::stock::StockShape;

    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/q02_3d_mold.step");
    let import = ImportModel::new(&path).expect("the recorded q02 mould must import");
    let part_name = import.name.clone();
    history.execute(&world, import);
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 100.0,
                dy: 80.0,
                dz: 35.0,
            },
            Location::default(),
        )
        .expect("stock"),
    );
    let tool = camcraft_app::tools::catalog()
        .into_iter()
        .find(|(name, _)| *name == "BN-0600")
        .expect("ball-nose tool")
        .1;
    history.execute(
        &world,
        AddTool::new(&world, "BN-0600", tool).expect("add tool"),
    );
    // Finish the actual horizontal floor around the boss. Flat uses the
    // full imported mesh for support and clearance, without the costly
    // Waterline ladder over the curved boss.
    let params = OpParams::Finish3D {
        pattern: FinishPattern::Flat,
        direction_angle: 0.0,
        pass_start: camcraft_app::machining2d::PassStart::Auto,
        gap_link: GapLink::Jump,
        stepover: Some(2.0),
        tolerance: 0.05,
        clearance_z: Height::fixed(40.0),
        retract_z: None,
        top_z: Height::fixed(30.0),
        bottom_z: Height::fixed(18.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
        fill_openings: None,
        waterlines: true,
        picks: Default::default(),
    };
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Q02Flat",
            "BN-0600",
            SrcRef::Entity(part_name),
            params,
        )
        .expect("create 3D finish"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Q02Flat").expect("generate 3D finish"),
    );
    let op = camcraft_app::model::find_by_name(&world, "Q02Flat").expect("operation");
    let toolpath = camcraft_app::model::toolpath_of(&world, &op).expect("generated toolpath");
    assert!(
        toolpath.path.all_finite(),
        "the generated path must be finite"
    );
    let floor_cut: f64 = toolpath
        .path
        .moves
        .iter()
        .filter(|m| {
            m.kind == MoveKind::Feed
                && (m.from.z - 18.0).abs() < 1e-6
                && (m.to.z - 18.0).abs() < 1e-6
        })
        .map(|m| (m.to.x - m.from.x).hypot(m.to.y - m.from.y))
        .sum();
    assert!(
        floor_cut > 10.0,
        "the 3D finish must cut the q02 floor: {floor_cut} mm"
    );
}
