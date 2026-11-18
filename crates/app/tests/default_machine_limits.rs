//! #2321: a document with no mounted machine posts with the default
//! machine file (`assets/machines/3axis-vmc.mch`), so the program gate
//! judges it against the limits that file states - the spindle range and
//! each axis's feed rate. Before the fix the gate read only a mounted
//! machine, and an op above the default machine's 12000 rpm posted.
//! A mounted machine is judged by its own file only: an op that the
//! default machine refuses posts on a mounted machine that states no
//! spindle limit, exactly as before.
//! Provenance: our own fixtures over our own commands; the limits are
//! read from our own shipped `3axis-vmc.mch`.
//!
//! Gate map: covers app::posting (default_limits, machine_problems,
//! verification_report, post_setup, post_mounts).

use camcraft_app::commands::{AddTool, CreateOperation, CreateSetup, GenerateOperation, History, SetSetupMachine, SetStock};
use camcraft_app::machine::Machine;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;
use serde_json::json;

fn face(feed: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// Box stock, one tool at `rpm` and one facing op per entry of `ops`
/// (name, feed, its setup's machine). With `setups` false the document
/// has no setup at all - the plainest document.
fn doc(rpm: f64, setups: bool, ops: &[(&str, f64, Option<Machine>)]) -> World {
    let world = new_world();
    let mut h = History::default();
    let mut tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog entry");
    tool.rpm = rpm;
    h.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("tool"));
    for (k, (name, feed, machine)) in ops.iter().enumerate() {
        if setups {
            h.execute(&world, CreateSetup::new(&world, None, None).expect("setup"));
        }
        if k == 0 || setups {
            let cmd = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
            h.execute(&world, cmd.expect("box stock"));
        }
        if let Some(m) = machine {
            let setup = format!("Setup{}", k + 1);
            let cmd = SetSetupMachine::new(&world, &setup, Some(("bench-mill.mch".into(), Some(m.clone()))));
            h.execute(&world, cmd.expect("mount"));
        }
        h.execute(&world, CreateOperation::new(&world, name, "EM-0400-2FL", SrcRef::Stock, face(*feed)).expect("create"));
        h.execute(&world, GenerateOperation::new(&world, name).expect("generate"));
    }
    world
}

/// A mill that states nothing: no spindle range, no feed rates.
fn open_mill() -> Machine {
    Machine { model: "Bench mill".into(), kind: Some("mill".into()), ..Default::default() }
}

fn tmp(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-2321-{tag}-{}", std::process::id()))
}

/// Post into a scratch file: the program, or the refusal - and a refusal
/// writes nothing.
fn post(world: &World, tag: &str) -> Result<String, String> {
    let path = tmp(tag).with_extension("nc");
    let _ = std::fs::remove_file(&path);
    let out = posting::post_setup(world, &path).map(|p| p.program);
    if out.is_err() {
        assert!(!path.exists(), "{tag}: a refused post writes nothing");
    }
    let _ = std::fs::remove_file(&path);
    out
}

#[test]
fn no_mounted_machine_is_refused_above_the_default_spindle_maximum() {
    let world = doc(15000.0, false, &[("Face1", 1200.0, None)]);
    let err = post(&world, "rpm").expect_err("15000 rpm is above the default machine's 12000");
    assert!(err.contains("default machine VMC 3-AXIS"), "the refusal names the default machine: {err}");
    assert!(
        err.contains("Face1: tool EM-0400-2FL runs 15000 rpm, above the machine's 12000"),
        "the refusal names the op and the limit: {err}"
    );
    assert!(err.contains("mount a machine on the setup"), "the refusal says what to do: {err}");
}

#[test]
fn no_mounted_machine_is_refused_above_the_default_feed_rate() {
    let world = doc(8000.0, false, &[("Face1", 7000.0, None)]);
    let err = post(&world, "feed").expect_err("7000 mm/min is above the default machine's 5000");
    assert!(err.contains("default machine VMC 3-AXIS"), "the refusal names the default machine: {err}");
    assert!(err.contains("Face1: move") && err.contains("feeds 7000 mm/min"), "the refusal names the move: {err}");
    assert!(err.contains("tops out at 5000"), "the refusal names the limit: {err}");
}

#[test]
fn no_mounted_machine_within_the_default_limits_posts() {
    let world = doc(8000.0, false, &[("Face1", 1200.0, None)]);
    let program = post(&world, "ok").expect("within the default machine's limits");
    assert!(program.contains("S8000"), "the program posts the op's speed:\n{program}");
    assert!(posting::verification_report(&world).problems.is_empty());
}

#[test]
fn a_mounted_machine_is_judged_by_its_own_file_only() {
    // The open mill states no spindle maximum: 15000 rpm and 7000 mm/min
    // post there, as before #2321, although the default machine refuses
    // both.
    let world = doc(15000.0, true, &[("Face1", 7000.0, Some(open_mill()))]);
    let program = post(&world, "mounted").expect("a mounted machine that states no limit");
    assert!(program.contains("S15000"), "the program posts the op's speed:\n{program}");
    assert!(posting::verification_report(&world).problems.is_empty());
}

#[test]
fn only_the_mount_with_no_machine_is_judged_against_the_default() {
    // Setup1 mounts the open mill; Setup2 mounts none. Only Face2 is
    // named, by the default machine.
    let world = doc(15000.0, true, &[("Face1", 1200.0, Some(open_mill())), ("Face2", 1200.0, None)]);
    let problems = posting::verification_report(&world).problems.join("; ");
    assert!(problems.contains("default machine VMC 3-AXIS"), "{problems}");
    assert!(problems.contains("Face2: tool EM-0400-2FL runs 15000 rpm"), "Face2 is judged: {problems}");
    assert!(!problems.contains("Face1:"), "Face1 is on a mounted machine: {problems}");
    let dir = tmp("mounts");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch folder");
    let err = posting::post_mounts(&world, &dir.join("job.nc")).expect_err("one file per mount is refused");
    let written = std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(err.contains("Face2: tool EM-0400-2FL runs 15000 rpm"), "{err}");
    assert_eq!(written, 0, "a refusal writes no file");
}

#[test]
fn the_default_limits_are_read_from_the_post_grammar() {
    let text = posting::default_mch_text().expect("the shipped default machine");
    let v: serde_json::Value = serde_json::from_str(&text).expect("json");
    let m = posting::default_limits(&v);
    assert_eq!(m.model, "VMC 3-AXIS");
    assert_eq!(m.spindle_max, Some(12000.0));
    assert_eq!(m.spindle_min, None, "a zero minimum is unstated");
    let feeds: Vec<(String, Option<f64>, Option<f64>)> = m.axes.iter().map(|a| (a.name.clone(), a.feed, a.rapid)).collect();
    assert_eq!(
        feeds,
        vec![
            ("X".to_owned(), Some(5000.0), Some(6000.0)),
            ("Y".to_owned(), Some(5000.0), Some(6000.0)),
            ("Z".to_owned(), Some(3000.0), Some(4000.0)),
        ]
    );
    // Zero rates are unstated and check nothing (the unstated-limit law).
    let zeros = json!({"name": "Z", "kinematics": {"static": {
        "head": {"spindle": {"min": 0.0, "max": 0.0}},
        "axes": [{"letter": "X", "speeds": {"rapid": 0.0, "normal": 0.0}}]}}});
    let m = posting::default_limits(&zeros);
    assert_eq!((m.spindle_min, m.spindle_max), (None, None));
    assert_eq!((m.axes[0].feed, m.axes[0].rapid), (None, None));
}
