//! #1195: the ribbon's machine modes. Every machine kind with a shipped
//! built-in machine has a mode row, every row mounts the shipped machine
//! of its own kind, the door wears the plain caption while the menu rows
//! carry the sections they add, the Plasma cutting mode puts the plasma
//! section on the ribbon, and switching modes rides undo.
//! Provenance: our own tests over our own ribbon; no vendor material consulted.

use camcraft_app::machine::{self, Kind, library};
use camcraft_app::operation::{OpType, Section};

use super::panes::{self, Cmd, DROP_W, ribbon_plan};
use crate::machine_modes::{self, MACHINE_MODES};
use crate::state::{CommandKind, State};

fn run(state: &mut State, kind: CommandKind) {
    state.pending.push_back(kind);
    state.drain_actions();
}

#[test]
fn every_kind_with_a_shipped_machine_has_a_mode() {
    let shipped: Vec<Kind> = library::builtins().iter().filter_map(|e| e.kind).collect();
    assert!(shipped.contains(&Kind::Plasma), "the Tormach 1300PL ships");
    for kind in shipped {
        assert!(
            MACHINE_MODES.iter().any(|(k, _, _)| *k == kind),
            "{kind:?} has a shipped machine but no ribbon mode row"
        );
    }
}

#[test]
fn every_mode_mounts_a_shipped_machine_of_its_own_kind() {
    let shipped = library::builtins();
    for (kind, caption, name) in MACHINE_MODES {
        let entry = shipped.iter().find(|e| e.name == name);
        assert_eq!(
            entry.and_then(|e| e.kind),
            Some(kind),
            "'{caption}' must mount the built-in '{name}' of its own kind"
        );
    }
}

#[test]
fn a_menu_row_adds_only_the_sections_its_caption_does_not_name() {
    assert_eq!(machine_modes::mode_label(Kind::Mill), "Milling");
    assert_eq!(machine_modes::mode_label(Kind::Lathe), "Turning", "the caption names its section");
    assert_eq!(machine_modes::mode_label(Kind::Plasma), "Plasma cutting", "the caption names its section");
    assert_eq!(
        machine_modes::mode_label(Kind::MillTurn),
        "Mill-turning - Turning",
        "a mill-turn adds the turning section to a mill"
    );
}

#[test]
fn the_door_wears_the_plain_caption_for_every_mode() {
    for (kind, caption, name) in MACHINE_MODES {
        let mut state = State::new();
        run(&mut state, CommandKind::LoadBuiltinMachine(name.into()));
        assert_eq!(machine::kind_of(&state.world), kind, "{name} mounted");
        assert_eq!(
            panes::text_of(Cmd::Machine, &state),
            format!("{caption}…"),
            "the door's word is the caption, never the menu row"
        );
    }
}

#[test]
fn the_door_tip_lists_every_mode() {
    let state = State::new();
    let tip = panes::tip_of(Cmd::Machine, &state);
    for (_, caption, _) in MACHINE_MODES {
        assert!(tip.contains(caption), "the door tip names '{caption}': {tip}");
    }
}

#[test]
fn the_menu_is_as_wide_as_its_longest_row_and_never_narrower_than_the_door_menu() {
    assert_eq!(panes::machine_menu_width(10.0), DROP_W, "short rows keep the usual width");
    assert_eq!(panes::machine_menu_width(500.0), 516.0, "a long row sets the width, text not cut");
}

#[test]
fn plasma_cutting_mode_shows_the_plasma_profile_and_switching_back_restores_the_mill() {
    let plasma = OpType::PlasmaProfile.ordinal();
    let mut state = State::new();
    run(&mut state, CommandKind::SetStock {
        shape: camcraft_app::stock::StockShape::Box { dx: 40.0, dy: 40.0, dz: 6.0 },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    run(&mut state, CommandKind::LoadBuiltinMachine("VMC 3-axis".into()));
    run(&mut state, CommandKind::AddTool {
        name: "PT-KERF-150".into(),
        tool: camcraft_app::tools::torch_tool(1.5),
    });
    assert_eq!(machine::kind_of(&state.world), Kind::Mill);
    assert!(!panes::enabled(Cmd::Create(plasma), &state), "no plasma cell on a mill");

    machine_modes::set_machine_mode(&mut state, Kind::Plasma);
    state.drain_actions();
    assert_eq!(machine::kind_of(&state.world), Kind::Plasma);
    let sections: Vec<Section> = ribbon_plan(Kind::Plasma, &[]).into_iter().filter_map(|g| g.section).collect();
    assert_eq!(sections, vec![Section::Plasma], "the plasma section shows");
    assert!(panes::enabled(Cmd::Create(plasma), &state), "Plasma Profile can be created");

    machine_modes::set_machine_mode(&mut state, Kind::Plasma);
    assert!(state.pending.is_empty(), "the mode already running mounts nothing");

    machine_modes::set_machine_mode(&mut state, Kind::Mill);
    state.drain_actions();
    assert_eq!(machine::kind_of(&state.world), Kind::Mill, "the mill sections come back");
    assert!(!panes::enabled(Cmd::Create(plasma), &state));

    run(&mut state, CommandKind::Undo);
    assert_eq!(machine::kind_of(&state.world), Kind::Plasma, "one undo: back to plasma");
    run(&mut state, CommandKind::Undo);
    assert_eq!(machine::kind_of(&state.world), Kind::Mill, "two undos: back to the mill");
}
