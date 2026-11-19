//! #1053: the cross drill on a model imported coarser than the
//! importer promises. Its part law (`cross_part`), radial rapid law
//! (`rapids_radial`), standoffs (`cross_stock`), raw-stock entry
//! (`cross_entry`) and reach/holder laws (`drill_reach`) all tighten
//! by the model's excess while a positive tolerance is left, and read
//! untightened past that where the program gate refuses by name.
//!
//! Gate map (#1053): covers app::operation (fidelity_law, cross_spindle,
//! cross_stock, cross_entry, cross_part, drill_reach, drill_station,
//! generate, kept_rejudge), app::model_fidelity (op_problem),
//! sim::verify (rapids_radial).
//! Lane shape (M4.4): all tests here run in seconds.
//!
//! Provenance: our own box/bar fixtures (from the cross suites) over
//! our own laws; no vendor material consulted.

use flecs_ecs::prelude::World;

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::drilling::{CrossHole, CycleChoice};
use camcraft_app::geometry::{cube, Mesh, Point3, Triangle};
use camcraft_app::machine::{AxisLimits, Machine};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::{model_fidelity, posting};
use camcraft_app::world::new_world;

const TIGHT: f64 = 0.012;
const TOO_COARSE: f64 = 0.030;
const EXCESS: f64 = 0.007;

fn quad(a: Point3, b: Point3, c: Point3, d: Point3) -> Vec<Triangle> {
    vec![Triangle::new(a, b, c), Triangle::new(a, c, d)]
}

fn sp(along: usize, u: usize, v: usize, s: f64, pu: f64, pv: f64) -> Point3 {
    let mut c = [0.0; 3];
    c[along] = s;
    c[u] = pu;
    c[v] = pv;
    Point3::new(c[0], c[1], c[2])
}

fn circle(cu: f64, cv: f64, r: f64, n: usize) -> Vec<(f64, f64)> {
    (0..n).map(|k| { let a = std::f64::consts::TAU * k as f64 / n as f64; (cu + r * a.cos(), cv + r * a.sin()) }).collect()
}

fn wall_ring(along: usize, u: usize, v: usize, poly: &[(f64, f64)], s0: f64, s1: f64) -> Vec<Triangle> {
    let (s0, s1) = if matches!((u, v, along), (0, 1, 2) | (1, 2, 0) | (2, 0, 1)) { (s0, s1) } else { (s1, s0) };
    let mut f = Vec::new();
    for i in 0..poly.len() {
        let j = (i + 1) % poly.len();
        let (a, b, c, d) = (sp(along, u, v, s0, poly[i].0, poly[i].1), sp(along, u, v, s0, poly[j].0, poly[j].1), sp(along, u, v, s1, poly[j].0, poly[j].1), sp(along, u, v, s1, poly[i].0, poly[i].1));
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    }
    f
}

fn disc(axes: [usize; 3], center: (f64, f64), r: f64, n: usize, s: f64) -> Vec<Triangle> {
    let [along, u, v] = axes;
    let p = circle(center.0, center.1, r, n);
    let ctr = sp(along, u, v, s, center.0, center.1);
    (0..n).map(|i| Triangle::new(ctr, sp(along, u, v, s, p[i].0, p[i].1), sp(along, u, v, s, p[(i + 1) % n].0, p[(i + 1) % n].1))).collect()
}

fn annulus(axes: [usize; 3], center: (f64, f64), half: (f64, f64), r: f64, n: usize, s: f64) -> Vec<Triangle> {
    let [along, u, v] = axes;
    let (cu, cv) = center;
    let (hu, hv) = half;
    let q = |k: usize| {
        let a = std::f64::consts::TAU * k as f64 / n as f64;
        let (co, si) = (a.cos(), a.sin());
        let t = (co.abs() / hu).max(si.abs() / hv);
        (cu + co / t, cv + si / t)
    };
    let c = |k: usize| { let a = std::f64::consts::TAU * k as f64 / n as f64; (cu + r * a.cos(), cv + r * a.sin()) };
    let mut f = Vec::new();
    for k in 0..n {
        let (qa, qb, ca, cb) = (sp(along, u, v, s, q(k).0, q(k).1), sp(along, u, v, s, q(k + 1).0, q(k + 1).1), sp(along, u, v, s, c(k).0, c(k).1), sp(along, u, v, s, c(k + 1).0, c(k + 1).1));
        f.push(Triangle::new(qa, ca, cb));
        f.push(Triangle::new(qa, cb, qb));
    }
    f
}

/// X [-15, 15], Y [-5, 5], Z [-15, 15] with a blind X hole at z = 10
/// (floor x = 5) and a through X hole at z = -10.
fn radial_bar() -> Vec<Triangle> {
    let p = Point3::new;
    let mut t = Vec::new();
    t.extend(quad(p(-15.0, 5.0, -15.0), p(-15.0, 5.0, 15.0), p(15.0, 5.0, 15.0), p(15.0, 5.0, -15.0)));
    t.extend(quad(p(-15.0, -5.0, -15.0), p(15.0, -5.0, -15.0), p(15.0, -5.0, 15.0), p(-15.0, -5.0, 15.0)));
    t.extend(quad(p(-15.0, -5.0, 15.0), p(15.0, -5.0, 15.0), p(15.0, 5.0, 15.0), p(-15.0, 5.0, 15.0)));
    t.extend(quad(p(-15.0, -5.0, -15.0), p(-15.0, 5.0, -15.0), p(15.0, 5.0, -15.0), p(15.0, -5.0, -15.0)));
    t.extend(quad(p(-15.0, -5.0, -5.0), p(-15.0, -5.0, 15.0), p(-15.0, 5.0, 15.0), p(-15.0, 5.0, -5.0)));
    t.extend(annulus([0, 1, 2], (0.0, -10.0), (5.0, 5.0), 2.0, 24, -15.0));
    t.extend(annulus([0, 1, 2], (0.0, 10.0), (5.0, 5.0), 2.0, 24, 15.0));
    t.extend(annulus([0, 1, 2], (0.0, -10.0), (5.0, 5.0), 2.0, 24, 15.0));
    t.extend(quad(p(15.0, -5.0, -5.0), p(15.0, 5.0, -5.0), p(15.0, 5.0, 5.0), p(15.0, -5.0, 5.0)));
    let mut cw = circle(0.0, 10.0, 2.0, 24);
    cw.reverse();
    t.extend(wall_ring(0, 1, 2, &cw, 5.0, 15.0));
    t.extend(disc([0, 1, 2], (0.0, 10.0), 2.0, 24, 5.0));
    let mut cw2 = circle(0.0, -10.0, 2.0, 24);
    cw2.reverse();
    t.extend(wall_ring(0, 1, 2, &cw2, -15.0, 15.0));
    t
}

fn hole(x: f64, y: f64, z: f64, dx: f64, depth: f64, through: bool) -> CrossHole {
    CrossHole { origin: Point3::new(x, y, z), dir: Point3::new(dx, 0.0, 0.0), radius: 2.0, entry_d: 0.0, floor_d: -depth, through }
}

fn drill_params(cross: Vec<CrossHole>) -> OpParams {
    OpParams::Drill {
        break_through: None,
        break_through_from: Default::default(),
        clearance_z: Height::fixed(20.0),
        retract_z: None,
        top_z: Height::fixed(15.0),
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![],
        cross,
    }
}

fn dr0400() -> Tool {
    catalog().into_iter().find(|(n, _)| *n == "DR-0400").map(|(_, t)| t).expect("catalog DR-0400")
}

fn doc(part_coarse: Option<f64>, stock: StockShape, stock_loc: Location, tool: Tool) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, SetStock::new(&world, stock, stock_loc).expect("stock"));
    let mut m = Machine { model: "TestBench mill-turn".into(), kind: Some("mill-turn".into()), ..Default::default() };
    m.axes.push(AxisLimits { name: "B".into(), rotary: true, axis: Some([0.0, 1.0, 0.0]), travel: Some((-120.0, 120.0)), ..Default::default() });
    h.execute(&world, LoadMachine::new(&world, "bench.mch".into(), Some(m)).expect("machine"));
    h.execute(&world, AddTool::new(&world, "DR-0400", tool).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(radial_bar())).with_fidelity(part_coarse));
    (world, h)
}

fn bar_stock() -> (StockShape, Location) {
    (StockShape::Box { dx: 30.0, dy: 10.0, dz: 30.0 }, Location { pos: Point3::new(0.0, 0.0, -15.0), ..Location::default() })
}

#[test]
fn cross_drill_is_judged_while_a_tolerance_is_left_and_refused_by_name_past_it() {
    let (stock, loc) = bar_stock();
    let mut t = dr0400();
    t.flute_len = 50.0;
    let holes = vec![hole(15.0, 0.0, 10.0, -1.0, 10.0, false)];

    for coarse in [None, Some(TIGHT)] {
        let (world, mut h) = doc(coarse, stock.clone(), loc, t.clone());
        h.execute(&world, CreateOperation::new(&world, "CD", "DR-0400", SrcRef::Entity("bar".into()), drill_params(holes.clone())).expect("create"));
        let op = model::operation_of(&world, &model::find_by_name(&world, "CD").expect("op")).expect("op");
        assert_eq!(model_fidelity::op_problem(&world, "CD", &op), None, "coarse {coarse:?} is judged");
        h.execute(&world, GenerateOperation::new(&world, "CD").expect("generate"));
        assert_eq!(operation::status_of(&world, &model::find_by_name(&world, "CD").expect("op")), OpStatus::Ok);
        let gate = posting::verification_gate(&world);
        assert!(gate.is_empty(), "coarse {coarse:?} posts cleanly: {gate:?}");
    }

    let (world, mut h) = doc(Some(TOO_COARSE), stock, loc, t);
    h.execute(&world, CreateOperation::new(&world, "CD", "DR-0400", SrcRef::Entity("bar".into()), drill_params(holes)).expect("create"));
    let op = model::operation_of(&world, &model::find_by_name(&world, "CD").expect("op")).expect("op");
    let why = model_fidelity::op_problem(&world, "CD", &op).expect("refused past the bound");
    assert!(why.starts_with("CD: model 'bar' was imported too coarsely to verify"), "{why}");
    assert!(!posting::verification_gate(&world).is_empty(), "gate refuses past the bound");
}

#[test]
fn standoffs_and_detour_grow_by_the_excess_on_a_coarse_model() {
    let (stock, loc) = bar_stock();
    let mut t = dr0400();
    t.flute_len = 50.0;
    let holes = vec![hole(15.0, 0.0, 10.0, -1.0, 10.0, false), hole(-15.0, 0.0, -10.0, 1.0, 10.0, false)];
    let generate_at = |coarse: Option<f64>| {
        let (world, _h) = doc(coarse, stock.clone(), loc, t.clone());
        let op = Operation {
            tool: "DR-0400".into(),
            src: SrcRef::Entity("bar".into()),
            params: drill_params(holes.clone()),
            stamp: None,
            setup: None,
            area: Default::default(),
            coolant: Default::default(),
            after: Default::default(),
            suppressed: false,
        };
        operation::generate(&world, &op, "").expect("generate").0
    };
    let (plain, tight, past) = (generate_at(None), generate_at(Some(TIGHT)), generate_at(Some(TOO_COARSE)));
    let max_r = |p: &camcraft_app::cl::Toolpath| p.moves.iter().filter(|m| m.kind == MoveKind::Rapid).map(|m| m.to.x.hypot(m.to.y)).fold(0.0_f64, f64::max);
    let max_z = |p: &camcraft_app::cl::Toolpath| p.moves.iter().filter(|m| m.kind == MoveKind::Rapid).map(|m| m.to.z).fold(f64::NEG_INFINITY, f64::max);
    assert!((max_r(&tight) - (max_r(&plain) + EXCESS)).abs() < 1e-6, "standoff grows by excess: {} vs {}", max_r(&plain), max_r(&tight));
    assert!((max_z(&tight) - (max_z(&plain) + EXCESS)).abs() < 1e-6, "detour Z grows by excess: {} vs {}", max_z(&plain), max_z(&tight));
    assert!((max_r(&past) - max_r(&plain)).abs() < 1e-9, "past the bound: generator reads untightened");
}

#[test]
fn cross_reach_and_raw_stock_mouth_grow_by_the_excess() {
    // Part-only stock: flute exactly 10.0 passes at full fidelity,
    // fails at TIGHT (10.007 > 10.0), passes with 10.02.
    let (stock, loc) = bar_stock();
    let holes = vec![hole(15.0, 0.0, 10.0, -1.0, 10.0, false)];
    let try_reach = |coarse: Option<f64>, flute: f64| {
        let mut t = dr0400();
        t.flute_len = flute;
        let (world, _h) = doc(coarse, stock.clone(), loc, t);
        let op = Operation { tool: "DR-0400".into(), src: SrcRef::Entity("bar".into()), params: drill_params(holes.clone()), stamp: None, setup: None, area: Default::default(), coolant: Default::default(), after: Default::default(), suppressed: false };
        operation::generate(&world, &op, "").map_err(|e| e.to_string())
    };
    assert!(try_reach(None, 10.0).is_ok());
    let why = try_reach(Some(TIGHT), 10.0).expect_err("coarse reach refused");
    assert!(why.contains("flute reaches"), "{why}");
    assert!(try_reach(Some(TIGHT), 10.02).is_ok());

    // Wider raw box stock (x in [-20, 20]): depth from raw face is 15.0.
    let try_raw = |coarse: Option<f64>, flute: f64| {
        let mut t = dr0400();
        t.flute_len = flute;
        let raw_loc = Location { pos: Point3::new(0.0, 0.0, -15.0), ..Location::default() };
        let (world, _h) = doc(coarse, StockShape::Box { dx: 40.0, dy: 40.0, dz: 30.0 }, raw_loc, t);
        let op = Operation { tool: "DR-0400".into(), src: SrcRef::Entity("bar".into()), params: drill_params(holes.clone()), stamp: None, setup: None, area: Default::default(), coolant: Default::default(), after: Default::default(), suppressed: false };
        operation::generate(&world, &op, "").map(|(p, _)| p).map_err(|e| e.to_string())
    };
    let p_plain = try_raw(None, 15.0).expect("plain raw fits 15.0 flute");
    let why = try_raw(Some(TIGHT), 15.0).expect_err("coarse raw depth 15.007 exceeds 15.0");
    assert!(why.contains("flute reaches"), "{why}");
    let p_tight = try_raw(Some(TIGHT), 15.02).expect("longer flute fits");
    let plunge_start = |p: &camcraft_app::cl::Toolpath| p.moves.iter().find(|m| m.kind == MoveKind::Plunge).expect("plunge").from.x;
    assert!((plunge_start(&p_tight) - (plunge_start(&p_plain) + EXCESS)).abs() < 1e-6, "feed entry moves out by excess");
}

#[test]
fn cross_holder_between_the_plain_and_tightened_clearance_is_refused_only_on_a_coarse_model() {
    let holes4 = vec![hole(15.0, 0.0, 10.0, -1.0, 4.0, false)];
    let (stock, loc) = bar_stock();
    let mut part_mesh = radial_bar();
    part_mesh.extend(cube(Point3::new(15.5, -5.0, 13.0), Point3::new(20.0, 5.0, 15.0)).facets.to_vec());
    let try_flange = |coarse: Option<f64>, shank: f64| {
        let mut t = dr0400();
        t.flute_len = 6.0;
        t.shank_len = shank;
        t.holder_d = 12.0;
        t.holder_len = 10.0;
        let world = new_world();
        let mut h = History::default();
        h.execute(&world, SetStock::new(&world, stock.clone(), loc).expect("stock"));
        let mut m = Machine { model: "TestBench mill-turn".into(), kind: Some("mill-turn".into()), ..Default::default() };
        m.axes.push(AxisLimits { name: "B".into(), rotary: true, axis: Some([0.0, 1.0, 0.0]), travel: Some((-120.0, 120.0)), ..Default::default() });
        h.execute(&world, LoadMachine::new(&world, "bench.mch".into(), Some(m)).expect("machine"));
        h.execute(&world, AddTool::new(&world, "DR-0400", t).expect("tool"));
        h.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(part_mesh.clone())).with_fidelity(coarse));
        let op = Operation { tool: "DR-0400".into(), src: SrcRef::Entity("bar".into()), params: drill_params(holes4.clone()), stamp: None, setup: None, area: Default::default(), coolant: Default::default(), after: Default::default(), suppressed: false };
        operation::generate(&world, &op, "").map_err(|e| e.to_string())
    };
    let plain = try_flange(None, 3.004);
    assert!(plain.is_ok(), "0.004 mm clear passes at full fidelity: {:?}", plain.err());
    let why = try_flange(Some(TIGHT), 3.004).expect_err("refused on coarse model");
    assert!(why.contains("the holder"), "{why}");
    assert!(try_flange(Some(TIGHT), 3.012).is_ok(), "0.012 mm clear passes on coarse model");
}

#[test]
fn cross_part_gouge_between_the_tolerances_is_refused_only_on_a_coarse_model() {
    let (stock, loc) = bar_stock();
    let try_depth = |coarse: Option<f64>, depth: f64| {
        let mut t = dr0400();
        t.flute_len = 50.0;
        let (world, _h) = doc(coarse, stock.clone(), loc, t);
        let op = Operation { tool: "DR-0400".into(), src: SrcRef::Entity("bar".into()), params: drill_params(vec![hole(15.0, 0.0, 10.0, -1.0, depth, false)]), stamp: None, setup: None, area: Default::default(), coolant: Default::default(), after: Default::default(), suppressed: false };
        operation::generate(&world, &op, "").map_err(|e| e.to_string())
    };
    // 0.006 mm past the modelled floor (x = 5): inside GOUGE_TOL (0.01),
    // past the tightened 0.003 on TIGHT.
    assert!(try_depth(None, 10.006).is_ok(), "silent at full fidelity");
    let why = try_depth(Some(TIGHT), 10.006).expect_err("refused on coarse model");
    assert!(why.contains("past the modelled hole's floor"), "{why}");
    assert!(try_depth(None, 10.05).is_err() && try_depth(Some(TIGHT), 10.05).is_err(), "real gouge refused both ways");
}

#[test]
fn a_coarse_stock_copied_from_a_model_tightens_cross_drill_too() {
    let mut t = dr0400();
    t.flute_len = 50.0;
    let holes = vec![hole(15.0, 0.0, 10.0, -1.0, 10.0, false)];
    let setup = |stock_coarse: Option<f64>| {
        let world = new_world();
        let mut h = History::default();
        let m = Machine { model: "TestBench mill-turn".into(), kind: Some("mill-turn".into()), ..Default::default() };
        h.execute(&world, LoadMachine::new(&world, "bench.mch".into(), Some(m)).expect("machine"));
        h.execute(&world, AddTool::new(&world, "DR-0400", t.clone()).expect("tool"));
        h.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(radial_bar())));
        let blank = cube(Point3::new(-15.0, -5.0, -15.0), Point3::new(15.0, 5.0, 15.0));
        h.execute(&world, ImportModel::from_mesh("blank", blank).with_fidelity(stock_coarse));
        h.execute(&world, SetStock::new(&world, StockShape::FromModel { source: "blank".into() }, Location::default()).expect("stock"));
        h.execute(&world, CreateOperation::new(&world, "CD", "DR-0400", SrcRef::Entity("bar".into()), drill_params(holes.clone())).expect("create"));
        world
    };
    let w_tight = setup(Some(TIGHT));
    let op = model::operation_of(&w_tight, &model::find_by_name(&w_tight, "CD").expect("op")).expect("op");
    assert_eq!(model_fidelity::op_problem(&w_tight, "CD", &op), None);

    let w_bad = setup(Some(TOO_COARSE));
    let op_bad = model::operation_of(&w_bad, &model::find_by_name(&w_bad, "CD").expect("op")).expect("op");
    let why = model_fidelity::op_problem(&w_bad, "CD", &op_bad).expect("refused past the bound");
    assert!(why.starts_with("CD: model 'blank' was imported too coarsely to verify"), "{why}");
}
