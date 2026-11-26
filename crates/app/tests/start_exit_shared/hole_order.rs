//! #3328: Drill, Bore and Thread (mill) take a start pick and an exit pick
//! as the hole order. The hole under the start pick is cut first, the
//! hole under the exit pick is cut last, and the holes between keep the
//! list order, rotated to the start hole. A pick that is not on a hole is
//! refused by name. The picks ride `EditOperationParams` (undo, redo) and
//! the MCP `edit_operation` door.
//!
//! Gate map: covers app::operation (generate: the Drill, Bore and Thread
//! arms; pick_kinds), app::commands (EditOperationParams), app::mcp
//! (edit_operation), camcraft_motion::drilling (order).
//! Provenance: our own tests over our own ordering rule; no vendor
//! material consulted.

use flecs_ecs::prelude::World;
use serde_json::json;

use camcraft_app::cl::Toolpath;
use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machining2d::PassStart;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::path_picks::{PathPick, PathPicks};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::world::new_world;

use super::common::{gen_to_cltext, write_golden};
use super::{Server, call};

const TOP: f64 = 20.0;
/// The hole centres along X, in the list order: A, B, C, D.
const XS: [f64; 4] = [20.0, 60.0, 100.0, 140.0];
const Y: f64 = 30.0;

fn tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog tool")
}

fn picks(start: Option<(f64, f64)>, exit: Option<(f64, f64)>) -> PathPicks {
    let one = |p: Option<(f64, f64)>| p.map(|(x, y)| PathPick::xy(Point2::new(x, y))).into_iter().collect();
    PathPicks { start: one(start), exit: one(exit) }
}

fn holes(radius: f64, floor: f64) -> Vec<Hole> {
    XS.iter()
        .map(|&x| Hole { xy: Point2::new(x, Y), radius, entry_z: TOP, floor_z: floor, through: false, stepped: false })
        .collect()
}

fn drill(picks: PathPicks) -> OpParams {
    OpParams::Drill {
        break_through: None,
        break_through_from: Default::default(),
        clearance_z: Height::fixed(TOP + 10.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        bottom_z: None,
        cycle: CycleChoice::Drill,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: holes(3.0, 12.0),
        cross: vec![],
        picks,
    }
}

fn bore(picks: PathPicks) -> OpParams {
    OpParams::Bore {
        pass_start: PassStart::Auto,
        break_through: None,
        links: camcraft_app::links::Links::PLANAR,
        clearance_z: Height::fixed(TOP + 5.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        bottom_z: None,
        stock_to_leave: 0.0,
        stepdown: 5.0,
        feed: 500.0,
        plunge_feed: 150.0,
        climb: true,
        tolerance: 0.01,
        holes: holes(15.0, 10.0),
        picks,
    }
}

/// A 160 x 60 x 20 box stock with its corner at the origin, a Ø6 drill
/// and a Ø8 end mill.
fn stock_world() -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 160.0, dy: 60.0, dz: TOP }, Location::default()).expect("stock"));
    h.execute(&world, AddTool::new(&world, "DR", tool("DR-0600")).expect("drill"));
    h.execute(&world, AddTool::new(&world, "EM", tool("EM-0800-4FL")).expect("end mill"));
    world
}

fn op(tool: &str, params: OpParams) -> Operation {
    Operation {
        tool: tool.into(),
        src: SrcRef::Stock,
        params,
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    }
}

fn generate(world: &World, tool: &str, params: OpParams) -> Result<Toolpath, String> {
    operation::generate(world, &op(tool, params), "Op").map(|(p, _)| p).map_err(|e| e.to_string())
}

/// The X of each hole in the order the path first cuts below the top.
fn cut_order(t: &Toolpath) -> Vec<f64> {
    let mut out: Vec<f64> = Vec::new();
    for m in &t.moves {
        if m.to.z >= TOP - 1e-6 {
            continue;
        }
        let Some(&x) = XS.iter().min_by(|a, b| (m.to.x - **a).abs().total_cmp(&(m.to.x - **b).abs())) else {
            continue;
        };
        if out.last() != Some(&x) {
            out.push(x);
        }
    }
    out
}

const A: f64 = XS[0];
const B: f64 = XS[1];
const C: f64 = XS[2];
const D: f64 = XS[3];

#[test]
fn drill_cuts_the_picked_first_hole_and_last_hole() {
    let world = stock_world();
    let plain = generate(&world, "DR", drill(PathPicks::default())).expect("the list order");
    assert_eq!(cut_order(&plain), vec![A, B, C, D], "no pick is the list order");
    // The start hole C: the list order, rotated to C.
    let start = generate(&world, "DR", drill(picks(Some((C, Y)), None))).expect("a start pick");
    assert_eq!(cut_order(&start), vec![C, D, A, B]);
    // The exit hole B alone goes last.
    let exit = generate(&world, "DR", drill(picks(None, Some((B, Y))))).expect("an exit pick");
    assert_eq!(cut_order(&exit), vec![A, C, D, B]);
    // Both: start at C, end at D, the others in the rotated order.
    let both = generate(&world, "DR", drill(picks(Some((C, Y)), Some((D, Y))))).expect("both picks");
    assert_eq!(cut_order(&both), vec![C, A, B, D]);
    let cycles: Vec<f64> = both.cycles.iter().map(|c| c.xy.x).collect();
    assert_eq!(cycles, vec![C, A, B, D], "the cycle records follow the same order");
    assert!((both.moves[0].from.x - C).abs() < 1e-9, "the path starts over the first hole");
    write_golden("hole_order_drill.cl", &gen_to_cltext(&both));
}

#[test]
fn bore_cuts_the_picked_first_hole_and_last_hole() {
    let world = stock_world();
    let plain = generate(&world, "EM", bore(PathPicks::default())).expect("the list order");
    assert_eq!(cut_order(&plain), vec![A, B, C, D], "no pick is the list order");
    // A pick inside a hole's circle, off its centre, names that hole.
    let both = generate(&world, "EM", bore(picks(Some((B + 4.0, Y - 3.0)), Some((A, Y + 9.0))))).expect("both picks");
    assert_eq!(cut_order(&both), vec![B, C, D, A]);
    write_golden("hole_order_bore.cl", &gen_to_cltext(&both));
}

/// A Ø36 disc, z 0 to 20, centred at (`cx`, 0), with a blind hole of
/// `radius` from the top down to z 4. Outward-wound.
fn disc(cx: f64, radius: f64, f: &mut Vec<Triangle>) {
    let n = 96;
    let at = |r: f64, k: usize, z: f64| {
        let a = std::f64::consts::TAU * (k % n) as f64 / n as f64;
        Point3::new(cx + r * a.cos(), r * a.sin(), z)
    };
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    for k in 0..n {
        let (o, i) = (18.0, radius);
        quad(at(o, k, 0.0), at(o, k + 1, 0.0), at(o, k + 1, TOP), at(o, k, TOP), f);
        quad(at(o, k, TOP), at(o, k + 1, TOP), at(i, k + 1, TOP), at(i, k, TOP), f);
        quad(at(i, k + 1, 4.0), at(i, k, 4.0), at(i, k, TOP), at(i, k + 1, TOP), f);
        f.push(Triangle::new(Point3::new(cx, 0.0, 4.0), at(i, k, 4.0), at(i, k + 1, 4.0)));
        f.push(Triangle::new(Point3::new(cx, 0.0, 0.0), at(o, k + 1, 0.0), at(o, k, 0.0)));
    }
}

/// M8 x 1.25's minor radius (ISO 724).
const R_M8: f64 = 3.3235;
/// The three threaded holes, in the list order.
const TX: [f64; 3] = [-40.0, 0.0, 40.0];

fn thread_holes(floor: f64) -> Vec<Hole> {
    TX.iter()
        .map(|&x| Hole { xy: Point2::new(x, 0.0), radius: R_M8, entry_z: TOP, floor_z: floor, through: false, stepped: false })
        .collect()
}

fn thread(picks: PathPicks) -> OpParams {
    OpParams::Thread {
        break_through: None,
        clearance_z: Height::fixed(TOP + 10.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        bottom_z: None,
        infeed: 0.25,
        feed: 300.0,
        plunge_feed: 150.0,
        climb: true,
        holes: thread_holes(8.0),
        pitch: None,
        hand: camcraft_app::cl::ThreadHand::Right,
        external: false,
        passes: camcraft_app::machining2d::ThreadPasses::default(),
        picks,
    }
}

/// Three discs with a tap-drill hole each, a box stock around them, and
/// a generated Drill operation that opens the three holes.
fn thread_world() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 120.0, dy: 40.0, dz: TOP }, Location { pos: Point3::new(0.0, 0.0, 0.0), ..Location::default() });
    h.execute(&world, stock.expect("stock"));
    let mut facets = Vec::new();
    for x in TX {
        disc(x, R_M8, &mut facets);
    }
    h.execute(&world, ImportModel::from_mesh("discs", Mesh::from_facets(facets)));
    h.execute(&world, AddTool::new(&world, "TM", tool("TM-0600-P125")).expect("thread mill"));
    h.execute(&world, AddTool::new(&world, "TD", Tool { diameter: 6.6, shank_d: 6.6, flute_len: 25.0, ..tool("DR-0600") }).expect("tap drill"));
    let mut ahead = drill(PathPicks::default());
    if let OpParams::Drill { holes, .. } = &mut ahead {
        *holes = thread_holes(4.0).into_iter().map(|h| Hole { radius: 3.3, ..h }).collect();
    }
    h.execute(&world, CreateOperation::new(&world, "Drill1", "TD", SrcRef::Entity("discs".into()), ahead).expect("create drill"));
    h.execute(&world, GenerateOperation::new(&world, "Drill1").expect("generate drill"));
    (world, h)
}

/// The X of each threaded hole in the order the path first cuts in it.
fn thread_order(t: &Toolpath) -> Vec<f64> {
    let mut out: Vec<f64> = Vec::new();
    for m in t.moves.iter().filter(|m| m.to.z < TOP - 1e-6) {
        let Some(&x) = TX.iter().min_by(|a, b| (m.to.x - **a).abs().total_cmp(&(m.to.x - **b).abs())) else {
            continue;
        };
        if out.last() != Some(&x) {
            out.push(x);
        }
    }
    out
}

fn cached(world: &World, name: &str) -> Toolpath {
    let e = model::find_by_name(world, name).expect("op");
    model::toolpath_of(world, &e).expect("cached path").path
}

#[test]
fn thread_cuts_the_picked_first_hole_and_last_hole_and_undo_restores_the_order() {
    let (world, mut h) = thread_world();
    let src = SrcRef::Entity("discs".into());
    h.execute(&world, CreateOperation::new(&world, "Thread1", "TM", src, thread(PathPicks::default())).expect("create thread"));
    h.execute(&world, GenerateOperation::new(&world, "Thread1").expect("generate"));
    assert_eq!(thread_order(&cached(&world, "Thread1")), vec![-40.0, 0.0, 40.0], "no pick is the list order");

    let picked = thread(picks(Some((0.0, 0.0)), Some((-40.0, 1.0))));
    h.execute(&world, EditOperationParams::new(&world, "Thread1", picked).expect("the picks are taken"));
    h.execute(&world, GenerateOperation::new(&world, "Thread1").expect("generate"));
    let path = cached(&world, "Thread1");
    assert_eq!(thread_order(&path), vec![0.0, 40.0, -40.0], "start at the middle hole, end at the first of the list");
    write_golden("hole_order_thread.cl", &gen_to_cltext(&path));

    // Undo the generate and the edit: the picks are gone. Redo the edit:
    // the picks are back. Undo it again and generate: the list order.
    let live = |world: &World| model::find_by_name(world, "Thread1").and_then(|e| model::operation_of(world, &e)).expect("the op").params.path_picks();
    assert!(h.undo(&world) && h.undo(&world));
    assert!(live(&world).is_empty(), "undo clears the picks");
    assert!(h.redo(&world));
    assert_eq!(live(&world), picks(Some((0.0, 0.0)), Some((-40.0, 1.0))), "redo restores the picks");
    assert!(h.undo(&world));
    h.execute(&world, GenerateOperation::new(&world, "Thread1").expect("generate"));
    assert_eq!(thread_order(&cached(&world, "Thread1")), vec![-40.0, 0.0, 40.0], "undo restores the list order");
}

#[test]
fn a_pick_that_is_not_on_a_hole_is_refused_by_name() {
    let world = stock_world();
    for (kind, tool, params) in [
        ("Drill", "DR", drill(picks(Some((40.0, Y)), None))),
        ("Bore", "EM", bore(picks(None, Some((80.0, 55.0))))),
        ("Thread", "TM", thread(picks(Some((20.0, 0.0)), None))),
    ] {
        let why = params.named_refusal().unwrap_or_else(|| panic!("{kind}: a pick between two holes is refused"));
        assert!(why.contains(kind), "{kind}: the refusal names the kind: {why}");
        assert!(why.contains("is not on a hole"), "{kind}: {why}");
        if kind != "Thread" {
            // The generator refuses it too, for a caller that skipped the door.
            let late = generate(&world, tool, params).expect_err("no path");
            assert!(late.contains("is not on a hole"), "{kind}: {late}");
        }
    }
    // Two picks on one hole of a longer list: refused, never guessed.
    let same = drill(picks(Some((A, Y)), Some((A + 1.0, Y)))).named_refusal().expect("one hole for both");
    assert!(same.contains("Drill") && same.contains("same hole"), "{same}");
    // The picks that the tests above use are sane.
    assert_eq!(drill(picks(Some((C, Y)), Some((D, Y)))).named_refusal(), None);
}

#[test]
fn a_pick_on_a_drill_with_cross_holes_is_refused_by_name() {
    let mut params = drill(picks(Some((A, Y)), None));
    if let OpParams::Drill { holes, cross, .. } = &mut params {
        holes.clear();
        cross.push(camcraft_app::drilling::CrossHole {
            origin: Point3::new(10.0, 0.0, 5.0),
            dir: Point3::new(-1.0, 0.0, 0.0),
            radius: 2.0,
            entry_d: 0.0,
            floor_d: -5.0,
            through: false,
        });
    }
    let why = params.named_refusal().expect("a cross-hole drill takes no pick");
    assert!(why.contains("Drill") && why.contains("start point") && why.contains("cross holes"), "{why}");
}

#[test]
fn mcp_edit_operation_sets_the_hole_order_picks_and_undo_clears_them() {
    let mut s = Server::new();
    assert!(!call(&mut s, "set_stock", json!({ "box": { "dx": 160.0, "dy": 60.0, "dz": 20.0 } })).0);
    assert!(!call(&mut s, "add_tool", json!({ "catalog": "DR-0600" })).0);
    let list: Vec<_> = XS
        .iter()
        .map(|x| json!({ "xy": [x, Y], "radius": 3.0, "entry_z": 20.0, "floor_z": 12.0, "through": false }))
        .collect();
    let (err, r) = call(&mut s, "add_operation", json!({ "kind": "Drill", "tool": "DR-0600", "name": "D1", "params": { "holes": list } }));
    assert!(!err, "{r}");
    let (err, r) = call(&mut s, "edit_operation", json!({ "name": "D1", "params": { "picks": { "start": [[C, Y]], "exit": [[A, Y]] } } }));
    assert!(!err, "the picks are taken: {r}");
    let (_, job) = call(&mut s, "get_job", json!({}));
    assert!(job.to_string().contains("picks"), "the job shows the picks: {job}");
    // A pick between two holes is refused by name and changes nothing.
    let (err, r) = call(&mut s, "edit_operation", json!({ "name": "D1", "params": { "picks": { "start": [[40.0, Y]] } } }));
    let text = r.to_string();
    assert!(err && text.contains("Drill") && text.contains("is not on a hole"), "{text}");
    assert!(!call(&mut s, "undo", json!({})).0);
    let (_, job) = call(&mut s, "get_job", json!({}));
    assert!(!job.to_string().contains("picks"), "undo clears the picks: {job}");
    // The kind list says that the three kinds take both picks.
    let (_, kinds) = call(&mut s, "list_operation_types", json!({}));
    let text = kinds["content"][0]["text"].as_str().map(str::to_owned).unwrap_or_else(|| kinds.to_string());
    let listed: serde_json::Value = serde_json::from_str(&text).unwrap_or(kinds);
    for kind in ["Drill", "Bore", "Thread"] {
        let row = listed["kinds"].as_array().and_then(|a| a.iter().find(|k| k["label"] == kind)).unwrap_or_else(|| panic!("{kind} is listed"));
        assert_eq!(row["start_exit"], json!({ "start": true, "exit": true, "field": "picks" }), "{kind}");
    }
}
