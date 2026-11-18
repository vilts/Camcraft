//! The stock's turning radius on a lathe stream (#1666): the stream
//! states `StockTurnRadius`, the radius the stock sweeps about the
//! spindle axis (a round bar's radius, a square bar's half-diagonal), and
//! the Tormach PathPilot lathe post orders its moves around G30 by it.
//! Before, the post took the stock box's half-width, so a square bar's
//! rotating corners stood outside the radius the post used.
//!
//! Provenance: our own fixtures and checks; a square of side a sweeps a
//! circle of radius a / sqrt(2) when it turns about its centre (general
//! geometry).
//!
//! Gate map (M4.4): covers app::stock_turn, app::posting (stream_records:
//! the StockTurnRadius PARAM), post::post (dump; the engine on the
//! PathPilot lathe dialect: the G30 approach and departure).
//! Lane shape (M4.4): all tests here run in seconds.

mod tormach_lathe_support;

use flecs_ecs::prelude::World;
use serde_json::json;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, SetStock};
use camcraft_app::geometry::Point3;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::post::{cnc, engine};
use camcraft_app::posting;
use camcraft_app::schema::{Location, PostOptions};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use tormach_lathe_support::{create, lathe_mch, post, revolve, stock_params, world_with, CPS};

const HALF: f64 = 6.0;

fn diag() -> f64 {
    HALF * std::f64::consts::SQRT_2
}

/// The flanged Ø12 bar's rough pass, on this stock.
fn turning_world(stock: StockShape) -> (World, History) {
    let flanged = [(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (3.5, 2.0), (3.5, 16.0), (0.0, 16.0)];
    let (world, mut history) = world_with(revolve(&flanged, 16), stock, -16.0, &["TN-1600-0R8"], false);
    let rough = OpParams::ProfileRough {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(-16.0),
        stepdown: 0.8,
        stock_to_leave: 0.25,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    create(&world, &mut history, "R1", "TN-1600-0R8", rough);
    (world, history)
}

fn square_bar() -> StockShape {
    StockShape::Box { dx: 2.0 * HALF, dy: 2.0 * HALF, dz: 32.0 }
}

fn params(records: &[cnc::Record], n: &str) -> Vec<f64> {
    records
        .iter()
        .filter_map(|r| match r {
            cnc::Record::Param { name, value } if name == n => value.as_f64(),
            _ => None,
        })
        .collect()
}

fn word(line: &str, c: char) -> Option<f64> {
    line.split(' ').skip(1).find_map(|w| w.strip_prefix(c)).and_then(|v| v.parse().ok())
}

fn rapid(r: f64, z: f64) -> cnc::Record {
    mv(MoveKind::Rapid, r, z, None)
}
fn feed(r: f64, z: f64) -> cnc::Record {
    mv(MoveKind::Feed, r, z, Some(100.0))
}
fn mv(kind: MoveKind, x: f64, z: f64, feed: Option<f64>) -> cnc::Record {
    cnc::Record::Move(cnc::MoveRec { kind, x, y: 0.0, z, feed, center: None, ccw: false, plane: Default::default(), abc: None })
}

/// One hand-built section on the square bar's box (half-width 6, face at
/// Z10, clearance Z30), with or without its turning radius stated.
fn square_section(turn: Option<f64>, moves: Vec<cnc::Record>) -> engine::PostOutput {
    let mut r = vec![cnc::Record::Param { name: "SpindleSpeed".into(), value: json!(1000.0) }];
    r.extend(stock_params(HALF, 10.0, 30.0));
    if let Some(t) = turn {
        r.push(cnc::Record::Param { name: "StockTurnRadius".into(), value: json!(t) });
    }
    r.push(cnc::Record::Tool { number: 1, name: "T1".into() });
    r.push(cnc::Record::Section { begin: true });
    r.extend(moves);
    r.push(cnc::Record::Section { begin: false });
    engine::run(CPS, &lathe_mch(false), &cnc::serialize(&r)).expect("runs")
}

// ------------------------------------------------------------ pins

#[test]
fn a_lathe_stream_states_the_radius_its_stock_sweeps() {
    // A square bar: half its diagonal, though its box is only 6 wide.
    let (world, _) = turning_world(square_bar());
    let records = posting::stream_records(&world).expect("generated");
    let turn = params(&records, "StockTurnRadius");
    assert!(!turn.is_empty() && turn.iter().all(|r| (r - diag()).abs() < 1e-9), "{turn:?}");
    assert!(params(&records, "StockXYRight").iter().all(|r| (r - HALF).abs() < 1e-9));
    // A round bar: its own radius, the box's half-width.
    let (world, _) = turning_world(StockShape::Cylinder { diameter: 2.0 * HALF, height: 32.0 });
    let records = posting::stream_records(&world).expect("generated");
    let turn = params(&records, "StockTurnRadius");
    assert!(!turn.is_empty() && turn.iter().all(|r| (r - HALF).abs() < 1e-9), "{turn:?}");
    // About the work offset's axis: a bar off the origin sweeps farther.
    let world = new_world();
    let at = Location { pos: Point3::new(3.0, 4.0, 0.0), ..Location::default() };
    let mut history = History::default();
    history.execute(&world, SetStock::new(&world, square_bar(), at).expect("stock"));
    let r = camcraft_app::stock_turn::turn_radius_for(&world, None, [0.0, 0.0]).expect("a stock");
    assert!((r - (9.0_f64).hypot(10.0)).abs() < 1e-9, "the far corner (9, 10): {r}");
    assert!(camcraft_app::stock_turn::turn_radius_for(&world, None, [f64::NAN, 0.0]).is_none());
    assert!(camcraft_app::stock_turn::turn_radius_for(&new_world(), None, [0.0, 0.0]).is_none(), "no stock");
}

#[test]
fn a_mill_stream_states_no_turning_radius() {
    // A face op on a square block on the default mill.
    let world = new_world();
    let mut history = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default());
    history.execute(&world, stock.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, "EM-0600-4FL", tool).expect("catalog tool"));
    let face = OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    let cmd = CreateOperation::new(&world, "Face1", "EM-0600-4FL", SrcRef::Stock, face);
    history.execute(&world, cmd.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    let records = posting::stream_records(&world).expect("a mill streams");
    assert!(params(&records, "StockTurnRadius").is_empty(), "a mill stream is as it was");
    assert!(!params(&records, "StockZTop").is_empty(), "the stream states its stock box");
}

#[test]
fn the_g30_moves_use_the_square_bars_corner_circle() {
    // A section that ends behind the face at r 7: outside the box's
    // half-width 6, inside the corners' circle (8.49). With the turning
    // radius stated, it ends inside the stock's envelope: refused.
    let moves = || vec![rapid(9.0, 5.0), feed(7.0, 5.0)];
    let out = square_section(None, moves());
    assert!(!out.warnings.iter().any(|w| w.contains("inside the stock's envelope")), "box only: {:?}", out.warnings);
    let out = square_section(Some(diag()), moves());
    assert!(out.warnings.iter().any(|w| w.contains("inside the stock's envelope")), "{:?}", out.warnings);
    // A start at r 7 behind the face is not outside the corners' circle:
    // no X-first approach to it from G30.
    let out = square_section(Some(diag()), vec![rapid(7.0, 5.0), feed(6.5, 5.0)]);
    assert!(!out.warnings.is_empty(), "an approach inside the corners' circle is refused: {}", out.program);
    // A hole on centre: the departure goes +X to the corners' diameter.
    let out = square_section(Some(diag()), vec![rapid(0.0, 30.0), rapid(0.0, 12.0), feed(0.0, 5.0), rapid(0.0, 30.0)]);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let xs: Vec<f64> = out.program.lines().filter(|l| *l != "G30").filter_map(|l| word(l, 'X')).collect();
    let depart = out.program.lines().rev().find(|l| l.starts_with("G0 X")).and_then(|l| word(l, 'X'));
    assert!(depart.is_some_and(|x| x >= 2.0 * diag() - 1e-3), "+X to the corners' diameter: {xs:?}\n{}", out.program);
    // A stated radius under the box's half-width never shrinks it.
    let out = square_section(Some(1.0), vec![rapid(0.0, 30.0), rapid(0.0, 12.0), feed(0.0, 5.0), rapid(0.0, 30.0)]);
    let depart = out.program.lines().rev().find(|l| l.starts_with("G0 X")).and_then(|l| word(l, 'X'));
    assert!(depart.is_some_and(|x| (x - 2.0 * HALF).abs() < 1e-3), "{}", out.program);
}

#[test]
fn a_square_bar_program_has_no_g30_rapid_inside_the_corners_circle() {
    let (world, _) = turning_world(square_bar());
    let records = posting::stream_records(&world).expect("generated");
    let (program, warnings) = post(&world, &records, PostOptions::default());
    assert!(warnings.is_empty(), "{warnings:?}");
    let lines: Vec<&str> = program.lines().collect();
    let g30: Vec<usize> = lines.iter().enumerate().filter(|(_, l)| **l == "G30").map(|(i, _)| i).collect();
    assert_eq!(g30.len(), 2, "the program has an approach and departure G30:\n{program}");
    // Follow motion blocks, skipping tool, spindle and coolant commands.
    // The departure may carry both X and Z words.
    let moves: Vec<&&str> = lines[g30[0] + 1..g30[1]]
        .iter()
        .filter(|l| matches!(l.split_whitespace().next(), Some("G0" | "G1" | "G2" | "G3")))
        .collect();
    assert!(moves.len() >= 2, "distinct approach and departure moves:\n{program}");
    let mut checked = 0;
    for (phase, line) in [("approach", moves.first()), ("departure", moves.last())] {
        let line = line.expect("motion blocks exist");
        assert!(line.starts_with("G0 "), "the G30 {phase} is rapid: {line}\n{program}");
        let x = word(line, 'X').expect("the G30 motion states X");
        assert!(x >= 2.0 * diag() - 1e-3, "a G30 {phase} inside the corners' circle: {line}\n{program}");
        checked += 1;
    }
    assert_eq!(checked, 2, "both G30 motion blocks were checked:\n{program}");
}
