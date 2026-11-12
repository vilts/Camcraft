//! Per-axis direction (#373): a machine whose linear axis moves the other
//! way. The reader states it (`"direction": 1 | -1`, standard when absent)
//! and refuses anything else by name; the travel check judges the stroke
//! in the machine's own coordinates; the project file round-trips it; and
//! the post (#374) writes an inverted X / Y mirrored while an inverted Z
//! refuses posting by name - never standard-sign code for it.
//!
//! Fixture law (clean-room): every value below is our own invention.
//!
//! Gate map (M4.4): covers app::machine, post::machine, app::posting, app::fixture, sim::fixture, app::project, app::commands, post::machine.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock, SetWcsOrigin,
};
use camcraft_app::geometry::{IDENTITY_Q, Point3, cube};
use camcraft_app::machine::{self, AxisDirection, AxisLimits, Machine, check_document, parse_mch};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{fixture, posting, project};

/// A file-form machine whose X part carries `direction` as written.
fn mch_with_x_direction(direction: &str) -> String {
    format!(
        r#"{{
      "general": {{"model": "Flip 1", "vendor": "CamCraft Testbench", "kind": "mill"}},
      "kinematics": {{"default": {{"units": {{"length": "mm"}}, "parts": [
        {{"type": "linear", "name": "X", "direction": {direction}, "travel": {{"min": -10, "max": 300}},
          "parts": [
            {{"type": "linear", "name": "Y", "travel": {{"min": -100, "max": 100}}}},
            {{"type": "linear", "name": "Z", "direction": 1}}
          ]}}
      ]}}}}
    }}"#
    )
}

fn axis(name: &str, travel: (f64, f64), direction: AxisDirection) -> AxisLimits {
    AxisLimits { name: name.into(), travel: Some(travel), direction, ..Default::default() }
}

/// A ±100 mill whose X is inverted (symmetric, so no stroke problem).
fn flipped() -> Machine {
    Machine {
        model: "Flip 1".into(),
        vendor: "CamCraft Testbench".into(),
        axes: vec![
            axis("X", (-100.0, 100.0), AxisDirection::Inverted),
            axis("Y", (-100.0, 100.0), AxisDirection::Standard),
            axis("Z", (-100.0, 100.0), AxisDirection::Standard),
        ],
        ..Default::default()
    }
}

/// One Face op on a 60x60 box at the origin (the machine suite's shape).
fn faced_doc() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let loc = Location { pos: Point3::new(0.0, 0.0, 0.0), rot: IDENTITY_Q };
    let cmd = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, loc);
    history.execute(&world, cmd.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog tool");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("add tool"));
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::new(-30.0, -30.0, 0.0), Point3::new(30.0, 30.0, 18.0))),
    );
    let params = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(18.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params);
    history.execute(&world, cmd.expect("create face"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate face"));
    (world, history)
}

// ---- the reader ----

#[test]
fn direction_reads_plus_one_minus_one_and_absent() {
    let m = parse_mch(&mch_with_x_direction("-1")).expect("inverted X reads");
    let dir = |n: &str| m.axes.iter().find(|a| a.name == n).map(|a| a.direction);
    assert_eq!(dir("X"), Some(AxisDirection::Inverted));
    assert_eq!(dir("Y"), Some(AxisDirection::Standard), "absent is standard");
    assert_eq!(dir("Z"), Some(AxisDirection::Standard), "1 is standard");
    assert_eq!(m.inverted_axes(), vec!["X"]);
    let m = parse_mch(&mch_with_x_direction("1.0")).expect("1.0 is +1");
    assert!(m.inverted_axes().is_empty());
    let m = parse_mch(&mch_with_x_direction("null")).expect("null is unstated");
    assert!(m.inverted_axes().is_empty());
}

#[test]
fn any_other_direction_refuses_by_name() {
    for bad in ["0", "2", "-2", "0.5", "\"-1\"", "\"left\"", "[1, 0, 0]", "true", "{}"] {
        let e = parse_mch(&mch_with_x_direction(bad)).expect_err(bad);
        assert!(e.contains("axis X") && e.contains("+1 or -1"), "{bad}: {e}");
    }
}

#[test]
fn a_reversed_rotary_refuses_and_other_rotary_shapes_are_not_read() {
    let rot = |d: &str| {
        format!(
            r#"{{"general": {{}}, "kinematics": {{"default": {{"parts": [
              {{"type": "linear", "name": "X"}},
              {{"type": "rotary", "name": "C", "axis": [0, 0, 1], "direction": {d}}}
            ]}}}}}}"#
        )
    };
    let e = parse_mch(&rot("-1")).expect_err("reversed rotary");
    assert!(e.contains("rotary axis C") && e.contains("not supported"), "{e}");
    for fine in ["1", "[0, 0, 1]"] {
        let m = parse_mch(&rot(fine)).expect(fine);
        assert!(m.inverted_axes().is_empty(), "{fine}");
    }
}

#[test]
fn every_shipped_machine_stays_standard() {
    for (name, text) in posting::MACHINES {
        let m = parse_mch(text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(m.inverted_axes().is_empty(), "{name} states no inverted axis");
        assert_eq!(m.inverted_post_refusal(), None, "{name}");
        let json = serde_json::to_string(&m).expect("serialize");
        assert!(!json.contains("\"direction\""), "{name}: a standard axis writes no key: {json}");
        // The post grammar the scripts read is untouched by #373.
        assert!(!post_mirror(&m).contains("Inverted"), "{name}");
    }
}

fn post_mirror(m: &Machine) -> String {
    camcraft_app::post::machine::mirror(m).to_string()
}

// ---- the model (project-file serde) ----

#[test]
fn direction_round_trips_through_serde_and_refuses_garbage() {
    let m = flipped();
    let json = serde_json::to_string(&m).expect("serialize");
    assert!(json.contains("\"direction\":-1"), "{json}");
    let back: Machine = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, m);
    let bad = json.replace("\"direction\":-1", "\"direction\":3");
    let e = serde_json::from_str::<Machine>(&bad).expect_err("3 is no direction");
    assert!(e.to_string().contains("+1 or -1"), "{e}");
}

#[test]
fn an_inverted_machine_survives_a_project_save_and_load() {
    let world = new_world();
    let cmd = LoadMachine::new(&world, "flip.mch".into(), Some(flipped())).expect("sane");
    History::default().execute(&world, cmd);
    let p = std::env::temp_dir().join(format!("camcraft_axisdir_{}.camproj", std::process::id()));
    project::save(&world, &p).expect("save");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    let _ = std::fs::remove_file(&p);
    assert_eq!(machine::of(&world2).1, Some(flipped()), "the inverted X survives");
}

#[test]
fn loading_an_inverted_machine_undoes() {
    let world = new_world();
    let mut history = History::default();
    let cmd = LoadMachine::new(&world, "flip.mch".into(), Some(flipped())).expect("sane");
    history.execute(&world, cmd);
    assert_eq!(machine::of(&world).1, Some(flipped()));
    assert!(history.undo(&world));
    assert_eq!(machine::of(&world).1, None, "undo puts the previous machine back");
}

// ---- the travel check ----

#[test]
fn travel_is_judged_in_the_machines_own_coordinates() {
    // Slide the WCS so the program's X sits wholly positive (about 68..132):
    // a standard X with stroke 0..200 holds it; the same stroke on an
    // inverted X reads it as -132..-68 and refuses, naming the inversion.
    let (world, mut history) = faced_doc();
    let slid = Location { pos: Point3::new(-100.0, 0.0, 0.0), rot: IDENTITY_Q };
    history.execute(&world, SetWcsOrigin::new(&world, slid).expect("slide the WCS"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    let with_x = |travel, direction| Machine {
        model: "Flip 1".into(),
        axes: vec![axis("X", travel, direction)],
        ..Default::default()
    };
    let ok = check_document(&world, &with_x((0.0, 200.0), AxisDirection::Standard)).expect("check");
    assert!(ok.is_ok(), "{:?}", ok.problems);
    let c = check_document(&world, &with_x((0.0, 200.0), AxisDirection::Inverted)).expect("check");
    assert!(!c.is_ok(), "the inverted X reads the program negative");
    assert!(c.problems.iter().all(|p| p.contains("in X (inverted axis)") && p.contains("reaches -")), "{:?}", c.problems);
    let ok = check_document(&world, &with_x((-200.0, 0.0), AxisDirection::Inverted)).expect("check");
    assert!(ok.is_ok(), "the mirrored stroke holds it: {:?}", ok.problems);
    let c = check_document(&world, &with_x((-200.0, 0.0), AxisDirection::Standard)).expect("check");
    assert!(!c.is_ok(), "and a standard X with that stroke does not");
}

#[test]
fn the_table_box_maps_an_inverted_stroke() {
    let world = new_world();
    let mut m = flipped();
    m.axes[0].travel = Some((-10.0, 300.0));
    let cmd = LoadMachine::new(&world, "flip.mch".into(), Some(m)).expect("sane");
    History::default().execute(&world, cmd);
    let b = fixture::table_box(&world, None).expect("all three strokes stated");
    assert_eq!((b.min.x, b.max.x), (-300.0, 10.0), "X -10..300 on the machine is -300..10 in the program");
    assert_eq!((b.min.y, b.max.y), (-100.0, 100.0), "standard Y unchanged");
}

// ---- the post (#374): an inverted X / Y translates, an inverted Z refuses ----

const Z_REFUSAL: &str = "machine Flip 1 has an inverted Z axis; posting for an inverted Z is not supported yet - tool-length compensation (G43) and the canned cycles' retract assume the controller's +Z leaves the work";

fn post_text(world: &World, tag: &str) -> Result<String, String> {
    let out = std::env::temp_dir().join(format!("camcraft_axisdir_{tag}_{}.nc", std::process::id()));
    let r = posting::post_setup(world, &out).map(|p| p.program);
    let _ = std::fs::remove_file(&out);
    r
}

/// One posted line with every X word negated and G2/G3 swapped - what
/// the standard program's line becomes on a machine whose X is inverted.
fn mirrored_in_x(line: &str) -> String {
    line.split(' ')
        .map(|w| match w {
            "G02" => "G03".to_owned(),
            "G03" => "G02".to_owned(),
            "G2" => "G3".to_owned(),
            "G3" => "G2".to_owned(),
            _ if (w.starts_with('X') || w.starts_with('I')) && w[1..].parse::<f64>().is_ok_and(|v| v != 0.0) => {
                let body = &w[1..];
                let flipped = body.strip_prefix('-').map_or_else(|| format!("-{body}"), str::to_owned);
                format!("{}{flipped}", &w[..1])
            }
            _ => w.to_owned(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn an_inverted_x_machine_posts_the_mirrored_program() {
    let (world, mut history) = faced_doc();
    let mut std_m = flipped();
    std_m.axes[0].direction = AxisDirection::Standard;
    history.execute(&world, LoadMachine::new(&world, "std.mch".into(), Some(std_m)).expect("sane"));
    let standard = post_text(&world, "std").expect("a standard machine posts");
    history.execute(&world, LoadMachine::new(&world, "flip.mch".into(), Some(flipped())).expect("sane"));
    let gate = posting::verification_gate(&world);
    assert!(!gate.iter().any(|p| p.contains("inverted")), "an inverted X is translated, not refused: {gate:?}");
    let inverted = post_text(&world, "flip").expect("an inverted X posts");
    assert_ne!(standard, inverted, "the inverted program is not the standard one");
    let a: Vec<&str> = standard.lines().collect();
    // #375: the header states the inverted axis - the one added line.
    let note = "(MACHINE X+ MOVES LEFT - INVERTED AXIS)";
    assert_eq!(inverted.lines().filter(|l| *l == note).count(), 1, "the header note:\n{inverted}");
    let b: Vec<&str> = inverted.lines().filter(|l| *l != note).collect();
    assert_eq!(a.len(), b.len(), "the same blocks");
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(mirrored_in_x(x), *y, "every X word negated, G2/G3 swapped");
    }
}

#[test]
fn an_inverted_z_refuses_posting_by_name() {
    let (world, mut history) = faced_doc();
    let mut m = flipped();
    m.axes[0].direction = AxisDirection::Standard;
    m.axes[2].direction = AxisDirection::Inverted;
    history.execute(&world, LoadMachine::new(&world, "flipz.mch".into(), Some(m)).expect("sane"));
    let gate = posting::verification_gate(&world);
    assert!(gate.iter().any(|p| p == Z_REFUSAL), "{gate:?}");
    let e = post_text(&world, "flipz").expect_err("never standard-sign code");
    assert!(e.contains(Z_REFUSAL), "{e}");
}

#[test]
fn the_refusal_names_every_inverted_axis() {
    let mut m = flipped();
    assert_eq!(m.inverted_post_refusal(), None, "an inverted X alone is translated");
    m.axes[2].direction = AxisDirection::Inverted;
    assert_eq!(
        m.inverted_post_refusal().as_deref(),
        Some(
            "machine Flip 1 has inverted X and Z axes; posting for an inverted Z is not supported yet - tool-length compensation (G43) and the canned cycles' retract assume the controller's +Z leaves the work"
        )
    );
}
