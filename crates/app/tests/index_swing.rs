//! #804: the index-swing clause of the program gate. Before a 4th axis
//! that turns the work indexes, the tool sits at the safe Z; its tip must
//! clear the radius the stock, parts and turning fixtures sweep about the
//! A centerline by `MARGIN_MM`, or the post is refused naming the op, the
//! A, the swept radius and the clearance. Machines with no work-turning A
//! are untouched.
//! Provenance: our own tests over our own commands and machine files; the
//! microARC 4 / 770MX numbers from Tormach's published spec sheets.
//!
//! Gate map (M4.4): covers app::posting (index_swing, verification_gate),
//! post::machine (nose_to_axis, turns_part, axis_to_table), project
//! (swing_note, #813), app::posting::swing_standing (#1107).

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SelectPost, SetFixtureFixed, SetStock,
    TagFixture,
};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::geo2d::Point2;
use camcraft_app::machine::{Machine, parse_mch};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::{Height, OpParams, SrcRef, Tilt};
use camcraft_app::posting::{MACHINES, index_swing::MARGIN_MM, verification_gate};
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use flecs_ecs::prelude::World;

const IN: f64 = 25.4;
const R: f64 = 0.375 * IN;
const STICK: f64 = 1.5 * IN;
const ARC4: &str = "Tormach 770MX + microARC 4";
/// The microARC file's statement: 13.75 in nose to table less 3.15 in
/// table to centerline.
const NOSE: f64 = 10.6 * IN;

fn shipped(name: &str) -> Machine {
    let text = MACHINES.iter().find(|(n, _)| *n == name).map(|(_, t)| *t).expect("shipped");
    parse_mch(text).expect("parses")
}

fn ok<T, E: std::fmt::Debug>(r: Result<T, E>, what: &str) -> T {
    r.unwrap_or_else(|e| panic!("{what}: {e:?}"))
}

/// The kit's 3/4" bar (its bounding box as stock, 1.5" out of the chuck)
/// on `machine`, one 1/4" end mill `len` mm long from nose to tip, and
/// one shallow flat at `a` (None: upright).
fn doc(machine: Machine, len: f64, a: Option<f64>) -> World {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, ok(LoadMachine::new(&world, "test".into(), Some(machine)), "machine"));
    h.execute(&world, ok(SelectPost::new(&world, PostRef::Named("Tormach PathPilot".into())), "post"));
    let stock = SetStock::new(
        &world,
        StockShape::Box { dx: STICK, dy: 2.0 * R, dz: 2.0 * R },
        Location { pos: Point3::new(-STICK / 2.0, 0.0, -R), ..Location::default() },
    );
    h.execute(&world, ok(stock, "stock"));
    let mut t = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog");
    t.diameter = 0.25 * IN;
    t.shank_d = t.diameter;
    t.rpm = 6000.0;
    t.feed = 15.0 * IN;
    t.holder_len = len - t.flute_len - t.shank_len;
    assert!(t.holder_len > 0.0, "the holder takes up the rest of the length");
    h.execute(&world, ok(AddTool::new(&world, "EM 1/4", t), "tool"));
    let tilt = a.map(|a_deg| Tilt { a_deg, b_deg: 0.0, origin: [0.0; 3] });
    let flat = OpParams::Slot {
        a: Point2::new(-0.75 * IN, 0.0),
        b: Point2::new(-0.25 * IN, 0.0),
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
        tilt,
        extend_start: 0.0,
        extend_end: 0.0,
    };
    h.execute(&world, ok(CreateOperation::new(&world, "FLAT", "EM 1/4", SrcRef::Stock, flat), "op"));
    h.execute(&world, ok(GenerateOperation::new(&world, "FLAT"), "generate"));
    world
}

fn swing_lines(world: &World) -> Vec<String> {
    verification_gate(world)
        .into_iter()
        .filter(|p| p.contains("index turns the work") || p.contains("nose_to_axis"))
        .collect()
}

/// The bar's box corner is the farthest swept point: R * sqrt(2).
fn bar_swept() -> f64 {
    R * std::f64::consts::SQRT_2
}

#[test]
fn the_shipped_microarc_states_its_nose_height_and_a_work_turning_a() {
    let m = shipped(ARC4);
    let a = m.axes.iter().find(|a| a.name == "A").expect("an A");
    assert_eq!(a.turns_part, Some(true), "the microARC sits on the bed: its A turns the work");
    let nose = a.nose_to_axis.expect("stated");
    assert!((nose - NOSE).abs() < 1e-9, "{nose}");
    // A head tilt carries the spindle: its A turns the tool, not the work.
    let tilt = shipped("Tilt-head 5-axis");
    assert!(tilt.axes.iter().filter(|a| a.rotary).all(|a| a.turns_part == Some(false) && a.nose_to_axis.is_none()));
}

#[test]
fn a_short_tool_over_the_kit_bar_clears_and_posts() {
    let world = doc(shipped(ARC4), 3.0 * IN, Some(90.0));
    assert!(swing_lines(&world).is_empty(), "{:?}", verification_gate(&world));
    let out = std::env::temp_dir().join(format!("camcraft_swing_ok_{}.nc", std::process::id()));
    let posted = camcraft_app::posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    let nc = posted.unwrap_or_else(|e| panic!("the short tool must post: {e}")).program;
    assert!(nc.contains("G0 A90"), "{nc}");
}

#[test]
fn a_tool_too_long_to_clear_the_bar_is_refused_by_name() {
    // The tip sits 2 mm short of the margin over the bar's swept corner.
    let len = NOSE - bar_swept() - MARGIN_MM + 2.0;
    let world = doc(shipped(ARC4), len, Some(90.0));
    let lines = swing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let l = &lines[0];
    assert!(l.starts_with("FLAT: the A90.000 index turns the work"), "{l}");
    assert!(l.contains(&format!("sweep {:.3} mm", bar_swept())), "{l}");
    assert!(l.contains("a clearance of 3.000 mm against the 5.0 mm needed"), "{l}");
    let out = std::env::temp_dir().join(format!("camcraft_swing_long_{}.nc", std::process::id()));
    let err = camcraft_app::posting::post_setup(&world, &out).expect_err("a tool that hits must not post");
    assert!(err.contains("FLAT: the A90.000 index"), "{err}");
    assert!(!out.exists(), "a refused post writes nothing");
}

#[test]
fn a_fat_chuck_turning_with_the_bar_is_refused_by_name() {
    let world = doc(shipped(ARC4), 3.0 * IN, Some(90.0));
    let mut h = History::default();
    // A 400 mm square "chuck" behind the bar: it turns with the work and
    // its corners sweep 283 mm, past the 3" tool's 193 mm tip.
    let chuck = cube(Point3::new(-STICK - 40.0, -200.0, -200.0), Point3::new(-STICK - 5.0, 200.0, 200.0));
    h.execute(&world, ImportModel::from_mesh("chuck", chuck));
    h.execute(&world, ok(TagFixture::new(&world, "chuck", None), "tag"));
    let lines = swing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains(&format!("sweep {:.3} mm", 200.0 * std::f64::consts::SQRT_2)), "{}", lines[0]);
}

#[test]
fn a_work_turning_a_with_no_nose_height_is_refused_by_name() {
    let mut m = shipped(ARC4);
    for a in &mut m.axes {
        a.nose_to_axis = None;
    }
    let world = doc(m, 3.0 * IN, Some(-90.0));
    let lines = swing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("FLAT: Tormach 770MX + microARC 4 turns the work on A but states no nose_to_axis"), "{}", lines[0]);
    assert!(lines[0].contains("A-90.000 index"), "{}", lines[0]);
}

#[test]
fn a_machine_with_no_a_is_unaffected() {
    // The same too-long tool, upright on the plain 770MX: no A, no clause.
    let len = NOSE - bar_swept() - MARGIN_MM + 2.0;
    let world = doc(shipped("Tormach 770MX"), len, None);
    assert!(swing_lines(&world).is_empty(), "{:?}", verification_gate(&world));
}

/// #813: the shipped microARC text with its A's `axis` vector struck -
/// a user's own table-A file that never states the line.
fn arc4_without_axis_vector() -> Machine {
    let text = MACHINES.iter().find(|(n, _)| *n == ARC4).map(|(_, t)| *t).expect("shipped");
    let custom = text.replacen("\"axis\": [1.0, 0.0, 0.0],", "", 1);
    assert_ne!(custom, text, "the A's axis vector was struck");
    let m = parse_mch(&custom).expect("a table A with no axis vector still parses");
    assert!(m.axes.iter().any(|a| a.name == "A" && a.rotary && a.axis.is_none()));
    m
}

#[test]
fn a_custom_table_a_with_no_axis_vector_is_judged_about_x() {
    // RS-274: A turns about X, so the unstated line is X - judged, not passed.
    let len = NOSE - bar_swept() - MARGIN_MM + 2.0;
    let world = doc(arc4_without_axis_vector(), len, Some(90.0));
    let lines = swing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("FLAT: the A90.000 index turns the work"), "{}", lines[0]);
    let out = std::env::temp_dir().join(format!("camcraft_swing_noaxis_{}.nc", std::process::id()));
    let err = camcraft_app::posting::post_setup(&world, &out).expect_err("never silently posted");
    assert!(err.contains("FLAT: the A90.000 index"), "{err}");
    assert!(!out.exists(), "a refused post writes nothing");
    // The same file with a short tool clears and posts.
    let world = doc(arc4_without_axis_vector(), 3.0 * IN, Some(90.0));
    assert!(swing_lines(&world).is_empty(), "{:?}", verification_gate(&world));
}

#[test]
fn a_work_turning_rotary_with_no_line_and_no_rs274_letter_is_refused_by_name() {
    let mut m = arc4_without_axis_vector();
    for a in m.axes.iter_mut().filter(|a| a.rotary) {
        a.name = "U".into();
    }
    let world = doc(m, 3.0 * IN, None);
    let lines: Vec<String> =
        verification_gate(&world).into_iter().filter(|p| p.contains("states no axis vector")).collect();
    assert_eq!(lines.len(), 1, "{:?}", verification_gate(&world));
    assert!(lines[0].starts_with("FLAT: Tormach 770MX + microARC 4 turns the work on rotary U"), "{}", lines[0]);
    let out = std::env::temp_dir().join(format!("camcraft_swing_unlined_{}.nc", std::process::id()));
    let err = camcraft_app::posting::post_setup(&world, &out).expect_err("never silently posted");
    assert!(err.contains("rotary U but states no axis vector"), "{err}");
    assert!(!out.exists(), "a refused post writes nothing");
}

#[test]
fn a_project_saved_before_804_names_its_unjudged_table_a_on_load() {
    let world = doc(shipped(ARC4), 3.0 * IN, Some(90.0));
    let text = ok(serde_json::to_string_pretty(&camcraft_app::project::collect(&world)), "save");
    assert!(text.contains("\"turns_part\": true"), "the fresh save states the A's kind");
    let fresh = ok(camcraft_app::project::open_noted(&camcraft_app::world::new_world(), &text), "opens");
    assert!(fresh.iter().all(|n| !n.contains("index-swing")), "{fresh:?}");
    // A pre-#804 save never wrote the key.
    let old = text.replace("\"turns_part\": true,", "").replace(",\n          \"turns_part\": true", "");
    assert!(!old.contains("turns_part"), "{old}");
    let notes = ok(camcraft_app::project::open_noted(&camcraft_app::world::new_world(), &old), "opens");
    let swing: Vec<&String> = notes.iter().filter(|n| n.contains("index-swing")).collect();
    assert_eq!(swing.len(), 1, "{notes:?}");
    let n = swing[0];
    assert!(n.contains("machine Tormach 770MX + microARC 4") && n.contains("rotary A"), "{n}");
    assert!(n.contains("reload the machine"), "{n}");
    // A machine with no rotary is never named.
    let plain = doc(shipped("Tormach 770MX"), 3.0 * IN, None);
    let text = ok(serde_json::to_string_pretty(&camcraft_app::project::collect(&plain)), "save");
    let notes = ok(camcraft_app::project::open_noted(&camcraft_app::world::new_world(), &text), "opens");
    assert!(notes.iter().all(|n| !n.contains("index-swing")), "{notes:?}");
}

// ---- #1107: the swept work against what stands still ----

/// The gate lines of the standing-steel half (#1107).
fn standing_lines(world: &World) -> Vec<String> {
    verification_gate(world).into_iter().filter(|p| p.contains("swings the work into")).collect()
}

/// The shipped microARC with its centre height over the table set to `h` mm.
fn arc4_at(h: Option<f64>) -> Machine {
    let mut m = shipped(ARC4);
    for a in m.axes.iter_mut().filter(|a| a.rotary) {
        a.axis_to_table = h;
    }
    m
}

/// A fixed (standing) box fixture `name` from `lo` to `hi`.
fn stand(world: &World, name: &str, lo: Point3, hi: Point3) {
    let mut h = History::default();
    h.execute(world, ImportModel::from_mesh(name, cube(lo, hi)));
    h.execute(world, ok(TagFixture::new(world, name, None), "tag"));
    h.execute(world, ok(SetFixtureFixed::new(world, name, true), "fixed"));
}

#[test]
fn the_shipped_microarc_states_its_centre_height_over_the_table() {
    let a = shipped(ARC4).axes.into_iter().find(|a| a.name == "A").expect("an A");
    let h = a.axis_to_table.expect("stated");
    assert!((h - 3.15 * IN).abs() < 1e-9, "{h}");
}

#[test]
fn work_swinging_into_the_face_the_rotary_stands_on_is_refused_by_name() {
    // A rotary too low for the bar: the corner sweeps 2 mm inside the margin.
    let world = doc(arc4_at(Some(bar_swept() + MARGIN_MM - 2.0)), 3.0 * IN, Some(90.0));
    let lines = standing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let l = &lines[0];
    assert!(l.starts_with("FLAT: the A90.000 index swings the work into the face the rotary stands on"), "{l}");
    assert!(l.contains(&format!("sweep {:.3} mm", bar_swept())), "{l}");
    assert!(l.contains("a clearance of 3.000 mm against the 5.0 mm needed"), "{l}");
    assert!(swing_lines(&world).is_empty(), "the short tool itself clears: {lines:?}");
    let out = std::env::temp_dir().join(format!("camcraft_swing_table_{}.nc", std::process::id()));
    let err = camcraft_app::posting::post_setup(&world, &out).expect_err("work that hits the table must not post");
    assert!(err.contains("swings the work into the face"), "{err}");
    assert!(!out.exists(), "a refused post writes nothing");
    // The same bar with 2 mm to spare clears.
    let world = doc(arc4_at(Some(bar_swept() + MARGIN_MM + 2.0)), 3.0 * IN, Some(90.0));
    assert!(standing_lines(&world).is_empty(), "{:?}", verification_gate(&world));
}

#[test]
fn a_standing_fixture_under_the_work_is_judged_by_its_distance_from_the_line() {
    let world = doc(shipped(ARC4), 3.0 * IN, Some(90.0));
    // A plate 25 mm under the line, under the bar: the corner (13.5 mm) clears.
    stand(&world, "low plate", Point3::new(-30.0, -50.0, -35.0), Point3::new(-10.0, 50.0, -25.0));
    // A block whose top is 15 mm under the line, but beside the bar in X.
    stand(&world, "beside", Point3::new(5.0, -50.0, -35.0), Point3::new(30.0, 50.0, -15.0));
    // A block that only touches the bar's end face (x = 0) is beside it too.
    stand(&world, "touching", Point3::new(0.0, -50.0, -35.0), Point3::new(10.0, 50.0, -12.0));
    assert!(standing_lines(&world).is_empty(), "{:?}", verification_gate(&world));
    // A riser whose top is 15 mm under the line, under the bar: struck.
    stand(&world, "riser", Point3::new(-30.0, -50.0, -35.0), Point3::new(-20.0, 50.0, -15.0));
    let lines = standing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("FLAT: the A90.000 index swings the work into standing fixture riser"), "{}", lines[0]);
    assert!(lines[0].contains("riser stands 15.000 mm from it"), "{}", lines[0]);
}

#[test]
fn standing_steel_the_swing_cannot_read_is_named_not_judged() {
    // No centre height stated: posts, with the face named not judged.
    let world = doc(arc4_at(None), 3.0 * IN, Some(90.0));
    let report = camcraft_app::posting::verification_report(&world);
    assert!(report.problems.iter().all(|p| !p.contains("swings the work into")), "{:?}", report.problems);
    let line = report.not_judged.expect("named, never silent");
    assert!(line.contains("index swing did not judge"), "{line}");
    assert!(line.contains("states no axis_to_table for A"), "{line}");
    // A tailstock reaching across the line over the bar cannot be judged by a radius.
    let world = doc(shipped(ARC4), 3.0 * IN, Some(90.0));
    let quiet = camcraft_app::posting::verification_report(&world).not_judged;
    assert!(quiet.as_deref().is_none_or(|l| !l.contains("index swing")), "the stated figure judges it: {quiet:?}");
    stand(&world, "tailstock", Point3::new(-5.0, -20.0, -80.0), Point3::new(20.0, 20.0, 3.0));
    let line = camcraft_app::posting::verification_report(&world).not_judged.expect("named");
    assert!(line.contains("standing fixture tailstock: it reaches across the A centerline"), "{line}");
}
