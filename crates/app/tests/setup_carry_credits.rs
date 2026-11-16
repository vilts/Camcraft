//! #1806: retained verdicts credit only the earlier setup's declared cuts.
//! Provenance: our own finished-block fixtures and cached-path fault injection.
//! Gate map: covers app::stock_sim (carry::walk, simulate_document, marks).

use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, History,
    ImportModel, SetSetupStockSource, SetStock,
};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::operation::{self, Height, OpParams, OpType, SrcRef};
use camcraft_app::setup_align::{AlignPreset, parts_facets_in};
use camcraft_app::setup_stock::StockSource;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::simulate_document;
use camcraft_app::tools::catalog;
use camcraft_app::{model, schema, world::new_world};
use flecs_ecs::prelude::World;

const CONE: &str = "CM-0600-90";
const MILL: &str = "EM-0600-4FL";

fn face(z: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(), tilt: None, blend_turns: false, stepover: 3.0,
        clearance_z: Height::fixed(50.0), cut_z: Height::fixed(z), top_z: None,
        stepdown: None, feed: 1200.0, plunge_feed: 300.0, climb: true,
    }
}

fn params(kind: OpType) -> OpParams {
    match kind {
        OpType::Engrave => OpParams::Engrave {
            clearance_z: Height::fixed(50.0), retract_z: None,
            top_z: Height::fixed(16.0), bottom_z: Height::fixed(15.7),
            stepdown: 1.0, feed: 800.0, plunge_feed: 200.0, climb: true, tilt: None,
        },
        OpType::Trace => OpParams::Trace {
            clearance_z: Height::fixed(50.0), retract_z: None,
            top_z: Height::fixed(16.0), bottom_z: Height::fixed(15.7),
            stepdown: 1.0, feed: 800.0, plunge_feed: 200.0, climb: true, tilt: None,
        },
        _ => operation::retype_params(&face(16.0), kind.ordinal()),
    }
}

fn add_op(world: &World, h: &mut History, name: &str, tool: &str, params: OpParams) {
    h.execute(world, CreateOperation::new(world, name, tool, SrcRef::Entity("part".into()), params).expect("create"));
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

fn job(kind: OpType, excess: f64, side_on: bool) -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(Point3::new(-30.0, -30.0, 0.0), Point3::new(30.0, 30.0, 16.0))));
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 16.0 }, Default::default()).expect("stock"));
    for name in [CONE, MILL] {
        let tool = catalog().into_iter().find(|(n, _)| *n == name).expect("catalog tool").1;
        h.execute(&world, AddTool::new(&world, name, tool).expect("tool"));
    }
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    let tool = if matches!(kind, OpType::Chamfer2D | OpType::Deburr) { CONE } else { MILL };
    add_op(&world, &mut h, "Cut1", tool, params(kind));
    if side_on {
        // Keep a generated straight pass away from the ends of
        // the new height axis. The final silhouette cannot see this cut;
        // only the verdict taken before the turn can judge it.
        let e = model::find_by_name(&world, "Cut1").expect("op");
        let mut cached = model::toolpath_of(&world, &e).expect("cached path");
        let mut pass = *cached.path.moves.iter().find(|m| {
            m.kind != camcraft_app::cl::MoveKind::Rapid
                && m.from.x.abs() > 29.0 && (m.from.x - m.to.x).abs() < 1e-9
                && (m.from.y - m.to.y).abs() > 10.0 && m.to.z < 16.0
        }).expect("straight edge pass");
        pass.from.y = pass.from.y.clamp(-20.0, 20.0);
        pass.to.y = pass.to.y.clamp(-20.0, 20.0);
        cached.path.moves = vec![pass];
        e.entity_view(&world).set(schema::Toolpath::new(cached.path, cached.warnings));
    }
    let single = simulate_document(&world).expect("single setup verdict").dev;
    assert_eq!(single.gouge_cells, 0, "the declared cut is clean alone: worst {}", single.max_gouge);
    if excess > 0.0 {
        let e = model::find_by_name(&world, "Cut1").expect("op");
        let mut cached = model::toolpath_of(&world, &e).expect("cached path");
        for m in &mut cached.path.moves {
            m.from.z -= excess;
            m.to.z -= excess;
        }
        e.entity_view(&world).set(schema::Toolpath::new(cached.path, cached.warnings));
    }
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2"));
    let turn = if side_on {
        AlignPartToSetup::angles(&world, "Setup2", [-90.0, 0.0, 0.0]).expect("side-on")
    } else {
        AlignPartToSetup::preset(&world, "Setup2", AlignPreset::FlipX).expect("flip")
    };
    h.execute(&world, turn);
    if !side_on {
        h.execute(&world, SetSetupStockSource::new(&world, "Setup2", StockSource::Own).expect("new block"));
    }
    let top = parts_facets_in(&world, Some("Setup2")).iter().flat_map(|t| [t.a.z, t.b.z, t.c.z]).fold(f64::NEG_INFINITY, f64::max);
    add_op(&world, &mut h, "Face2", MILL, face(top));
    world
}

fn clean(kind: OpType, side_on: bool) {
    let world = job(kind, 0.0, side_on);
    let dev = simulate_document(&world).expect("two setup verdict").dev;
    assert_eq!(dev.gouge_cells, 0, "{kind:?}, side-on {side_on}: worst {} at {:?}", dev.max_gouge, dev.max_gouge_at);
}

#[test]
fn earlier_breaks_are_credited_before_a_new_block() {
    for kind in [OpType::Chamfer2D, OpType::Deburr] {
        clean(kind, false);
    }
}

#[test]
fn earlier_marks_are_credited_before_a_new_block() {
    for kind in [OpType::Engrave, OpType::Trace] {
        clean(kind, false);
    }
}

#[test]
fn earlier_breaks_are_credited_before_a_side_on_turn() {
    clean(OpType::Chamfer2D, true);
}

#[test]
fn earlier_marks_are_credited_before_a_side_on_turn() {
    clean(OpType::Trace, true);
}

#[test]
fn real_gouges_under_earlier_breaks_and_marks_survive_both_transitions() {
    for side_on in [false, true] {
        for kind in [OpType::Chamfer2D, OpType::Trace] {
            let world = job(kind, 0.3, side_on);
            let dev = simulate_document(&world).expect("two setup verdict").dev;
            assert!(dev.gouge_cells > 0, "{kind:?}, side-on {side_on}: a real gouge disappeared");
            assert!((dev.max_gouge - 0.3).abs() < 1e-6, "{kind:?}, side-on {side_on}: charge only the excess: {}", dev.max_gouge);
        }
    }
}

#[test]
fn a_mark_from_another_setup_cannot_credit_a_real_gouge() {
    // Setup1 marks its block. Setup2 cuts the same numerical path with
    // a Face operation, which declares no mark, into a different block.
    // Setup3 leaves that block behind too, forcing its verdict to be kept.
    let world = job(OpType::Trace, 0.0, false);
    let cut = model::find_by_name(&world, "Cut1").expect("first op");
    let second = model::find_by_name(&world, "Face2").expect("second op");
    let path = model::toolpath_of(&world, &cut).expect("mark path");
    second.entity_view(&world).set(path);
    let mut h = History::default();
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup2")).expect("Setup3"));
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup3", AlignPreset::AsModelled).expect("turn back"));
    h.execute(&world, SetSetupStockSource::new(&world, "Setup3", StockSource::Own).expect("new block"));
    add_op(&world, &mut h, "Face3", MILL, face(16.0));
    let dev = simulate_document(&world).expect("three setup verdict").dev;
    assert!(dev.gouge_cells > 0, "Setup1's mark forgave a gouge in Setup2");
    assert!((dev.max_gouge - 0.3).abs() < 1e-6, "the uncredited cut: {}", dev.max_gouge);
}

#[test]
fn retained_top_credit_preserves_the_underside_gouge() {
    // The two cuts share columns: a real 0.2 mm underside gouge and a
    // deeper, declared 0.3 mm top mark. Crediting the top must leave the
    // underside's own depth and location, not erase the combined cell.
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(Point3::new(-30.0, -30.0, 0.0), Point3::new(30.0, 30.0, 16.0))));
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 16.0 }, Default::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == MILL).expect("catalog tool").1;
    h.execute(&world, AddTool::new(&world, MILL, tool).expect("tool"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup1", AlignPreset::FlipX).expect("underside"));
    add_op(&world, &mut h, "Face1", MILL, face(16.0));
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2"));
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup2", AlignPreset::AsModelled).expect("top side"));
    add_op(&world, &mut h, "Trace2", MILL, params(OpType::Trace));

    let trace = model::find_by_name(&world, "Trace2").expect("trace");
    let mut cached = model::toolpath_of(&world, &trace).expect("trace path");
    for m in &mut cached.path.moves {
        m.from.z += 0.1;
        m.to.z += 0.1;
    }
    let first = model::find_by_name(&world, "Face1").expect("first face");
    first.entity_view(&world).set(schema::Toolpath::new(cached.path, cached.warnings));
    h.execute(&world, GenerateOperation::new(&world, "Trace2").expect("regenerate after changed incoming stock"));

    // Read only the incoming body, before the top cut: this supplies the
    // underside count and location independently of retained credits.
    let incoming = camcraft_app::stock_sim::incoming_body(&world, "Setup2").expect("incoming damaged body");
    let underside = incoming.deviate(&parts_facets_in(&world, Some("Setup2")), camcraft_app::stock_sim::DEV_TOL).expect("underside verdict");
    assert!(underside.gouge_cells > 0);
    assert!((underside.max_gouge - 0.2).abs() < 1e-6);
    let at = underside.max_gouge_at.expect("underside location");
    assert!((at.z - 0.2).abs() < 1e-6, "underside in Setup2's frame: {at:?}");

    h.execute(&world, CreateSetup::new(&world, None, Some("Setup2")).expect("Setup3"));
    h.execute(&world, AlignPartToSetup::preset(&world, "Setup3", AlignPreset::FlipX).expect("turn"));
    h.execute(&world, SetSetupStockSource::new(&world, "Setup3", StockSource::Own).expect("new block"));
    add_op(&world, &mut h, "Face3", MILL, face(16.0));
    let dev = simulate_document(&world).expect("retained two-face verdict").dev;
    assert!((dev.max_gouge - 0.2).abs() < 1e-6, "lost the underside depth: {}", dev.max_gouge);
    assert_eq!(dev.max_gouge_at, underside.max_gouge_at, "keep the underside location in Setup2's frame");
    assert_eq!(dev.gouge_cells, underside.gouge_cells, "count only the real underside damage");
    assert!((dev.gouge_volume - underside.gouge_volume).abs() < 1e-6);
    assert!(!dev.is_clean(), "retained underside damage cannot be clean");
}
