//! #3333 (part of #1448): the start and exit picks of the two sinker
//! kinds. A Sinker Burn reads the start pick as the angle at which its
//! orbit starts round the burn position, and the exit pick as the angle
//! at which the orbit ends before it returns to the burn position and
//! retracts. A Sinker Side Burn reads each pick as an end of its side
//! move (`sinker_side.rs`). A pick that no orbit can hold is
//! refused by name, never moved. No pick keeps the path of every
//! earlier file.
//!
//! Gate map (#3333): covers motion::sinker_picks, motion::sinker_wear
//! (the picked orbit leg), app::operation (pick_kinds, sinker_op,
//! sinker_side_op), app::commands (create / edit doors, undo), app::mcp
//! (edit_operation picks), sim::stock_sim (the picked orbit's chords).
//! Provenance: our own SYNTHETIC fixtures (`edm/sinker_fixture.rs`) - the
//! sinker template of #530 with an invented condition, invented orbit
//! words and an invented maximum radius (no machine's manual).

#[path = "../edm/sinker_fixture.rs"]
mod sinker_fixture;
#[path = "sinker_side.rs"]
mod side;

use flecs_ecs::prelude::World;

use camcraft_app::cl::{Move, MoveKind};
use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::machine::parse_mch;
use camcraft_app::model;
use camcraft_app::operation::{OpParams, SinkerDatum, SinkerOrbit, SinkerOrbitKind, SrcRef};
use camcraft_app::path_picks::PathPick;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::simulate_document;
use camcraft_app::world::new_world;

use sinker_fixture::{block_with_cavity, electrode, ELECTRODE, GAP, SINKER};

/// The orbit radius of these tests and the machine's maximum, mm.
const R: f64 = 0.3;
const TIP: f64 = 14.0 + GAP;
/// The synthetic condition's electrode undersize per side, mm.
const UNDERSIZE: f64 = 0.4;

/// The sinker template with one synthetic condition and every orbit.
pub(crate) fn machine() -> String {
    let mut v: serde_json::Value = serde_json::from_str(SINKER).expect("template json");
    v["edm"]["conditions"] = serde_json::json!([{ "id": "S1", "gap": GAP, "undersize": UNDERSIZE, "role": "rough" }]);
    let words: Vec<_> = ["circular", "vector", "planetary"]
        .iter()
        .map(|k| serde_json::json!({ "kind": k, "word": format!("ORB-{k} R{{radius}}") }))
        .collect();
    v["edm"]["orbits"] = serde_json::Value::Array(words);
    v["edm"]["max_orbit_radius"] = serde_json::json!(0.5);
    v.to_string()
}

/// A sinker document whose cavity has the half sizes `hx`, `hy`.
pub(crate) fn doc(hx: f64, hy: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", block_with_cavity(hx, hy, 6.0)));
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default()).expect("stock"));
    let m = parse_mch(&machine()).expect("sinker parses");
    h.execute(&world, LoadMachine::new(&world, "sinker.mch".into(), Some(m)).expect("mount"));
    h.execute(&world, AddTool::new(&world, ELECTRODE, electrode()).expect("an electrode is a tool"));
    (world, h)
}

/// `p` with the given start and exit picks.
pub(crate) fn picked(mut p: OpParams, start: Option<[f64; 2]>, exit: Option<[f64; 2]>) -> OpParams {
    let list = |q: Option<[f64; 2]>| q.map(|[x, y]| PathPick::xy(Point2::new(x, y))).into_iter().collect::<Vec<_>>();
    if let Some(k) = p.path_picks_mut() {
        k.start = list(start);
        k.exit = list(exit);
    }
    p
}

pub(crate) fn moves(world: &World, name: &str) -> Vec<Move> {
    let e = model::find_by_name(world, name).expect("op");
    model::toolpath_of(world, &e).expect("generated").path.moves
}

pub(crate) fn notes(world: &World, name: &str) -> Vec<String> {
    let e = model::find_by_name(world, name).expect("op");
    model::toolpath_of(world, &e).expect("generated").path.notes
}

/// Create (or edit) `Op1` with `p`, generate it, and return its moves.
pub(crate) fn generated(world: &World, h: &mut History, p: OpParams) -> Vec<Move> {
    if model::find_by_name(world, "Op1").is_some() {
        h.execute(world, EditOperationParams::new(world, "Op1", p).expect("edit"));
    } else {
        h.execute(world, CreateOperation::new(world, "Op1", ELECTRODE, SrcRef::Stock, p).expect("create"));
    }
    h.execute(world, GenerateOperation::new(world, "Op1").expect("generates"));
    moves(world, "Op1")
}

/// The refusal text of `p` at the creation door.
pub(crate) fn refused(world: &World, p: OpParams) -> String {
    match CreateOperation::new(world, "Refused", ELECTRODE, SrcRef::Stock, p) {
        Ok(_) => panic!("the pick is taken, not refused"),
        Err(e) => e.to_string(),
    }
}

/// A 6 mm burn at the origin to the cavity floor at Z 14, with `orbit`.
fn burn(orbit: Option<SinkerOrbit>) -> OpParams {
    let mut p = sinker_fixture::burn(SinkerDatum::WorkTop, 6.0, 14.0);
    if let OpParams::SinkerBurn(b) = &mut p {
        b.orbit = orbit;
    }
    p
}

fn orbit(kind: SinkerOrbitKind) -> Option<SinkerOrbit> {
    Some(SinkerOrbit { kind, radius: R, vectors: 4, turns: 2 })
}

/// The cavity the electrode, the gap and the orbit radius leave.
fn orbit_doc() -> (World, History) {
    doc(5.0 + GAP + R, 3.0 + GAP + R)
}

/// The orbit's XY offsets: every feed after the plunge.
fn orbit_xy(path: &[Move]) -> Vec<(f64, f64)> {
    let feeds: Vec<_> = path.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    assert!(feeds.iter().all(|m| (m.to.z - TIP).abs() < 1e-12), "every feed ends at the burn floor's tip");
    feeds.iter().skip(1).map(|m| (m.to.x, m.to.y)).collect()
}

fn near(p: (f64, f64), x: f64, y: f64) -> bool {
    (p.0 - x).abs() < 1e-9 && (p.1 - y).abs() < 1e-9
}

/// The angle that the on-radius vertices of `o` turn through, counter-clockwise, in turns.
fn turns(o: &[(f64, f64)]) -> f64 {
    let rim: Vec<_> = o.iter().filter(|p| p.0.hypot(p.1) > R - 1e-9).collect();
    let mut sum = 0.0;
    for w in rim.windows(2) {
        let d = w[1].1.atan2(w[1].0) - w[0].1.atan2(w[0].0);
        sum += d.rem_euclid(std::f64::consts::TAU);
    }
    sum / std::f64::consts::TAU
}

#[test]
fn a_circular_orbit_starts_at_the_start_pick_and_ends_at_the_exit_pick() {
    let (world, mut h) = orbit_doc();
    let auto = generated(&world, &mut h, burn(orbit(SinkerOrbitKind::Circular)));
    // A start 5 mm out at +Y: the orbit starts at 90 degrees. An exit at -X: it ends at 180 degrees.
    let path = generated(&world, &mut h, picked(burn(orbit(SinkerOrbitKind::Circular)), Some([0.0, 5.0]), Some([-4.0, 0.0])));
    let o = orbit_xy(&path);
    assert!(near(o[0], 0.0, R), "the orbit starts at the start pick's angle: {:?}", o[0]);
    assert!(near(o[o.len() - 2], -R, 0.0), "the orbit ends at the exit pick's angle: {:?}", o[o.len() - 2]);
    assert_eq!(o.last(), Some(&(0.0, 0.0)), "then it returns to the burn position");
    assert!(o.iter().all(|p| p.0.hypot(p.1) <= R), "no move leaves the radius");
    for (x, y) in [(R, 0.0), (0.0, R), (-R, 0.0), (0.0, -R)] {
        assert!(o.iter().any(|p| near(*p, x, y)), "the axis direction ({x}, {y}) is a vertex");
    }
    assert!((turns(&o) - 1.25).abs() < 1e-9, "one full circle, then on to the exit: {} turns", turns(&o));
    let last = path.last().expect("a retract");
    assert_eq!((last.kind, last.to.x, last.to.y, last.to.z), (MoveKind::Rapid, 0.0, 0.0, 25.0), "the retract stays on the burn axis");
    assert!(notes(&world, "Op1").iter().any(|n| n.contains("start point") && n.contains("exit point")), "{:?}", notes(&world, "Op1"));
    let verdict = simulate_document(&world).expect("judged");
    assert_eq!(verdict.dev.gouge_cells, 0, "the picked orbit gouges at {:?}", verdict.dev.max_gouge_at);

    // A start on +X is the automatic start: the same moves as no pick.
    let same = generated(&world, &mut h, picked(burn(orbit(SinkerOrbitKind::Circular)), Some([2.0, 0.0]), None));
    assert_eq!(same, auto, "a start at 0 degrees is the automatic orbit");
    // Undo and redo restore the previous path.
    let exit_only = generated(&world, &mut h, picked(burn(orbit(SinkerOrbitKind::Circular)), None, Some([0.0, -1.0])));
    let o = orbit_xy(&exit_only);
    assert!(near(o[0], R, 0.0) && near(o[o.len() - 2], 0.0, -R), "an exit alone keeps the start at +X");
    h.undo(&world);
    h.undo(&world);
    assert_eq!(moves(&world, "Op1"), auto, "undo restores the previous path");
    h.redo(&world);
    h.redo(&world);
    assert_eq!(moves(&world, "Op1"), exit_only, "redo restores the picked path");
}

#[test]
fn a_planetary_orbit_reaches_its_radius_at_the_start_pick() {
    let (world, mut h) = orbit_doc();
    let path = generated(&world, &mut h, picked(burn(orbit(SinkerOrbitKind::Planetary)), Some([0.0, 5.0]), Some([3.0, -3.0])));
    let o = orbit_xy(&path);
    let first_rim = o.iter().find(|p| p.0.hypot(p.1) > R - 1e-9).expect("the spiral reaches the radius");
    assert!(near(*first_rim, 0.0, R), "the spiral reaches the radius at the start pick: {first_rim:?}");
    let s = R * std::f64::consts::FRAC_1_SQRT_2;
    assert!(near(o[o.len() - 2], s, -s), "the closing circle ends at the exit pick: {:?}", o[o.len() - 2]);
    assert_eq!(o.last(), Some(&(0.0, 0.0)));
    assert!(o.iter().all(|p| p.0.hypot(p.1) <= R), "no move leaves the radius");
    assert_eq!(simulate_document(&world).expect("judged").dev.gouge_cells, 0);
}

#[test]
fn a_vector_orbit_starts_on_the_picked_vector_and_ends_on_the_exit_vector() {
    let (world, mut h) = orbit_doc();
    let reach = |path: &[Move]| orbit_xy(path).into_iter().filter(|p| p.0.hypot(p.1) > R - 1e-9).collect::<Vec<_>>();
    let order = |got: &[(f64, f64)], want: [(f64, f64); 4]| got.len() == 4 && got.iter().zip(want).all(|(g, w)| near(*g, w.0 * R, w.1 * R));
    let (px, py, mx, my) = ((1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0));
    let auto = reach(&generated(&world, &mut h, burn(orbit(SinkerOrbitKind::Vector))));
    assert!(order(&auto, [px, py, mx, my]), "{auto:?}");
    let start = reach(&generated(&world, &mut h, picked(burn(orbit(SinkerOrbitKind::Vector)), Some([0.0, -2.0]), None)));
    assert!(order(&start, [my, px, py, mx]), "the picked vector is burned first: {start:?}");
    let exit = reach(&generated(&world, &mut h, picked(burn(orbit(SinkerOrbitKind::Vector)), None, Some([0.0, 2.0]))));
    assert!(order(&exit, [mx, my, px, py]), "the exit vector is burned last: {exit:?}");
    let both = reach(&generated(&world, &mut h, picked(burn(orbit(SinkerOrbitKind::Vector)), Some([1.0, 0.0]), Some([0.0, 3.0]))));
    assert!(order(&both, [px, my, mx, py]), "the vectors run the other way round to end on the exit: {both:?}");
    let path = moves(&world, "Op1");
    assert_eq!(orbit_xy(&path).last(), Some(&(0.0, 0.0)), "each vector returns to the burn position");
}

#[test]
fn a_pick_that_the_orbit_cannot_hold_is_refused_by_name() {
    let (world, _h) = orbit_doc();
    // No orbit: a straight plunge has no free start and no free exit.
    for (start, exit, what) in [(Some([0.0, 5.0]), None, "start point"), (None, Some([0.0, 5.0]), "exit point")] {
        let why = refused(&world, picked(burn(None), start, exit));
        assert!(why.contains("Sinker Burn") && why.contains(what) && why.contains("without an orbit"), "{why}");
    }
    // A pick on the burn position gives no angle.
    let why = refused(&world, picked(burn(orbit(SinkerOrbitKind::Circular)), Some([0.0, 0.0]), None));
    assert!(why.contains("Sinker Burn") && why.contains("start point") && why.contains("on the burn position"), "{why}");
    // A vector orbit: a pick between two vectors is on no vector.
    let why = refused(&world, picked(burn(orbit(SinkerOrbitKind::Vector)), Some([1.0, 1.0]), None));
    assert!(why.contains("Sinker Burn") && why.contains("start point") && why.contains("on no vector"), "{why}");
    // And an exit that the order from the start cannot end on.
    let why = refused(&world, picked(burn(orbit(SinkerOrbitKind::Vector)), Some([1.0, 0.0]), Some([-1.0, 0.0])));
    assert!(why.contains("Sinker Burn") && why.contains("exit point") && why.contains("180"), "{why}");
}

/// The undersize law and the cavity limits hold with a pick: a rough pass
/// whose orbit fits the finished cavity generates from the picked angle,
/// and one whose orbit burns past the cavity is refused by the same law.
#[test]
fn a_picked_orbit_keeps_the_undersize_law_and_the_cavity_limits() {
    use camcraft_app::operation::SinkerPass;
    let (world, mut h) = orbit_doc();
    let rough = |radius: f64| {
        let mut p = burn(Some(SinkerOrbit { kind: SinkerOrbitKind::Circular, radius, vectors: 4, turns: 1 }));
        if let OpParams::SinkerBurn(b) = &mut p {
            b.pass = SinkerPass::Rough;
            (b.cavity_x, b.cavity_y) = (10.0 + 2.0 * UNDERSIZE, 6.0 + 2.0 * UNDERSIZE);
        }
        picked(p, Some([0.0, 5.0]), Some([-4.0, 0.0]))
    };
    let path = generated(&world, &mut h, rough(R));
    let feeds: Vec<_> = path.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    let tip = 14.0 + UNDERSIZE - R;
    assert!(feeds.iter().all(|m| (m.to.z - tip).abs() < 1e-9), "the tip stands the undersize less the radius above the floor");
    assert!(near((feeds[1].to.x, feeds[1].to.y), 0.0, R), "the orbit starts at the start pick's angle");
    assert!(feeds.iter().all(|m| m.to.x.hypot(m.to.y) <= R), "no move leaves the radius, so the burn stays inside the cavity limits");
    // The gap plus a 0.4 radius is past the 0.4 undersize: the law refuses, pick or no pick.
    h.execute(&world, EditOperationParams::new(&world, "Op1", rough(0.4)).expect("the edit lands"));
    let why = match GenerateOperation::new(&world, "Op1") {
        Ok(_) => panic!("an orbit past the cavity generated"),
        Err(e) => e.to_string(),
    };
    assert!(why.contains("sinker rough") && why.contains("wider than the 10.800 cavity"), "{why}");
}
