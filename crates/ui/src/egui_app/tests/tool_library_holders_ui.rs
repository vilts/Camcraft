//! #3897 (U6 of #3663): the Holders page of the tool library window. The
//! list groups the built-in holders by taper with the count of the tools
//! that use each one; a holder opens in a read-only view that lists those
//! tools (a click opens the tool in the editor) and draws the outline,
//! built only when its input changes. No click here queues a command.
//! Gate map: covers ui::egui_app::tool_library_dialog, ui::state.
//! Provenance: our own structural tests; no HSMWorks material involved.

use egui::accesskit::Role;

use super::tool_library_editor::{H, click_window_button, library_window};
use super::tool_library_mockup::{HOLDER, seeded_in_holder};
use super::*;
use crate::egui_app::tool_library_dialog::{HOLDERS_PAGE, holder_view, holders};
use crate::state::LibraryPage;

/// Open the window on its Holders page.
pub(super) fn open_holders(h: &mut H) {
    h.get_by_role_and_label(Role::Button, "Tools").click();
    h.run_steps(2);
    click_window_button(h, HOLDERS_PAGE);
}

/// Narrow the holder rows to `HOLDER`, so its row is in view (a long
/// list scrolls, and a click lands where the row is drawn).
pub(super) fn filter_to_holder(h: &mut H) {
    if let Some(v) = h.state_mut().state.tool_library.as_mut() {
        v.holders.filter = HOLDER.to_owned();
    }
    h.run_steps(3);
}

fn has(h: &H, label: &str) -> bool {
    library_window(h).query_all_by_label(label).next().is_some()
}

#[test]
fn a_holder_lists_the_tools_that_use_it() {
    let mut h = shell_harness();
    h.run_steps(2);
    let name = seeded_in_holder(&mut h);
    open_holders(&mut h);
    let done = h.state().state.history.done_len();
    assert!(has(&h, holders::ALL), "the taper column shows");
    filter_to_holder(&mut h);
    assert!(has(&h, "1 tool"), "the list counts the one tool in {HOLDER}");
    click_window_button(&mut h, HOLDER);
    let view = h.state().state.tool_library.as_ref().map(|v| v.holders.open.clone());
    assert_eq!(view, Some(Some(HOLDER.to_owned())), "the holder view is open");
    assert!(has(&h, holder_view::BACK), "the view has its way back");
    assert!(has(&h, holders::READ_ONLY), "the view says it is read-only");
    assert!(has(&h, &name), "the view lists {name}");
    assert!(has(&h, "ER32 collet"), "the view shows the clamp");
    // A click on the tool opens it in the editor, on the tool list.
    library_window(&h).get_by_label(&name).click();
    h.run_steps(3);
    let st = &h.state().state;
    let v = st.tool_library.as_ref().unwrap_or_else(|| panic!("the window stays open"));
    assert_eq!(v.page, LibraryPage::Tools, "the tool opens on the tool list");
    assert_eq!(v.open_tool(), Some(name.as_str()), "the tool is open in the editor");
    assert_eq!(st.history.done_len(), done, "browsing holders lands no command");
    assert!(st.pending.is_empty(), "and queues none");
}

#[test]
fn the_machine_taper_comes_first_and_a_taper_filters_the_list() {
    let rows = holders::rows();
    assert!(rows.len() >= 40, "the built-in library: {} holders", rows.len());
    let tapers = holders::tapers(&rows, Some("bt40"));
    assert_eq!(tapers.first().copied(), Some("BT40"), "the machine's taper first: {tapers:?}");
    let unsorted = holders::tapers(&rows, None);
    assert_eq!(tapers.len(), unsorted.len(), "every taper stays");
    let mut h = shell_harness();
    h.run_steps(2);
    open_holders(&mut h);
    assert!(has(&h, "BT30 ER20"), "all holders show");
    click_window_button(&mut h, "CAT40");
    assert!(has(&h, "CAT40 ER32") && !has(&h, "BT30 ER20"), "the CAT40 pick shows its holders only");
    click_window_button(&mut h, holders::ALL);
    assert!(has(&h, "BT30 ER20"), "All holders shows them again");
}

#[test]
fn a_still_frame_builds_no_holder_outline() {
    let mut h = shell_harness();
    h.run_steps(2);
    open_holders(&mut h);
    filter_to_holder(&mut h);
    click_window_button(&mut h, HOLDER);
    let b = holder_view::builds();
    h.run_steps(3);
    assert_eq!(holder_view::builds(), b, "a still frame builds no outline");
    // A changed input (the dimension toggle) builds it once, then holds.
    library_window(&h).get_by_label(holder_view::DIMS).click();
    h.run_steps(2);
    let b1 = holder_view::builds();
    assert!(b1 > b, "the toggle builds the outline again");
    h.run_steps(3);
    assert_eq!(holder_view::builds(), b1, "and then holds");
    assert!(h.state().state.tool_library.as_ref().is_some_and(|v| v.holders.hide_dims), "the toggle is view state");
}
