//! #2389: a tilted (3+2) operation measures its reference heights along
//! its turned tool axis. "Top of stock" on a Slot indexed to A 90 is the
//! stock's extent toward the tool, not the setup's highest Z: on main it
//! read the setup Z (15 on a 10 x 30 bar) and cut air - or, on the other
//! sides of an asymmetric bar, plunged as if the stock were thinner or
//! thicker than it is.
//!
//! Provenance: our own synthetic fixtures (box stocks off the A axis),
//! rigid-frame projection by hand; no vendor material.
//!
//! Gate map (#2389): covers app::operation (tilt_heights, job,
//! generate's height fold, material_source's section), cl::frame.
//! Lane shape: all tests here run in seconds.

use camcraft_app::cl::{MoveKind, TiltedFrame};
use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::Point3;
use camcraft_app::model;
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef, Tilt};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use flecs_ecs::prelude::World;

const TIGHT: f64 = 1e-9;

fn from_stock(offset: f64) -> Height {
    Height { reference: HeightReference::TopOfStock, offset }
}

/// A slot along local X across the bar, every height from Top of stock:
/// clearance +10, top +0, bottom -1 (one 1 mm level).
fn slot(tilt: Tilt) -> OpParams {
    OpParams::Slot {
        links: camcraft_app::links::Links::PLANAR,
        a: Point2::new(-10.0, 0.0),
        b: Point2::new(10.0, 0.0),
        width: 6.0,
        stock_to_leave: 0.0,
        clearance_z: from_stock(10.0),
        retract_z: None,
        top_z: from_stock(0.0),
        bottom_z: from_stock(-1.0),
        stepdown: 1.0,
        stepover: 4.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        tilt: Some(tilt),
        extend_start: 0.0,
        extend_end: 0.0,
    }
}

fn tilt(a_deg: f64, b_deg: f64, origin: [f64; 3]) -> Tilt {
    Tilt { basis_version: 1, a_deg, b_deg, origin }
}

/// A box stock `dx` x `dy` x `dz` whose centre sits at (0, `cy`, `cz`):
/// a box stands on its location's Z and is centred on its X and Y.
fn bench(dy: f64, dz: f64, cy: f64, cz: f64) -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let stock = StockShape::Box { dx: 40.0, dy, dz };
    let at = Location { pos: Point3::new(0.0, cy, cz - dz * 0.5), ..Default::default() };
    history.execute(&world, SetStock::new(&world, stock, at).expect("stock"));
    let (_, tool) = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").expect("catalog");
    history.execute(&world, AddTool::new(&world, "mill", tool).expect("tool"));
    (world, history)
}

/// Generate the slot and read its path's heights along the tool axis:
/// (the highest point of any move = the clearance plane, the lowest
/// cutting point = the bottom level).
fn axis_heights(world: &World, history: &mut History, t: Tilt) -> (f64, f64) {
    history.execute(world, CreateOperation::new(world, "Slot", "mill", SrcRef::Stock, slot(t)).expect("create"));
    history.execute(world, GenerateOperation::new(world, "Slot").expect("generate"));
    let e = model::find_by_name(world, "Slot").expect("op");
    let tp = model::toolpath_of(world, &e).expect("the tilted slot generates");
    let f = TiltedFrame::from_angles(Point3::new(t.origin[0], t.origin[1], t.origin[2]), t.a_deg, t.b_deg)
        .expect("frame");
    assert!(!tp.path.moves.is_empty(), "a path: {:?}", tp.path.notes);
    let top = tp.path.moves.iter().flat_map(|m| [m.from, m.to]).map(|p| f.z_local(p)).fold(f64::MIN, f64::max);
    let floor = tp
        .path
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .flat_map(|m| [m.from, m.to])
        .map(|p| f.z_local(p))
        .fold(f64::MAX, f64::min);
    (top, floor)
}

#[test]
fn a_slot_at_a90_reads_top_of_stock_along_the_tool_axis() {
    // The issue's proof: a 10 (Y) x 30 (Z) bar centred on the A axis.
    // At A 90 the tool comes in along -Y, so the stock's top along the
    // axis is 5 (main read the setup's top Z, 15).
    let (world, mut history) = bench(10.0, 30.0, 0.0, 0.0);
    let (clear, floor) = axis_heights(&world, &mut history, tilt(90.0, 0.0, [0.0; 3]));
    assert!((clear - 15.0).abs() < TIGHT, "clearance = stock top 5 along the axis + 10: {clear}");
    assert!((floor - 4.0).abs() < TIGHT, "bottom = stock top 5 along the axis - 1: {floor}");
}

#[test]
fn an_offset_bar_reads_its_own_side_at_a_minus_90_and_the_underside() {
    // An asymmetric bar: Y -1..11, Z -20..10, and a plane origin off
    // the world origin (the fold's `- origin.z` must land on the local
    // value). A -90 points the tool along +Y: the far face is 11 out,
    // less nothing for the origin's Y of 0 - so 11. The underside (the
    // door's spelling of A 180 is A 0 B 180) points it along -Z: the
    // bottom face is 20 out, measured from origin Z 3 = 23.
    for (a, b, origin, top) in [(-90.0, 0.0, [2.0, 0.0, 3.0], 11.0), (0.0, 180.0, [0.0, 0.0, 3.0], 23.0)] {
        let (world, mut history) = bench(12.0, 30.0, 5.0, -5.0);
        let (clear, floor) = axis_heights(&world, &mut history, tilt(a, b, origin));
        assert!((clear - (top + 10.0)).abs() < 1e-6, "A {a} B {b}: clearance along the axis {clear}, want {}", top + 10.0);
        assert!((floor - (top - 1.0)).abs() < 1e-6, "A {a} B {b}: bottom along the axis {floor}, want {}", top - 1.0);
    }
}

/// A contour round a picked-whole model, every height from Top of model.
fn contour(tilt: Tilt) -> OpParams {
    let from_model = |offset| Height { reference: HeightReference::TopOfModel, offset };
    OpParams::Contour {
        tilt: Some(tilt),
        stock_to_leave: 0.0,
        clearance_z: from_model(10.0),
        retract_z: from_model(5.0),
        top_z: from_model(-1.0),
        bottom_z: from_model(-2.0),
        stepdown: 1.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: camcraft_app::machining2d::Side::Outside,
        lead: camcraft_app::machining2d::Lead::None,
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

#[test]
fn a_tilted_contour_sections_the_model_along_the_tool_axis() {
    // The section plane (Top of Part Z) rides the same fold: a 10 (Y) x
    // 30 (Z) part on the A axis, indexed A 90, sections 1 under its top
    // along the axis (4), never at the setup Z (14 - above the part
    // along the axis, where there is nothing to contour).
    let (world, mut history) = bench(40.0, 50.0, 0.0, 0.0);
    let part = camcraft_app::geometry::cube(Point3::new(-10.0, -5.0, -15.0), Point3::new(10.0, 5.0, 15.0));
    history.execute(&world, camcraft_app::commands::ImportModel::from_mesh("part", part));
    let t = tilt(90.0, 0.0, [0.0; 3]);
    history.execute(
        &world,
        CreateOperation::new(&world, "Contour", "mill", SrcRef::Entity("part".into()), contour(t)).expect("create"),
    );
    history.execute(&world, GenerateOperation::new(&world, "Contour").expect("generate"));
    let e = model::find_by_name(&world, "Contour").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("the tilted contour generates");
    let f = TiltedFrame::from_angles(Point3::ZERO, 90.0, 0.0).expect("frame");
    let cuts: Vec<f64> = tp
        .path
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed)
        .flat_map(|m| [m.from, m.to])
        .map(|p| f.z_local(p))
        .collect();
    assert!(!cuts.is_empty(), "the section finds the part: {:?}", tp.path.notes);
    let floor = cuts.iter().copied().fold(f64::MAX, f64::min);
    assert!((floor - 3.0).abs() < 1e-6, "bottom = model top 5 along the axis - 2: {floor}");
    // The profile is the part's X-Z outline seen from -Y: 20 x 30, offset
    // by the tool radius - its world Z span must exceed the part's 30.
    let zs = tp.path.moves.iter().filter(|m| m.kind == MoveKind::Feed).flat_map(|m| [m.from.z, m.to.z]);
    let (lo, hi) = zs.fold((f64::MAX, f64::MIN), |(lo, hi), z| (lo.min(z), hi.max(z)));
    assert!(lo < -15.0 && hi > 15.0, "the contour rings the part's side: Z {lo}..{hi}");
}

// The setup Z top is 10, but the A -90 tool-axis top is 11. A fixed
// bottom between them is legal and must pass both command doors.
fn mixed_reference_slot() -> OpParams {
    let mut params = slot(tilt(-90.0, 0.0, [0.0; 3]));
    if let OpParams::Slot { bottom_z, .. } = &mut params {
        *bottom_z = Height::fixed(10.5);
    }
    params
}

#[test]
fn tilted_mixed_reference_depth_is_accepted_on_creation() {
    let (world, mut history) = bench(12.0, 30.0, 5.0, -5.0);
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Slot",
            "mill",
            SrcRef::Stock,
            mixed_reference_slot(),
        )
        .expect("tool-axis top 11 is above fixed bottom 10.5"),
    );
    let e = model::find_by_name(&world, "Slot").expect("created");
    assert_eq!(
        model::operation_of(&world, &e).expect("operation").params,
        mixed_reference_slot()
    );
}

#[test]
fn tilted_mixed_reference_depth_is_accepted_on_editing() {
    let (world, mut history) = bench(12.0, 30.0, 5.0, -5.0);
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Slot",
            "mill",
            SrcRef::Stock,
            slot(tilt(-90.0, 0.0, [0.0; 3])),
        )
        .expect("create same-reference slot"),
    );
    history.execute(
        &world,
        EditOperationParams::new(&world, "Slot", mixed_reference_slot())
            .expect("tool-axis top 11 is above fixed bottom 10.5"),
    );
    let e = model::find_by_name(&world, "Slot").expect("edited");
    assert_eq!(
        model::operation_of(&world, &e).expect("operation").params,
        mixed_reference_slot()
    );
}

fn tilted_reference_pocket_preserves_part(src: SrcRef) {
    let (world, mut history) = bench(40.0, 50.0, 0.0, 0.0);
    let part = camcraft_app::geometry::cube(
        Point3::new(-10.0, -5.0, -15.0),
        Point3::new(10.0, 5.0, 15.0),
    );
    history.execute(
        &world,
        camcraft_app::commands::ImportModel::from_mesh("part", part),
    );
    let params = OpParams::Pocket {
        rest: None,
        blend_radius: None,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: from_stock(10.0),
        retract_z: Some(from_stock(5.0)),
        top_z: from_stock(0.0),
        bottom_z: Height {
            reference: HeightReference::TopOfModel,
            offset: -2.0,
        },
        stepdown: 2.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: camcraft_app::machining2d::PocketStrategy::Zigzag,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        tilt: Some(tilt(90.0, 0.0, [0.0; 3])),
        links: camcraft_app::links::Links::PLANAR,
    };
    history.execute(
        &world,
        CreateOperation::new(&world, "Pocket", "mill", src, params).expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Pocket").expect("generate"),
    );
    let e = model::find_by_name(&world, "Pocket").expect("pocket");
    let tp = model::toolpath_of(&world, &e).expect("path");
    assert!(
        !tp.path.moves.is_empty(),
        "a real pocket: {:?}",
        tp.path.notes
    );
    let f = TiltedFrame::from_angles(Point3::ZERO, 90.0, 0.0).expect("frame");
    let floor = tp
        .path
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed)
        .flat_map(|m| [m.from, m.to])
        .map(|p| f.z_local(p))
        .fold(f64::MAX, f64::min);
    assert!(
        (floor - 3.0).abs() < 1e-6,
        "cuts to tool-axis model top 5 minus 2: {floor}"
    );
    assert!(
        !tp.warnings
            .iter()
            .any(|w| matches!(w, camcraft_app::verify::Warning::Gouge { .. })),
        "islands must protect the part at the generator's tool-axis levels: {:?}",
        tp.warnings
    );
}

#[test]
fn tilted_whole_model_reference_pocket_preserves_part_islands() {
    tilted_reference_pocket_preserves_part(SrcRef::Entity("part".into()));
}

#[test]
fn tilted_stock_reference_pocket_preserves_part_islands() {
    tilted_reference_pocket_preserves_part(SrcRef::Stock);
}

#[test]
fn tilted_upward_mixed_reference_depth_is_refused_on_creation_and_editing() {
    let (world, mut history) = bench(12.0, 30.0, 5.0, -5.0);
    history.execute(&world,
        CreateOperation::new(&world, "Slot", "mill", SrcRef::Stock, slot(tilt(-90.0, 0.0, [0.0; 3])))
            .expect("create same-reference slot"));
    for reverse in [false, true] {
        let mut upward = mixed_reference_slot();
        if let OpParams::Slot { top_z, bottom_z, .. } = &mut upward {
            if reverse {
                // Setup Z would incorrectly accept 10.5 > stock top 10;
                // the tool-axis stock top is 11, above this fixed top.
                *top_z = Height::fixed(10.5);
                *bottom_z = from_stock(0.0);
            } else {
                *bottom_z = Height::fixed(11.5);
            }
        }
        assert!(CreateOperation::new(&world, "upward", "mill", SrcRef::Stock, upward.clone()).is_err(),
            "creation refuses bottom above tool-axis top (reverse={reverse})");
        assert!(EditOperationParams::new(&world, "Slot", upward).is_err(),
            "editing refuses bottom above tool-axis top (reverse={reverse})");
    }
}
