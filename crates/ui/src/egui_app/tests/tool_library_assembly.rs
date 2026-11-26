//! #3895 (U4 of #3663): the assembly group of the tool library window -
//! a holder pick, its gauge length and projection (the stick-out) are the
//! window's own draft and land only through Apply holder, as ONE undoable
//! `SetToolBody::library`; the preview draws the draft holder with its
//! stick-out and gauge; a shank the holder cannot take is warned by name
//! before the click and refused by name on it; a change of the stored
//! tool's length re-seeds the projection draft only (#808 item 2), never
//! the stored `HolderRef`; the window's holder draft and the shape
//! window's stay apart. #3930: the folded Body section imports and places
//! a body from the window's own body draft; Apply body is one
//! `SetToolBody`, and the shape window's body draft is not touched.
//! Gate map: covers ui::egui_app::tool_library_dialog, ui::egui_app::tool_holder_picker, ui::state.
//! Provenance: our own structural tests over the AccessKit tree and the
//! command history; no HSMWorks material involved.

use camcraft_app::holder_library::HolderRef;

use super::tool_library_editor::{H, click_window_button, draft, library_window, open_in_window, tool, two_tools};
use super::*;
use crate::egui_app::tool_library_dialog::assembly_group::{BODY, HEADING};
use crate::egui_app::tool_library_dialog::model as preview_model;
use crate::state::{ErrorScope, ToolProfileUi};

/// A 6 mm end mill on an 8 mm shank (flutes 15, shank 40: stickout 55).
const TOOL: &str = "EM-0600-4FL";

pub(super) fn window() -> H {
    let mut h = shell_harness();
    h.run_steps(2);
    open_in_window(&mut h, TOOL);
    h
}

/// #3930: a tall shell with `name` open in the window, so the whole Tool
/// tab (the assembly group and the Body section) is in view.
pub(super) fn tall_window(name: &str) -> H {
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 1400.0)).build_ui_state(super::frame, ShellUi::seeded());
    h.run_steps(2);
    open_in_window(&mut h, name);
    h
}

/// #3930: unfold the window's Body section.
pub(super) fn open_body(h: &mut H) {
    library_window(h).get_by_label(BODY).click();
    h.run_steps(2);
}

/// #3930: the window's body draft (the editor must be open).
pub(super) fn body(h: &mut H) -> &mut crate::state::ToolBodyUi {
    match h.state_mut().state.tool_library.as_mut().and_then(|v| v.editor.as_mut()) {
        Some(e) => &mut e.body,
        None => panic!("no tool is open in the tool library window"),
    }
}

/// The window's holder draft (the editor must be open).
pub(super) fn holder(h: &mut H) -> &mut ToolProfileUi {
    match h.state_mut().state.tool_library.as_mut().and_then(|v| v.editor.as_mut()) {
        Some(e) => &mut e.holder,
        None => panic!("no tool is open in the tool library window"),
    }
}

/// Pick `entry` in the window's holder draft, as its picker does.
pub(super) fn pick(h: &mut H, entry: &str) {
    h.state_mut().state.with_library_draft(|st, _| st.pick_library_holder(entry));
    h.run_steps(2);
}

/// "Apply holder" of the window (its own draft and tool).
pub(super) fn apply_holder(h: &mut H) {
    h.state_mut().state.with_library_draft(|st, _| st.queue_apply_tool_holder());
    h.run_steps(3);
}

pub(super) fn reference(h: &H, name: &str) -> Option<HolderRef> {
    let st = &h.state().state;
    model::find_by_name(&st.world, name)
        .and_then(|e| camcraft_app::tool_body::body_of(&st.world, &e))
        .and_then(|b| b.library)
}

pub(super) fn shows(h: &H, text: &str) -> bool {
    library_window(h).query_all_by_label_contains(text).next().is_some()
}

fn last() -> std::rc::Rc<preview_model::ToolModel> {
    preview_model::last().unwrap_or_else(|| panic!("the preview built no model"))
}

/// The pick, the gauge length and the stick-out are a draft: the preview
/// draws them, nothing is stored. Apply holder lands them as one undo
/// step, and one undo takes the holder away again.
#[test]
fn apply_holder_is_one_set_tool_body() {
    let mut h = window();
    assert!(shows(&h, HEADING), "the Tool tab has the assembly group");
    assert!(shows(&h, "Library holder"), "the group holds the holder pick");
    let done = h.state().state.history.done_len();
    pick(&mut h, "BT30 ER20");
    let d = holder(&mut h).clone();
    assert_eq!((d.library.as_deref(), d.gauge, d.projection), (Some("BT30 ER20"), 60.0, 55.0), "entry gauge, tool stickout");
    holder(&mut h).projection = 70.0;
    h.run_steps(2);
    assert!(shows(&h, "Total gauge length"), "the group shows the total");
    let dims = last().dims;
    assert!((dims.stick_out - 70.0).abs() < 1e-6, "the preview's stick-out is the draft projection: {dims:?}");
    assert!((dims.gauge - 130.0).abs() < 1e-6, "the preview's gauge is projection plus gauge length: {dims:?}");
    assert_eq!(reference(&h, TOOL), None, "a draft stores nothing");
    assert_eq!(h.state().state.history.done_len(), done, "a draft is no undo step");
    apply_holder(&mut h);
    let want = HolderRef { entry: "BT30 ER20".into(), gauge_len: 60.0, projection: 70.0 };
    assert_eq!(reference(&h, TOOL), Some(want), "status: {}", h.state().state.status);
    assert_eq!(h.state().state.history.done_len(), done + 1, "Apply holder is one undo step");
    assert_eq!(h.state().state.tool_profile_ui.library, None, "the Properties holder draft is not touched");
    crate::actions::undo(&mut h.state_mut().state);
    h.run_steps(2);
    assert_eq!(reference(&h, TOOL), None, "one undo takes the holder away");
}

/// A Weldon bore of another size is warned in the window before the
/// click, and Apply holder is refused by name; nothing lands.
#[test]
fn a_shank_the_holder_cannot_take_is_warned() {
    let mut h = window();
    let done = h.state().state.history.done_len();
    pick(&mut h, "BT30 Weldon 12 mm");
    assert!(shows(&h, "does not fit"), "the group names the shank it cannot take");
    apply_holder(&mut h);
    assert_eq!(reference(&h, TOOL), None, "nothing lands");
    assert_eq!(h.state().state.history.done_len(), done, "a refusal is no undo step");
    let err = h.state().state.error_for(ErrorScope::Tool).unwrap_or_default().to_owned();
    assert!(err.contains("holder not set") && err.contains("does not fit"), "refused by name: {err}");
    assert!(shows(&h, "holder not set"), "the refusal shows in the window");
}

/// A stick-out the holder cannot stand (the nose in the flutes) is named
/// before the click, by the same checks as Apply holder.
#[test]
fn a_projection_in_the_flutes_is_named_before_apply() {
    let mut h = window();
    pick(&mut h, "ER20 chuck");
    holder(&mut h).projection = 5.0;
    h.run_steps(2);
    let err = last().holder_err.clone().unwrap_or_default();
    assert!(err.contains("reaches into the flutes"), "the preview names the refusal: {err:?}");
    assert!(shows(&h, "Apply holder refuses this"), "the group shows it");
}

/// Review of #3931: Apply holder judges the STORED tool, so the refusal
/// note does too. Unapplied short flutes (5 mm, stored 15 mm) must not
/// hide the refusal of a 10 mm projection that Apply holder gives.
#[test]
fn the_refusal_judges_the_stored_tool_not_shorter_draft_flutes() {
    let mut h = window();
    assert_eq!(tool(&h, TOOL).flute_len, 15.0, "the stored flutes");
    draft(&mut h).flute_len = 5.0;
    pick(&mut h, "ER20 chuck");
    holder(&mut h).projection = 10.0;
    h.run_steps(2);
    let err = last().holder_err.clone().unwrap_or_default();
    assert!(err.contains("reaches into the flutes"), "the stored tool's refusal: {err:?}");
    assert!(shows(&h, "Apply holder refuses this"), "the group shows it");
    let done = h.state().state.history.done_len();
    apply_holder(&mut h);
    assert_eq!(reference(&h, TOOL), None, "Apply holder refuses it too");
    assert_eq!(h.state().state.history.done_len(), done, "a refusal is no undo step");
}

/// The reverse: unapplied long flutes (25 mm) refuse a 20 mm projection,
/// but the stored 15 mm flutes take it. The group does not say that Apply
/// holder refuses; it gives a separate draft note, and Apply holder lands.
#[test]
fn longer_draft_flutes_give_a_draft_note_not_an_apply_refusal() {
    let mut h = window();
    draft(&mut h).flute_len = 25.0;
    pick(&mut h, "ER20 chuck");
    holder(&mut h).projection = 20.0;
    h.run_steps(2);
    let m = last();
    assert_eq!(m.holder_err, None, "Apply holder accepts the stored tool");
    let note = m.draft_err.clone().unwrap_or_default();
    assert!(note.contains("reaches into the flutes"), "the draft numbers get a note: {note:?}");
    assert!(!shows(&h, "Apply holder refuses this"), "no false refusal");
    assert!(shows(&h, "unapplied tool numbers"), "the draft note says what it judges");
    let gauge_len = holder(&mut h).gauge;
    apply_holder(&mut h);
    let want = HolderRef { entry: "ER20 chuck".into(), gauge_len, projection: 20.0 };
    assert_eq!(reference(&h, TOOL), Some(want), "status: {}", h.state().state.status);
}

/// The stored tool gets longer: the projection draft takes the new
/// stickout and says so; the stored `HolderRef` keeps its projection
/// until Apply holder stores the new one.
#[test]
fn a_tool_length_change_reseeds_the_projection_draft_only() {
    let mut h = window();
    pick(&mut h, "ER20 chuck");
    apply_holder(&mut h);
    let stored = reference(&h, TOOL).map(|r| r.projection);
    assert_eq!(stored, Some(55.0), "status: {}", h.state().state.status);
    assert!(!holder(&mut h).reseeded, "no length change yet");
    let shank = tool(&h, TOOL).shank_len;
    draft(&mut h).shank_len = shank + 10.0;
    click_window_button(&mut h, "Apply");
    h.run_steps(2);
    assert_eq!(tool(&h, TOOL).shank_len, shank + 10.0, "the tool is longer: {}", h.state().state.status);
    let d = holder(&mut h).clone();
    assert_eq!((d.projection, d.reseeded), (65.0, true), "the draft follows the new stickout");
    assert!(shows(&h, "The tool length changed"), "the group marks the re-seed");
    assert_eq!(reference(&h, TOOL).map(|r| r.projection), Some(55.0), "the stored HolderRef does not change by itself");
    apply_holder(&mut h);
    assert_eq!(reference(&h, TOOL).map(|r| r.projection), Some(65.0), "Apply holder stores the re-seed");
    assert!(!holder(&mut h).reseeded, "a landed holder is no longer marked");
}

/// Properties' shape window shows one tool and the library window
/// another; each keeps its own holder pick over frames.
#[test]
fn the_window_holder_draft_and_the_shape_window_do_not_mix() {
    let mut h = shell_harness();
    h.run_steps(2);
    let (a, b) = two_tools(&mut h);
    crate::actions::target(&mut h.state_mut().state, UiTarget::Tool(a.clone()));
    h.run_steps(2);
    crate::actions::open_tool_shape_editor(&mut h.state_mut().state, &a);
    h.run_steps(3);
    h.state_mut().state.pick_library_holder("BT30 ER20");
    open_in_window(&mut h, &b);
    pick(&mut h, "ER20 chuck");
    h.run_steps(4);
    let props = h.state().state.tool_profile_ui.clone();
    assert_eq!((props.tool.as_str(), props.library.as_deref()), (a.as_str(), Some("BT30 ER20")), "the shape window keeps its pick");
    assert_eq!(holder(&mut h).library.as_deref(), Some("ER20 chuck"), "the window keeps its pick");
}

/// #3930: the Body section imports into the window's own body draft. The
/// draft stores nothing; Apply body lands it as one `SetToolBody`, one
/// undo takes it off, and the shape window's body draft is not touched.
#[test]
fn the_body_section_imports_into_the_window_draft_only() {
    let mut h = tall_window(TOOL);
    assert!(!shows(&h, "Import body…"), "the Body section is folded");
    open_body(&mut h);
    for label in ["Holder diameter", "Holder length", "Import body…", "Clear body"] {
        assert!(shows(&h, label), "the Body section shows {label:?}");
    }
    let path = super::tool_body::temp_stl("window");
    super::tool_body::prism_stl(&path, 15.0, 0.0, 30.0, 24);
    let top = super::tool_body::shank_top(&h.state().state);
    h.state_mut().state.with_library_draft(|st, _| {
        st.import_tool_body_from(&path);
        st.tool_body_ui.offset = top;
    });
    h.run_steps(2);
    assert_eq!(body(&mut h).source, "holder.stl", "the window's draft holds the import");
    assert!(h.state().state.tool_body_ui.facets.is_empty(), "the shape window's body draft is not touched");
    let body_of = |h: &H| {
        let st = &h.state().state;
        model::find_by_name(&st.world, TOOL)
            .and_then(|e| camcraft_app::tool_body::body_of(&st.world, &e))
            .map(|b| b.source)
    };
    assert_eq!(body_of(&h), None, "a draft stores nothing");
    assert!(shows(&h, "Pick tip") && shows(&h, "Pick axis"), "the placement picks stand in the window");
    let done = h.state().state.history.done_len();
    click_window_button(&mut h, "Apply body");
    assert_eq!(body_of(&h).as_deref(), Some("holder.stl"), "status: {}", h.state().state.status);
    assert_eq!(h.state().state.history.done_len(), done + 1, "Apply body is one undo step");
    crate::actions::undo(&mut h.state_mut().state);
    h.run_steps(2);
    assert_eq!(body_of(&h), None, "one undo takes the body off");
    let _ = std::fs::remove_dir_all(path.parent().unwrap_or(std::path::Path::new(".")));
}

/// #3930: a turning tool takes no library holder, so the window shows no
/// picker; its Body section is where the holder is imported and placed
/// (the lathe frame), as in the tool shape window.
#[test]
fn a_turning_tool_has_the_body_section_and_no_holder_pick() {
    const INSERT: &str = "TN-1600-0R8";
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 1400.0)).build_ui_state(super::frame, ShellUi::seeded());
    h.run_steps(2);
    super::rack(&mut h.state_mut().state, &[INSERT]);
    h.run_steps(2);
    open_in_window(&mut h, INSERT);
    assert!(shows(&h, HEADING), "the group heading stands");
    assert!(!shows(&h, "Library holder"), "a turning tool has no holder pick");
    assert!(shows(&h, "import its holder in the Body section"), "the group says where the holder goes");
    open_body(&mut h);
    assert!(shows(&h, "Import body…"), "Import body… stands in the window's Body section");
    assert!(shows(&h, "the holder is not checked for collisions"), "a bare turning tool says so");
}
