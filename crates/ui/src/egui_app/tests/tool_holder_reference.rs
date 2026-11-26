//! #2016: a library holder is the tool's holder by reference. Apply
//! holder stores the entry's name and the two lengths; the tool's holder
//! cylinder rows (#3930: in the tool library window's Body section, before
//! in Properties) then show the holder cylinder the laws read for that
//! entry instead of two fields to size by hand; the Profile tab names
//! the entry and the window's picker shows it; the unchanged outline
//! applied again keeps the reference, an edited point makes the holder a
//! drawn profile and the two fields come back. Provenance: our own page law.
//! Gate map: covers ui::egui_app::tool_settings_grid, ui::egui_app::tool_profile_page,
//! ui::egui_app::tool_holder_picker, ui::egui_app::tool_library_dialog, ui::state (tool profile).

use super::tool_library_assembly::{holder, open_body};
use super::tool_library_editor::{H, library_window, open_in_window};
use super::*;
use camcraft_app::holder_library::HolderRef;
use egui_kittest::kittest::NodeT;

/// A 6 mm end mill on an 8 mm shank (flutes 15, shank 40).
const TOOL: &str = "EM-0600-4FL";
const FROM: &str = "from the library holder BT30 ER20";

fn reference(state: &State) -> Option<HolderRef> {
    model::find_by_name(&state.world, TOOL)
        .and_then(|e| camcraft_app::tool_body::body_of(&state.world, &e))
        .and_then(|b| b.library)
}

fn properties() -> H {
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 1400.0)).build_ui_state(super::frame, ShellUi::seeded());
    h.run();
    crate::egui_app::browser_pane::click(&mut h.state_mut().state, panes::Leaf::Tool(TOOL.into()));
    h.run_steps(3);
    h
}

/// Properties on the tool, and the tool open in the library window with
/// its Body section unfolded (#3930: the holder cylinder rows live there).
fn body_rows() -> H {
    let mut h = properties();
    open_in_window(&mut h, TOOL);
    open_body(&mut h);
    h
}

/// The values of the typed number fields in the library window.
fn spins(h: &H) -> Vec<String> {
    library_window(h).query_all_by_role(egui::accesskit::Role::SpinButton).filter_map(|n| n.value()).collect()
}

/// Are the tool's own holder diameter and length (the catalog's
/// Ø44 x 35) on screen as typed fields?
fn typed(h: &H) -> bool {
    let s = spins(h);
    s.iter().any(|v| v == "44.0 mm") && s.iter().any(|v| v == "35.0 mm")
}

fn set_bt30(h: &mut H) {
    let cmd = CommandKind::SetToolHolder { name: TOOL.into(), entry: "bt30 er20".into(), gauge: None, projection: None };
    h.state_mut().state.pending.push_back(cmd);
    h.run_steps(3);
}

fn apply_profile(h: &mut H, edit: impl Fn(&mut Vec<camcraft_app::spun::SpunPoint>)) {
    let st = &mut h.state_mut().state;
    st.sync_tool_profile_ui();
    edit(&mut st.tool_profile_ui.points);
    st.queue_apply_tool_profile();
    h.run_steps(3);
}

#[test]
fn a_library_holder_sizes_the_holder_fields_until_its_outline_is_edited() {
    let mut h = body_rows();
    let win = |h: &H, text: &str| library_window(h).query_all_by_label_contains(text).count();
    assert_eq!(win(&h, "from the library holder"), 0, "a holder cylinder is typed");
    assert!(typed(&h), "the catalog's Ø44 x 35 holder cylinder is typed: {:?}", spins(&h));
    set_bt30(&mut h);
    let want = HolderRef { entry: "BT30 ER20".into(), gauge_len: 60.0, projection: 55.0 };
    assert_eq!(reference(&h.state().state), Some(want.clone()), "status: {}", h.state().state.status);
    assert_eq!(win(&h, FROM), 2, "holder diameter and length name the entry");
    assert!(win(&h, "46.000 mm") > 0, "the BT30 flange is the holder diameter");
    assert!(win(&h, "60.000 mm") > 0, "nose face to gauge line is its length");
    assert!(!typed(&h), "the two fields are not typed: {:?}", spins(&h));
    // The outline as it stands, applied again: the same holder.
    apply_profile(&mut h, |_| {});
    assert_eq!(reference(&h.state().state), Some(want), "status: {}", h.state().state.status);
    // One point moved: a drawn profile of its own, sized by hand again.
    apply_profile(&mut h, |p| {
        p[1].r += 1.0;
        p[2].r += 1.0;
    });
    assert_eq!(reference(&h.state().state), None, "status: {}", h.state().state.status);
    assert_eq!(win(&h, "from the library holder"), 0);
    assert!(typed(&h), "{:?}", spins(&h));
    h.state_mut().state.pending.push_back(CommandKind::Undo);
    h.run_steps(3);
    assert!(reference(&h.state().state).is_some(), "undo: the library holder again");
    assert_eq!(win(&h, FROM), 2);
}

#[test]
fn the_profile_tab_names_the_entry_and_the_picker_shows_it() {
    let mut h = properties();
    set_bt30(&mut h);
    crate::actions::open_tool_shape_editor(&mut h.state_mut().state, TOOL);
    h.run_steps(3);
    // The property grid settles the window's size over its first frames.
    // egui hit-tests a press with the previous frame, so hover the tab
    // for one frame before the click, as a real mouse always does.
    h.get_by_label("Profile").hover();
    h.run();
    h.get_by_label("Profile").click();
    h.run_steps(2);
    assert_eq!(
        h.state().state.tool_profile_ui.column,
        crate::state::ToolColumn::Profile,
        "the tab click lands on the Profile column"
    );
    assert!(h.query_all_by_label_contains("In use: library holder BT30 ER20").next().is_some());
    // #3930: the picker is the tool library window's; it opens on the
    // tool's holder.
    open_in_window(&mut h, TOOL);
    let d = holder(&mut h).clone();
    assert_eq!((d.library.as_deref(), d.gauge, d.projection), (Some("BT30 ER20"), 60.0, 55.0), "the picker shows the tool's holder");
    assert!(!library_window(&h).get_by_label("Apply holder").accesskit_node().is_disabled());
}

/// #2007: the holder library controls align label-left on one row,
/// matching the property grid. #3930: they stand in the tool library
/// window's assembly group.
#[test]
fn holder_picker_uses_label_left_rows() {
    let mut h = properties();
    set_bt30(&mut h);
    open_in_window(&mut h, TOOL);
    let win = library_window(&h);
    let texts = super::structural::painted(&h);
    let lib_l = super::structural::at(&texts, "Library holder");
    let lib_c = win
        .query_all_by_role(egui::accesskit::Role::ComboBox)
        .find(|n| n.value().as_deref() == Some("BT30 ER20"))
        .expect("Library holder combo")
        .rect();
    assert!(lib_l.x < lib_c.center().x, "Library holder label sits left of combo: {lib_l:?} vs {lib_c:?}");
    assert!((lib_l.y - lib_c.center().y).abs() < 15.0, "Library holder shares a row: {lib_l:?} vs {lib_c:?}");

    for (label, value) in [("Gauge length", "60.0 mm"), ("Projection", "55.0 mm")] {
        let l = super::structural::at(&texts, label);
        let input = win
            .query_all_by_role(egui::accesskit::Role::SpinButton)
            .find(|n| n.value().as_deref() == Some(value))
            .unwrap_or_else(|| panic!("{label:?} shows {value:?}"))
            .rect();
        assert!(l.x < input.center().x, "{label:?} label sits left of input: {l:?} vs {input:?}");
        assert!((l.y - input.center().y).abs() < 15.0, "{label:?} shares a row: {l:?} vs {input:?}");
    }
}
