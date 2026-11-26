//! #1904: the "Your first continuous 4-axis part" guide's demo job, built
//! through the app's own commands - `examples/first-continuous-4axis-part.camproj`
//! and the program it posts, `examples/first-continuous-4axis-part.nc`. A 3/4"
//! round bar in the microARC 4 chuck on a Tormach 770MX, axis along world X,
//! the free end at X0 (G54 X0 at the end face, Y0 Z0 on the A centreline).
//! One Rotary Parallel pass with a 1/8" ball tool finishes a journal 0.680"
//! across and 1.25" long. The A axis turns inside the operation, so the post
//! writes wrapped A (#796): A counts on, a move that turns A with X Y Z runs
//! in G93, and G94 comes back after it. The guide page quotes this program,
//! and `site/tests/test_first_continuous_4axis_part.py` holds the page to
//! these files.
//!
//! Rotary Parallel leaves the chuck-side end face of the model (#2343): the
//! stock goes on past that end into the jaws, so the pass stops at the end
//! plane. One test here pins that fact, so the guide stays true to the program.
//! Provenance: our own test over our own commands, machine file and post.
//!
//! Regenerate the project and the program after an intentional change:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test continuous_4axis_part`
//!
//! Gate map (M4.4): covers app::posting (post_setup, verification_gate),
//! post::post (tormach.cps), app::operation (Rotary Parallel), app::project,
//! app::machine (check_document).

#[path = "common/continuous_project.rs"]
mod continuous_project;

#[path = "common/wrapped_nc.rs"]
mod wrapped_nc;

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SelectPost,
    SetModelSource, SetStock, SetUnits,
};
use camcraft_app::geometry::Point3;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::units::Unit;
use camcraft_app::{machine, posting, project};
use flecs_ecs::prelude::World;
use std::path::{Path, PathBuf};

const IN: f64 = 25.4;
/// 3/4" bar; the stock is the 2" that sticks out of the chuck.
const R: f64 = 0.375 * IN;
const STICK: f64 = 2.0 * IN;
/// The finished radius of the journal (the model) and its length.
const JOURNAL_R: f64 = 0.340;
const JOURNAL_LEN: f64 = 1.25;
const MACHINE: &str = "Tormach 770MX + microARC 4";
const PROJECT: &str = "examples/first-continuous-4axis-part.camproj";
const PROGRAM: &str = "examples/first-continuous-4axis-part.nc";
const MODEL: &str = "examples/models/first-continuous-4axis-part.obj";
const MODEL_NAME: &str = "first-continuous-4axis-part";
const OP: &str = "Journal";
/// The flute length the guide types: long enough to reach the A centreline.
const FLUTE: f64 = 0.625;

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn ok<T, E: std::fmt::Debug>(r: Result<T, E>, what: &str) -> T {
    r.unwrap_or_else(|e| panic!("{what}: {e:?}"))
}

/// The 1/8" ball tool, from the catalog's ball-nose entry.
fn ball(flute_in: f64) -> Tool {
    let mut t = catalog()
        .into_iter()
        .find(|(n, _)| *n == "BN-0600")
        .map(|(_, t)| t)
        .expect("the catalog's ball-nose entry");
    t.diameter = 0.125 * IN;
    t.shank_d = t.diameter;
    t.corner_r = t.diameter / 2.0;
    t.flute_len = flute_in * IN;
    t.rpm = 8000.0;
    t.feed = 8.0 * IN;
    t
}

/// The document: machine, unit, post, stock, model, tool, the one op.
fn build() -> World {
    build_with(FLUTE)
}

fn build_with(flute_in: f64) -> World {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, ok(LoadMachine::builtin(&world, MACHINE), "machine"));
    h.execute(&world, ok(SetUnits::new(&world, Unit::Inches), "inch document"));
    h.execute(&world, ok(SelectPost::new(&world, PostRef::Named("Tormach PathPilot".into())), "post"));
    h.execute(&world, ok(ImportModel::new(&repo(MODEL)), "model"));
    // The committed example names no file on this machine (#2463).
    h.execute(&world, ok(SetModelSource::unlink(&world, MODEL_NAME), "unlink"));
    // The box the bar's 2" stick-out fits in, centred on the A centreline.
    let stock = SetStock::new(
        &world,
        StockShape::Box { dx: STICK, dy: 2.0 * R, dz: 2.0 * R },
        Location { pos: Point3::new(-STICK / 2.0, 0.0, -R), ..Location::default() },
    );
    h.execute(&world, ok(stock, "stock"));
    h.execute(&world, ok(AddTool::new(&world, "BN 1/8", ball(flute_in)), "T1"));
    // Rotary Parallel reads its heights as radii about the bar's axis.
    // Rotary Parallel reads its heights as radii about the bar's axis.
    let wrap = OpParams::RotaryParallel {
        tolerance: 0.02 * IN,
        stepover: Some(0.1 * IN),
        clearance_z: Height::fixed(R + 0.5 * IN),
        retract_z: None,
        top_z: Height::fixed(R),
        bottom_z: Height::fixed(-R),
        feed: 8.0 * IN,
        plunge_feed: 2.0 * IN,
        climb: true,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
        stated_axis: None,
        picks: Default::default(),
    };
    h.execute(&world, ok(CreateOperation::new(&world, OP, "BN 1/8", SrcRef::Entity(MODEL_NAME.into()), wrap), OP));
    h.execute(&world, ok(GenerateOperation::new(&world, OP), OP));
    world
}

fn post(world: &World, tag: &str) -> posting::Posted {
    let out = std::env::temp_dir().join(format!("camcraft_cont_4axis_{tag}_{}.nc", std::process::id()));
    let posted = posting::post_setup(world, &out);
    let _ = std::fs::remove_file(&out);
    posted.unwrap_or_else(|e| panic!("the continuous 4-axis part must post: {e}"))
}

fn num(word: &str) -> Option<f64> {
    word.get(1..).and_then(|s| s.parse().ok())
}

fn word(line: &str, letter: char) -> Option<f64> {
    line.split(' ').find(|w| w.starts_with(letter)).and_then(num)
}

#[test]
fn the_committed_project_uses_the_current_file_version() {
    let text = std::fs::read_to_string(repo(PROJECT)).expect("the example project");
    let saved: serde_json::Value = serde_json::from_str(&text).expect("the example project JSON");
    assert_eq!(saved["version"], project::PROJECT_VERSION, "regenerate {PROJECT}");
}

#[test]
fn the_continuous_4axis_part_posts_its_wrapped_program() {
    // The committed project is the built document, as the app saves it. The
    // committed program is what that saved project posts: a saved toolpath
    // keeps fewer digits than a fresh one, so the two programs can differ
    // in a last digit, and the reader gets the one their file posts.
    let built = build();
    let saved = std::env::temp_dir().join(format!("camcraft_cont_4axis_{}.camproj", std::process::id()));
    project::save(&built, &saved).expect("save the built document");
    let text = std::fs::read_to_string(&saved).expect("the saved document");
    let _ = std::fs::remove_file(&saved);
    let golden = repo(PROGRAM);
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(repo(PROJECT), &text).expect("write the example project");
        let world = camcraft_app::world::new_world();
        ok(project::load_noted(&world, &repo(PROJECT)), "load the example project");
        std::fs::write(&golden, post(&world, "golden").program).expect("write the example program");
    }
    let committed = std::fs::read_to_string(repo(PROJECT)).expect("the example project");
    continuous_project::compare(&text, &committed).unwrap_or_else(|e| {
        panic!("{PROJECT} is stale; regenerate with WRITE_GOLDEN=1: {e}")
    });
    let nc = std::fs::read_to_string(&golden).expect("the example program");
    let lines: Vec<&str> = nc.lines().collect();
    assert!(nc.starts_with("(Tormach 770MX + microARC 4)\nG20 "), "an inch PathPilot program: {nc}");
    assert_eq!(lines[lines.len() - 2..], ["G53 G0 Z0", "M30"], "{nc}");
    // The first A of the operation is placed alone, with Z at the top.
    let first_a = lines.iter().position(|l| l.starts_with("G0 A")).expect("the first A");
    assert_eq!(lines[first_a - 1], "G53 G0 Z0", "{nc}");
    assert_eq!(lines.iter().filter(|l| l.starts_with("G0 A")).count(), 1, "one placing index");
    // The guide's statements about the body of the program.
    let (mut g93, mut last_a, mut turning, mut a_only) = (false, None::<f64>, 0usize, 0usize);
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for l in &lines[first_a..] {
        let ws: Vec<&str> = l.split(' ').collect();
        let was_g93 = g93;
        if ws.contains(&"G93") {
            g93 = true;
        }
        if ws.contains(&"G94") {
            g93 = false;
        }
        let a = word(l, 'A');
        if let (Some(p), Some(a)) = (last_a, a) {
            // A counts on: no word jumps the long way round, and none of
            // them is a second way to write the same angle.
            assert!((a - p).abs() <= 180.0, "A unwrapped: {p} -> {a} at {l}");
        }
        if let Some(a) = a {
            last_a = Some(a);
            lo = lo.min(a);
            hi = hi.max(a);
        }
        let feed = ws[0] == "G1" || ws.get(1) == Some(&"G1");
        let xyz = ['X', 'Y', 'Z'].iter().any(|c| word(l, *c).is_some());
        if feed {
            let f = word(l, 'F').unwrap_or_else(|| panic!("each feed carries F: {l}"));
            assert!(f > 0.0, "{l}");
            match (a.is_some(), xyz) {
                (true, true) => {
                    assert!(g93, "a move that turns A with X Y Z is in G93: {l}");
                    turning += 1;
                }
                (true, false) => {
                    assert!(!g93, "an A-only feed is in G94: {l}");
                    assert!(f <= 360.0, "an A-only feed is in deg/min, within the A rate: {l}");
                    a_only += 1;
                }
                _ => assert!(!g93, "a feed that does not turn A is in G94: {l}"),
            }
        } else if ws[0] == "G0" {
            assert!(!g93 && !was_g93, "a rapid never runs in G93: {l}");
        }
        if ws[0] == "M5" {
            assert!(!g93, "the operation ends in G94: {nc}");
        }
    }
    assert!(turning > 0, "the pass turns A on its feeds");
    assert!(a_only > 0, "the program has A-only feeds in deg/min");
    // This example stays within one turn; the shipped machine states no A travel.
    assert!(lo >= -360.0 && hi <= 360.0, "A {lo}..{hi} is within -360..360");
    assert!(hi - lo > 180.0, "A turns more than a half turn inside the operation: {lo}..{hi}");
    // The passes are at the journal radius, 0.340" from the centreline.
    let at_journal = lines.iter().filter(|l| l.starts_with("G1 ") && word(l, 'A').is_none())
        .filter_map(|l| word(l, 'Z')).filter(|z| (z - JOURNAL_R).abs() < 0.002).count();
    assert!(at_journal > 20, "the passes run at Z{JOURNAL_R}: {at_journal}");
    // The tool passes the free end of the journal by its own radius, 1/16".
    // At the chuck-side end it stops on the end plane (#2343), 3/4" from
    // the jaws at X-2.
    let xs: Vec<f64> = lines.iter().filter(|l| l.contains("G1 ")).filter_map(|l| word(l, 'X')).collect();
    let (x_lo, x_hi) = xs.iter().fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(*x), b.max(*x)));
    assert!((x_lo + JOURNAL_LEN).abs() < 0.001, "the chuck-side end: X{x_lo}");
    assert!(x_hi < 0.125 && x_hi > 0.0, "the free end: X{x_hi}");
}

/// #2343: the stock goes on 3/4" past the chuck-side end of the model
/// (X-1.25), into the jaws. The pass leaves that end face: the centre of the
/// ball stays on the model's side of the end plane, so the tip stops on the
/// plane and never goes below the journal there, and the operation says so.
/// The free end (X0) has air past the model: its end face is still cut to
/// the centreline.
#[test]
fn the_pass_leaves_the_chuck_side_end_face() {
    let world = build();
    let nc = post(&world, "held").program;
    let feeds: Vec<(f64, Option<f64>)> = nc.lines().filter(|l| l.contains("G1 "))
        .filter_map(|l| word(l, 'X').map(|x| (x, word(l, 'Z')))).collect();
    let x_lo = feeds.iter().map(|(x, _)| *x).fold(f64::MAX, f64::min);
    assert!(x_lo > -JOURNAL_LEN - 0.001, "the tool passes the chuck-side end plane: X{x_lo}");
    assert!(x_lo < -JOURNAL_LEN + 0.001, "the journal is finished to its end: X{x_lo}");
    let low = feeds.iter().filter(|(x, _)| *x < -JOURNAL_LEN + 0.07).filter_map(|(_, z)| *z).fold(f64::MAX, f64::min);
    assert!(low > JOURNAL_R - 0.002, "the tool goes below the journal at the chuck-side end: Z{low}");
    let free = feeds.iter().filter(|(x, _)| *x > 0.0).filter_map(|(_, z)| *z).fold(f64::MAX, f64::min);
    assert!(free < 0.01, "the free end face is cut to Z{free}");
    let op = camcraft_app::model::find_by_name(&world, OP).expect("the operation");
    let said = camcraft_app::model::toolpath_of(&world, &op).expect("generated").path.notes;
    assert!(
        said.iter().any(|n| n.starts_with("Rotary Parallel: the stock goes on") && n.contains("end face")),
        "the operation names the end face that it left: {said:?}"
    );
}

/// Before #2343 a 1/2" flute was blocked at post: the cut down the
/// chuck-side end face put the shank in the stock ("shank/holder at move N
/// collides with the uncut stock"). That cut is gone, so the short flute
/// posts, and its program keeps to the model's side of the end plane too.
#[test]
fn a_short_flute_posts_now_that_the_chuck_side_end_face_is_left() {
    let nc = post(&build_with(0.5), "short").program;
    let x_lo = nc.lines().filter(|l| l.contains("G1 ")).filter_map(|l| word(l, 'X')).fold(f64::MAX, f64::min);
    assert!(x_lo > -JOURNAL_LEN - 0.001, "the tool passes the chuck-side end plane: X{x_lo}");
}

/// Each G93 move is held to both rates the guide names: its time (1/F
/// minutes) is long enough for its A turn at the machine file's A rate.
#[test]
fn each_g93_move_holds_the_a_rate() {
    let nc = std::fs::read_to_string(repo(PROGRAM)).expect("the example program");
    let (mut g93, mut last_a, mut checked) = (false, None::<f64>, 0usize);
    for l in nc.lines() {
        if l.split(' ').any(|w| w == "G93") {
            g93 = true;
        }
        if l.split(' ').any(|w| w == "G94") {
            g93 = false;
        }
        let a = word(l, 'A');
        if g93 && l.contains("G1 ") {
            let f = word(l, 'F').expect("each G93 move carries F");
            if let (Some(p), Some(a)) = (last_a, a) {
                let deg_per_min = (a - p).abs() * f;
                assert!(deg_per_min <= 360.0 * 1.001, "{l}: A runs at {deg_per_min:.1} deg/min");
                checked += 1;
            }
        }
        if a.is_some() {
            last_a = a;
        }
    }
    assert!(checked > 0, "{nc}");
}

/// Sample the controller's XYZ/A interpolation independently of the CPS time
/// formula. Work in inches, as posted, and rotate each sample back into the
/// part frame. Comparing endpoints alone misses a curved turning trajectory.
fn check_generated_turning_feeds(nc: &str) {
    let (mut xyz, mut angle, mut inverse_time) = ([0.0; 3], 0.0, false);
    let mut checked = 0;
    for line in nc.lines() {
        let words: Vec<_> = line.split(' ').collect();
        if words.contains(&"G93") {
            inverse_time = true;
        }
        if words.contains(&"G94") {
            inverse_time = false;
        }
        if !words.contains(&"G0") && !words.contains(&"G1") {
            continue;
        }
        let next_xyz = std::array::from_fn(|i| word(line, ['X', 'Y', 'Z'][i]).unwrap_or(xyz[i]));
        let next_angle = word(line, 'A').unwrap_or(angle);
        if inverse_time && words.contains(&"G1") {
            let inverse_minutes = word(line, 'F').expect("every G93 feed carries F");
            assert!(
                inverse_minutes.is_finite() && inverse_minutes > 0.0,
                "{line}"
            );
            let part = |u: f64| {
                let m: [f64; 3] = std::array::from_fn(|i| xyz[i] + (next_xyz[i] - xyz[i]) * u);
                let (sin, cos) = (angle + (next_angle - angle) * u).to_radians().sin_cos();
                [m[0], m[1] * cos - m[2] * sin, m[1] * sin + m[2] * cos]
            };
            let mut previous = part(0.0);
            let mut length = 0.0;
            for j in 1..=128 {
                let next = part(f64::from(j) / 128.0);
                length += next
                    .iter()
                    .zip(previous)
                    .map(|(p, q)| (p - q).powi(2))
                    .sum::<f64>()
                    .sqrt();
                previous = next;
            }
            let feed = length * inverse_minutes;
            assert!(
                feed <= 8.0 + 1e-8,
                "sampled {feed} in/min exceeds the example's F8: {line}"
            );
            let a_rate = (next_angle - angle).abs() * inverse_minutes;
            assert!(a_rate <= 360.0 + 1e-8, "A runs at {a_rate} deg/min: {line}");
            checked += 1;
        }
        xyz = next_xyz;
        angle = next_angle;
    }
    assert!(
        checked > 100,
        "the generated example exercises turning feeds: {checked}"
    );
}

/// The committed project is the built document: it loads on the shipped
/// machine and post, passes the machine check and the post gate, and
/// posts the committed program.
#[test]
fn the_committed_project_posts_the_committed_program() {
    let world = camcraft_app::world::new_world();
    let notes = ok(project::load_noted(&world, &repo(PROJECT)), "load the example project");
    assert!(notes.is_empty(), "the example loads without migration notes: {notes:?}");
    let (_, mch) = machine::of(&world);
    let mch = mch.expect("the mounted machine");
    assert_eq!(mch.model, MACHINE);
    assert_eq!(posting::post_of(&world), PostRef::Named("Tormach PathPilot".into()));
    let names: Vec<String> = camcraft_app::setup::operation_order(&world).into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, [OP]);
    let check = ok(machine::check_document(&world, &mch), "the machine check");
    assert!(check.is_ok(), "{:?}", check.problems);
    let gate = posting::verification_gate(&world);
    assert!(gate.is_empty(), "the post gate is silent: {gate:?}");
    let want = std::fs::read_to_string(repo(PROGRAM)).expect("the example program");
    let generated = post(&world, "loaded").program;
    wrapped_nc::compare(&generated, &want)
        .unwrap_or_else(|e| panic!("{PROJECT} posts {PROGRAM}: {e}"));
    check_generated_turning_feeds(&generated);
}
