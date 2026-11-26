//! #3414: on the tilt head the rotaries turn first, at a standstill on
//! the clearance plane, and the approach then descends under the pose
//! that is already held. A leaning 5-Axis Contour generates on
//! `assets/machines/tilthead.mch` and posts with `assets/posts/fanuc.cps`.
//! The opening rotary turn posts on G00 at the clearance plane, and the
//! G01 approach that follows carries no rotary words.
//! Provenance: analytic test geometry (spherical cap); RS-274 modal
//! rotary words; our own design notes.
//! Gate map: covers app::posting (generate + post seam), the standstill
//! swing of motion multiaxis stamp and curve, post::dump,
//! assets/posts/fanuc.cps.

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel, LoadMachine,
    SetStock, SetWcsOrigin,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle, IDENTITY_Q};
use camcraft_app::operation::{self, Height, HeightReference, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::{model, posting, world::new_world};
use flecs_ecs::prelude::World;
use std::path::PathBuf;

const FANUC: &str = include_str!("../../../assets/posts/fanuc.cps");
const BALL: &str = "BN-0600";

fn ok<T, E: std::fmt::Debug>(r: Result<T, E>, what: &str) -> T {
    r.unwrap_or_else(|e| panic!("{what}: {e:?}"))
}

/// A spherical cap, radius 12, open to 60 degrees, top at Z 3.
fn cap_facets() -> Vec<Triangle> {
    const TAU: f64 = std::f64::consts::TAU;
    let (rad, max, nl, nc) = (12.0, std::f64::consts::FRAC_PI_3, 4usize, 12usize);
    let s = |x: f64, y: f64, z: f64| Point3::new(x, y, z - 9.0);
    let ring = |k: usize| -> Vec<Point3> {
        let ph = max * k as f64 / nl as f64;
        let (z, rr) = (rad * ph.cos(), rad * ph.sin());
        (0..nc).map(|j| s(rr * (TAU * j as f64 / nc as f64).cos(), rr * (TAU * j as f64 / nc as f64).sin(), z)).collect()
    };
    let (apex, first, mut f) = (s(0.0, 0.0, rad), ring(1), Vec::new());
    for j in 0..nc {
        f.push(Triangle::new(apex, first[j], first[(j + 1) % nc]));
    }
    for k in 1..nl {
        let (a, b) = (ring(k), ring(k + 1));
        for j in 0..nc {
            let n = (j + 1) % nc;
            f.push(Triangle::new(a[j], b[j], b[n]));
            f.push(Triangle::new(a[j], b[n], a[n]));
        }
    }
    let (rim, c) = (ring(nl), s(0.0, 0.0, rad * max.cos()));
    for j in 0..nc {
        f.push(Triangle::new(c, rim[j], rim[(j + 1) % nc]));
    }
    f
}

/// The cap on the tilt head, posted with the Fanuc post.
fn posted_contour5d() -> String {
    let world: World = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", Mesh::from_facets(cap_facets())));
    let tool = catalog().into_iter().find(|(n, _)| *n == BALL).map(|(_, t)| t).expect("ball");
    h.execute(&world, ok(AddTool::new(&world, BALL, tool), "add tool"));
    h.execute(&world, ok(CreateSetup::new(&world, None, None), "Setup1"));
    let mch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines/tilthead.mch");
    h.execute(&world, ok(LoadMachine::from_file(&world, &mch), "mount tilthead"));
    h.execute(&world, ok(SetWcsOrigin::new(&world, Location { pos: Point3::new(0.0, 0.0, 50.0), rot: IDENTITY_Q }), "G54"));
    let stock = StockShape::Box { dx: 30.0, dy: 30.0, dz: 10.0 };
    h.execute(&world, ok(SetStock::new(&world, stock, Location { pos: Point3::new(0.0, 0.0, -5.0), rot: IDENTITY_Q }), "stock"));
    let params = OpParams::Contour5D {
        tolerance: 0.1, clearance_z: Height::fixed(20.0), retract_z: None,
        top_z: Height::of(HeightReference::TopOfModel, -0.5), bottom_z: Height::of(HeightReference::TopOfModel, -2.5),
        feed: 500.0, plunge_feed: 100.0, climb: true, lead_deg: 15.0, lag_deg: 0.0, max_tilt_deg: 60.0, shank_avoid: false,
        picks: Default::default(),
    };
    h.execute(&world, ok(CreateOperation::new(&world, "OP", BALL, SrcRef::Entity("part".into()), params), "create"));
    h.execute(&world, ok(GenerateOperation::new(&world, "OP"), "generate"));
    let e = model::find_by_name(&world, "OP").expect("op");
    let cached = model::toolpath_of(&world, &e).unwrap_or_else(|| panic!("no path: {:?}", operation::status_of(&world, &e)));
    assert!(!cached.warnings.iter().any(|w| w.blocks_post()), "blocking warnings {:?}", cached.warnings);
    let out = std::env::temp_dir().join(format!("camcraft_3414_{}.nc", std::process::id()));
    let posted = posting::post_script(&world, &out, FANUC);
    let _ = std::fs::remove_file(&out);
    posted.unwrap_or_else(|e| panic!("the Fanuc post must post the program: {e}")).program
}

fn word(line: &str, letter: char) -> Option<f64> {
    line.split_whitespace().find_map(|w| w.strip_prefix(letter).and_then(|v| v.parse::<f64>().ok()))
}

fn turns(line: &str) -> bool {
    word(line, 'A').is_some() || word(line, 'B').is_some()
}

#[test]
fn the_tilthead_rotary_turn_posts_on_g00_before_the_g01_approach() {
    let prog = posted_contour5d();
    let lines: Vec<&str> = prog.lines().collect();
    // The opening rotary turn: the first line that commands a lean.
    let leans = |l: &&str| ['A', 'B'].iter().any(|&c| word(l, c).is_some_and(|v| v.abs() > 1e-6));
    let turn = lines.iter().position(leans).unwrap_or_else(|| panic!("the case must lean:\n{prog}"));
    let at = lines[turn];
    assert!(at.starts_with("G00"), "the opening rotary turn rides a rapid: {at}\n{prog}");
    // The clearance plane: the last Z that a retract-only G00 commanded
    // before the turn. The turn moves no Z below it.
    let clear_z = lines[..turn]
        .iter()
        .rev()
        .find(|l| l.starts_with("G00") && word(l, 'Z').is_some() && word(l, 'X').is_none() && word(l, 'Y').is_none())
        .and_then(|l| word(l, 'Z'))
        .unwrap_or_else(|| panic!("the tool retracts to a clearance before the turn:\n{prog}"));
    let held_z = lines[..=turn].iter().rev().find_map(|l| word(l, 'Z')).unwrap_or(clear_z);
    assert!((held_z - clear_z).abs() < 1e-6, "the turn stands at the clearance Z{clear_z}, not Z{held_z}: {at}\n{prog}");
    // The approach: the first feed after the turn, and the feeds that
    // reach the part, carry no rotary words.
    let approach = lines[turn + 1..]
        .iter()
        .position(|l| l.starts_with("G01"))
        .map(|i| turn + 1 + i)
        .unwrap_or_else(|| panic!("an approach feed follows the turn:\n{prog}"));
    assert!(
        lines[turn + 1..approach].iter().all(|l| !turns(l)),
        "no rotary words between the turn and the approach:\n{prog}"
    );
    assert!(!turns(lines[approach]), "the G01 approach carries no rotary words: {}\n{prog}", lines[approach]);
}
