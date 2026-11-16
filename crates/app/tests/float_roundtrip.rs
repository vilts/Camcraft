//! Issue #169: floats survive the project file bit-for-bit. Stamps hash
//! the stock and part floats, so a one-ulp misread on load would stale
//! every op after each save and load.
//!
//! Provenance: our own fixtures (an off-round stock placement under a
//! contour of a 10 x 10 x 5 box); specced from the Prime Directive
//! (fail-safe stale gates must not fire on an unchanged project) and the
//! serde_json `float_roundtrip` feature; no vendor material.
//!
//! Also the arc's plane key in the project file (M11.2 A): it stayed in
//! app when the rest of `arc_planes` moved to crates/post/tests (#926).
//!
//! Gate map (#169): covers camcraft_app::project (save, load, ArcDto),
//! camcraft_app::operation (stamp).
//! Lane shape: all tests here run in seconds.

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{Mesh, Point3, cube};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const OFF_ROUND: f64 = -14.966666666666667;

fn contour() -> OpParams {
    OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(10.0),
        retract_z: Height::fixed(8.0),
        top_z: Height::fixed(5.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 2.5,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    }
}

fn tmp(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft_f169_{tag}_{}.camproj", std::process::id()))
}

#[test]
fn off_round_stock_placement_and_contour_survive_save_and_load() {
    let world = new_world();
    let mut h = History::default();
    let shape = StockShape::Box { dx: 100.0, dy: 100.0, dz: 5.0 };
    let at = Location { pos: Point3::new(OFF_ROUND, 0.1 + 0.2, 1.0 / 3.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, shape, at).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, "EM-0600-4FL", tool).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))));
    let cmd = CreateOperation::new(&world, "Contour1", "EM-0600-4FL", SrcRef::Entity("part".into()), contour());
    h.execute(&world, cmd.expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Contour1").expect("generate"));
    let op = model::find_by_name(&world, "Contour1").expect("op");
    assert_eq!(operation::status_of(&world, &op), OpStatus::Ok, "fixture generates Ok");

    let path = tmp("stock");
    project::save(&world, &path).expect("save");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    let _ = std::fs::remove_file(&path);

    let s1 = model::find_by_name(&world, "Stock").expect("stock");
    let s2 = model::find_by_name(&world2, "Stock").expect("loaded stock");
    let (p1, p2) = (model::location_of(&world, &s1).expect("loc"), model::location_of(&world2, &s2).expect("loc2"));
    assert_eq!(p1.x.to_bits(), OFF_ROUND.to_bits());
    for (a, b) in [(p1.x, p2.x), (p1.y, p2.y), (p1.z, p2.z)] {
        assert_eq!(a.to_bits(), b.to_bits(), "stock placement float drifted");
    }
    let op2 = model::find_by_name(&world2, "Contour1").expect("loaded op");
    assert_eq!(operation::status_of(&world2, &op2), OpStatus::Ok, "the loaded contour must not read Stale");
}

#[test]
fn imported_mesh_vertices_are_bit_identical_after_save_and_load() {
    let world = new_world();
    let mut h = History::default();
    let mesh: Mesh = cube(Point3::new(0.1, -14.966666666666667, 1.0 / 3.0), Point3::new(2.0 / 3.0, 7.123456789012345, 0.7));
    h.execute(&world, ImportModel::from_mesh("m", mesh));
    let path = tmp("mesh");
    project::save(&world, &path).expect("save");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    let _ = std::fs::remove_file(&path);

    let a = model::facets_of(&world, &model::find_by_name(&world, "m").expect("m")).expect("facets");
    let b = model::facets_of(&world2, &model::find_by_name(&world2, "m").expect("m2")).expect("facets2");
    assert_eq!(a.len(), b.len());
    for (ta, tb) in a.iter().zip(b.iter()) {
        for (pa, pb) in [(&ta.a, &tb.a), (&ta.b, &tb.b), (&ta.c, &tb.c)] {
            assert_eq!(
                [pa.x.to_bits(), pa.y.to_bits(), pa.z.to_bits()],
                [pb.x.to_bits(), pb.y.to_bits(), pb.z.to_bits()],
                "vertex drifted"
            );
        }
    }
}

/// M11.2 A: an XY arc writes what it always wrote; a vertical plane is
/// stated, and an absent key reads as XY.
#[test]
fn the_project_file_states_only_a_vertical_plane() {
    let xy = project::ArcDto { center: [1.0, 2.0], ccw: true, plane: project::ArcPlaneDto::Xy };
    let xz = project::ArcDto { center: [1.0, 2.0], ccw: false, plane: project::ArcPlaneDto::Xz };
    let sxy = serde_json::to_string(&xy).unwrap_or_default();
    let sxz = serde_json::to_string(&xz).unwrap_or_default();
    assert_eq!(sxy, r#"{"center":[1.0,2.0],"ccw":true}"#, "XY writes what it always wrote");
    assert_eq!(sxz, r#"{"center":[1.0,2.0],"ccw":false,"plane":"xz"}"#);
    let old: project::ArcDto = serde_json::from_str(r#"{"center":[1.0,2.0],"ccw":true}"#).expect("pre-M11.2 arc");
    assert_eq!(old, xy, "absent is XY");
    let back: project::ArcDto = serde_json::from_str(&sxz).expect("xz");
    assert_eq!(back, xz);
    let yz: project::ArcDto = serde_json::from_str(r#"{"center":[0.0,0.0],"ccw":true,"plane":"yz"}"#).expect("yz");
    assert_eq!(yz.plane, project::ArcPlaneDto::Yz);
}
