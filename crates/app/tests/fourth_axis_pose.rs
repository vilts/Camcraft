//! #797: the machine check reads a 4th-axis table's pose the way the
//! PathPilot post does - through the tool axis. On a machine whose only
//! rotary turns about X (the 770MX + microARC 4), the face underneath,
//! which the pose law writes as A0 B180, is the A180 that table reaches:
//! accepted and judged as A180 against the travel. A pose leaning along X
//! (any B a turn about X cannot make) is still refused by name, a machine
//! with a second rotary keeps the letter law, and the letter-writing
//! Fanuc dialect refuses a B on that table by itself.
//! Provenance: our own tests over our own commands, machine file and posts;
//! RS-274's A as a rotation about X, and our `cl::pose` law.
//!
//! Gate map (M4.4): covers app::machine, post::machine (judge: the pose gate),
//! app::posting (post_setup), post::post (tormach.cps, fanuc.cps).

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SelectPost,
    SetStock,
};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{AxisLimits, Machine, check_document, parse_mch};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef, Tilt};
use camcraft_app::post::{cnc, engine, machine as mirror};
use camcraft_app::posting::MACHINES;
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use flecs_ecs::prelude::World;
use std::path::Path;

const IN: f64 = 25.4;
const R: f64 = 0.375 * IN;
const STICK: f64 = 1.5 * IN;
const ARC4: &str = "Tormach 770MX + microARC 4";
const UNDER: Tilt = Tilt { a_deg: 0.0, b_deg: 180.0, origin: [0.0; 3] };

fn ok<T, E: std::fmt::Debug>(r: Result<T, E>, what: &str) -> T {
    r.unwrap_or_else(|e| panic!("{what}: {e:?}"))
}

fn shipped() -> Machine {
    let text = MACHINES.iter().find(|(n, _)| *n == ARC4).map(|(_, t)| *t).expect("shipped");
    ok(parse_mch(text), "parses")
}

/// The test part's bar core: a 48-gon prism along X, -STICK..0.
fn prism(r: f64) -> Mesh {
    let n = 48;
    let pt = |x: f64, j: usize| {
        let t = std::f64::consts::TAU * (j % n) as f64 / n as f64;
        Point3::new(x, r * t.cos(), r * t.sin())
    };
    let mut f = Vec::new();
    for j in 0..n {
        let (a0, a1, b0, b1) = (pt(-STICK, j), pt(-STICK, j + 1), pt(0.0, j), pt(0.0, j + 1));
        f.push(Triangle::new(a0, a1, b1));
        f.push(Triangle::new(a0, b1, b0));
        f.push(Triangle::new(Point3::new(-STICK, 0.0, 0.0), a1, a0));
        f.push(Triangle::new(Point3::new(0.0, 0.0, 0.0), b0, b1));
    }
    Mesh::from_facets(f)
}

/// A 0.015" flat on the face UNDER the bar, 1/2" along it: the slot's
/// ends are stated in the world and read into the tilt's own frame, so
/// the test never leans on the frame's in-plane basis choice.
fn under_flat() -> OpParams {
    let frame = UNDER.frame().expect("the B180 frame");
    let local = |x: f64| {
        let p = frame.to_local(Point3::new(x, 0.0, -R));
        Point2::new(p.x, p.y)
    };
    OpParams::Slot {
        a: local(-0.75 * IN),
        b: local(-0.25 * IN),
        width: 0.25 * IN,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(R + 0.5 * IN),
        retract_z: Some(Height::fixed(R + 0.1 * IN)),
        top_z: Height::fixed(R),
        bottom_z: Height::fixed(R - 0.015 * IN),
        stepdown: 0.015 * IN,
        stepover: 0.1 * IN,
        feed: 15.0 * IN,
        plunge_feed: 4.0 * IN,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        links: camcraft_app::links::Links::PLANAR,
        tilt: Some(UNDER),
    }
}

/// The microARC 4 bar job with one op: the A0 B180 flat underneath.
fn build() -> World {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, ok(LoadMachine::builtin(&world, ARC4), "machine"));
    h.execute(&world, ok(SelectPost::new(&world, PostRef::Named("Tormach PathPilot".into())), "post"));
    let stock = SetStock::new(
        &world,
        StockShape::Box { dx: STICK, dy: 2.0 * R, dz: 2.0 * R },
        Location { pos: Point3::new(-STICK / 2.0, 0.0, -R), ..Location::default() },
    );
    h.execute(&world, ok(stock, "stock"));
    h.execute(&world, ImportModel::from_mesh("bar core", prism(0.340 * IN)));
    let mut t = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog");
    t.diameter = 0.25 * IN;
    t.shank_d = t.diameter;
    t.rpm = 6000.0;
    t.feed = 15.0 * IN;
    h.execute(&world, ok(AddTool::new(&world, "EM 1/4", t), "tool"));
    h.execute(&world, ok(CreateOperation::new(&world, "FLAT UNDER", "EM 1/4", SrcRef::Stock, under_flat()), "op"));
    h.execute(&world, ok(GenerateOperation::new(&world, "FLAT UNDER"), "generate"));
    world
}

/// Lean every tilted move of the op's cache to `axis` (a fixture poke on
/// the cache, not a UI state change: the gate must judge a pose that no
/// generator here would make).
fn lean_all(world: &World, axis: [f64; 3]) {
    let e = model::find_by_name(world, "FLAT UNDER").expect("the op");
    let mut t = model::toolpath_of(world, &e).expect("a cache");
    for m in t.path.moves.iter_mut().filter(|m| m.axis.is_some()) {
        m.axis = Some(axis);
    }
    e.entity_view(world).set(t);
}

#[test]
fn an_a0_b180_slot_posts_as_a180_on_the_microarc4() {
    let world = build();
    let c = ok(check_document(&world, &shipped()), "checks");
    assert!(c.is_ok(), "the face underneath is A180 on an A about X: {:?}", c.problems);
    let out = std::env::temp_dir().join(format!("camcraft_797_{}.nc", std::process::id()));
    let posted = camcraft_app::posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    let nc = posted.unwrap_or_else(|e| panic!("the A0 B180 flat must post: {e}")).program;
    let idx: Vec<&str> = nc.lines().filter(|l| l.starts_with("G0 A")).collect();
    assert_eq!(idx, ["G0 A180"], "{nc}");
    assert!(
        !nc.lines().filter(|l| !l.starts_with('(')).any(|l| l.split(' ').any(|w| w.starts_with('B'))),
        "no B word on an A-only table: {nc}"
    );
    // After the index the cut sits on the skin, Z up the tool axis, and
    // inside the stick-out.
    for l in nc.lines().filter(|l| l.starts_with("G1 ")) {
        for w in l.split(' ') {
            let v: Option<f64> = w.get(1..).and_then(|s| s.parse().ok());
            match (w.chars().next(), v) {
                (Some('Z'), Some(z)) => assert!((0.375 - 0.0151..=0.375 + 0.2).contains(&z), "{l}"),
                (Some('X'), Some(x)) => assert!((-1.5..=0.0).contains(&x), "{l}"),
                _ => {}
            }
        }
    }
}

#[test]
fn the_face_underneath_is_judged_as_a180_against_the_travel() {
    let world = build();
    let mut m = shipped();
    let a = m.axes.iter_mut().find(|x| x.rotary).expect("the A");
    a.travel = Some((-90.0, 90.0));
    let c = ok(check_document(&world, &m), "checks");
    assert!(!c.is_ok(), "A180 is past -90..90");
    assert!(
        c.problems.iter().all(|p| p.starts_with("FLAT UNDER: move ")
            && p.contains("commands A 180.000 deg (the pose A 0.000 B 180.000), the travel is -90.000..90.000")),
        "{:?}",
        c.problems
    );
    // A table stated the other way round ([-1,0,0]) commands -180.
    let a = m.axes.iter_mut().find(|x| x.rotary).expect("the A");
    a.axis = Some([-1.0, 0.0, 0.0]);
    a.travel = Some((-180.0, 0.0));
    assert!(ok(check_document(&world, &m), "checks").is_ok());
    let a = m.axes.iter_mut().find(|x| x.rotary).expect("the A");
    a.travel = Some((0.0, 180.0));
    let c = ok(check_document(&world, &m), "checks");
    assert!(c.problems.iter().any(|p| p.contains("commands A -180.000 deg")), "{:?}", c.problems);
}

#[test]
fn a_pose_the_a_axis_cannot_reach_is_still_refused_by_name() {
    let world = build();
    // 30 degrees toward +X: a turn about X never leans the tool along X.
    let (s, c) = 30f64.to_radians().sin_cos();
    lean_all(&world, [s, 0.0, c]);
    let check = ok(check_document(&world, &shipped()), "checks");
    assert!(!check.is_ok(), "a B30 lean is out of an A table's reach");
    assert!(
        check.problems.iter().any(|p| p.starts_with("FLAT UNDER: move ")
            && p.contains("commands A 0.000 B 30.000 C 0.000 deg")
            && p.contains("Tormach 770MX + microARC 4's only rotary, A about X, cannot reach that pose")),
        "{:?}",
        check.problems
    );
    let out = std::env::temp_dir().join(format!("camcraft_797_lean_{}.nc", std::process::id()));
    let err = camcraft_app::posting::post_setup(&world, &out).expect_err("an unreachable pose must not post");
    assert!(!out.exists(), "a refused post writes nothing");
    assert!(err.contains("cannot reach that pose"), "{err}");
}

#[test]
fn a_second_rotary_keeps_the_letter_law() {
    // An A about X beside a C about Z is no 4th-axis table: the check
    // reads its letters, and B180 has no B to hold it.
    let world = build();
    let mut m = shipped();
    m.axes.push(AxisLimits { name: "C".into(), rotary: true, axis: Some([0.0, 0.0, 1.0]), ..Default::default() });
    let c = ok(check_document(&world, &m), "checks");
    assert!(
        c.problems.iter().any(|p| p.contains("commands B 180.000 deg") && p.contains("states no B axis")),
        "{:?}",
        c.problems
    );
}

#[test]
fn the_fanuc_dialect_refuses_a_b_on_an_a_only_table() {
    // fanuc.cps writes the stream's letters as they come (G43.4), so the
    // A0 B180 the check now passes on this table would reach the control
    // as a B word: the post refuses the letter by name instead.
    let mch = mirror::mirror(&shipped());
    let cps = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/posts/fanuc.cps"))
        .expect("the post");
    let stream = |abc: [f64; 3]| {
        let mv = |z: f64| {
            cnc::Record::Move(cnc::MoveRec {
                kind: camcraft_app::cl::MoveKind::Feed,
                x: 0.0,
                y: 0.0,
                z,
                feed: Some(300.0),
                center: None,
                ccw: false,
                plane: Default::default(),
                abc: Some(abc),
            })
        };
        cnc::serialize(&[
            cnc::Record::Tool { number: 1, name: "EM".into() },
            cnc::Record::Section { begin: true },
            mv(-5.0),
            mv(-6.0),
            cnc::Record::Section { begin: false },
        ])
    };
    let under = ok(engine::run(&cps, &mch, &stream([0.0, 180.0, 0.0])), "runs");
    assert!(
        under.warnings.iter().any(|w| w.contains("tilted move commands B - the machine states no B rotary")),
        "{:?}",
        under.warnings
    );
    assert!(!under.program.lines().any(|l| l.contains("B180")), "{}", under.program);
    let side = ok(engine::run(&cps, &mch, &stream([90.0, 0.0, 0.0])), "runs");
    assert!(!side.warnings.iter().any(|w| w.contains("states no")), "{:?}", side.warnings);
    assert!(side.program.contains("A90"), "{}", side.program);
}

#[test]
fn the_fanuc_dialect_refuses_a_lone_a_stated_along_minus_x() {
    // The check judges a [-1,0,0] table by the negated angle (A -45
    // against 0..90 passes as +45); fanuc.cps writes the letter unnegated,
    // so A-45 would post outside the judged travel. It refuses by name.
    let mut m = shipped();
    let a = m.axes.iter_mut().find(|x| x.rotary).expect("the A");
    a.axis = Some([-1.0, 0.0, 0.0]);
    a.travel = Some((0.0, 90.0));
    let mch = mirror::mirror(&m);
    let cps = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/posts/fanuc.cps"))
        .expect("the post");
    let mv = |z: f64| {
        cnc::Record::Move(cnc::MoveRec {
            kind: camcraft_app::cl::MoveKind::Feed,
            x: 0.0,
            y: 0.0,
            z,
            feed: Some(300.0),
            center: None,
            ccw: false,
            plane: Default::default(),
            abc: Some([-45.0, 0.0, 0.0]),
        })
    };
    let stream = cnc::serialize(&[
        cnc::Record::Tool { number: 1, name: "EM".into() },
        cnc::Record::Section { begin: true },
        mv(-5.0),
        mv(-6.0),
        cnc::Record::Section { begin: false },
    ]);
    let out = ok(engine::run(&cps, &mch, &stream), "runs");
    assert!(
        out.warnings.iter().any(|w| w.contains("tilted move on rotary A, stated along -X")),
        "{:?}",
        out.warnings
    );
    assert!(!out.program.lines().any(|l| l.contains("A-45")), "{}", out.program);
    // The same table stated along +X posts the letter as before.
    let a = m.axes.iter_mut().find(|x| x.rotary).expect("the A");
    a.axis = Some([1.0, 0.0, 0.0]);
    a.travel = Some((-90.0, 90.0));
    let plus = ok(engine::run(&cps, &mirror::mirror(&m), &stream), "runs");
    assert!(!plus.warnings.iter().any(|w| w.contains("along -X")), "{:?}", plus.warnings);
    assert!(plus.program.contains("A-45"), "{}", plus.program);
}

/// Two tilted feeds at `abc`, for the letter-writing dialects.
fn tilted(abc: [f64; 3]) -> String {
    let mv = |z: f64| {
        cnc::Record::Move(cnc::MoveRec {
            kind: camcraft_app::cl::MoveKind::Feed,
            x: 0.0,
            y: 0.0,
            z,
            feed: Some(300.0),
            center: None,
            ccw: false,
            plane: Default::default(),
            abc: Some(abc),
        })
    };
    cnc::serialize(&[
        cnc::Record::Tool { number: 1, name: "EM".into() },
        cnc::Record::Section { begin: true },
        mv(-5.0),
        mv(-6.0),
        cnc::Record::Section { begin: false },
    ])
}

fn post_file(name: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/posts").join(name))
        .expect("the post")
}

/// The Doosan post with its A brake declared (M10/M11), as an operator sets it.
fn run_dnm(m: &Machine, abc: [f64; 3]) -> engine::PostOutput {
    let opts = serde_json::json!({ "rotaryBrake": true });
    ok(engine::run_with(&post_file("doosan-dnm.cps"), &mirror::mirror(m), &tilted(abc), &opts), "runs")
}

fn dnm() -> Machine {
    let text = MACHINES.iter().find(|(n, _)| *n == "Doosan DNM 5700").map(|(_, t)| *t).expect("shipped");
    ok(parse_mch(text), "parses")
}

#[test]
fn an_upright_a0_still_posts_on_a_minus_x_fanuc_table() {
    // A0 is the same word on either sign: only a turning A is refused.
    let mut m = shipped();
    let a = m.axes.iter_mut().find(|x| x.rotary).expect("the A");
    a.axis = Some([-1.0, 0.0, 0.0]);
    let out = ok(engine::run(&post_file("fanuc.cps"), &mirror::mirror(&m), &tilted([0.0, 0.0, 0.0])), "runs");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.contains("A0"), "{}", out.program);
}

#[test]
fn the_doosan_dialect_refuses_a_b_on_its_a_only_table() {
    // The shipped DNM's A about X leans off Z, so it takes the RTCP path,
    // which writes letters: the A0 B180 the check passes is refused by
    // name, never posted as a B word to a machine with no B.
    assert!(ok(check_document(&build(), &dnm()), "checks").is_ok(), "the check passes A0 B180 as A180");
    let under = run_dnm(&dnm(), [0.0, 180.0, 0.0]);
    assert!(
        under.warnings.iter().any(|w| w.contains("tilted move commands B - the machine states no B rotary")),
        "{:?}",
        under.warnings
    );
    assert!(!under.program.lines().any(|l| l.contains("B180")), "{}", under.program);
    let side = run_dnm(&dnm(), [90.0, 0.0, 0.0]);
    assert!(side.warnings.is_empty(), "{:?}", side.warnings);
    assert!(side.program.contains("A90"), "{}", side.program);
}

#[test]
fn the_doosan_dialect_refuses_a_turning_a_stated_along_minus_x() {
    let mut m = dnm();
    let a = m.axes.iter_mut().find(|x| x.rotary).expect("the A");
    a.axis = Some([-1.0, 0.0, 0.0]);
    a.travel = Some((0.0, 90.0));
    let out = run_dnm(&m, [-45.0, 0.0, 0.0]);
    assert!(
        out.warnings.iter().any(|w| w.contains("tilted move on rotary A, stated along -X")),
        "{:?}",
        out.warnings
    );
    assert!(!out.program.lines().any(|l| l.contains("A-45")), "{}", out.program);
    let upright = run_dnm(&m, [0.0, 0.0, 0.0]);
    assert!(upright.warnings.is_empty(), "{:?}", upright.warnings);
}
