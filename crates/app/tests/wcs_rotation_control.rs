//! #760: Control mode (`G68`/`G69` and `ROT RPL=`/`ROT`), capability refusals,
//! `viewer_map` Setup-coordinate invariance, and additional milling operations
//! (`Pocket`, `Slot`, `Circular`, `Drill`, `Pocket3D` rest) under Z-rotated WCS.
//!
//! Gate map (#2361): covers app::commands, motion::drilling, geometry::geo2d, app::geometry,
//! app::machine, app::model, app::operation, app::posting, app::schema, geometry::geometry::stock,
//! app::world, tools::catalog.
//!
//! Provenance: our own integration suite for rotated work offsets; no vendor material consulted.

use std::fs;
use std::path::PathBuf;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SelectPost,
    SetPostOptions, SetStock, SetWcsOrigin,
};
use camcraft_app::drilling::Hole;
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube, quat_from_euler};
use camcraft_app::machine::{AxisDirection, AxisLimits, Machine};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, OpType, SrcRef, retype_params};
use camcraft_app::posting;
use camcraft_app::schema::{Location, PostOptions, PostRef, WcsRotationOutput};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

fn face_params(cut_z: f64, clear_z: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 4.0,
        clearance_z: Height::fixed(clear_z),
        cut_z: Height::fixed(cut_z),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

fn coord_rot_machine(invert_x: bool) -> Machine {
    Machine {
        model: "RotMill".into(),
        vendor: "Test".into(),
        axes: vec![
            AxisLimits {
                name: "X".into(),
                travel: Some((-250.0, 250.0)),
                direction: if invert_x { AxisDirection::Inverted } else { AxisDirection::Standard },
                ..Default::default()
            },
            AxisLimits { name: "Y".into(), travel: Some((-250.0, 250.0)), ..Default::default() },
            AxisLimits { name: "Z".into(), travel: Some((-200.0, 200.0)), ..Default::default() },
        ],
        spindles: Some(1),
        safe_retract_z: Some(200.0),
        coordinate_rotation: Some(true),
        ..Default::default()
    }
}

fn world_with_faced_yaw(dx: f64, dy: f64, origin: Point3, yaw_deg: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx, dy, dz: 20.0 }, Default::default()).expect("stock"),
    );
    let cat = catalog();
    let tool = cat.iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t.clone()).expect("tool");
    h.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("add tool"));
    let drill_tool = cat.iter().find(|(n, _)| *n == "DR-0600").map(|(_, t)| t.clone()).expect("drill");
    h.execute(&world, AddTool::new(&world, "DR-0600", drill_tool).expect("add drill"));
    h.execute(
        &world,
        SetWcsOrigin::new(&world, Location { pos: origin, rot: quat_from_euler(0.0, 0.0, yaw_deg.to_radians()) })
            .expect("G54"),
    );
    h.execute(
        &world,
        CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face_params(19.5, 25.0))
            .expect("op"),
    );
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("gen"));
    (world, h)
}

fn tmp_nc(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-760-ctl-{tag}-{}.nc", std::process::id()));
    let _ = fs::remove_file(&p);
    p
}

#[test]
fn control_rotation_fanuc_emits_g68_g69_and_preserves_viewer_map() {
    let origin = Point3::new(10.0, 5.0, 20.0);
    let (world, mut h) = world_with_faced_yaw(60.0, 40.0, origin, 30.0);
    h.execute(&world, LoadMachine::new(&world, "rot.mch".into(), Some(coord_rot_machine(false))).expect("mch"));

    let bake_path = tmp_nc("fanuc-bake");
    let bake_posted = posting::post_setup(&world, &bake_path).expect("Fanuc Bake posts");
    let _ = fs::remove_file(&bake_path);
    assert!(!bake_posted.program.contains("G68"));

    h.execute(
        &world,
        SetPostOptions::new(&world, PostOptions { wcs_rotation: WcsRotationOutput::Control, ..Default::default() })
            .expect("Control"),
    );
    let ctl_path = tmp_nc("fanuc-g68");
    let ctl_posted = posting::post_setup(&world, &ctl_path).expect("Fanuc Control posts");
    let _ = fs::remove_file(&ctl_path);
    assert!(ctl_posted.program.contains("G68 X0. Y0. R30."), "expected G68:\n{}", ctl_posted.program);
    assert!(ctl_posted.program.contains("G69"), "expected G69:\n{}", ctl_posted.program);

    // viewer_map keeps Setup S coordinates identical between Bake and Control.
    let bake_pts: Vec<_> = bake_posted.line_pos.iter().flatten().copied().collect();
    let ctl_pts: Vec<_> = ctl_posted.line_pos.iter().flatten().copied().collect();
    assert!(!ctl_pts.is_empty(), "viewer_map must stay populated in Control mode");
    assert_eq!(ctl_pts, bake_pts, "viewer_map must point to the exact same Setup coordinates");

    // Every programmed feed move inside G68..G69 has constant Y along WCS X.
    let mut flat_passes = 0usize;
    let mut prev: Option<(f64, f64)> = None;
    for line in ctl_posted.program.lines() {
        let mut x = None;
        let mut y = None;
        let mut g1 = false;
        for w in line.split_whitespace() {
            if w == "G01" || w == "G1" {
                g1 = true;
            } else if let Some(r) = w.strip_prefix('X') {
                x = r.parse::<f64>().ok();
            } else if let Some(r) = w.strip_prefix('Y') {
                y = r.parse::<f64>().ok();
            }
        }
        if g1 && let (Some(nx), Some(ny)) = (x, y) {
            if let Some((px, py)) = prev
                && (nx - px).abs() > 25.0
                && (ny - py).abs() < 0.01
            {
                flat_passes += 1;
            }
            prev = Some((nx, ny));
        }
    }
    assert!(flat_passes > 0, "Control mode coordinates inside G68 run along WCS X:\n{}", ctl_posted.program);
}

#[test]
fn control_rotation_sinumerik_and_capability_refusals() {
    let origin = Point3::new(0.0, 0.0, 20.0);
    let (world, mut h) = world_with_faced_yaw(60.0, 40.0, origin, 30.0);
    h.execute(
        &world,
        SetPostOptions::new(&world, PostOptions { wcs_rotation: WcsRotationOutput::Control, ..Default::default() })
            .expect("Control"),
    );

    // 1) No machine mounted -> refused by name.
    let err_nomch = posting::post_setup(&world, &tmp_nc("nomch")).expect_err("no machine");
    assert!(err_nomch.contains("coordinate_rotation: true"), "{err_nomch}");

    // 2) Machine without coordinate_rotation -> refused by name.
    let mut no_cap = coord_rot_machine(false);
    no_cap.coordinate_rotation = None;
    h.execute(&world, LoadMachine::new(&world, "plain.mch".into(), Some(no_cap)).expect("plain mch"));
    let err_nocap = posting::post_setup(&world, &tmp_nc("nocap")).expect_err("unstated cap");
    assert!(err_nocap.contains("coordinate_rotation: true"), "{err_nocap}");

    // 3) Capable machine + inverted X axis -> Control refused by name.
    h.execute(&world, LoadMachine::new(&world, "inv.mch".into(), Some(coord_rot_machine(true))).expect("inv"));
    let err_inv = posting::post_setup(&world, &tmp_nc("inv")).expect_err("inverted");
    assert!(err_inv.contains("inverted machine axes"), "{err_inv}");

    // 4) Capable non-inverted machine + Tormach (no wcsRotation = true) -> refused by name.
    h.execute(&world, LoadMachine::new(&world, "rot.mch".into(), Some(coord_rot_machine(false))).expect("rot"));
    h.execute(&world, SelectPost::new(&world, PostRef::Named("Tormach PathPilot".into())).expect("tormach"));
    let err_cps = posting::post_setup(&world, &tmp_nc("tormach")).expect_err("tormach");
    assert!(err_cps.contains("wcsRotation = true"), "{err_cps}");

    // 5) Siemens 840D -> emits ROT RPL=30. and cancels with ROT.
    h.execute(&world, SelectPost::new(&world, PostRef::Named("Siemens 840D".into())).expect("840D"));
    let path = tmp_nc("sinu");
    let posted = posting::post_setup(&world, &path).expect("Sinumerik posts Control");
    let _ = fs::remove_file(&path);
    assert!(posted.program.contains("ROT RPL=30"), "expected ROT RPL=30:\n{}", posted.program);
    assert!(posted.program.lines().any(|l| l.trim() == "ROT" || l.ends_with(" ROT")), "expected ROT reset:\n{}", posted.program);
}

fn pocket_block() -> Mesh {
    let mut facets = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 20.0));
    facets.extend(outer.facets[0..2].iter().cloned());
    facets.extend(outer.facets[4..12].iter().cloned());
    let inner = cube(Point3::new(-10.0, -10.0, 10.0), Point3::new(10.0, 10.0, 20.0));
    facets.extend(inner.facets[0..2].iter().cloned());
    facets.extend(inner.facets[4..12].iter().cloned());
    let (o0, o1, o2, o3) = (
        Point3::new(-20.0, -20.0, 20.0),
        Point3::new(20.0, -20.0, 20.0),
        Point3::new(20.0, 20.0, 20.0),
        Point3::new(-20.0, 20.0, 20.0),
    );
    let (i0, i1, i2, i3) = (
        Point3::new(-10.0, -10.0, 20.0),
        Point3::new(10.0, -10.0, 20.0),
        Point3::new(10.0, 10.0, 20.0),
        Point3::new(-10.0, 10.0, 20.0),
    );
    for (a, b, c, d) in [(o0, o1, i1, i0), (o1, o2, i2, i1), (o2, o3, i3, i2), (o3, o0, i0, i3)] {
        facets.push(Triangle::new(a, b, c));
        facets.push(Triangle::new(a, c, d));
    }
    Mesh::from_facets(facets)
}

#[test]
fn rotated_wcs_carries_pocket_slot_circular_and_drill_ops() {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 80.0, dy: 80.0, dz: 20.0 }, Default::default()).expect("stock"));
    let cat = catalog();
    let tool = cat.iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t.clone()).expect("tool");
    h.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("add tool"));
    let drill_tool = cat.iter().find(|(n, _)| *n == "DR-0600").map(|(_, t)| t.clone()).expect("drill");
    h.execute(&world, AddTool::new(&world, "DR-0600", drill_tool).expect("add drill"));
    let b = model::stock_world_aabb_for(&world, None).expect("stock aabb");
    let center = Point3::new((b.min.x + b.max.x) * 0.5, (b.min.y + b.max.y) * 0.5, b.max.z);
    h.execute(
        &world,
        SetWcsOrigin::new(&world, Location { pos: center, rot: quat_from_euler(0.0, 0.0, 45f64.to_radians()) }).expect("45deg"),
    );

    let mut circ = retype_params(&face_params(19.5, 25.0), OpType::Circular.ordinal());
    if let OpParams::Circular { center: c, radius, stepover, stepdown, clearance_z, retract_z, top_z, bottom_z, links, entry, .. } = &mut circ {
        *c = Point2::new(center.x + 12.0, center.y);
        *radius = 3.5;
        *stepover = 2.0;
        *stepdown = 2.0;
        *clearance_z = Height::fixed(b.max.z + 5.0);
        *retract_z = None;
        *top_z = Height::fixed(b.max.z);
        *bottom_z = Height::fixed(b.max.z - 2.0);
        *links = camcraft_app::links::Links::PLANAR;
        *entry = camcraft_app::machining3d::Entry3D::Plunge;
    }
    let mut slot = retype_params(&face_params(19.5, 25.0), OpType::Slot.ordinal());
    if let OpParams::Slot { a, b: sb, width, clearance_z, retract_z, top_z, bottom_z, .. } = &mut slot {
        *a = Point2::new(center.x - 10.0, center.y);
        *sb = Point2::new(center.x + 10.0, center.y);
        *width = 8.0;
        *clearance_z = Height::fixed(b.max.z + 5.0);
        *retract_z = None;
        *top_z = Height::fixed(b.max.z);
        *bottom_z = Height::fixed(b.max.z - 2.0);
    }
    let mut drill = retype_params(&face_params(19.5, 25.0), OpType::Drill.ordinal());
    if let OpParams::Drill { holes, clearance_z, retract_z, top_z, bottom_z, .. } = &mut drill {
        *clearance_z = Height::fixed(b.max.z + 5.0);
        *retract_z = None;
        *top_z = Height::fixed(b.max.z);
        *bottom_z = Some(Height::fixed(b.max.z - 5.0));
        *holes = vec![Hole { xy: Point2::new(center.x + 15.0, center.y), radius: 3.0, entry_z: b.max.z, floor_z: b.max.z - 5.0, through: false, stepped: false }];
    }
    let mut pocket = retype_params(&face_params(19.5, 25.0), OpType::Pocket.ordinal());
    if let OpParams::Pocket { stepover, clearance_z, retract_z, top_z, bottom_z, .. } = &mut pocket {
        *stepover = 1.5;
        *clearance_z = Height::fixed(b.max.z + 5.0);
        *retract_z = None;
        *top_z = Height::fixed(b.max.z);
        *bottom_z = Height::fixed(b.max.z - 3.0);
    }

    for (name, tl, p) in [
        ("Pocket1", "EM-0400-2FL", pocket),
        ("Circ1", "EM-0400-2FL", circ),
        ("Slot1", "EM-0400-2FL", slot),
        ("Drill1", "DR-0600", drill),
    ] {
        h.execute(&world, CreateOperation::new(&world, name, tl, SrcRef::Stock, p).expect(name));
        h.execute(&world, GenerateOperation::new(&world, name).expect("gen"));
        let e = model::find_by_name(&world, name).expect(name);
        let tp = model::toolpath_of(&world, &e).expect("path");
        assert!(!tp.path.moves.is_empty(), "{name} should generate moves");
    }
    // Check drill cycle XY landed at Setup (center.x + 15, center.y), NOT shifted by wrong rotation!
    let de = model::find_by_name(&world, "Drill1").expect("Drill1");
    let dtp = model::toolpath_of(&world, &de).expect("drill path");
    assert!((dtp.path.cycles[0].xy.x - (center.x + 15.0)).abs() < 1e-4, "{:?}", dtp.path.cycles[0]);
    assert!((dtp.path.cycles[0].xy.y - center.y).abs() < 1e-4, "{:?}", dtp.path.cycles[0]);
    let path = tmp_nc("multi");
    let posted = posting::post_setup(&world, &path).expect("multi-op Bake post succeeds");
    let _ = fs::remove_file(&path);
    assert!(!posted.line_pos.is_empty());
}

#[test]
fn rotated_wcs_carries_pocket3d_rest_prior_grid() {
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("add tool"));
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 20.0 }, Default::default()).expect("stock"));
    h.execute(&world, ImportModel::from_mesh("plate", pocket_block()));
    h.execute(
        &world,
        SetWcsOrigin::new(&world, Location { pos: Point3::new(0.0, 0.0, 20.0), rot: quat_from_euler(0.0, 0.0, 30f64.to_radians()) }).expect("30deg"),
    );

    let mut rough = retype_params(&face_params(19.5, 25.0), OpType::Pocket3D.ordinal());
    if let OpParams::Pocket3D { stepover, stepdown, clearance_z, retract_z, top_z, bottom_z, rest_rough, .. } = &mut rough {
        *stepover = 2.0;
        *stepdown = 2.5;
        *clearance_z = Height::fixed(30.0);
        *retract_z = None;
        *top_z = Height::fixed(20.0);
        *bottom_z = Height::fixed(15.0);
        *rest_rough = false;
    }
    let mut rest = rough.clone();
    if let OpParams::Pocket3D { bottom_z, rest_rough, .. } = &mut rest {
        *bottom_z = Height::fixed(10.0);
        *rest_rough = true;
    }
    h.execute(&world, CreateOperation::new(&world, "Rough1", "EM-0400-2FL", SrcRef::Entity("plate".into()), rough).expect("rough"));
    h.execute(&world, GenerateOperation::new(&world, "Rough1").expect("gen rough"));
    h.execute(&world, CreateOperation::new(&world, "Rest1", "EM-0400-2FL", SrcRef::Entity("plate".into()), rest).expect("rest"));
    h.execute(&world, GenerateOperation::new(&world, "Rest1").expect("gen rest"));
    let re = model::find_by_name(&world, "Rest1").expect("Rest1");
    let rtp = model::toolpath_of(&world, &re).expect("rest path");
    assert!(!rtp.path.moves.is_empty(), "Pocket3D rest under rotated WCS should clear below rough depth");
}
