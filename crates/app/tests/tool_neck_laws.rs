//! #852 acceptance: a tool's drawn neck feeds the stack laws and the
//! simulated tool where it is narrower than the stated shank, never
//! wider. A necked tool clears steel its shank cylinder would strike
//! (the post gate's own per-op fixture law), the reach laws' cutter
//! carries the neck's bands, and the simulation draws the neck; a tool
//! with no drawn cutter - or a drawn one with no neck - judges and draws
//! bit-identically to the stated cylinder. Our own tests.
//!
//! Gate map (M4.4): covers app::tool_cutter, app::sim_tool_body, app::fixture_struck, app::operation::job, app::operation::stamp.

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, SetStock, SetToolCutter};
use camcraft_app::fixture::FixtureBox;
use camcraft_app::fixture_struck;
use camcraft_app::geometry::Point3;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::sim_tool_body;
use camcraft_app::spun::SpunPoint;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::verify::{Cutter, Warning};
use camcraft_app::world::new_world;
use camcraft_app::{tool_body, tool_cutter};
use flecs_ecs::prelude::World;

/// Ø10 x 22 flutes, Ø10 x 40 shank (stickout 62).
const TOOL: &str = "EM-1000-4FL";
const OP: &str = "Face1";

fn stored() -> Tool {
    catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog entry")
}

fn pt(z: f64, r: f64) -> SpunPoint {
    SpunPoint::line(z, r)
}

/// The tool drawn with a Ø8 neck from the flute top (22) to 30.
fn necked() -> Vec<SpunPoint> {
    vec![pt(0.0, 0.0), pt(0.0, 5.0), pt(22.0, 5.0), pt(22.0, 4.0), pt(30.0, 4.0), pt(30.0, 5.0), pt(62.0, 5.0), pt(62.0, 0.0)]
}

/// The tool drawn exactly as its numbers state it: no neck.
fn plain() -> Vec<SpunPoint> {
    vec![pt(0.0, 0.0), pt(0.0, 5.0), pt(62.0, 5.0), pt(62.0, 0.0)]
}

fn world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("stock"));
    history.execute(&world, AddTool::new(&world, TOOL, stored()).expect("tool"));
    let face = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    history.execute(&world, CreateOperation::new(&world, OP, TOOL, SrcRef::Stock, face).expect("op"));
    history.execute(&world, GenerateOperation::new(&world, OP).expect("generate"));
    (world, history)
}

fn draw(world: &World, history: &mut History, pts: Option<Vec<SpunPoint>>) {
    history.execute(world, SetToolCutter::new(world, TOOL, pts).expect("drawn cutter"));
}

/// A feed along +X with the tip at z = 0, beside a block whose near face
/// stands 4.5 mm off the path - outside the Ø8 neck, inside the Ø10
/// shank - over heights 23..28, inside the neck's span (22..30) and
/// above the flutes.
fn beside_the_neck(world: &World) -> Vec<Warning> {
    let op = model::find_by_name(world, OP).and_then(|e| model::operation_of(world, &e)).expect("op");
    let path = Toolpath::new(vec![Move::new(MoveKind::Feed, Point3::new(0.0, 0.0, 0.0), Point3::new(40.0, 0.0, 0.0), None)]);
    let block = FixtureBox { min: Point3::new(5.0, 4.5, 23.0), max: Point3::new(35.0, 20.0, 28.0) };
    fixture_struck::op_hits(world, &op, &path, &[], &[block], &[]).expect("judged")
}

#[test]
fn a_necked_tool_clears_steel_its_shank_cylinder_strikes() {
    let (world, mut history) = world();
    let cylinder = beside_the_neck(&world);
    assert!(!cylinder.is_empty(), "the stated Ø10 shank strikes the block 4.5 mm off");
    draw(&world, &mut history, Some(necked()));
    assert!(beside_the_neck(&world).is_empty(), "the drawn Ø8 neck passes 0.5 mm clear");
    // Undo the drawing: the cylinder is judged again.
    assert!(history.undo(&world));
    assert_eq!(beside_the_neck(&world), cylinder);
}

#[test]
fn a_drawn_cutter_with_no_neck_judges_bit_identically() {
    let (world, mut history) = world();
    let cylinder = beside_the_neck(&world);
    let outline = sim_tool_body::for_operation(&world, OP).expect("drawn").expect("a mill");
    draw(&world, &mut history, Some(plain()));
    assert_eq!(beside_the_neck(&world), cylinder);
    assert_eq!(sim_tool_body::for_operation(&world, OP).expect("drawn").expect("a mill"), outline);
    let e = model::find_by_name(&world, TOOL).expect("tool");
    assert!(tool_cutter::neck_of(&world, &e, &stored()).is_empty());
}

#[test]
fn the_reach_laws_cutter_carries_the_neck_and_no_neck_reads_the_cylinder() {
    let (world, mut history) = world();
    let e = model::find_by_name(&world, TOOL).expect("tool");
    let cutter = |world: &World| {
        let neck = tool_cutter::neck_of(world, &e, &stored());
        let (judged, body) = tool_body::judged(world, &e, stored()).expect("judged");
        let g = judged.geom();
        let stack = tool_body::stack(&g, body.as_ref()).with_neck(neck);
        Cutter::judged(&g, Some(Cutter::of(&judged).with_body(&g, &stack)))
    };
    assert_eq!(cutter(&world).shank_bands(), vec![(22.0, 5.0, 40.0)]);
    assert_eq!(cutter(&world), Cutter::of(&stored()), "no drawn cutter: the very cutter of the numbers");
    draw(&world, &mut history, Some(necked()));
    assert_eq!(cutter(&world).shank_bands(), vec![(22.0, 4.0, 8.0), (30.0, 5.0, 32.0)]);
    // A neck that no longer fits the numbers the laws read is dropped.
    let g = Tool { shank_d: 7.0, ..stored() }.geom();
    assert_eq!(Cutter::judged(&g, Some(Cutter { shank_radius: 3.5, ..cutter(&world) })).shank_bands(), vec![(22.0, 3.5, 40.0)]);
}

#[test]
fn the_simulation_draws_the_neck() {
    let (world, mut history) = world();
    let before = sim_tool_body::for_operation(&world, OP).expect("drawn").expect("a mill");
    assert!(before.cutter.iter().all(|p| p.0 <= 22.0 || p.1 == 0.0 || p.1 == 5.0), "the stated shank: Ø10 to the stickout");
    draw(&world, &mut history, Some(necked()));
    let after = sim_tool_body::for_operation(&world, OP).expect("drawn").expect("a mill");
    assert!(after.cutter.contains(&(22.0, 4.0)) && after.cutter.contains(&(30.0, 4.0)), "the Ø8 neck: {:?}", after.cutter);
    assert!(after.cutter.contains(&(30.0, 5.0)) && after.cutter.contains(&(62.0, 5.0)), "then the shank: {:?}", after.cutter);
    assert_eq!(after.cutter.last(), Some(&(62.0, 0.0)));
    assert_eq!(after.holder, before.holder, "the holder above is unchanged");
    assert!(history.undo(&world));
    assert_eq!(sim_tool_body::for_operation(&world, OP).expect("drawn").expect("a mill"), before);
}
