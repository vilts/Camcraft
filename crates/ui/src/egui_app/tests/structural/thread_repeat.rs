//! Provenance: our own structural assertion of the thread Passes page (#1848).

use super::*;
use camcraft_app::operation::{KindInfo, OpType};
use egui::accesskit::Role;

#[test]
fn thread_passes_repeat_each_depth_checkbox_has_label_role_and_enabled_state() {
    let _guard = snapshot_guard();
    let mut h = Harness::builder()
        .with_size(Vec2::new(1280.0, 1600.0))
        .build_ui_state(frame, ShellUi::seeded());
    let kind = KindInfo::ALL.iter().position(|k| k.ty == OpType::Threading).expect("threading kind");
    let params = h.state().state.op_ui.params.clone();
    h.state_mut().state.op_ui.params = crate::operations::retype(&params, kind);
    h.state_mut().state.op_tab = crate::egui_app::op_tabs::SECTIONS
        .iter()
        .position(|section| *section == "Passes")
        .expect("Passes tab");
    h.run_steps(3);

    let checkbox = h.get_by_role_and_label(Role::CheckBox, "Repeat each depth");
    assert_eq!(checkbox.accesskit_node().role(), Role::CheckBox, "the repeat control is a checkbox");
    assert!(!checkbox.accesskit_node().is_disabled(), "the repeat checkbox is enabled");
    assert_eq!(checkbox.accesskit_node().toggled(), Some(egui::accesskit::Toggled::False));
    checkbox.focus();
    h.run();
    h.key_press(egui::Key::Space);
    h.run();
    assert_eq!(
        h.get_by_role_and_label(Role::CheckBox, "Repeat each depth").accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True),
        "the named checkbox accepts a keyboard toggle"
    );
}
