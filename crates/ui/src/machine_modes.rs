//! The ribbon's machine modes: the table, the caption each mode wears,
//! the dropdown row text, and the undoable switch that mounts a mode.
//! Provenance: our own mode table over the shipped built-in library
//! (`camcraft_app::posting::MACHINES`); no vendor material consulted.

use camcraft_app::machine::Kind as MachineKind;
use camcraft_app::operation::shown_for;

use crate::state::{CommandKind, State};

/// The ribbon's machine modes (the ribbon law): the workspace kind,
/// the Title-case caption it wears on the bar, and the built-in
/// library machine switching to it mounts. The names must exist in
/// `camcraft_app::posting::MACHINES` - the switch is a
/// `LoadBuiltinMachine`, never a bare field poke, so a mode change
/// rides undo exactly like the Setup pane's picker does. A mill's
/// reference is the plain VMC (the library's entry 0); the Tilt-head
/// 5-axis stays picker-only, because a mode says what the document
/// machines, not which five axes it owns. The table is explicit (one
/// reference per kind, named here); `machine_mode_tests` fails when a
/// kind with a shipped machine has no row, or a row mounts nothing.
pub(crate) const MACHINE_MODES: [(MachineKind, &str, &str); 4] = [
    (MachineKind::Mill, "Milling", "VMC 3-axis"),
    (MachineKind::Lathe, "Turning", "Lathe"),
    (MachineKind::MillTurn, "Mill-turning", "Mill-turn"),
    (MachineKind::Plasma, "Plasma cutting", "Tormach 1300PL"),
];

/// The plain caption a mode wears on the door and in the status line.
pub(crate) fn caption(kind: MachineKind) -> &'static str {
    MACHINE_MODES
        .iter()
        .find(|(k, _, _)| *k == kind)
        .map_or("Machine", |(_, c, _)| *c)
}

/// The modes as a phrase for the door's tip: "Milling, Turning,
/// Mill-turning or Plasma cutting", read off the table.
pub(crate) fn modes_phrase() -> String {
    let names: Vec<&str> = MACHINE_MODES.iter().map(|(_, c, _)| *c).collect();
    match names.split_last() {
        Some((last, [])) => (*last).to_owned(),
        Some((last, head)) => format!("{} or {last}", head.join(", ")),
        None => String::new(),
    }
}

/// A mode's dropdown row text: its caption, then the sections it adds
/// to a mill document (`shown_for`, labels read off the registry) that
/// the caption does not already name. "Turning" and "Plasma cutting"
/// stand bare; a mill-turn reads "Mill-turning - Turning".
pub(crate) fn mode_label(kind: MachineKind) -> String {
    let caption = caption(kind);
    let mill = shown_for(MachineKind::Mill);
    let extra: Vec<&str> = shown_for(kind)
        .into_iter()
        .filter(|s| !mill.contains(s))
        .map(|s| s.label())
        .filter(|l| !caption.contains(*l))
        .collect();
    if extra.is_empty() {
        caption.to_owned()
    } else {
        format!("{caption} - {}", extra.join(", "))
    }
}

/// Switch the document's workspace kind by mounting that kind's
/// reference machine. Picking the mode already running mounts nothing
/// - the row says "(current)" and this guard is the same law's second
///   half, so no surface can push a no-change command (`LoadMachine`
///   would refuse it at the front door anyway).
pub(crate) fn set_machine_mode(state: &mut State, kind: MachineKind) {
    if camcraft_app::machine::kind_of(&state.world) == kind {
        return;
    }
    if let Some((_, caption, name)) = MACHINE_MODES.iter().find(|(k, _, _)| *k == kind) {
        state.pending.push_back(CommandKind::LoadBuiltinMachine(
            (*name).to_owned(),
        ));
        state.status = format!("machine mode: {caption}");
    }
}
