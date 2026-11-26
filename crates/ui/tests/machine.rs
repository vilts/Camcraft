//! The machine's ui wiring (M2.3 stage C): the queue's arms - load,
//! clear, refuse, undo - and the vote clause Deviate appends. The
//! reader and the checker themselves are pinned in
//! `crates/app/tests/machine.rs`; this half owns statuses, the command
//! law around them, and the sentence shape the bar prints.
//! Provenance: our own tests, specced from the panes' contract.
//!
//! Gate map (M4.4): covers ui::actions, ui::state, app::machine, app::operation, app::stock, app::tools.

use std::path::PathBuf;

use camcraft_app::machine;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_ui::actions;
use camcraft_ui::state::{CommandKind, ErrorScope, State};
use camcraft_ui::unsaved::UnsavedChoice;

fn catalog_tool(name: &str) -> camcraft_app::tools::Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog tool")
}

fn tmp_mch(tag: &str, text: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("camcraft_ui_machine_{tag}_{}.mch", std::process::id()));
    std::fs::write(&p, text).expect("write the fixture file");
    p
}

/// A 3-axis VMC with a stated stroke, in the `.mch` layout - our own
/// values, never a vendor's (clean-room fixture law).
fn mch_text(spindle_max: &str) -> String {
    format!(
        r#"{{
      "general": {{"model": "TestMill 1", "vendor": "CamCraft Testbench"}},
      "kinematics": {{"default": {{
        "units": {{"angle": "degrees", "length": "mm"}},
        "parts": [
          {{"type": "head", "name": "h",
            "spindle": {{"min_speed": 50, "max_speed": {spindle_max}}},
            "tool_station": {{"max_tool_length": 150, "max_tool_diameter": 20}},
            "parts": [
              {{"type": "linear", "name": "X", "control": "driven",
                "travel": {{"min": -100, "max": 100}},
                "parts": [
                  {{"type": "linear", "name": "Y", "control": "driven",
                    "travel": {{"min": -100, "max": 100}},
                    "parts": [
                      {{"type": "linear", "name": "Z", "control": "driven",
                        "travel": {{"min": -100, "max": 100}}}}
                    ]}}
                ]}}
            ]}}
        ]}}}},
      "controller": {{"synced_configuration": {{"parts": {{
        "X": {{"coordinate": 0, "max_normal_speed": 5000, "max_rapid_speed": 15000}},
        "Y": {{"coordinate": 1, "max_normal_speed": 5000, "max_rapid_speed": 15000}},
        "Z": {{"coordinate": 2, "max_normal_speed": 3000, "max_rapid_speed": 10000}}
      }}}}}}
    }}"#
    )
}

#[test]
fn load_machine_names_it_and_undo_mutes_it() {
    let path = tmp_mch("load", &mch_text("12000"));
    let mut state = State::new();
    state.pending.push_back(CommandKind::LoadMachineFile(path.clone()));
    state.drain_actions();
    assert!(
        state.status.starts_with("machine: CamCraft Testbench TestMill 1"),
        "the status names the machine: {}",
        state.status
    );
    let (p, m) = machine::of(&state.world);
    assert_eq!(p, path.display().to_string(), "the file it came from is carried");
    let m = m.expect("the machine is mounted");
    assert_eq!(m.axes.len(), 3);

    // Undo mutes the checker exactly - the previous absence, restored.
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    let (_, m) = machine::of(&state.world);
    assert!(m.is_none(), "undo restores the previous machine: nothing");

    // Redo mounts it again: the command law says the inverse is exact.
    state.pending.push_back(CommandKind::Redo);
    state.drain_actions();
    let (_, m) = machine::of(&state.world);
    assert!(m.is_some(), "redo replays the mount");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_bad_machine_file_refuses_without_touching_the_world() {
    let mut state = State::new();
    state
        .pending
        .push_back(CommandKind::LoadMachineFile(PathBuf::from("no_such_machine_file.mch")));
    state.drain_actions();
    assert!(state.status.contains("machine not loaded"), "{}", state.status);
    assert!(state.error_for(ErrorScope::Setup).is_some(), "the setup pane shows the reason");
    assert!(machine::of(&state.world).1.is_none());
    assert!(!state.history.can_undo(), "a refusal never enters the history");

    // Garbage of the well-formed-but-not-an-object kind refuses too -
    // the Prime Directive's file half: nonsense is an error, never a
    // crash and never a half-read machine.
    let junk = tmp_mch("junk", "[]");
    state.pending.push_back(CommandKind::LoadMachineFile(junk.clone()));
    state.drain_actions();
    assert!(state.status.contains("machine not loaded"), "{}", state.status);
    assert!(!state.history.can_undo());
    let _ = std::fs::remove_file(&junk);
}

#[test]
fn clearing_states_the_absence_and_undo_puts_the_machine_back() {
    let path = tmp_mch("clear", &mch_text("12000"));
    let mut state = State::new();
    state.pending.push_back(CommandKind::LoadMachineFile(path.clone()));
    state.drain_actions();
    state.pending.push_back(CommandKind::ClearMachine);
    state.drain_actions();
    assert_eq!(state.status, "machine cleared");
    let (p, m) = machine::of(&state.world);
    assert_eq!(p, "", "clearing names no file");
    assert!(m.is_none());
    // Clearing what is already absent is a no-op refusal, like units.
    state.pending.push_back(CommandKind::ClearMachine);
    state.drain_actions();
    assert!(state.status.contains("machine not cleared"), "{}", state.status);
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert!(machine::of(&state.world).1.is_some(), "undo of a clear remounts");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn new_document_retires_the_machine_with_the_rest() {
    let path = tmp_mch("newdoc", &mch_text("12000"));
    let mut state = State::new();
    state.pending.push_back(CommandKind::LoadMachineFile(path.clone()));
    state.drain_actions();
    assert!(machine::of(&state.world).1.is_some());
    state.pending.push_back(CommandKind::NewDocument);
    // The mount is unsaved work: New waits for the prompt's answer (#107).
    state.pending.push_back(CommandKind::ResolveUnsaved(UnsavedChoice::DontSave));
    state.drain_actions();
    assert!(
        machine::of(&state.world).1.is_none(),
        "a fresh blank never inherits the last document's limits"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_machine_vote_joins_the_deviate_clause() {
    // The vote is pure (the readout law): no window, and the same
    // program_ops gate the sim walks.
    let mut state = State::new();
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    state.pending.push_back(CommandKind::AddTool {
        name: "EM-0400-2FL".into(),
        tool: catalog_tool("EM-0400-2FL"),
    });
    state.drain_actions();
    state.pending.push_back(CommandKind::CreateOperation {
        name: "Face1".into(),
        tool: "EM-0400-2FL".into(),
        src: SrcRef::Stock,
        params: OpParams::Face { holes: Default::default(), blend_turns: false, blend_radius: None,
            tilt: None,
            stepover: 3.0,
            clearance_z: Height::fixed(25.0),
            cut_z: Height::fixed(18.0),
            top_z: None,
            stepdown: None,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
            picks: Default::default(),
        },
    });
    state.pending.push_back(CommandKind::GenerateOperation("Face1".into()));
    state.drain_actions();

    // No machine: no vote. The line never claims a judgment that was
    // not made (the unstated-limit law in sentence form).
    assert!(actions::machine_vote(&state.world).is_none());

    let ok = tmp_mch("vote", &mch_text("12000"));
    state.pending.push_back(CommandKind::LoadMachineFile(ok.clone()));
    state.drain_actions();
    assert_eq!(
        actions::machine_vote(&state.world).as_deref(),
        Some("; machine: within limits"),
        "the reference program is inside the reference machine"
    );

    // A tighter window: the same program speaks up, one problem per
    // op that outruns it.
    let tight = tmp_mch("tight", &mch_text("10000"));
    state.pending.push_back(CommandKind::LoadMachineFile(tight.clone()));
    state.drain_actions();
    assert_eq!(
        actions::machine_vote(&state.world).as_deref(),
        Some("; machine: 1 limit problem(s)"),
        "12000 rpm against a 10000 ceiling"
    );
    let _ = std::fs::remove_file(&ok);
    let _ = std::fs::remove_file(&tight);
}

#[test]
fn the_machine_gate_refuses_the_post_by_name() {
    // Stage D through the ui's door: the same mounted ceiling that
    // earned its Deviate clause now blocks the write, in the one
    // status sentence the shell prints. Nothing is on disk to read
    // back - a refusal never posts.
    let mut state = State::new();
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    state.pending.push_back(CommandKind::AddTool {
        name: "EM-0400-2FL".into(),
        tool: catalog_tool("EM-0400-2FL"),
    });
    state.pending.push_back(CommandKind::CreateOperation {
        name: "Face1".into(),
        tool: "EM-0400-2FL".into(),
        src: SrcRef::Stock,
        params: OpParams::Face { holes: Default::default(), blend_turns: false, blend_radius: None,
            tilt: None,
            stepover: 3.0,
            clearance_z: Height::fixed(25.0),
            cut_z: Height::fixed(19.5),
            top_z: None,
            stepdown: None,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
            picks: Default::default(),
        },
    });
    state.pending.push_back(CommandKind::GenerateOperation("Face1".into()));
    state.drain_actions();

    let tight = tmp_mch("gate", &mch_text("10000"));
    state.pending.push_back(CommandKind::LoadMachineFile(tight.clone()));
    state.drain_actions();

    let nc = std::env::temp_dir().join(format!("camcraft_ui_gate_{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&nc);
    state.pending.push_back(CommandKind::PostProgramFile(nc.clone()));
    state.drain_actions();
    assert!(
        state.status.contains("post refused: post blocked - machine TestMill 1: 1 limit problem(s)"),
        "the refusal names the machine and the count: {}",
        state.status
    );
    assert!(!nc.exists(), "the gate wrote nothing");

    // Within limits the same click posts - the gate opens without
    // re-planning anything.
    let ok = tmp_mch("gate-ok", &mch_text("12000"));
    state.pending.push_back(CommandKind::LoadMachineFile(ok.clone()));
    state.drain_actions();
    state.pending.push_back(CommandKind::PostProgramFile(nc.clone()));
    state.drain_actions();
    assert!(state.status.starts_with("posted "), "{}", state.status);
    assert!(nc.exists());
    let _ = std::fs::remove_file(&nc);
    let _ = std::fs::remove_file(&ok);
    let _ = std::fs::remove_file(&tight);
}
