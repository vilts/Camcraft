//! #818: the index-swing clause judges every work-turning rotary about a
//! HORIZONTAL line through the WCS origin, not only an A about X: a table
//! B (stated `[0,1,0]`, or unstated and named B) and an A whose stated
//! vector is level but off X get the microARC's judgement - the tip at
//! the safe Z against the farthest turning vertex from that line - with
//! the same refusal for an unstated `nose_to_axis`. A vertical line (C)
//! keeps every height and stays unjudged; a leaning or garbled line is
//! refused by name, never judged as if level.
//! Provenance: our own tests over our own commands and a test-only
//! machine file (a 770MX-sized mill with a table B, numbers from
//! Tormach's microARC spec sheet); RS-274 / ISO 6983 rotary lettering.
//!
//! #1107 on those lines: the swept work against the standing steel is
//! judged about the same line (span and distance along and off it), or
//! about the WCS origin for stacked rotaries.
//!
//! Gate map (M4.4): covers app::posting (index_swing, verification_gate),
//! project (swing_note), app::posting::swing_standing (#1107).

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SelectPost, SetFixtureFixed,
    SetStock, TagFixture,
};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::machine::{Machine, parse_mch};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::{Height, OpParams, SrcRef, Tilt};
use camcraft_app::posting::index_swing::{MARGIN_MM, RotaryLine, rotary_line};
use camcraft_app::posting::verification_gate;
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use flecs_ecs::prelude::World;

const IN: f64 = 25.4;
const R: f64 = 0.375 * IN;
const STICK: f64 = 1.5 * IN;
/// The test machine's statement, as the microARC's: 10.6 in.
const NOSE: f64 = 10.6 * IN;
const MODEL: &str = "Test VMC + table B";

/// A test-only machine file: a 3-axis mill with a rotary table B bolted
/// to the bed (outside the head's chain, so it turns the work) about Y.
const TABLE_B: &str = r#"{
  "general": { "model": "Test VMC + table B", "vendor": "Camcraft tests", "kind": "mill" },
  "kinematics": { "default": {
    "units": { "length": "inch", "angle": "degrees" },
    "parts": [
      { "id": "base", "name": "table", "type": "table", "parts": [
        { "id": "col", "name": "Z", "type": "linear", "travel": { "min": -13.25, "max": 13.25 }, "safe_retract": 0.0, "parts": [
          { "id": "ram", "name": "Y", "type": "linear", "travel": { "min": -7.5, "max": 7.5 }, "parts": [
            { "id": "sld", "name": "X", "type": "linear", "travel": { "min": -14.0, "max": 14.0 } }
          ] }
        ] },
        { "id": "rtb", "name": "B", "type": "rotary", "axis": [0.0, 1.0, 0.0],
          "travel": { "min": -360.0, "max": 360.0 }, "nose_to_axis": 10.6 },
        { "id": "head", "name": "head", "type": "head",
          "spindle": { "min_speed": 175.0, "max_speed": 10000.0, "power": 1.1 } }
      ] }
    ] } },
  "controller": { "synced_configuration": { "parts": {
    "X": { "coordinate": 0 }, "Y": { "coordinate": 1 }, "Z": { "coordinate": 2 }, "B": { "coordinate": 3 }
  } } },
  "tooling": { "number_of_tools": 10, "spindle_mount": "BT30", "default_holder": "BT30 ER20" }
}"#;

fn ok<T, E: std::fmt::Debug>(r: Result<T, E>, what: &str) -> T {
    r.unwrap_or_else(|e| panic!("{what}: {e:?}"))
}

fn table_b() -> Machine {
    let m = ok(parse_mch(TABLE_B), "the test table-B file parses");
    let b = m.axes.iter().find(|a| a.name == "B").expect("a B");
    assert_eq!(b.turns_part, Some(true), "no head below it: the B turns the work");
    m
}

/// The table B with its rotary renamed and its vector restated (None:
/// struck), and its nose_to_axis replaced (Some(None): struck).
fn table_b_as(name: &str, axis: Option<[f64; 3]>, nose: Option<Option<f64>>) -> Machine {
    let mut m = table_b();
    for a in m.axes.iter_mut().filter(|a| a.rotary) {
        a.name = name.into();
        a.axis = axis;
        if let Some(n) = nose {
            a.nose_to_axis = n;
        }
    }
    m
}

/// The 3/4" bar (its bounding box as stock, 1.5" long along X) on
/// `machine`, one 1/4" end mill `len` mm from nose to tip, one shallow
/// flat with `tilt` (None: upright).
fn doc(machine: Machine, len: f64, tilt: Option<(f64, f64)>) -> World {
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
        tilt: tilt.map(|(a_deg, b_deg)| Tilt { basis_version: 1, a_deg, b_deg, origin: [0.0; 3] }),
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
        .filter(|p| {
            p.contains("index turns the work") || p.contains("nose_to_axis") || p.contains("index swing is not judged")
        })
        .collect()
}

/// The bar's farthest box corner from the level line through the origin
/// along unit `u`: `max |p x u|` over its eight corners (x from -STICK to
/// the WCS origin along the bar, y and z within R of its centerline).
fn bar_swept(u: [f64; 3]) -> f64 {
    let mut r = 0.0_f64;
    for x in [-STICK, 0.0] {
        for y in [-R, R] {
            for z in [-R, R] {
                let c = [y * u[2] - z * u[1], z * u[0] - x * u[2], x * u[1] - y * u[0]];
                r = r.max(c[0].hypot(c[1]).hypot(c[2]));
            }
        }
    }
    r
}

/// The tool that sits 2 mm short of the margin over `swept`.
fn too_long(swept: f64) -> f64 {
    NOSE - swept - MARGIN_MM + 2.0
}

fn refused_post(world: &World, tag: &str, needle: &str) {
    let out = std::env::temp_dir().join(format!("camcraft_swing818_{tag}_{}.nc", std::process::id()));
    let err = camcraft_app::posting::post_setup(world, &out).expect_err("never silently posted");
    assert!(err.contains(needle), "{err}");
    assert!(!out.exists(), "a refused post writes nothing");
}

#[test]
fn a_table_b_too_long_to_clear_the_bar_is_refused_by_name() {
    let swept = bar_swept([0.0, 1.0, 0.0]);
    assert!((swept - STICK.hypot(R)).abs() < 1e-9, "about Y the bar's far end sweeps: {swept}");
    let world = doc(table_b(), too_long(swept), Some((0.0, 90.0)));
    let lines = swing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let l = &lines[0];
    assert!(l.starts_with("FLAT: the B") && l.contains(" index turns the work"), "{l}");
    assert!(l.contains("above the B centerline"), "{l}");
    assert!(l.contains(&format!("sweep {swept:.3} mm,")), "{l}");
    assert!(l.contains("a clearance of 3.000 mm against the 5.0 mm needed"), "{l}");
    refused_post(&world, "b_long", "FLAT: the B");
}

#[test]
fn a_table_b_with_a_short_tool_clears() {
    let world = doc(table_b(), 3.0 * IN, Some((0.0, 90.0)));
    assert!(swing_lines(&world).is_empty(), "{:?}", verification_gate(&world));
}

#[test]
fn a_table_b_with_no_nose_height_is_refused_by_name() {
    let world = doc(table_b_as("B", Some([0.0, 1.0, 0.0]), Some(None)), 3.0 * IN, None);
    let lines = swing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with(&format!("FLAT: {MODEL} turns the work on B but states no nose_to_axis")), "{}", lines[0]);
    assert!(lines[0].contains("B0.000 index"), "{}", lines[0]);
    refused_post(&world, "b_nonose", "states no nose_to_axis");
}

#[test]
fn a_table_b_with_no_axis_vector_is_judged_about_y() {
    // RS-274: B turns about Y, so the unstated line is Y - judged, not passed.
    let swept = bar_swept([0.0, 1.0, 0.0]);
    let world = doc(table_b_as("B", None, None), too_long(swept), None);
    let lines = swing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("FLAT: the B0.000 index turns the work"), "{}", lines[0]);
    assert!(lines[0].contains(&format!("sweep {swept:.3} mm,")), "{}", lines[0]);
}

#[test]
fn an_a_stated_level_but_off_x_is_judged_about_its_own_line() {
    let u = [0.6, 0.8, 0.0];
    let swept = bar_swept(u);
    assert!(swept > R * std::f64::consts::SQRT_2 + 1.0, "off X the bar sweeps more than about X: {swept}");
    let world = doc(table_b_as("A", Some([3.0, 4.0, 0.0]), None), too_long(swept), None);
    let lines = swing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("FLAT: the A0.000 index turns the work"), "{}", lines[0]);
    assert!(lines[0].contains(&format!("sweep {swept:.3} mm,")), "{}", lines[0]);
    assert!(lines[0].contains("a clearance of 3.000 mm"), "{}", lines[0]);
    // A short tool over the same line clears.
    let world = doc(table_b_as("A", Some([3.0, 4.0, 0.0]), None), 3.0 * IN, None);
    assert!(swing_lines(&world).is_empty(), "{:?}", verification_gate(&world));
}

#[test]
fn a_leaning_or_zero_work_turning_line_is_refused_not_judged() {
    for (tag, v) in [("lean", [1.0, 0.0, 1.0]), ("zero", [0.0; 3])] {
        let world = doc(table_b_as("B", Some(v), None), 3.0 * IN, None);
        let lines = swing_lines(&world);
        assert_eq!(lines.len(), 1, "{tag}: {lines:?}");
        assert!(
            lines[0].starts_with(&format!("FLAT: {MODEL} turns the work on rotary B about a line that is neither level")),
            "{tag}: {}",
            lines[0]
        );
        refused_post(&world, tag, "index swing is not judged");
    }
}

fn line_of(m: &Machine) -> RotaryLine {
    rotary_line(m.axes.iter().find(|a| a.rotary).expect("a rotary"))
}

#[test]
fn a_non_finite_vector_or_nose_height_never_reads_as_judged() {
    // The machine command refuses a non-finite number at creation ...
    let world = camcraft_app::world::new_world();
    for m in [
        table_b_as("B", Some([f64::NAN, 1.0, 0.0]), None),
        table_b_as("B", Some([0.0, 1.0, 0.0]), Some(Some(f64::INFINITY))),
    ] {
        assert!(LoadMachine::new(&world, "test".into(), Some(m)).is_err(), "refused at command creation");
    }
    // ... and the clause reads such a line as leaning (refused), never level.
    assert_eq!(line_of(&table_b_as("B", Some([f64::NAN, 1.0, 0.0]), None)), RotaryLine::Leaning);
    assert_eq!(line_of(&table_b_as("B", Some([0.0, f64::INFINITY, 0.0]), None)), RotaryLine::Leaning);
    // A level vector is normalised; a slightly tilted one is not level.
    assert_eq!(line_of(&table_b_as("B", Some([0.0, 2.0, 0.0]), None)), RotaryLine::Horizontal([0.0, 1.0, 0.0]));
    assert_eq!(line_of(&table_b_as("B", Some([0.0, 1.0, 0.01]), None)), RotaryLine::Leaning);
    assert_eq!(line_of(&table_b_as("C", Some([0.0, 0.0, -3.0]), None)), RotaryLine::Vertical);
}

#[test]
fn a_work_turning_c_alone_stays_unjudged() {
    // Turning about a vertical line keeps every height: even a too-long
    // tool and no nose height raise nothing.
    let len = too_long(bar_swept([1.0, 0.0, 0.0]));
    for axis in [Some([0.0, 0.0, 1.0]), None] {
        let world = doc(table_b_as("C", axis, Some(None)), len, None);
        assert!(swing_lines(&world).is_empty(), "{axis:?}: {:?}", verification_gate(&world));
    }
}

#[test]
fn two_rotaries_turning_the_work_sweep_from_the_wcs_origin() {
    // A table B carrying a C: the C moves the bar off the B line, so the
    // sweep is the farthest corner from the origin itself.
    let mut m = table_b();
    let mut c = m.axes.iter().find(|a| a.name == "B").cloned().expect("a B");
    c.name = "C".into();
    c.axis = Some([0.0, 0.0, 1.0]);
    m.axes.push(c);
    let sphere = STICK.hypot(R).hypot(R);
    let world = doc(m, too_long(sphere), None);
    let lines = swing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("FLAT: the B0.000 index turns the work"), "{}", lines[0]);
    assert!(lines[0].contains(&format!("sweep {sphere:.3} mm (from the WCS origin")), "{}", lines[0]);
}

#[test]
fn a_project_saved_before_804_names_its_unjudged_table_b_on_load() {
    let world = doc(table_b(), 3.0 * IN, None);
    let text = ok(serde_json::to_string_pretty(&camcraft_app::project::collect(&world)), "save");
    assert!(text.contains("\"turns_part\": true"), "the fresh save states the B's kind");
    let old = text.replace("\"turns_part\": true,", "").replace(",\n          \"turns_part\": true", "");
    assert!(!old.contains("turns_part"), "{old}");
    let notes = ok(camcraft_app::project::open_noted(&camcraft_app::world::new_world(), &old), "opens");
    let swing: Vec<&String> = notes.iter().filter(|n| n.contains("index-swing")).collect();
    assert_eq!(swing.len(), 1, "{notes:?}");
    assert!(swing[0].contains(&format!("machine {MODEL}")) && swing[0].contains("rotary B"), "{}", swing[0]);
}

// ---- #1107: the swept work against what stands still, off X ----

/// The gate lines of the standing-steel half (#1107).
fn standing_lines(world: &World) -> Vec<String> {
    verification_gate(world).into_iter().filter(|p| p.contains("swings the work into")).collect()
}

/// `m` with every rotary's centre height over its face set to `h` mm.
fn at(mut m: Machine, h: Option<f64>) -> Machine {
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
fn a_table_b_judges_the_work_against_standing_steel_about_y() {
    let r = bar_swept([0.0, 1.0, 0.0]);
    // The face: 2 mm inside the margin is refused, 2 mm outside clears.
    let world = doc(at(table_b(), Some(r + MARGIN_MM - 2.0)), 3.0 * IN, None);
    let lines = standing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("FLAT: the B0.000 index swings the work into the face the rotary stands on"),
        "{}",
        lines[0]
    );
    assert!(lines[0].contains(&format!("sweep {r:.3} mm about the B centerline")), "{}", lines[0]);
    assert!(lines[0].contains("a clearance of 3.000 mm"), "{}", lines[0]);
    refused_post(&world, "standing_b", "swings the work into the face");
    let world = doc(at(table_b(), Some(r + MARGIN_MM + 2.0)), 3.0 * IN, None);
    assert!(standing_lines(&world).is_empty(), "{:?}", verification_gate(&world));
    // Beside the bar along Y (its span is y within R): clear, however close.
    stand(&world, "beside", Point3::new(-30.0, 15.0, -35.0), Point3::new(-10.0, 40.0, -3.0));
    // A plate 50 mm under the line: clear of the bar's swing about Y.
    stand(&world, "plate", Point3::new(-60.0, -50.0, -80.0), Point3::new(20.0, 50.0, -50.0));
    assert!(standing_lines(&world).is_empty(), "{:?}", verification_gate(&world));
    // A riser past the bar's end in X but across its Y span: the bar's
    // end sweeps about Y into it (beside it in X, as an about-X law
    // would wrongly read it).
    stand(&world, "riser", Point3::new(5.0, -50.0, -35.0), Point3::new(30.0, 50.0, -3.0));
    let lines = standing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let d = 5.0_f64.hypot(3.0);
    assert!(lines[0].starts_with("FLAT: the B0.000 index swings the work into standing fixture riser"), "{}", lines[0]);
    assert!(lines[0].contains(&format!("riser stands {d:.3} mm from it")), "{}", lines[0]);
}

#[test]
fn an_a_stated_level_but_off_x_judges_standing_steel_about_its_own_line() {
    let u = [0.6, 0.8, 0.0];
    let r = bar_swept(u);
    let m = || table_b_as("A", Some([3.0, 4.0, 0.0]), None);
    let world = doc(at(m(), Some(r + MARGIN_MM - 2.0)), 3.0 * IN, None);
    let lines = standing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains(&format!("sweep {r:.3} mm about the A centerline")), "{}", lines[0]);
    let world = doc(at(m(), Some(r + MARGIN_MM + 2.0)), 3.0 * IN, None);
    // A plate whose top is r + margin + 1 under the line: clear.
    let top = -(r + MARGIN_MM + 1.0);
    stand(&world, "plate", Point3::new(-80.0, -80.0, top - 20.0), Point3::new(40.0, 80.0, top));
    assert!(standing_lines(&world).is_empty(), "{:?}", verification_gate(&world));
    // ... and one 2 mm inside the margin: struck.
    let top = -(r + MARGIN_MM - 2.0);
    stand(&world, "riser", Point3::new(-80.0, -80.0, top - 20.0), Point3::new(40.0, 80.0, top));
    let lines = standing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].contains("standing fixture riser") && lines[0].contains("a clearance of 3.000 mm"),
        "{}",
        lines[0]
    );
}

#[test]
fn stacked_rotaries_judge_standing_steel_from_the_wcs_origin_and_name_the_face() {
    let mut m = table_b();
    let mut c = m.axes.iter().find(|a| a.name == "B").cloned().expect("a B");
    c.name = "C".into();
    c.axis = Some([0.0, 0.0, 1.0]);
    m.axes.push(c);
    let m = at(m, Some(200.0));
    let sphere = STICK.hypot(R).hypot(R);
    let world = doc(m, 3.0 * IN, None);
    // The face under a table B carrying a C moves: named, never judged as fixed.
    let report = camcraft_app::posting::verification_report(&world);
    assert!(report.problems.iter().all(|p| !p.contains("swings the work into")), "{:?}", report.problems);
    let line = report.not_judged.expect("named, never silent");
    assert!(line.contains("index swing did not judge") && line.contains("more than one rotary"), "{line}");
    // Far from the origin: clear.
    stand(&world, "far", Point3::new(-200.0, -50.0, -50.0), Point3::new(-150.0, 50.0, 50.0));
    assert!(standing_lines(&world).is_empty(), "{:?}", verification_gate(&world));
    // Beside the bar along the B line but within the sphere: the C turns
    // the bar into it, so it is struck by its distance from the origin.
    stand(&world, "block", Point3::new(-10.0, 15.0, -35.0), Point3::new(10.0, 40.0, -30.0));
    let d = 15.0_f64.hypot(30.0);
    assert!(d < sphere + MARGIN_MM, "inside the sphere: {d} vs {sphere}");
    let lines = standing_lines(&world);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("FLAT: the B0.000 index swings the work into standing fixture block"), "{}", lines[0]);
    assert!(
        lines[0].contains("about the WCS origin") && lines[0].contains(&format!("block stands {d:.3} mm")),
        "{}",
        lines[0]
    );
}
