//! #904: turning, the rotary kinds and the multi-axis ball kinds on a
//! model imported coarser than the importer promises. #864 refused them
//! outright; their laws now give the model's excess back out of their
//! tolerances while a positive tolerance is left, so they are judged -
//! and past that they are refused by name, as the upright mill kinds
//! are. Kinds whose verdict is still a generator's own proof (swarf, the
//! cone and engrave rides, the cross drill) stay refused; probe and
//! plasma joined the judged kinds in #1053 (`coarse_probe_plasma`).
//!
//! What "right" means here: on a model flagged 0.012 mm (0.007 mm past
//! the promise) a turning, rotary or 5-axis operation generates,
//! verifies and passes the fidelity clause; a cut between the tightened
//! and the plain tolerance is convicted there and only there; a real
//! gouge is convicted either way; at 0.030 mm the program gate names the
//! operation and the model. The laws' own numbers are pinned one crate
//! down (camcraft-motion `turning_coarse`, camcraft-sim
//! `coarse_collision` and `model_excess`).
//!
//! Gate map (#904): covers app::operation (fidelity_law, generate's
//! turning and rotary verdict wiring, probe, material::judged_material,
//! rotary_law), app::model_fidelity (op_problem, gate_problems),
//! app::posting (verification_gate).
//! Lane shape (M4.4): all tests here run in seconds.
//!
//! Provenance: our own fixtures (the blade, thread and rotary suites'
//! bars) over our own laws; no vendor material.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geometry::{cube, Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::operation::{retype_params, Height, HeightReference, KindInfo, OpParams, OpType, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;
use camcraft_app::{model, model_fidelity, posting};

/// A chord error that leaves a positive tolerance (excess 0.007), one
/// that leaves none of the 0.01 mm laws, and a metre-unit STEP file's.
const TIGHT: f64 = 0.012;
const TOO_COARSE: f64 = 0.03;
const METRE: f64 = 0.01;

fn revolve(meridian: &[(f64, f64)]) -> Vec<Triangle> {
    let v = |j: usize, r: f64, z: f64| {
        let th = std::f64::consts::TAU * j as f64 / 16.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..16 {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                facets.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                facets.push(Triangle::new(a1, a2, b2));
            } else {
                facets.push(Triangle::new(a1, a2, b2));
                facets.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    facets
}

/// The blade suite's grooved bar: wall r 6, a groove 3.2 wide to r 4.
fn grooved() -> Mesh {
    Mesh::from_facets(revolve(&[(0.0, -16.0), (6.0, -16.0), (6.0, -6.8), (4.0, -6.8), (4.0, -3.6), (6.0, -3.6), (6.0, 28.0), (0.0, 28.0)]))
}

/// The thread suite's bar: a 60-degree V 1.2 deep every 2.0, z 8 to 14.
fn threaded() -> Mesh {
    let half = 1.2 * 30.0_f64.to_radians().tan();
    let mut m = vec![(0.0, -16.0), (6.0, -16.0), (6.0, 8.0 - half)];
    for c in [8.0, 10.0, 12.0, 14.0] {
        m.push((4.8, c));
        m.push((6.0, c + half));
        if c < 14.0 {
            m.push((6.0, c + 2.0 - half));
        }
    }
    m.extend([(6.0, 40.0), (0.0, 40.0)]);
    Mesh::from_facets(revolve(&m))
}

/// A lathe document: `bar` imported at `coarse_mm` in bar-sized stock.
fn lathe(bar: Mesh, height: f64, coarse_mm: Option<f64>) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = StockShape::Cylinder { diameter: 12.0, height };
    h.execute(&world, SetStock::new(&world, stock, Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() }).expect("bar stock"));
    let lathe = Machine { model: "TestBench lathe".into(), kind: Some("lathe".into()), ..Default::default() };
    h.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe)).expect("lathe"));
    for name in ["GT-0200", "GT-0300", "TD-1732-60"] {
        let tool = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry");
        h.execute(&world, AddTool::new(&world, name, tool).expect("tool"));
    }
    h.execute(&world, ImportModel::from_mesh("bar", bar).with_fidelity(coarse_mm));
    (world, h)
}

fn create(world: &World, h: &mut History, name: &str, tool: &str, params: OpParams) {
    h.execute(world, CreateOperation::new(world, name, tool, SrcRef::Entity("bar".into()), params).expect("create"));
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

fn cache(world: &World, name: &str) -> camcraft_app::schema::Toolpath {
    model::find_by_name(world, name).and_then(|e| model::toolpath_of(world, &e)).expect("cached")
}

fn blocking(world: &World, name: &str) -> Vec<Warning> {
    cache(world, name).warnings.into_iter().filter(Warning::blocks_post).collect()
}

/// The program gate's fidelity lines: the refusals this issue is about.
fn fidelity_lines(world: &World) -> Vec<String> {
    posting::verification_gate(world).into_iter().filter(|l| l.contains("was imported")).collect()
}

fn refusal(op: &str, mm: &str) -> String {
    format!("{op}: model 'bar' was imported too coarsely to verify; re-import with a larger STEP facet budget (surfaces are approximated to within {mm} mm)")
}

fn groove_finish(tolerance: f64) -> OpParams {
    OpParams::GrooveFinish {
        tolerance,
        clearance_z: Height::fixed(20.0),
        top_z: Height::fixed(-3.6),
        bottom_z: Height::fixed(-6.8),
        feed: 150.0,
        plunge_feed: 60.0,
    }
}

fn threading(depth: f64) -> OpParams {
    OpParams::Threading {
        tolerance: 0.05,
        clearance_z: Height::fixed(24.0),
        top_z: Height::fixed(14.3),
        bottom_z: Height::fixed(7.7),
        pitch: 2.0,
        depth,
        infeed: 0.4,
        feed: 120.0,
        plunge_feed: 40.0,
        infeed_plan: Default::default(),
        hand: camcraft_app::cl::ThreadHand::Right,
    }
}

#[test]
fn a_groove_on_a_reduced_model_is_judged_and_past_the_bound_refused_by_name() {
    // Judged: generated, verified silent, and no fidelity refusal.
    let (world, mut h) = lathe(grooved(), 44.0, Some(TIGHT));
    create(&world, &mut h, "G1", "GT-0200", groove_finish(0.05));
    assert!(blocking(&world, "G1").is_empty(), "a flush groove is no gouge on a coarse model: {:?}", blocking(&world, "G1"));
    assert_eq!(fidelity_lines(&world), Vec::<String>::new());
    // Too coarse for the 0.01 mm laws: refused by name.
    let (world, mut h) = lathe(grooved(), 44.0, Some(TOO_COARSE));
    create(&world, &mut h, "G1", "GT-0200", groove_finish(0.05));
    assert_eq!(fidelity_lines(&world), vec![refusal("G1", "0.030")]);
    // The floor is the kind's own: a groove held to 0.004 mm cannot be
    // judged on a metre-unit file (0.005 mm past the promise), though
    // the same file leaves the 0.01 mm laws half their tolerance.
    let (world, mut h) = lathe(grooved(), 44.0, Some(METRE));
    create(&world, &mut h, "LOOSE", "GT-0200", groove_finish(0.05));
    create(&world, &mut h, "FINE", "GT-0200", groove_finish(0.004));
    assert_eq!(fidelity_lines(&world), vec![refusal("FINE", "0.010")]);
}

#[test]
fn a_thread_cut_between_the_tolerances_is_convicted_only_on_the_reduced_model() {
    // The model's V is 1.2 deep and the numbers ask for 0.046 more: the
    // kernel cuts what it is told and the wave verdict measures it.
    // Inside the 0.05 mm tolerance at full fidelity; past the 0.043 mm
    // a 0.012 mm mesh leaves.
    let verdict = |coarse: Option<f64>, depth: f64| {
        let (world, mut h) = lathe(threaded(), 56.0, coarse);
        create(&world, &mut h, "TH", "TD-1732-60", threading(depth));
        blocking(&world, "TH")
    };
    assert!(verdict(None, 1.246).is_empty(), "full fidelity keeps today's verdict: {:?}", verdict(None, 1.246));
    match verdict(Some(TIGHT), 1.246).as_slice() {
        [Warning::Envelope { gouge_mm, .. }] => assert!(*gouge_mm > 0.043 && *gouge_mm <= 0.05, "{gouge_mm}"),
        other => panic!("the tightened wave verdict convicts it: {other:?}"),
    }
    // The matched thread is silent on the reduced model.
    assert!(verdict(Some(TIGHT), 1.2).is_empty(), "{:?}", verdict(Some(TIGHT), 1.2));
    // Too coarse for the 0.01 mm laws: refused by name. The wave law's
    // own 0.05 mm still has 0.025 left there, so it keeps convicting -
    // tighter, never looser.
    let (world, mut h) = lathe(threaded(), 56.0, Some(TOO_COARSE));
    create(&world, &mut h, "TH", "TD-1732-60", threading(1.246));
    assert_eq!(fidelity_lines(&world), vec![refusal("TH", "0.030")]);
    assert!(!blocking(&world, "TH").is_empty());
    // Past its own tolerance the wave law reads untightened: no gouge
    // verdict is read off a mesh that cannot carry one.
    assert!(verdict(Some(0.06), 1.246).is_empty(), "{:?}", verdict(Some(0.06), 1.246));
}

/// A grooved bar finished (G2) and parted (P1) with a probe ask of
/// `ask` mm on the parting; G2's cached floor is then sunk `sink` mm.
fn probed(coarse: Option<f64>, ask: f64, sink: f64) -> World {
    let (world, mut h) = lathe(grooved(), 44.0, coarse);
    create(&world, &mut h, "G2", "GT-0200", groove_finish(0.05));
    let part = OpParams::Part {
        tolerance: 0.05,
        clearance_z: Height::fixed(20.0),
        top_z: Height::fixed(28.0),
        bottom_z: Height::fixed(-10.0),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 60.0,
        bar_load: true,
        bar_advance: 0.0,
        bar_unload: true,
        bar_transfer: true,
        inspect_tol: ask,
    };
    create(&world, &mut h, "P1", "GT-0300", part);
    let e = model::find_by_name(&world, "G2").expect("op");
    let mut t = model::toolpath_of(&world, &e).expect("cached");
    for m in t.path.moves.iter_mut().filter(|m| m.kind == camcraft_app::cl::MoveKind::Feed) {
        let rho = (m.to.x * m.to.x + m.to.y * m.to.y).sqrt();
        let k = (rho - sink) / rho;
        m.to = Point3::new(m.to.x * k, m.to.y * k, m.to.z);
    }
    e.entity_view(&world).set(camcraft_app::schema::Toolpath::new(t.path, t.warnings));
    // The parting's stamp reads the program ahead of it: generate it
    // again over the sunk groove, so the program is one the gate judges.
    h.execute(&world, GenerateOperation::new(&world, "P1").expect("regenerate"));
    world
}

fn ask_lines(world: &World) -> Vec<String> {
    posting::verification_gate(world).into_iter().filter(|l| l.starts_with("probe ask")).collect()
}

#[test]
fn a_probe_ask_is_judged_at_the_tightened_budget_on_a_reduced_model() {
    // The composed scan at the post: a floor 0.046 mm under the model
    // meets a 0.05 mm ask at full fidelity, and not on the coarse model.
    assert_eq!(ask_lines(&probed(None, 0.05, 0.046)), Vec::<String>::new());
    let lines = ask_lines(&probed(Some(TIGHT), 0.05, 0.046));
    assert!(matches!(lines.as_slice(), [l] if l.contains("cut below the model by G2")), "{lines:?}");
    // The flush program meets the ask on the coarse model too.
    assert_eq!(ask_lines(&probed(Some(TIGHT), 0.05, 0.0)), Vec::<String>::new());
    // An ask the excess uses up is the Part's own floor: refused by name
    // (0.006 mm asked, 0.007 mm of excess), though every 0.01 mm law
    // still has a tolerance left.
    let world = probed(Some(TIGHT), 0.006, 0.0);
    assert_eq!(fidelity_lines(&world), vec![refusal("P1", "0.012")]);
}

// ---- rotary ---------------------------------------------------------------

/// The rotary suite's turned bar (wall r 6, shoulder, neck r 5) standing
/// in a coaxial round bar, a Rotary Contour on a 6 mm ball.
fn rotary(coarse: Option<f64>, clearance: f64) -> World {
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == "BN-0600").map(|(_, t)| t).expect("catalog ball");
    h.execute(&world, AddTool::new(&world, "BN-0600", tool).expect("tool"));
    let loc = Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Cylinder { diameter: 12.4, height: 32.0 }, loc).expect("bar stock"));
    let bar = revolve(&[(0.0, -16.0), (6.0, -16.0), (6.0, -2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)]);
    h.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(bar)).with_fidelity(coarse));
    let params = OpParams::RotaryContour {
        tolerance: 0.1,
        clearance_z: Height::fixed(clearance),
        retract_z: None,
        top_z: Height::fixed(14.0),
        bottom_z: Height::fixed(-14.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
    };
    create(&world, &mut h, "OP", "BN-0600", params);
    world
}

#[test]
fn a_rotary_contour_on_a_reduced_model_is_judged_and_past_the_bound_refused_by_name() {
    let world = rotary(Some(TIGHT), 25.0);
    assert!(blocking(&world, "OP").is_empty(), "the tightened laws pass an honest program: {:?}", blocking(&world, "OP"));
    assert_eq!(fidelity_lines(&world), Vec::<String>::new());
    // The radial field law still convicts what is really wrong: a
    // clearance plane inside the bar drives the hops through stock.
    let low = rotary(Some(TIGHT), 10.0);
    assert!(blocking(&low, "OP").iter().any(|w| matches!(w, Warning::RapidInStock { .. })), "{:?}", blocking(&low, "OP"));
    assert_eq!(fidelity_lines(&rotary(Some(TOO_COARSE), 25.0)), vec![refusal("OP", "0.030")]);
}

// ---- the registry ----------------------------------------------------------

fn face() -> OpParams {
    OpParams::Face {
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
        blend_turns: false,
    }
}

/// A brick imported at `coarse_mm`, optionally in a stock copied from a
/// second model, and the fidelity clause's word on an op of `kind`.
fn problem(kind: OpType, coarse_mm: Option<f64>, stock_of: Option<(&str, Option<f64>)>) -> Option<String> {
    let world = new_world();
    let mut h = History::default();
    let brick = || cube(Point3::new(-20.0, -10.0, 0.0), Point3::new(20.0, 10.0, 10.0));
    h.execute(&world, ImportModel::from_mesh("bar", brick()).with_fidelity(coarse_mm));
    if let Some((name, coarse)) = stock_of {
        h.execute(&world, ImportModel::from_mesh(name, brick()).with_fidelity(coarse));
        h.execute(&world, SetStock::new(&world, StockShape::FromModel { source: name.into() }, Default::default()).expect("stock"));
    }
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-1000-4FL").map(|(_, t)| t).expect("catalog mill");
    h.execute(&world, AddTool::new(&world, "EM-1000-4FL", tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "BASE", "EM-1000-4FL", SrcRef::Entity("bar".into()), face()).expect("create"));
    let base = model::operation_of(&world, &model::find_by_name(&world, "BASE").expect("op")).expect("an op to copy");
    let op = camcraft_app::schema::Operation { params: retype_params(&face(), kind.ordinal()), ..base };
    model_fidelity::op_problem(&world, "OP", &op)
}

#[test]
fn every_kind_is_either_judged_or_refused_for_a_stated_reason_on_a_reduced_model() {
    use OpType::*;
    // Still refused: the verdict is a generator's own proof (swarf), or
    // no tolerance holds the ride to the model at all (the cone and
    // engrave rides). Probe and plasma read the excess since #1053.
    // #532: the wire's kerf law does not read the excess yet either.
    // SinkerBurn (#541): the burn is verified against the model's floor and
    // walls, so a reduced model is refused until it reads the excess.
    // SinkerSideBurn (#545): its lateral move is judged against the same
    // walls - refused on a reduced model for the same reason.
    let refused = [Swarf, AdvancedSwarf, Trace, Engrave, Chamfer2D, Deburr, WireContour, SinkerBurn, SinkerSideBurn];
    for row in KindInfo::ALL {
        assert_eq!(problem(row.ty, None, None), None, "{:?}: full fidelity is untouched", row.ty);
        let said = problem(row.ty, Some(TIGHT), None);
        if refused.contains(&row.ty) {
            let why = said.unwrap_or_else(|| panic!("{:?} must stay refused", row.ty));
            assert!(why.contains("this kind of operation cannot be verified against it"), "{:?}: {why}", row.ty);
        } else {
            assert_eq!(said, None, "{:?} is judged while a tolerance is left", row.ty);
            let why = problem(row.ty, Some(TOO_COARSE), None).unwrap_or_else(|| panic!("{:?} must be refused past the bound", row.ty));
            assert!(why.starts_with("OP: model 'bar' was imported too coarsely to verify"), "{:?}: {why}", row.ty);
        }
    }
    // The families this issue took off the refused list.
    for kind in [Facing, ProfileRough, ProfileFinish, GrooveRough, GrooveFinish, Part, Threading, Chamfering, TaperTurning, MultiPassTurning, FormTurning, BoringTurning] {
        assert_eq!(problem(kind, Some(TIGHT), None), None, "turning: {kind:?}");
    }
    for kind in [RotaryContour, RotaryParallel, RotaryOffset, Contour5D, Parallel5D, Flow5D, MultiAxisContour, MultiAxisRough] {
        assert_eq!(problem(kind, Some(TIGHT), None), None, "multi-axis: {kind:?}");
    }
}

#[test]
fn a_stock_copied_from_a_coarser_model_is_what_the_operation_is_judged_against_too() {
    // The collision laws read clearance against the setup's stock: a
    // fine part in a stock copied from a coarse model is tightened by
    // that model's excess, and refused by its name once it is too coarse.
    assert_eq!(problem(OpType::Face, None, Some(("blank", None))), None);
    assert_eq!(problem(OpType::Face, None, Some(("blank", Some(TIGHT)))), None, "a tolerance is left");
    let why = problem(OpType::Face, None, Some(("blank", Some(TOO_COARSE)))).expect("refused");
    assert!(why.starts_with("OP: model 'blank' was imported too coarsely to verify"), "{why}");
    // The coarser of the two is the one named.
    let why = problem(OpType::Face, Some(TOO_COARSE), Some(("blank", Some(0.5)))).expect("refused");
    assert!(why.contains("model 'blank'") && why.contains("0.500 mm"), "{why}");
}
