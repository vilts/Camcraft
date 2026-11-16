//! Showing an inverted machine axis (#375): the setup sheet states each
//! inverted axis of the machine its setup mounts, the posts that
//! translate an inverted axis (Fanuc, Haas) write it in the program
//! header, and a standard machine adds no line anywhere.
//!
//! Provenance: our own tests over ISO 841's right-handed program frame
//! and RS-274 comment syntax; fixture values are our own invention.
//!
//! The posted stream is post's inverted-axis fixture, recorded
//! (`crates/post/tests/recorded/inputs/inverted_probe.cnc`, written and
//! guarded by post's `inverted_post`, #926).
//!
//! Gate map (M4.4): covers app::setup_sheet, machine::direction, post::machine, assets/posts.

use camcraft_app::commands::{CreateSetup, History, LoadMachine, SetSetupMachine};
use camcraft_app::machine::{self, AxisDirection, AxisLimits, Machine};
use camcraft_app::post::engine;
use camcraft_app::setup_sheet;
use camcraft_app::world::new_world;
use std::path::{Path, PathBuf};

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo(rel)).unwrap_or_else(|e| panic!("{rel}: {e}")).replace("\r\n", "\n")
}

fn cps(stem: &str) -> String {
    read(&format!("assets/posts/{stem}.cps"))
}

/// The shipped 3-axis machine, stating `letters` inverted the way the
/// mirror does (`invertedAxes`).
fn mch(letters: &[&str]) -> serde_json::Value {
    let mut m: serde_json::Value = serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json");
    if !letters.is_empty()
        && let Some(o) = m.as_object_mut()
    {
        o.insert("invertedAxes".into(), serde_json::json!(letters));
    }
    m
}

/// The inverted-axis fixture stream (lines, arcs in all three planes,
/// cutter compensation, a drilled hole, a work-offset origin).
fn probe_stream() -> String {
    read("crates/post/tests/recorded/inputs/inverted_probe.cnc")
}

fn mill(inverted: &[&str]) -> Machine {
    let axis = |n: &str| AxisLimits {
        name: n.into(),
        travel: Some((-100.0, 100.0)),
        direction: if inverted.contains(&n) { AxisDirection::Inverted } else { AxisDirection::Standard },
        ..Default::default()
    };
    Machine { model: "Flip 1".into(), axes: vec![axis("X"), axis("Y"), axis("Z")], ..Default::default() }
}

fn sheet_for(inverted: &[&str]) -> String {
    let world = new_world();
    let mut history = History::default();
    let load = LoadMachine::new(&world, "flip.mch".into(), Some(mill(inverted))).expect("load");
    history.execute(&world, load);
    setup_sheet::render(&world, None).expect("sheet")
}

#[test]
fn the_setup_sheet_states_each_inverted_axis() {
    let sheet = sheet_for(&["X"]);
    assert!(sheet.contains("MACHINE AXES\n"), "{sheet}");
    assert!(
        sheet.contains("  Machine X+ moves left (inverted axis) - the program is mirrored in X for this machine\n"),
        "{sheet}"
    );
    let both = sheet_for(&["X", "Y"]);
    assert!(both.contains("Machine X+ moves left (inverted axis)"), "{both}");
    assert!(both.contains("Machine Y+ moves toward the front (inverted axis) - the program is mirrored in Y"), "{both}");
    assert_eq!(both.matches("(inverted axis)").count(), 2, "one line per axis:\n{both}");
}

#[test]
fn a_standard_machine_adds_no_sheet_line() {
    let std_sheet = sheet_for(&[]);
    assert!(!std_sheet.contains("MACHINE AXES"), "{std_sheet}");
    assert!(!std_sheet.contains("inverted"), "{std_sheet}");
    let none = setup_sheet::render(&new_world(), None).expect("sheet");
    assert!(!none.contains("MACHINE AXES"), "no machine, no section:\n{none}");
    // The section is the only difference an inverted X makes to a sheet.
    let inv = sheet_for(&["X"]);
    let stripped = inv.replace(
        "MACHINE AXES\n  Machine X+ moves left (inverted axis) - the program is mirrored in X for this machine\n\n",
        "",
    );
    assert_eq!(stripped, std_sheet);
}

#[test]
fn the_sheet_reads_the_machine_its_own_setup_mounts() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, LoadMachine::new(&world, "std.mch".into(), Some(mill(&[]))).expect("load"));
    for _ in 0..2 {
        history.execute(&world, CreateSetup::new(&world, None, None).expect("setup"));
    }
    let own = SetSetupMachine::new(&world, "Setup2", Some(("flip.mch".into(), Some(mill(&["X"])))));
    history.execute(&world, own.expect("mount"));
    let s1 = setup_sheet::render(&world, Some("Setup1")).expect("sheet 1");
    let s2 = setup_sheet::render(&world, Some("Setup2")).expect("sheet 2");
    assert!(!s1.contains("MACHINE AXES"), "Setup1 follows the standard document machine:\n{s1}");
    assert!(s2.contains("Machine X+ moves left (inverted axis)"), "Setup2 mounts the inverted one:\n{s2}");
}

#[test]
fn the_header_states_the_inverted_axis_for_every_translating_post() {
    for stem in ["fanuc", "haas"] {
        let inv = engine::run(&cps(stem), &mch(&["X"]), &probe_stream()).expect("posts");
        let std = engine::run(&cps(stem), &mch(&[]), &probe_stream()).expect("posts");
        let note = "(MACHINE X+ MOVES LEFT - INVERTED AXIS)";
        let at = inv.program.lines().position(|l| l == note);
        assert!(at.is_some_and(|i| i <= 2), "{stem}: the note rides the header:\n{}", inv.program);
        assert_eq!(inv.program.matches(note).count(), 1, "{stem}: once");
        assert!(!std.program.contains("INVERTED"), "{stem}: a standard machine writes no note");
        let both = engine::run(&cps(stem), &mch(&["X", "Y"]), &probe_stream()).expect("posts");
        assert!(both.program.contains(note), "{stem}");
        assert!(both.program.contains("(MACHINE Y+ MOVES TOWARD THE FRONT - INVERTED AXIS)"), "{stem}");
    }
}

#[test]
fn one_wording_table_for_every_display() {
    assert_eq!(machine::inverted_axis_line("X"), "Machine X+ moves left (inverted axis)");
    assert_eq!(machine::inverted_motion("Y"), "toward the front");
    assert_eq!(machine::inverted_motion("Z"), "down");
    assert_eq!(machine::inverted_motion("U"), "the other way");
}
