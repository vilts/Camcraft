//! The dock layout law's frame tests: the viewport is permanent, a
//! closed pane comes back through the ribbon's View door at its default
//! side, Reset layout restores all three, and none of it touches the
//! command queue or the history (layout is view state).
//! Provenance: our own tests, specced from the egui_dock and
//! egui_kittest vendored sources; no HSMWorks material involved.

use super::super::layout;
use super::*;
use crate::state::{LayoutRequest, Panel};

fn blank_harness() -> Harness<'static, ShellUi> {
    Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, ShellUi::blank())
}

fn click(h: &mut Harness<'static, ShellUi>, at: egui::Pos2) {
    set_pos(h, at);
    button_at(h, egui::PointerButton::Primary, at, true);
    button_at(h, egui::PointerButton::Primary, at, false);
    h.run_steps(2);
}

/// The laid-out rect of the leaf holding `tab` (None when closed).
pub(super) fn leaf_rect(dock: &DockState<Tab>, tab: &Tab) -> Option<egui::Rect> {
    let path = dock.find_tab(tab)?;
    dock[path.node_path()].rect()
}

/// Nothing the layout law does may ride the command lane.
fn assert_no_commands(h: &Harness<'static, ShellUi>) {
    let st = &h.state().state;
    assert!(st.pending.is_empty(), "layout queued a command: {:?}", st.pending);
    assert!(st.history.is_empty(), "layout landed in the history");
}

#[test]
fn the_viewport_tab_is_permanent() {
    assert!(!layout::closeable(&Tab::Viewport), "the viewport must not close or float");
    assert!(layout::closeable(&Tab::Browser));
    assert!(layout::closeable(&Tab::Properties));
}

#[test]
fn a_closed_browser_comes_back_through_the_view_door() {
    let mut h = blank_harness();
    h.run();
    let vp = leaf_rect(&h.state().dock, &Tab::Viewport).expect("viewport laid out");
    // The tab's X removes the tab from the dock - the same removal.
    layout::apply(&mut h.state_mut().dock, LayoutRequest::Toggle(Panel::Browser));
    h.run_steps(2);
    assert!(!layout::is_open(&h.state().dock, &Tab::Browser));
    assert!(!h.state().state.panels_open.browser, "the mirror follows the dock");
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Panels").click();
    h.run_steps(2);
    assert!(h.state().state.ribbon_view_open, "pressing the door opens its list");
    h.get_by_role_and_label(egui::accesskit::Role::MenuItem, "Browser").click();
    h.run_steps(2);
    assert!(!h.state().state.ribbon_view_open, "a choice closes the list");
    let dock = &h.state().dock;
    assert!(layout::is_open(dock, &Tab::Browser), "the toggle reopens the browser");
    assert!(h.state().state.panels_open.browser);
    let br = leaf_rect(dock, &Tab::Browser).expect("browser laid out");
    let vp_now = leaf_rect(dock, &Tab::Viewport).expect("viewport laid out");
    assert!(br.right() <= vp_now.left() + 1.0, "the browser returns at the left: {br:?} vs {vp_now:?}");
    assert!((vp_now.width() - vp.width()).abs() < 2.0, "the viewport gets its width back");
    // And the same toggle closes it again.
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Panels").click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::MenuItem, "Browser").click();
    h.run_steps(2);
    assert!(!layout::is_open(&h.state().dock, &Tab::Browser));
    assert_no_commands(&h);
}

#[test]
fn a_closed_properties_pane_returns_at_the_right() {
    let mut h = blank_harness();
    h.run();
    layout::apply(&mut h.state_mut().dock, LayoutRequest::Toggle(Panel::Properties));
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Panels").click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::MenuItem, "Properties").click();
    h.run_steps(2);
    let dock = &h.state().dock;
    let pr = leaf_rect(dock, &Tab::Properties).expect("properties reopened");
    let vp = leaf_rect(dock, &Tab::Viewport).expect("viewport laid out");
    assert!(pr.left() + 1.0 >= vp.right(), "properties return at the right: {pr:?} vs {vp:?}");
    assert_no_commands(&h);
}

#[test]
fn reset_layout_restores_all_three_panes() {
    let mut h = blank_harness();
    h.run();
    let want: Vec<_> = [Tab::Browser, Tab::Viewport, Tab::Properties]
        .iter()
        .map(|t| leaf_rect(&h.state().dock, t).expect("default layout"))
        .collect();
    layout::apply(&mut h.state_mut().dock, LayoutRequest::Toggle(Panel::Browser));
    layout::apply(&mut h.state_mut().dock, LayoutRequest::Toggle(Panel::Properties));
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Panels").click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::MenuItem, "Reset layout").click();
    h.run_steps(2);
    let dock = &h.state().dock;
    for (t, w) in [Tab::Browser, Tab::Viewport, Tab::Properties].iter().zip(&want) {
        let got = leaf_rect(dock, t).expect("reset reopens every pane");
        assert!(
            (got.min - w.min).length() < 1.0 && (got.max - w.max).length() < 1.0,
            "{t:?} back at its default rect: {got:?} vs {w:?}"
        );
    }
    assert_no_commands(&h);
}

#[test]
fn the_viewport_row_is_inert_and_the_door_dismisses_outside() {
    let mut h = blank_harness();
    h.run();
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Panels").click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::MenuItem, "Viewport").click();
    h.run_steps(2);
    assert!(h.state().state.ribbon_view_open, "the always-open row keeps the list up");
    assert!(layout::is_open(&h.state().dock, &Tab::Viewport));
    // A press on the status strip, away from the door, dismisses it
    // (the viewport would pick, and a pick is a command).
    let panel_button = h
        .get_by_role_and_label(egui::accesskit::Role::Button, "Panels")
        .rect();
    let away: Vec<egui::Rect> = std::iter::once(panel_button)
        .chain(
            h.query_all_by_role(egui::accesskit::Role::MenuItem)
                .map(|node| node.rect()),
        )
        .collect();
    let outside = super::anchors::chrome_point(&h, &away);
    click(&mut h, outside);
    assert!(!h.state().state.ribbon_view_open, "an outside press closes the list");
    assert_no_commands(&h);
}

// ---------------------------------------------------------- Debug info

/// #2262: the Debug info panel is a dock tab, not a fixed window.
#[test]
fn the_debug_info_panel_is_present_docked_and_undocked() {
    let mut h = blank_harness();
    h.run();
    assert!(h.query_by_label("Copy").is_none(), "closed on a fresh state");
    assert!(!h.state().state.debug_info_open);
    // The status strip's door docks it, beside the properties pane.
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Debug info").click();
    h.run_steps(3);
    assert!(layout::is_open(&h.state().dock, &Tab::DebugInfo), "the door docks the panel");
    assert!(h.state().state.debug_info_open, "the flag mirrors the dock");
    assert!(h.query_by_label("Copy").is_some(), "the docked panel draws its Copy button");
    assert_no_commands(&h);
    // Undock: drag the tab out to a floating window.
    let path = h.state().dock.find_tab(&Tab::DebugInfo).expect("docked");
    h.state_mut().dock.remove_tab(path);
    h.state_mut().dock.add_window(vec![Tab::DebugInfo]);
    h.run_steps(3);
    assert!(layout::is_open(&h.state().dock, &Tab::DebugInfo), "a float still counts as open");
    assert!(h.query_by_label("Copy").is_some(), "the floating panel draws its Copy button");
    // The door closes it wherever it is.
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Debug info").click();
    h.run_steps(3);
    assert!(!layout::is_open(&h.state().dock, &Tab::DebugInfo));
    assert!(!h.state().state.debug_info_open);
    assert!(h.query_by_label("Copy").is_none());
    assert_no_commands(&h);
}

/// Click the status strip's Debug info door and let the dock settle.
fn debug_door(h: &mut Harness<'static, ShellUi>) {
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Debug info").click();
    h.run_steps(3);
}

/// #2262 review: on a document with operations the "N Operations" label
/// sat on the door centre and swallowed a pointer press. A press at the
/// centre of the door rect opens the panel on the seeded document.
#[test]
fn the_debug_info_door_opens_at_its_centre_on_a_document_with_operations() {
    let mut h = shell_harness();
    h.run();
    let st = &h.state().state;
    let ops = camcraft_app::model::named_entities(&st.world).into_iter().filter(|(_, e)| camcraft_app::model::is_operation(&st.world, e)).count();
    assert!(ops > 0, "the seed has operations: the status strip draws its count label");
    let door = h.get_by_role_and_label(egui::accesskit::Role::Button, "Debug info").rect();
    click(&mut h, door.center());
    assert!(layout::is_open(&h.state().dock, &Tab::DebugInfo), "a press at the door centre {door:?} opens the panel");
}

/// #2262: the door lands the panel as a tab in the properties pane's leaf,
/// like the Machines pane, not as a window of its own.
#[test]
fn the_debug_info_door_docks_a_tab_beside_properties() {
    let mut h = blank_harness();
    h.run();
    debug_door(&mut h);
    let dock = &h.state().dock;
    let d = dock.find_tab(&Tab::DebugInfo).expect("the door docked the panel");
    let p = dock.find_tab(&Tab::Properties).expect("properties still open");
    assert_eq!(d.node_path(), p.node_path(), "a tab in the properties pane's leaf");
    assert_eq!(h.state().state.layout_request, None, "the request was settled");
    assert_no_commands(&h);
}

/// #2262: the tab's own X (the dock removes the tab, no door press) closes
/// the panel and the open flag follows the dock. Replaces the old window's
/// close-button test (#698).
#[test]
fn closing_the_debug_info_tab_clears_the_flag_and_the_panel() {
    let mut h = blank_harness();
    h.run();
    debug_door(&mut h);
    assert!(h.state().state.debug_info_open);
    let path = h.state().dock.find_tab(&Tab::DebugInfo).expect("docked");
    h.state_mut().dock.remove_tab(path);
    h.run_steps(3);
    assert!(!h.state().state.debug_info_open, "the flag follows the dock");
    assert!(h.query_by_label("Copy").is_none(), "the panel is gone");
    // The door opens it again: closing left no stale state behind.
    debug_door(&mut h);
    assert!(layout::is_open(&h.state().dock, &Tab::DebugInfo));
    assert!(h.query_by_label("Copy").is_some());
    assert_no_commands(&h);
}

/// #2262: Copy puts the report on the clipboard and says so (the old
/// window's Copy test, kept for the docked panel).
#[test]
fn the_docked_debug_info_copy_button_copies_the_report() {
    let mut h = blank_harness();
    h.run();
    debug_door(&mut h);
    h.get_by_label("Copy").click();
    h.step();
    let copied = h.output().platform_output.commands.iter().any(|c| {
        matches!(c, egui::OutputCommand::CopyText(t)
            if t.starts_with("CamCraft ") && t.contains("Frame rate:"))
    });
    assert!(copied, "Copy put the report, frame-rate line included, on the clipboard");
    h.run();
    assert_eq!(h.state().state.status, "debug info copied to the clipboard");
    assert_no_commands(&h);
}
