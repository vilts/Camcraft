//! Built-in accessories under the existing fixture laws (#789): a strap
//! clamp's stud stands over the part, so a link laid low across it is
//! struck and the link #489's routing lays (up to the clamp's box top
//! plus its 5 mm gap, across, down) is clear - judged by the clamp's
//! mesh, so a link over the strap beside the stud is clear too. Parallels
//! fitted under the stock in a built-in vise lift the stock so its top
//! stands the stated amount over the jaws: the face pass the jaws struck
//! posts clean, and one undo puts the vise back.
//!
//! Gate map (M4.4): covers app::accessory, app::commands::accessory,
//! app::fixture, sim::fixture, app::fixture_struck, app::posting.

use camcraft_app::accessory::Accessory;
use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::commands::{
    AccessoryPlacement, AddTool, CreateOperation, GenerateOperation, History, InsertAccessory, InsertVise, SetStock,
    VisePlacement,
};
use camcraft_app::fixture;
use camcraft_app::fixture_struck;
use camcraft_app::fixture_view::{self, MeshState};
use camcraft_app::geometry::{IDENTITY_Q, Point3};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::vise;
use camcraft_app::world::new_world;

const IN: f64 = 25.4;
/// #489's routing gap over the highest box top under a link.
const ROUTE_GAP: f64 = 5.0;

/// 60 x 60 stock `dz` tall, a 4 mm end mill, a face pass `cut` high.
fn face_world(dz: f64, cut: f64) -> (flecs_ecs::prelude::World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz }, Location::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("tool"));
    let params = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(dz + 5.0),
        cut_z: Height::fixed(cut),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    history.execute(&world, CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params).expect("op"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    (world, history)
}

/// The post gate's per-op fixture law on a hand-laid path, against the
/// world's real steel (boxes, then meshes).
fn hits(world: &flecs_ecs::prelude::World, moves: Vec<Move>) -> Vec<usize> {
    let op = model::find_by_name(world, "Face1").and_then(|e| model::operation_of(world, &e)).expect("op");
    let (turning, fixed) = fixture::workholding_for(world, None);
    let meshes = fixture::meshes_for(world, None);
    let path = Toolpath::new(moves);
    let warnings = fixture_struck::op_hits(world, &op, &path, &turning, &fixed, &meshes).expect("judged");
    fixture_struck::warned_moves(&warnings)
}

fn rapid(a: [f64; 3], b: [f64; 3]) -> Move {
    Move::new(MoveKind::Rapid, Point3::new(a[0], a[1], a[2]), Point3::new(b[0], b[1], b[2]), None)
}

#[test]
fn a_strap_clamp_stud_strikes_a_low_link_and_the_routed_link_clears_it() {
    let (world, mut history) = face_world(20.0, 19.5);
    // The toe on the stock's +x edge, clamping its 20 mm top face.
    let clamp = Accessory::default_of("strap_clamp").expect("clamp").with_dim("clamp_height", 20.0).expect("dim");
    let at = Location { pos: Point3::new(25.0, 0.0, 0.0), rot: IDENTITY_Q };
    let cmd = InsertAccessory::new(&world, &clamp, None, AccessoryPlacement::At(at)).expect("placed");
    let name = cmd.name().to_owned();
    history.execute(&world, cmd);
    assert_eq!(fixture_view::mesh_states(&world, None), [(name.clone(), MeshState::Mesh)], "judged by its mesh");

    let e = model::find_by_name(&world, &name).expect("clamp");
    let top = model::entity_world_aabb(&world, &e).expect("box").max.z;
    let strap_top = 20.0 + 0.625 * IN;
    let stud_x = 25.0 + 2.25 * IN;
    assert!((top - (strap_top + 1.5 * IN)).abs() < 1e-9, "the stud stands 1.5 in over the strap: {top}");

    // A link at z 50 - over the strap, under the stud's top - across the stud.
    let low = 50.0;
    assert!(low > strap_top + 5.0 && low < top - 5.0);
    let link = vec![rapid([stud_x, -60.0, low], [stud_x, 60.0, low])];
    assert_eq!(hits(&world, link), [0], "the stud strikes a low link across it");

    // The same height over the strap beside the stud: the mesh acquits
    // what the clamp's box (topped by the stud) convicts.
    let beside = vec![rapid([40.0, -60.0, low], [40.0, 60.0, low])];
    assert_eq!(hits(&world, beside), Vec::<usize>::new(), "only the stud stands that high");

    // #489's re-laid link: up at its start, across over the box top plus
    // the gap, down at its end.
    let up = top + ROUTE_GAP;
    let routed = vec![
        rapid([stud_x, -60.0, low], [stud_x, -60.0, up]),
        rapid([stud_x, -60.0, up], [stud_x, 60.0, up]),
        rapid([stud_x, 60.0, up], [stud_x, 60.0, low]),
    ];
    assert_eq!(hits(&world, routed), Vec::<usize>::new(), "the routed link clears the stud");
}

#[test]
fn parallels_fitted_under_the_stock_lift_it_over_the_jaws() {
    let (world, mut history) = face_world(20.0, 19.5);
    let six = vise::size("6 in").expect("6 in").half_open();
    let cmd = InsertVise::new(&world, &six, None, VisePlacement::FitStock).expect("vise");
    let vise_name = cmd.name().to_owned();
    history.execute(&world, cmd);
    assert!(posting::verification_gate(&world).iter().any(|p| p.contains("strikes a fixture")), "the jaws stand over the pass");

    let above = 0.25 * IN;
    let par = Accessory::default_of("parallels").expect("parallels");
    let fit = AccessoryPlacement::UnderStockInVise { vise: vise_name.clone(), above };
    let cmd = InsertAccessory::new(&world, &par, None, fit).expect("fits");
    let name = cmd.name().to_owned();
    let Some(Accessory::Parallels { height, thickness, spacing, .. }) = cmd.params().cloned() else {
        panic!("parallels");
    };
    let want_h = 1.75 * IN + above - 20.0;
    assert!((height - want_h).abs() < 1e-9, "height {height} vs {want_h}");
    assert!((spacing + 2.0 * thickness - 60.0).abs() < 1e-9, "the pair spans the jaw opening");
    history.execute(&world, cmd);

    let v = model::find_by_name(&world, &vise_name).expect("vise");
    let vb = model::entity_world_aabb(&world, &v).expect("vise box");
    assert!((vb.max.z - (20.0 - above)).abs() < 1e-9, "the jaws stand {above} under the stock top: {}", vb.max.z);
    let pb = model::entity_world_aabb(&world, &model::find_by_name(&world, &name).expect("parallels")).expect("box");
    assert!(pb.max.z.abs() < 1e-9 && (pb.min.z + want_h).abs() < 1e-9, "on the floor, up to the stock: {pb:?}");
    assert!((pb.min.y + 30.0).abs() < 1e-9 && (pb.max.y - 30.0).abs() < 1e-9, "against both jaws: {pb:?}");
    let problems = posting::verification_gate(&world);
    assert!(problems.is_empty(), "the pass now runs over the jaws: {problems:?}");

    assert!(history.undo(&world));
    assert!(model::find_by_name(&world, &name).is_none(), "one undo removes the parallels");
    let vb = model::entity_world_aabb(&world, &v).expect("vise box");
    assert!((vb.max.z - 1.75 * IN).abs() < 1e-9, "and puts the vise back: {}", vb.max.z);
    assert!(history.redo(&world));
    assert!(model::find_by_name(&world, &name).is_some());

    // Refused by name: stock already standing that high, parallels too thick.
    let low = AccessoryPlacement::UnderStockInVise { vise: vise_name.clone(), above: -1.0 };
    assert!(InsertAccessory::new(&world, &par, None, low).is_err());
    let (tall, mut h2) = face_world(60.0, 59.5);
    let cmd = InsertVise::new(&tall, &six, None, VisePlacement::FitStock).expect("vise");
    let tv = cmd.name().to_owned();
    h2.execute(&tall, cmd);
    let fit = AccessoryPlacement::UnderStockInVise { vise: tv.clone(), above: 1.0 };
    let e = InsertAccessory::new(&tall, &par, None, fit).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(e.contains("no parallels fit under it"), "{e}");
    let thick = par.clone().with_dim("thickness", 31.0).expect("dim");
    let fit = AccessoryPlacement::UnderStockInVise { vise: vise_name, above };
    let e = InsertAccessory::new(&world, &thick, None, fit).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(e.contains("do not fit in the vise's"), "{e}");
    let fit = AccessoryPlacement::UnderStockInVise { vise: "Face1".into(), above };
    let e = InsertAccessory::new(&world, &par, None, fit).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(e.contains("not a built-in vise"), "{e}");
}
