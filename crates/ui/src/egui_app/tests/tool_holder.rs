//! #793: the holder library picker - it stands in the tool library
//! window's assembly group (#3930; before, in the tool shape window) with
//! Apply holder disabled until a holder is picked, a pick shows the
//! entry's dimensions and gauge length, Apply holder lands the BT30 ER20
//! as one undoable command that then edits as a Custom profile, and a
//! shank the holder cannot take is refused by name. #792: the picker lists
//! the standalone ER chucks, and its projection field rides the same
//! undoable command. #794: the list carries the Tormach TTS holders.
//! #2018: one Weldon row for each taper, its bore from the tool's shank.
//! Provenance: our own page law.
//! Gate map: covers ui::egui_app::tool_holder_picker, ui::egui_app::tool_library_dialog, ui::egui_app::tool_profile_page.

use super::tool_library_assembly::{holder, pick, tall_window};
use super::tool_library_editor::{H, library_window};
use super::*;
use camcraft_app::holder_library;
use egui_kittest::kittest::NodeT;

/// A 6 mm end mill on an 8 mm shank (flutes 15, shank 40).
const TOOL: &str = "EM-0600-4FL";

/// The tool library window, open on the tool.
fn editor() -> H {
    tall_window(TOOL)
}

fn drawn(state: &State) -> Option<Vec<(f64, f64)>> {
    model::find_by_name(&state.world, TOOL)
        .and_then(|e| camcraft_app::tool_body::body_of(&state.world, &e))
        .and_then(|b| b.spun)
        .map(|s| s.points().iter().map(|p| (p.z, p.r)).collect())
}

fn shows(h: &H, text: &str) -> bool {
    library_window(h).query_all_by_label_contains(text).next().is_some()
}

fn apply_disabled(h: &H) -> bool {
    library_window(h).get_by_label("Apply holder").accesskit_node().is_disabled()
}

/// Click the window's "Apply holder" (its own draft and tool).
fn click_apply_holder(h: &mut H) {
    library_window(h).get_by_label("Apply holder").scroll_to_me();
    h.run();
    library_window(h).get_by_label("Apply holder").click();
    h.run_steps(3);
}

/// #3930: the holder library heads the window's assembly group, and the
/// tool shape window no longer has a picker (it keeps the Body and
/// Profile tabs). Before: `the_holder_library_is_the_first_thing_the_shape_editor_shows`.
#[test]
fn the_holder_library_heads_the_assembly_group_not_the_shape_window() {
    let mut h = editor();
    for label in ["Holder library", "Library holder", "Apply holder"] {
        assert!(library_window(&h).query_by_label(label).is_some(), "{label} stands in the window's assembly group");
    }
    let heading = library_window(&h).get_by_label(crate::egui_app::tool_library_dialog::assembly_group::HEADING).rect();
    assert!(library_window(&h).get_by_label("Holder library").rect().top() > heading.top(), "the picker is under the group heading");
    assert!(apply_disabled(&h), "nothing picked yet");
    crate::actions::open_tool_shape_editor(&mut h.state_mut().state, TOOL);
    h.run_steps(3);
    let win = h
        .query_all_by_role(egui::accesskit::Role::Window)
        .find(|n| n.accesskit_node().label().as_deref() == Some("Tool shape: EM-0600-4FL"))
        .expect("shape window");
    for label in ["Holder library", "Library holder", "Apply holder"] {
        assert!(win.query_by_label(label).is_none(), "the shape window has no {label}");
    }
    assert!(win.query_by_label("Body").is_some(), "the Body tab is the default");
    assert!(win.query_by_label("Import body…").is_some(), "the Body section is behind its tab");
    assert!(win.query_by_label("Apply profile").is_none(), "the profile table is behind its tab");
}

#[test]
fn apply_holder_lands_the_bt30_er20_and_undo_restores() {
    let mut harness = editor();
    pick(&mut harness, "BT30 ER20");
    assert_eq!(holder(&mut harness).gauge, 60.0, "the entry's own gauge length");
    assert!(shows(&harness, "BT30 flange Ø46"), "the pick names its flange");
    assert!(shows(&harness, "ER20 nut Ø35"), "and its nut");
    assert!(!apply_disabled(&harness));
    click_apply_holder(&mut harness);
    let want = holder_library::find("BT30 ER20").and_then(|e| e.points(55.0, 60.0)).expect("draws");
    assert_eq!(
        drawn(&harness.state().state),
        Some(want.iter().map(|p| (p.z, p.r)).collect()),
        "status: {}",
        harness.state().state.status
    );
    let d = holder(&mut harness).clone();
    assert!(d.custom, "the landed holder edits as Custom");
    assert_eq!(d.library.as_deref(), Some("BT30 ER20"), "the pick survives the landing");
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
    let mut harness = editor();
    pick(&mut harness, "BT30 Weldon 12 mm");
    assert!(shows(&harness, "does not fit"), "the live check names it");
    click_apply_holder(&mut harness);
    assert!(drawn(&harness.state().state).is_none(), "nothing lands");
    assert!(shows(&harness, "holder not set"), "Apply refuses by name, inside the window");
    let win = library_window(&harness);
    assert!(win.rect().contains_rect(win.get_by_role_and_label(egui::accesskit::Role::Button, "Apply").rect()), "Apply stays inside the window with an error");
}

/// #792 (#800 review): the projection - nose face above the tip -
/// stands beside the gauge length, starts at the tool's stickout (6 mm
/// end mill: flutes 15 + shank 40), and lands through the same undoable
/// command; one undo restores the holder before it.
#[test]
fn the_projection_lands_through_apply_holder_and_undoes() {
    let mut harness = editor();
    pick(&mut harness, "ER20 chuck");
    assert!(shows(&harness, "Projection"), "the projection field stands");
    assert!(shows(&harness, "straight-shank chuck"), "the pick says what it is");
    let d = holder(&mut harness).clone();
    assert_eq!((d.gauge, d.projection), (45.0, 55.0), "entry gauge, tool stickout");
    holder(&mut harness).projection = 70.0;
    harness.run_steps(2);
    click_apply_holder(&mut harness);
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
    let mut harness = editor();
    library_window(&harness)
        .get_by(|n| n.role() == egui::accesskit::Role::ComboBox && n.value().as_deref() == Some("choose a holder"))
        .click();
    harness.run();
    harness.run();
    for n in ["TTS ER20", "TTS ER16", "TTS Weldon"] {
        assert!(harness.query_by_label(n).is_some(), "{n} is in the list");
    }
    // #2018: one Weldon row for the taper, not one for each bore.
    for n in ["TTS Weldon 3/8 in", "TTS Weldon 1/2 in", "TTS Weldon 6 mm"] {
        assert!(harness.query_by_label(n).is_none(), "{n} is not a row of its own");
    }
    pick(&mut harness, "TTS ER20");
    let d = holder(&mut harness).clone();
    assert_eq!((d.library.as_deref(), d.gauge), (Some("TTS ER20"), 52.0));
    assert!(shows(&harness, "TTS collar Ø38.1"), "the pick names its collar");
    assert!(shows(&harness, "ER20 nut Ø35"), "and its nut");
}

/// #2018: the list has one Weldon row for each taper; picking it on the
/// editor tool's 8 mm shank names the 8 mm bore and its Ø28 body, and
/// Apply holder lands the old "BT40 Weldon 8 mm" outline - the same
/// collision shape.
#[test]
fn one_weldon_row_for_each_taper_takes_the_bore_of_the_shank() {
    let mut harness = editor();
    pick(&mut harness, "BT40 Weldon");
    let d = holder(&mut harness).clone();
    assert_eq!((d.library.as_deref(), d.gauge), (Some("BT40 Weldon"), 63.0));
    assert!(shows(&harness, "Weldon bore Ø8.00, body Ø28.00"), "the bore of the shank");
    click_apply_holder(&mut harness);
    let want = holder_library::find("BT40 Weldon 8 mm").and_then(|e| e.points(55.0, 63.0)).expect("draws");
    assert_eq!(
        drawn(&harness.state().state),
        Some(want.iter().map(|p| (p.z, p.r)).collect()),
        "status: {}",
        harness.state().state.status
    );
    // The list: one Weldon row for each taper, none for each bore.
    library_window(&harness)
        .get_by(|n| n.role() == egui::accesskit::Role::ComboBox && n.value().as_deref() == Some("BT40 Weldon"))
        .click();
    harness.run();
    harness.run();
    for t in ["BT30", "BT40", "CAT40", "TTS"] {
        assert!(harness.query_all_by_label(&format!("{t} Weldon")).count() >= 1, "{t} has a Weldon row");
    }
    assert!(harness.query_by_label("BT40 Weldon 10 mm").is_none(), "no row for each bore");
    assert!(harness.query_by_label("BT30 Weldon 6 mm").is_none(), "no row for each bore");
}
