//! #2172: the model, the stock and the fixtures stand on the floor of the
//! work envelope (the grid). A loaded model stands on it; a fixture placed
//! under the stock stands on it and the model and the stock stand on the
//! fixture; removing the fixture brings them back down; undo and redo
//! restore each state. The floor is display only: no stored position and
//! no posted line changes.
//! Provenance: our own tests over our own viewport law; no vendor material.

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::machine::{self, AxisLimits, Machine};
use camcraft_app::{fixture, model, posting, setup};

use crate::egui_app::world;
use crate::state::{CommandKind, State};

fn mill() -> Machine {
    mill_z(-250.0, 0.0)
}

/// A mill with the Z stroke `lo..hi` (the seeded program clears at Z57,
/// so its post test needs a stroke above Z=0).
fn mill_z(lo: f64, hi: f64) -> Machine {
    let ax = |name: &str, a: f64, b: f64| AxisLimits { name: name.into(), travel: Some((a, b)), ..AxisLimits::default() };
    Machine {
        model: "Test mill".into(),
        axes: vec![ax("X", -300.0, 300.0), ax("Y", -200.0, 200.0), ax("Z", lo, hi)],
        ..Machine::default()
    }
}

/// A 20 x 20 x 30 block centred on the world origin: half of it is below
/// world Z=0, as a model drawn about its own centre loads.
fn centred_block() -> State {
    let mut st = State::new();
    st.history.execute(
        &st.world,
        ImportModel::from_mesh("Block", cube(Point3::new(-10.0, -10.0, -15.0), Point3::new(10.0, 10.0, 15.0))),
    );
    st.doc_version += 1;
    st
}

fn low_z(st: &State, name: &str) -> f64 {
    let e = model::find_by_name(&st.world, name).expect("the body exists");
    model::entity_world_aabb(&st.world, &e).expect("the body has geometry").min.z
}

fn stock_name(st: &State) -> String {
    setup::stock_entity_name(&st.world, setup::active(&st.world).as_deref())
}

/// The stored poses of every named body: the document truth the posted
/// program reads.
fn poses(st: &State) -> Vec<(String, [f64; 3])> {
    model::named_entities(&st.world)
        .into_iter()
        .filter_map(|(n, e)| model::transform_of(&st.world, &e).map(|l| (n, [l.pos.x, l.pos.y, l.pos.z])))
        .collect()
}

/// The grid and the envelope bottom both lie at `z`.
fn assert_furniture_at(st: &State, z: f64, why: &str) {
    assert_eq!(crate::floor::floor_z(st), z, "{why}: the floor");
    let grid = crate::interact::ground_grid_lines(st);
    assert!(!grid.is_empty(), "{why}: the grid draws");
    assert!(grid.iter().all(|v| v[2] == z as f32), "{why}: the grid lies on the floor");
    let env = crate::work_envelope::envelope_lines(st);
    assert_eq!(env.len(), 24, "{why}: the envelope draws");
    let bottom = env.iter().map(|v| v[2]).fold(f32::INFINITY, f32::min);
    let top = env.iter().map(|v| v[2]).fold(f32::NEG_INFINITY, f32::max);
    assert_eq!(bottom, z as f32, "{why}: the envelope stands on the floor");
    assert_eq!(top, (z + 250.0) as f32, "{why}: the envelope keeps its Z travel");
}

#[test]
fn a_loaded_model_has_its_lowest_point_on_the_floor() {
    let mut st = centred_block();
    machine::set(&st.world, "test.mch", Some(&mill()));
    st.show_work_envelope = true;
    let before = poses(&st);
    assert_eq!(low_z(&st, "Block"), -15.0, "the model loads half below world Z=0");
    assert_furniture_at(&st, -15.0, "a loaded model");
    assert_eq!(poses(&st), before, "the floor moves no stored body");
    // The full frame scene computes the floor too, and writes nothing.
    let _ = world::collect(&st);
    assert_eq!(poses(&st), before);
}

#[test]
fn a_created_fixture_stands_on_the_floor_and_lifts_the_model_and_stock() {
    let mut st = centred_block();
    machine::set(&st.world, "test.mch", Some(&mill()));
    st.show_work_envelope = true;
    st.pending.push_back(CommandKind::StockFit { margin: 2.0, shape: 0 });
    st.drain_actions();
    let stock = stock_name(&st);
    let stock_low = low_z(&st, &stock);
    assert_eq!(stock_low, -17.0, "the fitted stock is 2 mm under the model");
    assert_furniture_at(&st, stock_low, "model and stock, no fixture");

    let vise = camcraft_app::vise::size("6 in").map(camcraft_app::vise::ViseSize::half_open).expect("6 in");
    st.pending.push_back(CommandKind::InsertBuiltinVise { setup: None, params: vise, fit_stock: true, pos: [0.0; 3], yaw_deg: 0.0 });
    st.drain_actions();
    let tagged = fixture::tagged(&st.world, None);
    assert_eq!(tagged.len(), 1, "one vise is placed: {}", st.status);
    let name = tagged[0].0.clone();
    let vise_low = low_z(&st, &name);
    assert!(vise_low < stock_low, "the vise reaches under the stock: {vise_low} vs {stock_low}");
    assert_furniture_at(&st, vise_low, "the fixture stands on the floor");
    // The stock sits on the vise floor (its frame), the model in the stock.
    let seat = model::transform_of(&st.world, &tagged[0].1).expect("vise pose").pos.z;
    assert_eq!(low_z(&st, &stock), seat, "the stock stands on the fixture");
    assert!(low_z(&st, "Block") >= seat, "the model stands on the fixture");
    assert!(seat > vise_low, "the model and the stock are lifted off the floor");
}

#[test]
fn removing_the_fixture_drops_the_bodies_back_and_undo_redo_restore_each_state() {
    let mut st = centred_block();
    machine::set(&st.world, "test.mch", Some(&mill()));
    st.show_work_envelope = true;
    st.pending.push_back(CommandKind::StockFit { margin: 2.0, shape: 0 });
    st.drain_actions();
    let stock_low = low_z(&st, &stock_name(&st));
    let vise = camcraft_app::vise::size("6 in").map(camcraft_app::vise::ViseSize::half_open).expect("6 in");
    st.pending.push_back(CommandKind::InsertBuiltinVise { setup: None, params: vise, fit_stock: true, pos: [0.0; 3], yaw_deg: 0.0 });
    st.drain_actions();
    let name = fixture::tagged(&st.world, None).first().map(|(n, _)| n.clone()).expect("the vise is placed");
    let vise_low = low_z(&st, &name);
    let held = poses(&st);
    assert_furniture_at(&st, vise_low, "with the fixture");

    st.pending.push_back(CommandKind::DeleteFixture(name.clone()));
    st.drain_actions();
    assert!(model::find_by_name(&st.world, &name).is_none(), "the fixture is removed: {}", st.status);
    assert_furniture_at(&st, stock_low, "the fixture is removed: the bodies are on the floor again");

    st.pending.push_back(CommandKind::Undo);
    st.drain_actions();
    assert_furniture_at(&st, vise_low, "undo of the removal");
    assert_eq!(poses(&st), held, "undo restores every stored pose");

    st.pending.push_back(CommandKind::Redo);
    st.drain_actions();
    assert_furniture_at(&st, stock_low, "redo of the removal");

    st.pending.push_back(CommandKind::Undo); // the removal
    st.pending.push_back(CommandKind::Undo); // the placement
    st.drain_actions();
    assert!(fixture::tagged(&st.world, None).is_empty(), "undo of the placement");
    assert_furniture_at(&st, stock_low, "undo of the placement");
    st.pending.push_back(CommandKind::Redo);
    st.drain_actions();
    assert_furniture_at(&st, vise_low, "redo of the placement");
}

#[test]
fn the_floor_never_changes_the_posted_records_or_the_checks() {
    let mut st = State::new();
    world::seed(&mut st);
    machine::set(&st.world, "test.mch", Some(&mill_z(-200.0, 200.0)));
    // The record stream the post engine translates (the dialect-golden
    // seam) and the verification gate's findings (the collision, gouge
    // and travel verdicts): both read the stored world.
    let before = posting::stream_records(&st.world).expect("the seeded document streams");
    let gate = posting::verification_gate(&st.world);
    let held = poses(&st);
    // Every floor reader runs: the grid, the envelope and the frame scene.
    st.show_work_envelope = true;
    for _ in 0..2 {
        assert!(crate::floor::floor_z(&st) < 0.0, "the seeded stock reaches under world Z=0");
        assert!(!crate::interact::ground_grid_lines(&st).is_empty());
        assert!(!crate::work_envelope::envelope_lines(&st).is_empty());
        let _ = world::collect(&st);
    }
    assert_eq!(poses(&st), held, "no stored pose moves");
    let after = posting::stream_records(&st.world).expect("the seeded document streams again");
    assert!(!before.is_empty(), "the seeded program has records");
    assert_eq!(format!("{after:?}"), format!("{before:?}"), "the same posted records, to the last digit");
    assert_eq!(posting::verification_gate(&st.world), gate, "the same check findings");
}
