//! Pocket and slot recognition (#35): closed pockets with sharp and
//! rounded corners, a round-ended closed slot, a through slot and an
//! open slot are read off a model's faces with their floor, top, depth,
//! tightest corner, width and open sides; a part with none (a plain
//! block, a shoulder, a boss) answers an empty list; degenerate input
//! is a clean refusal or an empty list, never a panic. A recognised
//! feature becomes a Pocket or Slot operation through CreateOperation -
//! it generates without a gouge and one undo removes it.
//!
//! Gate map (#35): covers app::recognize (mod, walls, shape, op),
//! app::commands (CreateOperation, GenerateOperation).

mod pocket_support;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, Height, HeightReference, OpParams, OpType, SrcRef};
use camcraft_app::recognize::{self, Pocket, PocketKind, SlotEnd};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;
use pocket_support::{block_with_pocket, height_field, rounded_rect};

const EPS: f64 = 1e-6;

fn world_with(mesh: Mesh) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", mesh));
    (world, h)
}

fn only(world: &World) -> Pocket {
    let found = recognize::pockets_of(world, "part").expect("recognition runs");
    assert_eq!(found.len(), 1, "one feature: {found:#?}");
    found.into_iter().next().expect("one")
}

/// 60 x 40 x 20 block, a 30 x 16 pocket of corner radius `r` to z 8.
fn pocket_block(r: f64) -> Mesh {
    block_with_pocket(30.0, 20.0, &rounded_rect(15.0, 8.0, r, 6), 8.0, 20.0)
}

#[test]
fn a_rectangular_pocket_is_closed_with_sharp_corners() {
    let (world, _h) = world_with(pocket_block(0.0));
    let p = only(&world);
    assert_eq!(p.kind, PocketKind::Closed);
    assert!((p.floor_z - 8.0).abs() < EPS && (p.top_z - 20.0).abs() < EPS && (p.depth - 12.0).abs() < EPS, "{p:?}");
    assert_eq!(p.boundary.len(), 4, "{p:?}");
    assert!((p.area - 480.0).abs() < 1e-6, "{p:?}");
    assert_eq!(p.min_corner_radius, Some(0.0));
    assert!(p.open_spans.is_empty() && p.slot.is_none() && p.islands == 0);
    assert_eq!(recognize::op_kind_of(&p), OpType::Pocket);
}

#[test]
fn a_pocket_with_corner_radii_reports_its_tightest_corner() {
    let (world, _h) = world_with(pocket_block(4.0));
    let p = only(&world);
    assert_eq!(p.kind, PocketKind::Closed);
    let r = p.min_corner_radius.expect("walls meet at corners");
    assert!((r - 4.0).abs() < 1e-3, "corner radius {r}");
    assert!((p.depth - 12.0).abs() < EPS);
}

#[test]
fn a_round_ended_closed_slot_is_a_slot_between_its_cap_centres() {
    // 40 long, 10 wide, both ends semicircular.
    let ring = rounded_rect(20.0, 5.0, 5.0, 8);
    let (world, _h) = world_with(block_with_pocket(30.0, 20.0, &ring, 12.0, 20.0));
    let p = only(&world);
    assert_eq!(p.kind, PocketKind::Slot, "{p:?}");
    let s = p.slot.expect("slot shape");
    assert_eq!(s.ends, [SlotEnd::Round, SlotEnd::Round]);
    assert!((s.width - 10.0).abs() < 1e-3 && (s.length - 40.0).abs() < 1e-3, "{s:?}");
    let (lo, hi) = if s.a.x < s.b.x { (s.a, s.b) } else { (s.b, s.a) };
    assert!((lo.x + 15.0).abs() < 1e-3 && (hi.x - 15.0).abs() < 1e-3 && lo.y.abs() < 1e-3, "{s:?}");
    assert!((p.min_corner_radius.expect("arcs") - 5.0).abs() < 1e-3);
    assert_eq!(recognize::op_kind_of(&p), OpType::Slot);
}

/// 60 x 40 x 20 block, a 10 wide slot through along X to z 10.
fn through_slot() -> Mesh {
    height_field(&[-30.0, 30.0], &[-20.0, -5.0, 5.0, 20.0], |_, j| if j == 1 { 10.0 } else { 20.0 })
}

#[test]
fn a_through_slot_is_open_at_both_ends() {
    let (world, _h) = world_with(through_slot());
    let p = only(&world);
    assert_eq!(p.kind, PocketKind::Slot, "{p:?}");
    let s = p.slot.expect("slot");
    assert_eq!(s.ends, [SlotEnd::Open, SlotEnd::Open]);
    assert!((s.width - 10.0).abs() < 1e-6 && (s.length - 60.0).abs() < 1e-6);
    assert_eq!(p.open_spans.len(), 2, "{p:?}");
    let xs: Vec<f64> = [s.a.x, s.b.x].into_iter().map(f64::abs).collect();
    assert!(xs.iter().all(|x| (x - 30.0).abs() < 1e-6), "open ends cut to the floor's edge: {s:?}");
    assert_eq!(p.min_corner_radius, None, "no two walls meet");
    assert!((p.floor_z - 10.0).abs() < EPS && (p.depth - 10.0).abs() < EPS);
}

#[test]
fn an_open_slot_with_a_square_end_machines_as_a_pocket() {
    // Closed (square) at x -10, open at x +30.
    let mesh = height_field(&[-30.0, -10.0, 30.0], &[-20.0, -5.0, 5.0, 20.0], |i, j| if i == 1 && j == 1 { 10.0 } else { 20.0 });
    let (world, _h) = world_with(mesh);
    let p = only(&world);
    assert_eq!(p.kind, PocketKind::Slot, "{p:?}");
    let s = p.slot.expect("slot");
    let mut ends = s.ends.to_vec();
    ends.sort_by_key(|e| *e as u8);
    assert_eq!(ends, vec![SlotEnd::Open, SlotEnd::Square], "{s:?}");
    assert_eq!(p.open_spans.len(), 1);
    assert_eq!(p.min_corner_radius, Some(0.0), "the square end's corners");
    assert_eq!(recognize::op_kind_of(&p), OpType::Pocket, "a round cap would leave the square corners");
}

#[test]
fn parts_without_pockets_list_none() {
    let block = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 20.0));
    // A shoulder (one wall) and a corner step (two adjacent walls).
    let shoulder = height_field(&[-20.0, 0.0, 20.0], &[-20.0, 20.0], |i, _| if i == 0 { 20.0 } else { 10.0 });
    let corner = height_field(&[-20.0, 0.0, 20.0], &[-20.0, 0.0, 20.0], |i, j| if i == 1 && j == 1 { 10.0 } else { 20.0 });
    // A boss standing on a plate: its floor's outer ring is the plate rim.
    let boss = height_field(&[-20.0, -5.0, 5.0, 20.0], &[-20.0, -5.0, 5.0, 20.0], |i, j| if i == 1 && j == 1 { 20.0 } else { 10.0 });
    for (label, mesh) in [("block", block), ("shoulder", shoulder), ("corner", corner), ("boss", boss)] {
        let (world, _h) = world_with(mesh);
        let found = recognize::pockets_of(&world, "part").expect("recognition runs");
        assert!(found.is_empty(), "{label}: {found:#?}");
    }
}

#[test]
fn degenerate_input_refuses_cleanly() {
    let (world, _h) = world_with(pocket_block(0.0));
    assert!(recognize::pockets_of(&world, "ghost").is_err_and(|e| e.contains("no model named")));
    let nan = Point3::new(f64::NAN, 0.0, 0.0);
    let p = Point3::new(1.0, 1.0, 1.0);
    let junk = vec![
        Triangle::new(nan, p, Point3::new(2.0, 0.0, 0.0)),
        Triangle::new(p, p, p),
        Triangle::new(p, Point3::new(2.0, 2.0, 2.0), Point3::new(3.0, 3.0, 3.0)),
        Triangle::new(Point3::new(f64::INFINITY, 0.0, 0.0), p, p),
    ];
    let (world, _h) = world_with(Mesh::from_facets(junk));
    match recognize::pockets_of(&world, "part") {
        Ok(v) => assert!(v.is_empty(), "{v:?}"),
        Err(e) => assert!(e.contains("part"), "{e}"),
    }
    // A half-built pocket (floor and one wall, no top): nothing to read
    // as a pocket, and no panic on its open rims.
    // #718: an open shell's inside cannot be told from its outside, so it
    // is refused by name, never read.
    let mut torn = pocket_block(4.0).facets.to_vec();
    torn.truncate(torn.len() / 3);
    let (world, _h) = world_with(Mesh::from_facets(torn));
    let e = recognize::pockets_of(&world, "part").expect_err("an open shell is refused");
    assert!(e.contains("'part'") && e.contains("open body"), "{e}");
}

/// Every facet wound the other way: the body's outside is now its inside.
fn reversed(mesh: &Mesh) -> Mesh {
    Mesh::from_facets(mesh.facets.iter().map(|t| Triangle::new(t.a, t.c, t.b)).collect())
}

/// #718 (the review of #512): an inside-out cube's bottom face "faces up"
/// and its outer walls "lean in", so without the winding verdict its own
/// outline read as a part-deep closed pocket - a Pocket over the whole
/// footprint, cut to the base. It is refused by name; so is a reversed
/// pocketed block.
#[test]
fn an_inside_out_mesh_is_refused_never_read_as_a_pocket() {
    let cube = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(10.0, 10.0, 10.0));
    for (label, mesh) in [("cube", reversed(&cube)), ("pocket block", reversed(&pocket_block(4.0)))] {
        let (world, _h) = world_with(mesh);
        let e = recognize::pockets_of(&world, "part").expect_err(label);
        assert!(e.contains("'part'") && e.contains("inside-out"), "{label}: {e}");
    }
    // The same block wound outward still reads its one pocket.
    let (world, _h) = world_with(pocket_block(4.0));
    assert_eq!(recognize::pockets_of(&world, "part").map(|v| v.len()), Ok(1));
}

fn machine(world: &World, h: &mut History, p: &Pocket, tool: &str, name: &str) -> OpParams {
    let base = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 25.0),
        cut_z: Height::of(HeightReference::TopOfStock, 0.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false, blend_radius: None,
        picks: Default::default(),
    };
    let (src, params) = recognize::operation_for(p, &base).expect("an operation");
    h.execute(world, CreateOperation::new(world, name, tool, src, params.clone()).expect("create"));
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
    params
}

fn setup(world: &World, h: &mut History, tool: &str) {
    // The 60 x 40 x 20 parts in a box with 10 mm of margin round them.
    h.execute(world, SetStock::new(world, StockShape::Box { dx: 80.0, dy: 60.0, dz: 20.0 }, Default::default()).expect("stock"));
    let t = catalog().into_iter().find(|(n, _)| *n == tool).map(|(_, t)| t).expect("catalog");
    h.execute(world, AddTool::new(world, tool, t).expect("tool"));
}

fn path_of(world: &World, name: &str) -> (Vec<camcraft_app::cl::Move>, Vec<Warning>) {
    let e = model::find_by_name(world, name).expect("op");
    let tp = model::toolpath_of(world, &e).expect("a generated path");
    (tp.path.moves.clone(), tp.warnings.clone())
}

#[test]
fn a_recognised_pocket_becomes_an_undoable_pocket_that_cuts_to_its_floor() {
    let (world, mut h) = world_with(pocket_block(4.0));
    setup(&world, &mut h, "EM-0600-4FL");
    let p = only(&world);
    let params = machine(&world, &mut h, &p, "EM-0600-4FL", "P1");
    let OpParams::Pocket { top_z, bottom_z, .. } = params else { panic!("a pocket: {params:?}") };
    assert_eq!(bottom_z, Height::of(HeightReference::BottomOfSelection, 0.0));
    assert_eq!(top_z, Height::of(HeightReference::BottomOfSelection, 12.0));
    let e = model::find_by_name(&world, "P1").expect("op");
    let op = model::operation_of(&world, &e).expect("operation");
    assert!(matches!(&op.src, SrcRef::Selection(s) if s.len() == 1 && s[0].feature == FeatureSel::Face(p.floor)));
    let (moves, warnings) = path_of(&world, "P1");
    assert!(!warnings.iter().any(|w| matches!(w, Warning::Gouge { .. })), "{warnings:?}");
    let feeds: Vec<_> = moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    assert!(!feeds.is_empty());
    let low = feeds.iter().map(|m| m.to.z).fold(f64::INFINITY, f64::min);
    assert!((low - 8.0).abs() < 1e-6, "cuts down to the floor: {low}");
    // The 6 mm cutter's centre stays 3 inside the 30 x 16 ring.
    assert!(feeds.iter().all(|m| m.to.x.abs() <= 12.0 + 1e-6 && m.to.y.abs() <= 5.0 + 1e-6));
    h.undo(&world);
    h.undo(&world);
    assert!(model::find_by_name(&world, "P1").is_none(), "one undo step per command: create is gone");
}

/// Distance from `p` to the segment `a`-`b`, XY.
fn seg_dist(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    (p.0 - a.0 - t * dx).hypot(p.1 - a.1 - t * dy)
}

#[test]
fn a_recognised_slot_becomes_a_slot_cut_along_its_centreline() {
    for (label, mesh) in [
        ("through", through_slot()),
        ("round", block_with_pocket(30.0, 20.0, &rounded_rect(20.0, 5.0, 5.0, 8), 12.0, 20.0)),
    ] {
        let (world, mut h) = world_with(mesh);
        setup(&world, &mut h, "EM-0600-4FL");
        let p = only(&world);
        let s = p.slot.expect("a slot");
        let params = machine(&world, &mut h, &p, "EM-0600-4FL", "S1");
        let OpParams::Slot { a, b, width, top_z, bottom_z, .. } = params else { panic!("{label}: a slot: {params:?}") };
        assert_eq!((a, b), (s.a, s.b), "{label}");
        assert!((width - 10.0).abs() < 1e-3, "{label}");
        assert_eq!(bottom_z, Height::fixed(p.floor_z));
        assert_eq!(top_z, Height::fixed(p.top_z));
        // The cut, measured exactly: every cutting centre within
        // width/2 - r of the centreline (the 6 mm cutter's edge never
        // past the walls) and the deepest pass on the floor. The
        // verifier's own Slot verdict is not asserted here: it convicts
        // every typed Slot riding its cap today (the stadium region's
        // chorded caps, #510), recognised or not.
        let (moves, _) = path_of(&world, "S1");
        let feeds: Vec<_> = moves.iter().filter(|m| m.kind != MoveKind::Rapid).collect();
        assert!(!feeds.is_empty(), "{label}");
        for m in &feeds {
            let d = seg_dist((m.to.x, m.to.y), (a.x, a.y), (b.x, b.y));
            assert!(d <= 2.0 + 1e-6, "{label}: centre {d} off the centreline: {m:?}");
        }
        let low = feeds.iter().map(|m| m.to.z).fold(f64::INFINITY, f64::min);
        assert!((low - p.floor_z).abs() < 1e-6, "{label}: {low}");
        h.undo(&world);
        h.undo(&world);
        assert!(model::find_by_name(&world, "S1").is_none(), "{label}");
    }
}

#[test]
fn an_open_slot_machined_as_a_pocket_cuts_its_floor_without_a_gouge() {
    let mesh = height_field(&[-30.0, -10.0, 30.0], &[-20.0, -5.0, 5.0, 20.0], |i, j| if i == 1 && j == 1 { 10.0 } else { 20.0 });
    let (world, mut h) = world_with(mesh);
    setup(&world, &mut h, "EM-0600-4FL");
    let p = only(&world);
    let params = machine(&world, &mut h, &p, "EM-0600-4FL", "P1");
    assert!(matches!(params, OpParams::Pocket { .. }), "{params:?}");
    let (moves, warnings) = path_of(&world, "P1");
    assert!(!warnings.iter().any(|w| matches!(w, Warning::Gouge { .. })), "{warnings:?}");
    let feeds: Vec<_> = moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    assert!(feeds.iter().all(|m| m.to.y.abs() <= 2.0 + 1e-6 && m.to.x >= -7.0 - 1e-6 && m.to.x <= 27.0 + 1e-6));
    let low = feeds.iter().map(|m| m.to.z).fold(f64::INFINITY, f64::min);
    assert!((low - 10.0).abs() < 1e-6, "{low}");
}
