//! #669: the plasma machine kind and the Tormach 1300PL. A `.mch` that
//! declares `"kind": "plasma"` refuses spindle, coolant and rotary keys
//! by name at both doors (reader and validator); the 1300PL ships with
//! its manual's figures; the machine check judges XY travel and feed,
//! skips Z depth and rpm with a named note, and refuses a milling op on
//! a plasma table by name; a setup on the 1300PL round-trips.
//! Provenance: our own tests over our own machine files; the 1300PL
//! figures are Tormach's public 1300PL Operator's Manual UM10720 v1223A
//! §2.2.1 p.17 (travel; the 03/24 spec sheet's smaller X/Y taken - see
//! docs/decisions/669-plasma-kind.md) and p.16 (feeds). Fixture values
//! in the crafted files are our own.
//!
//! Gate map (#669): covers post::machine (kind, plasma, parse),
//! app::machine, post::machine (check_ops plasma branch, validate, draft), app::posting
//! (MACHINES, post_serves, post_offer), app::commands (family gate),
//! app::project (machine round-trip).

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel, LoadMachine, SetSetupMachine,
    SetStock,
};
use camcraft_app::geometry::{cube, Point3, IDENTITY_Q};
use camcraft_app::machine::validate::validate_text;
use camcraft_app::machine::{self, check_document, kind, parse_mch, Kind, Machine};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting::{self, MACHINES};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

const IN: f64 = 25.4;
const TOOL: &str = "EM-0400-2FL";

/// A plasma table in the `.mch` layout, our own numbers; `patch` breaks it.
fn plasma(patch: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut v = serde_json::json!({
      "general": { "model": "TEST PLASMA", "vendor": "Testbench", "kind": "plasma" },
      "kinematics": { "default": {
        "units": { "length": "mm", "angle": "degrees" },
        "parts": [ { "id": "base", "name": "table", "type": "table", "parts": [
          { "id": "z", "name": "Z", "type": "linear", "travel": { "min": -80.0, "max": 80.0 }, "parts": [
            { "id": "y", "name": "Y", "type": "linear", "travel": { "min": -600.0, "max": 600.0 }, "parts": [
              { "id": "x", "name": "X", "type": "linear", "travel": { "min": -600.0, "max": 600.0 } }
            ] }
          ] },
          { "id": "torch", "name": "torch", "type": "head" }
        ] } ]
      } },
      "controller": { "synced_configuration": { "parts": {
        "X": { "coordinate": 0, "max_normal_speed": 20000.0 },
        "Y": { "coordinate": 1, "max_normal_speed": 20000.0 },
        "Z": { "coordinate": 2, "max_normal_speed": 3000.0 }
      } } }
    });
    patch(&mut v);
    v.to_string()
}

const HEAD: &str = "/kinematics/default/parts/0/parts/1";

fn keys(text: &str) -> Vec<String> {
    validate_text(text).into_iter().map(|r| r.key).collect()
}

#[test]
fn a_plasma_file_stating_a_spindle_is_refused_by_name() {
    let clean = plasma(|_| {});
    assert!(validate_text(&clean).is_empty(), "{:?}", validate_text(&clean));
    let m = parse_mch(&clean).expect("a clean plasma table reads");
    assert_eq!(kind(Some(&m)), Kind::Plasma);
    assert!(m.is_sane() && m.plasma_sane());

    let spun = plasma(|v| {
        v.pointer_mut(HEAD).expect("head")["spindle"] = serde_json::json!({ "max_speed": 6000.0, "power": 2.0 });
    });
    let err = parse_mch(&spun).expect_err("a plasma table with a spindle is malformed");
    assert!(err.contains("spindle.") && err.contains("no spindle"), "{err}");
    let k = keys(&spun);
    assert!(k.contains(&"spindle.max_speed".to_owned()) && k.contains(&"spindle.power".to_owned()), "{k:?}");

    let cooled = plasma(|v| {
        v.pointer_mut(HEAD).expect("head")["tool_station"] = serde_json::json!({ "coolants": [ { "name": "Flood", "mCode": "M8" } ] });
    });
    assert!(parse_mch(&cooled).expect_err("coolant refuses").contains("coolants"));
    assert!(keys(&cooled).contains(&"coolants".to_owned()));

    let turned = plasma(|v| {
        v["kinematics"]["default"]["parts"][0]["parts"]
            .as_array_mut()
            .expect("parts")
            .push(serde_json::json!({ "id": "a", "name": "A", "type": "rotary", "axis": [1.0, 0.0, 0.0] }));
    });
    assert!(parse_mch(&turned).expect_err("a rotary refuses").contains("A: a plasma table"));
    assert!(keys(&turned).contains(&"A".to_owned()));

    // Z is the torch lifter: a plasma table without one is incomplete.
    let no_z = plasma(|v| {
        let x = v.pointer("/kinematics/default/parts/0/parts/0/parts/0/parts/0").cloned().expect("x");
        let y = serde_json::json!({ "id": "y", "name": "Y", "type": "linear", "parts": [x] });
        v["kinematics"]["default"]["parts"][0]["parts"][0] = y;
    });
    assert!(keys(&no_z).contains(&"Z axis".to_owned()), "{:?}", validate_text(&no_z));

    // The parsed door: a hand-edited project machine that declares
    // plasma and states an rpm is not a machine this app can check.
    let insane = Machine { spindle_max: Some(6000.0), ..m };
    assert!(!insane.is_sane());
}

#[test]
fn an_unknown_kind_is_still_refused() {
    let t = plasma(|v| v["general"]["kind"] = "laser".into());
    let err = parse_mch(&t).expect_err("an unknown kind refuses");
    assert!(err.contains("not mill, lathe, mill-turn, plasma, wire_edm or sinker_edm"), "{err}");
    assert!(keys(&t).contains(&"general.kind".to_owned()));
    // Plasma is declared, never inferred: the same axes undeclared are a mill.
    let undeclared = plasma(|v| v["general"]["kind"] = serde_json::Value::Null);
    assert_eq!(kind(Some(&parse_mch(&undeclared).expect("reads"))), Kind::Mill);
    assert_eq!(Kind::from_word("Plasma"), Some(Kind::Plasma));
    assert_eq!(Kind::Plasma.word(), "plasma");
}

fn shipped_1300pl() -> Machine {
    let text = MACHINES
        .iter()
        .find(|(n, _)| *n == "Tormach 1300PL")
        .map(|(_, t)| *t)
        .expect("the 1300PL ships");
    parse_mch(text).expect("the 1300PL reads")
}

fn close(a: Option<f64>, b: f64) -> bool {
    a.is_some_and(|a| (a - b).abs() < 1e-6)
}

#[test]
fn the_1300pl_states_its_manuals_figures() {
    let m = shipped_1300pl();
    assert_eq!(kind(Some(&m)), Kind::Plasma);
    assert_eq!(m.units.as_deref(), Some("inch"));
    assert!(m.is_sane());
    // Travel: the smaller of UM10720 §2.2.1 (X 50 / Y 51.5) and the spec
    // sheet (X 48.5 / Y 50); Z 3.9 in both. Stated -L..L (#457).
    for (axis, l) in [("X", 48.5), ("Y", 50.0), ("Z", 3.9)] {
        let a = m.axes.iter().find(|a| a.name == axis).expect("axis");
        let (lo, hi) = a.travel.expect("travel");
        assert!(close(Some(hi), l * IN) && close(Some(lo), -l * IN), "{axis}: {lo}..{hi}");
        assert_eq!(a.rapid, None, "{axis}: no rapid figure is published");
        assert!(!a.rotary);
    }
    // Feeds: 1000 IPM X/Y, 120 IPM Z (UM10720 p.16).
    assert!(close(m.axis(0).and_then(|a| a.feed), 1000.0 * IN));
    assert!(close(m.axis(1).and_then(|a| a.feed), 1000.0 * IN));
    assert!(close(m.axis(2).and_then(|a| a.feed), 120.0 * IN));
    // No spindle, no coolant: the torch is Tool 1, the only one.
    assert_eq!((m.spindle_min, m.spindle_max, m.spindle_power, m.spindles), (None, None, None, None));
    assert!(m.coolants.is_empty() && m.spindle_ranges.is_empty());
    assert_eq!(m.tool_count, Some(1));
    assert_eq!(m.safe_retract_z, None, "the plasma post retracts with G30 (#671)");
    assert!(m.summary().contains("torch, no spindle"), "{}", m.summary());
    // No cutting post serves it; the plasma post (#671) alone does.
    for (n, _) in posting::BUILTINS {
        assert_eq!(posting::post_serves(n, Kind::Plasma), posting::PLASMA_POSTS.contains(n), "{n}");
    }
    assert_eq!(posting::PLASMA_POSTS, &["Tormach PathPilot Plasma"], "#671 ships the first plasma post");
}

/// A faced document (one mill Face op, generated) - no machine mounted.
fn faced_doc() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let loc = Location { pos: Point3::new(0.0, 0.0, 0.0), rot: IDENTITY_Q };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, loc).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("part", cube(Point3::new(-30.0, -30.0, 0.0), Point3::new(30.0, 30.0, 18.0))));
    h.execute(&world, CreateOperation::new(&world, "Face1", TOOL, SrcRef::Stock, face()).expect("face"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    (world, h)
}

fn face() -> OpParams {
    OpParams::Face {
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
    }
}

#[test]
fn the_plasma_check_skips_depth_and_rpm_with_a_note_and_judges_xy() {
    let (world, _h) = faced_doc();
    let c = check_document(&world, &shipped_1300pl()).expect("checks");
    assert_eq!(c.notes.len(), 2, "{:?}", c.notes);
    assert!(c.notes.iter().any(|n| n.contains("Z is the torch lifter") && n.contains("not judged")));
    // #672: the note names where the through-cut IS judged.
    assert!(c.notes.iter().any(|n| n.contains("plasma laws at the post gate") && n.contains("cut order") && n.contains("stock sim does not run")), "{:?}", c.notes);
    assert!(c.notes.iter().any(|n| n.contains("no spindle") && n.contains("rpm")));
    assert!(c.problems.iter().any(|p| p.contains("Face1: a Face operation is milling work - a plasma table cuts only plasma operations")), "{:?}", c.problems);
    assert!(c.problems.iter().any(|p| p.contains("rpm - a plasma table has no spindle")), "{:?}", c.problems);
    assert!(!c.problems.iter().any(|p| p.contains("above the machine") || p.contains("spindle power")), "{:?}", c.problems);

    // Z: a lifter stroke the face path leaves, and a crawling Z ceiling,
    // are never judged - XY is: a narrow X stroke and a slow XY ceiling are.
    let mut m = shipped_1300pl();
    for a in &mut m.axes {
        match a.name.as_str() {
            "Z" => (a.travel, a.feed) = (Some((-0.1, 0.1)), Some(1.0)),
            "X" => (a.travel, a.feed) = (Some((-1.0, 1.0)), Some(100.0)),
            _ => a.feed = Some(100.0),
        }
    }
    let c = check_document(&world, &m).expect("checks");
    assert!(!c.problems.iter().any(|p| p.contains(" in Z,")), "Z travel is the lifter's: {:?}", c.problems);
    assert!(c.problems.iter().any(|p| p.contains(" in X,")), "X travel is judged: {:?}", c.problems);
    assert!(c.problems.iter().any(|p| p.contains("feeds 1200 mm/min") && p.contains("tops out at 100")), "{:?}", c.problems);
    assert!(!c.problems.iter().any(|p| p.contains("Z tops out")), "Z feed is the lifter's: {:?}", c.problems);
    // A mill keeps its own check: no notes, nothing about plasma.
    let mut mill = m.clone();
    mill.kind = Some("mill".into());
    let c = check_document(&world, &mill).expect("checks");
    assert!(c.notes.is_empty() && !c.problems.iter().any(|p| p.contains("plasma")), "{c:?}");
    // ...and judges the Z the plasma check skipped - the skip is not vacuous.
    assert!(c.problems.iter().any(|p| p.contains(" in Z,")), "{:?}", c.problems);
    assert!(c.problems.iter().any(|p| p.contains("Z tops out")), "{:?}", c.problems);
}

#[test]
fn a_mill_op_on_the_1300pl_is_refused_by_name() {
    // At the door: a new milling op on a plasma table never exists.
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    let loc = Location { pos: Point3::new(0.0, 0.0, 0.0), rot: IDENTITY_Q };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, loc).expect("stock"));
    h.execute(&world, LoadMachine::builtin(&world, "Tormach 1300PL").expect("mounts"));
    assert_eq!(machine::kind_of(&world), Kind::Plasma);
    let err = CreateOperation::new(&world, "Face1", TOOL, SrcRef::Stock, face()).err().expect("refused");
    assert!(format!("{err:?}").contains("plasma table, which cuts only plasma operations"), "{err:?}");
    // #670: the plasma table's ribbon shows the Plasma section alone.
    assert_eq!(camcraft_app::operation::shown_for(Kind::Plasma), vec![camcraft_app::operation::Section::Plasma]);

    // At the gate: a mill op the table was mounted under after the fact
    // blocks the post, named.
    let (world, mut h) = faced_doc();
    h.execute(&world, LoadMachine::builtin(&world, "Tormach 1300PL").expect("mounts"));
    let report = posting::verification_report(&world);
    assert!(
        report.problems.iter().any(|p| p.contains("plasma table cuts only plasma operations")),
        "{:?}",
        report.problems
    );
}

#[test]
fn a_setup_on_the_1300pl_round_trips() {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    h.execute(&world, SetSetupMachine::builtin(&world, "Setup1", "Tormach 1300PL").expect("mounts"));
    let (path, m) = machine::of_setup(&world, Some("Setup1"));
    assert_eq!(path, posting::builtin_path("Tormach 1300PL"));
    assert_eq!(m.as_ref().map(|m| kind(Some(m))), Some(Kind::Plasma));

    let p = std::env::temp_dir().join(format!("camcraft_plasma_rt_{}.ccam", std::process::id()));
    camcraft_app::project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    let again = new_world();
    camcraft_app::project::load(&again, &p).expect("load");
    assert_eq!(machine::of_setup(&again, Some("Setup1")), (path, m));
    camcraft_app::project::save(&again, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert!(first.contains("\"plasma\""), "the declared kind is written: {first}");
    assert_eq!(first, second, "byte-identical");

    // A hand-edited plasma setup that grew an rpm is a corrupt file.
    let bad = first.replacen("\"kind\":\"plasma\"", "\"kind\":\"plasma\",\"spindle_max\":6000.0", 1);
    let bad = if bad == first { first.replacen("\"kind\": \"plasma\"", "\"kind\": \"plasma\", \"spindle_max\": 6000.0", 1) } else { bad };
    assert_ne!(bad, first, "the test must reach the kind spelling");
    assert!(camcraft_app::project::open(&new_world(), &bad).is_err(), "an rpm on a plasma table refuses the file");
}
