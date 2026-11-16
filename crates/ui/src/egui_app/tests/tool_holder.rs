//! #793: the tool editor's holder library picker - it stands in the
//! Profile tab with Apply holder disabled until a holder is picked, a
//! pick shows the entry's dimensions and gauge length, Apply holder
//! lands the BT30 ER20 as one undoable command that then edits as a
//! Custom profile, and a shank the holder cannot take is refused by
//! name. #792: the picker lists the standalone ER chucks, and its
//! projection field rides the same undoable command. #794: the list
//! carries the Tormach TTS holders. #2018: one Weldon row for each
//! taper, its bore from the tool's shank. Provenance: our own page law.

use super::*;
use camcraft_app::holder_library;
use egui_kittest::kittest::NodeT;

/// A 6 mm end mill on a 6 mm shank (flutes 15, shank 40).
const TOOL: &str = "EM-0600-4FL";

fn profile_tab() -> Harness<'static, ShellUi> {
    let mut harness = shell_harness();
    harness.run();
    crate::actions::open_tool_editor(&mut harness.state_mut().state, TOOL);
    harness.run();
    harness.run();
    harness.get_by_label("Profile").click();
    harness.run();
    harness.run();
    harness
}

fn drawn(state: &State) -> Option<Vec<(f64, f64)>> {
    model::find_by_name(&state.world, TOOL)
        .and_then(|e| camcraft_app::tool_body::body_of(&state.world, &e))
        .and_then(|b| b.spun)
        .map(|s| s.points().iter().map(|p| (p.z, p.r)).collect())
}

#[test]
fn the_picker_stands_in_the_profile_tab_and_waits_for_a_pick() {
    let harness = profile_tab();
    for label in ["Holder library", "Library holder", "Apply holder"] {
        assert!(harness.query_by_label(label).is_some(), "{label} stands in the Profile tab");
    }
    assert!(harness.get_by_label("Apply holder").accesskit_node().is_disabled(), "nothing picked yet");
}

#[test]
fn apply_holder_lands_the_bt30_er20_and_undo_restores() {
    let mut harness = profile_tab();
    harness.state_mut().state.pick_library_holder("BT30 ER20");
    harness.run();
    harness.run();
    assert_eq!(harness.state().state.tool_profile_ui.gauge, 60.0, "the entry's own gauge length");
    assert!(harness.query_all_by_label_contains("BT30 flange Ø46").next().is_some(), "the pick names its flange");
    assert!(harness.query_all_by_label_contains("ER20 nut Ø35").next().is_some(), "and its nut");
    assert!(!harness.get_by_label("Apply holder").accesskit_node().is_disabled());
    harness.get_by_label("Apply holder").click();
    harness.run();
    harness.run();
    let want = holder_library::find("BT30 ER20").and_then(|e| e.points(55.0, 60.0)).expect("draws");
    assert_eq!(
        drawn(&harness.state().state),
        Some(want.iter().map(|p| (p.z, p.r)).collect()),
        "status: {}",
        harness.state().state.status
    );
    let st = &harness.state().state;
    assert!(st.tool_profile_ui.custom, "the landed holder edits as Custom");
    assert_eq!(st.tool_profile_ui.library.as_deref(), Some("BT30 ER20"), "the pick survives the landing");
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    harness.run();
    assert!(drawn(&harness.state().state).is_none(), "one undo restores the holder cylinder");
    harness.state_mut().state.pending.push_back(CommandKind::Redo);
    harness.run();
    assert!(drawn(&harness.state().state).is_some(), "redo puts it back");
}

#[test]
fn a_shank_the_holder_cannot_take_is_refused_by_name() {
    let mut harness = profile_tab();
    harness.state_mut().state.pick_library_holder("BT30 Weldon 12 mm");
    harness.run();
    harness.run();
    assert!(harness.query_all_by_label_contains("does not fit").next().is_some(), "the live check names it");
    harness.get_by_label("Apply holder").click();
    harness.run();
    harness.run();
    assert!(drawn(&harness.state().state).is_none(), "nothing lands");
    assert!(harness.query_all_by_label_contains("holder not set").next().is_some(), "Apply refuses by name");
}

/// #792 (#800 review): the projection - nose face above the tip -
/// stands beside the gauge length, starts at the tool's stickout (6 mm
/// end mill: flutes 15 + shank 40), and lands through the same undoable
/// command; one undo restores the holder before it.
#[test]
fn the_projection_lands_through_apply_holder_and_undoes() {
    let mut harness = profile_tab();
    harness.state_mut().state.pick_library_holder("ER20 chuck");
    harness.run();
    harness.run();
    assert!(harness.query_all_by_label_contains("projection").next().is_some(), "the projection field stands");
    assert!(harness.query_all_by_label_contains("straight-shank chuck").next().is_some(), "the pick says what it is");
    let st = &harness.state().state;
    assert_eq!((st.tool_profile_ui.gauge, st.tool_profile_ui.projection), (45.0, 55.0), "entry gauge, tool stickout");
    harness.state_mut().state.tool_profile_ui.projection = 70.0;
    harness.get_by_label("Apply holder").click();
    harness.run();
    harness.run();
    let want = holder_library::find("ER20 chuck").and_then(|e| e.points(70.0, 45.0)).expect("draws");
    assert_eq!(
        drawn(&harness.state().state),
        Some(want.iter().map(|p| (p.z, p.r)).collect()),
        "status: {}",
        harness.state().state.status
    );
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    harness.run();
    assert!(drawn(&harness.state().state).is_none(), "one undo restores the holder cylinder");
}

/// #794: the Library holder list carries the Tormach TTS holders, and a
/// pick names its collar and lands at its published installed height
/// (TTS ER20: 52 mm) as the gauge length.
#[test]
fn the_picker_lists_the_tts_holders_and_names_the_collar() {
    let mut harness = profile_tab();
    harness.get_by_label("Library holder").click();
    harness.run();
    harness.run();
    for n in ["TTS ER20", "TTS ER16", "TTS Weldon"] {
        assert!(harness.query_by_label(n).is_some(), "{n} is in the list");
    }
    // #2018: one Weldon row for the taper, not one for each bore.
    for n in ["TTS Weldon 3/8 in", "TTS Weldon 1/2 in", "TTS Weldon 6 mm"] {
        assert!(harness.query_by_label(n).is_none(), "{n} is not a row of its own");
    }
    harness.state_mut().state.pick_library_holder("TTS ER20");
    harness.run();
    harness.run();
    let st = &harness.state().state;
    assert_eq!((st.tool_profile_ui.library.as_deref(), st.tool_profile_ui.gauge), (Some("TTS ER20"), 52.0));
    assert!(harness.query_all_by_label_contains("TTS collar Ø38.1").next().is_some(), "the pick names its collar");
    assert!(harness.query_all_by_label_contains("ER20 nut Ø35").next().is_some(), "and its nut");
}

/// #2018: the list has one Weldon row for each taper; picking it on the
/// editor tool's 8 mm shank names the 8 mm bore and its Ø28 body, and
/// Apply holder lands the old "BT40 Weldon 8 mm" outline - the same
/// collision shape.
#[test]
fn one_weldon_row_for_each_taper_takes_the_bore_of_the_shank() {
    let mut harness = profile_tab();
    harness.state_mut().state.pick_library_holder("BT40 Weldon");
    harness.run();
    harness.run();
    let st = &harness.state().state;
    assert_eq!((st.tool_profile_ui.library.as_deref(), st.tool_profile_ui.gauge), (Some("BT40 Weldon"), 63.0));
    assert!(harness.query_all_by_label_contains("Weldon bore Ø8.00, body Ø28.00").next().is_some(), "the bore of the shank");
    harness.get_by_label("Apply holder").click();
    harness.run();
    harness.run();
    let want = holder_library::find("BT40 Weldon 8 mm").and_then(|e| e.points(55.0, 63.0)).expect("draws");
    assert_eq!(
        drawn(&harness.state().state),
        Some(want.iter().map(|p| (p.z, p.r)).collect()),
        "status: {}",
        harness.state().state.status
    );
    // The list: one Weldon row for each taper, none for each bore.
    harness.get_by_label("Library holder").click();
    harness.run();
    harness.run();
    for t in ["BT30", "BT40", "CAT40", "TTS"] {
        assert!(harness.query_all_by_label(&format!("{t} Weldon")).count() >= 1, "{t} has a Weldon row");
    }
    assert!(harness.query_by_label("BT40 Weldon 10 mm").is_none(), "no row for each bore");
    assert!(harness.query_by_label("BT30 Weldon 6 mm").is_none(), "no row for each bore");
}
