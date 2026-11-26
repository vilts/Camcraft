//! #874: the machined stock carried into a setup turned side-on - a plate
//! faced flat in Setup1, stood on its edge in Setup2 with "what Setup1
//! leaves behind" as its stock. The carried body is never smaller than
//! the faced plate; Setup2's rest reading and the program verdict read
//! it; a gouge Setup1 cut still reads after the turn; the verdict says
//! the stock was turned; the field engine refuses by name.
//! Provenance: our own fixtures (a faced block) over plain re-clamp
//! practice; no vendor material consulted.
//!
//! Gate map (#874): covers app::stock_sim (carry: walk, entry,
//! side_on_note; simulate_document, incoming_body), app::rest (across),
//! app::setup_stock, app::posting (sim_clause), sim::stock_sim
//! (carry_side).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel, SetStock,
};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::setup_align::{AlignPreset, parts_facets_in};
use camcraft_app::setup_stock::{self, StockSource};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::{Backend, DEV_TOL, FIELD_BAND, FIELD_SIDE_CARRY_REFUSAL, simulate_document, with_carry_engine};
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0400-2FL";

fn face(cut_z: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false, blend_radius: None,
        stepover: 3.0,
        clearance_z: Height::fixed(cut_z + 30.0),
        cut_z: Height::fixed(cut_z),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

/// A 60 x 60 x 16 part in a stock Z -2..18, Setup1 faced to `cut1`, and
/// Setup2 holding the part as `turn` says.
fn document(cut1: f64, turn: impl FnOnce(&World) -> AlignPartToSetup) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(Point3::new(-30.0, -30.0, 0.0), Point3::new(30.0, 30.0, 16.0))));
    let at = Location { pos: Point3::new(0.0, 0.0, -2.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, at).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    add_face(&world, &mut h, "Face1", SrcRef::Stock, cut1);
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2"));
    h.execute(&world, turn(&world));
    (world, h)
}

fn side_on(world: &World) -> AlignPartToSetup {
    AlignPartToSetup::angles(world, "Setup2", [-90.0, 0.0, 0.0]).expect("stand it on its edge")
}

fn add_face(world: &World, h: &mut History, name: &str, src: SrcRef, cut: f64) {
    let op = CreateOperation::new(world, name, TOOL, src, face(cut)).expect("create");
    h.execute(world, op);
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

/// The top of the part as Setup2 holds it.
fn part_top(world: &World) -> f64 {
    let tris = parts_facets_in(world, Some("Setup2"));
    tris.iter().flat_map(|t| [t.a.z, t.b.z, t.c.z]).fold(f64::NEG_INFINITY, f64::max)
}

#[test]
fn a_faced_plate_carries_side_on_with_its_volume_and_verdict() {
    let (world, mut h) = document(16.0, side_on);
    assert_eq!(setup_stock::carry_refusal(&world, "Setup2"), None);
    assert_eq!(setup_stock::effective(&world, "Setup2"), Some(StockSource::FromPrevious));
    // Setup2 faces the edge down to the part: its rest reading and the
    // program verdict both read the carried body.
    let top = part_top(&world);
    add_face(&world, &mut h, "Face2", SrcRef::Entity("part".into()), top);
    // Setup1 faced the block (Z -2..18) to 16: 60 x 60 x 18 reaches Setup2,
    // 60 tall on its edge, as Face2 enters it. Never smaller; under one grid cell over a face.
    let inc = setup_stock::incoming(&world, "Setup2").expect("the carried body");
    let mut size = [inc.max[0] - inc.min[0], inc.max[1] - inc.min[1], inc.max[2] - inc.min[2]];
    assert!((size[2] - 60.0).abs() < 1e-9, "the old Y is the height: {size:?}");
    size.sort_by(f64::total_cmp);
    assert!(size[0] >= 18.0 - 1e-9 && size[0] < 18.5 && (size[1] - 60.0).abs() < 1e-9, "{size:?}");
    let real = 60.0 * 60.0 * 18.0;
    assert!(inc.volume >= real - 1e-6 && inc.volume < real * 1.03, "volume {} vs {real}", inc.volume);
    let sim = simulate_document(&world).expect("judged");
    assert_eq!(sim.dev.gouge_cells, 0, "worst {}", sim.dev.max_gouge);
    // One millimetre into the part from the new top is a gouge.
    add_face(&world, &mut h, "Face3", SrcRef::Entity("part".into()), top - 1.0);
    let sim = simulate_document(&world).expect("judged");
    assert!(sim.dev.gouge_cells > 0 && (sim.dev.max_gouge - 1.0).abs() < 1e-6, "worst {}", sim.dev.max_gouge);
}

#[test]
fn a_gouge_cut_before_a_side_on_turn_still_reads_in_the_verdict() {
    // Setup1 faces 1 mm into the part. Side-on that face is a wall the
    // new tool axis cannot see: the verdict taken before the turn keeps it.
    let (world, mut h) = document(15.0, side_on);
    add_face(&world, &mut h, "Face2", SrcRef::Entity("part".into()), part_top(&world));
    let sim = simulate_document(&world).expect("judged");
    assert!(sim.dev.gouge_cells > 0 && sim.dev.max_gouge > DEV_TOL, "the gouge is kept");
    assert!((sim.dev.max_gouge - 1.0).abs() < 1e-6, "worst {}", sim.dev.max_gouge);
}

#[test]
fn the_verdict_says_the_stock_was_turned_side_on() {
    let (world, mut h) = document(16.0, side_on);
    add_face(&world, &mut h, "Face2", SrcRef::Entity("part".into()), part_top(&world));
    let clause = camcraft_app::posting::sim_clause(&world);
    assert_eq!(clause.problem, None);
    let line = clause.not_judged.expect("the note");
    assert!(line.starts_with("note: the stock carried from setup 'Setup1' into setup 'Setup2' is turned side-on"), "{line}");
    assert!(line.contains("never inside it"), "{line}");
    // A flip keeps the tool axis: no note.
    let flip = |w: &World| AlignPartToSetup::preset(w, "Setup2", AlignPreset::FlipX).expect("flip");
    let (world, mut h) = document(16.0, flip);
    add_face(&world, &mut h, "Face2", SrcRef::Entity("part".into()), part_top(&world));
    assert_eq!(camcraft_app::posting::sim_clause(&world).not_judged, None);
}

#[test]
fn the_field_engine_refuses_the_side_on_carry_by_name() {
    let (world, mut h) = document(16.0, side_on);
    add_face(&world, &mut h, "Face2", SrcRef::Entity("part".into()), part_top(&world));
    let err = with_carry_engine(Backend::Field { band: FIELD_BAND }, || simulate_document(&world)).expect_err("refused");
    assert!(err.contains(FIELD_SIDE_CARRY_REFUSAL), "{err}");
}
