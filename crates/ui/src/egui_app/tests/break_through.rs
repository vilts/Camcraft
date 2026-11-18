//! Structural pins for the hole pages' Break-through row (#2162 review).
//! The shipping shell stages the edit, Apply writes one command, and undo
//! restores the default. No pixels or test-local widget substitutes.

use super::*;
use camcraft_app::operation::OpType;
use egui::accesskit::{Role, Toggled};

fn heights(ty: OpType) -> Harness<'static, ShellUi> {
    let mut h = shell_harness();
    rack(&mut h.state_mut().state, &["DR-0400", "TM-0600-P125"]);
    crate::actions::add_operation(&mut h.state_mut().state, ty.ordinal());
    h.run_steps(2);
    assert_eq!(h.state().state.op_ui.params.kind(), ty);
    h.get_by_label("Heights").click();
    h.run_steps(2);
    h
}

fn run(params: &OpParams) -> Option<f64> {
    match params {
        OpParams::Drill { break_through, .. }
        | OpParams::Bore { break_through, .. }
        | OpParams::Thread { break_through, .. } => *break_through,
        _ => panic!("expected a hole operation"),
    }
}

fn live(h: &Harness<'static, ShellUi>) -> Option<f64> {
    let st = &h.state().state;
    let e = model::find_by_name(&st.world, &st.op_ui.name).expect("the operation exists");
    run(&model::operation_of(&st.world, &e)
        .expect("operation parameters")
        .params)
}

fn assert_default(h: &Harness<'static, ShellUi>, text: &str) {
    let row = h.get_by_label("Break-through");
    let toggle = h.get_by_role_and_label(Role::CheckBox, "Default");
    assert_eq!(toggle.accesskit_node().toggled(), Some(Toggled::True));
    assert!(
        !toggle.accesskit_node().is_disabled(),
        "the default can be overridden"
    );
    assert!(
        (toggle.rect().center().y - row.rect().center().y).abs() < 3.0,
        "the checkbox belongs to this row"
    );
    assert!(
        h.query_all_by_label(text)
            .any(|n| n.accesskit_node().is_disabled()),
        "the derived default is visible: {text}"
    );
    assert_eq!(run(&h.state().state.op_ui.params), None);
}

#[test]
fn drill_and_bore_break_through_rows_show_the_default_and_edit_control() {
    let _g = snapshot_guard();
    for ty in [OpType::Drill, OpType::Bore] {
        let mut h = heights(ty);
        assert_default(&h, "1.000 mm");
        h.get_by_role_and_label(Role::CheckBox, "Default")
            .scroll_to_me();
        h.run();
        h.get_by_role_and_label(Role::CheckBox, "Default").click();
        h.run_steps(2);
        assert_eq!(run(&h.state().state.op_ui.params), Some(1.0));
        let y = h.get_by_label("Break-through").rect().center().y;
        assert!(h.query_all_by_role(Role::SpinButton).any(|n|
            (n.rect().center().y - y).abs() < 3.0 && !n.accesskit_node().is_disabled()
        ), "{ty:?} exposes an editable length on the Break-through row");
    }
}

#[test]
fn thread_break_through_is_one_pitch_for_internal_and_absent_for_external() {
    let _g = snapshot_guard();
    let mut h = heights(OpType::Thread);
    assert_default(&h, "one pitch");
    h.get_by_role_and_label(Role::CheckBox, "Default")
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::CheckBox, "Default").click();
    h.run_steps(2);
    let y = h.get_by_label("Break-through").rect().center().y;
    assert!(
        h.query_all_by_role(Role::SpinButton)
            .any(|n| (n.rect().center().y - y).abs() < 3.0 && !n.accesskit_node().is_disabled()),
        "an internal thread exposes an editable Break-through length"
    );
    if let OpParams::Thread { external, .. } = &mut h.state_mut().state.op_ui.params {
        *external = true;
    }
    h.run_steps(2);
    assert!(
        h.query_by_label("Break-through").is_none(),
        "an external thread has no through-hole exit"
    );
    assert!(
        h.query_by_role_and_label(Role::CheckBox, "Default")
            .is_none()
    );
}

#[test]
fn break_through_edits_apply_through_command_history_and_undo() {
    let _g = snapshot_guard();
    for ty in [OpType::Drill, OpType::Bore, OpType::Thread] {
        let mut h = heights(ty);
        assert_eq!(live(&h), None);
        h.get_by_role_and_label(Role::CheckBox, "Default")
            .scroll_to_me();
        h.run();
        h.get_by_role_and_label(Role::CheckBox, "Default").click();
        h.run_steps(2);
        assert_eq!(run(&h.state().state.op_ui.params), Some(1.0));
        let row_y = h.get_by_label("Break-through").rect().center().y;
        h.query_all_by_role(Role::SpinButton)
            .find(|n| (n.rect().center().y - row_y).abs() < 3.0)
            .expect("the Break-through length control")
            .click();
        h.run();
        h.query_all_by_role(Role::SpinButton)
            .find(|n| (n.rect().center().y - row_y).abs() < 3.0)
            .expect("the focused Break-through length control")
            .type_text("2.5");
        h.run_steps(2);
        assert_eq!(run(&h.state().state.op_ui.params), Some(2.5));
        assert_eq!(live(&h), None, "a staged edit must not write the document");
        h.get_by_label("Apply").scroll_to_me();
        h.run();
        h.get_by_label("Apply").click();
        h.run_steps(2);
        assert_eq!(live(&h), Some(2.5), "Apply writes the Break-through value");
        let st = &mut h.state_mut().state;
        assert!(
            st.history.undo(&st.world),
            "Apply must add an undoable command"
        );
        assert_eq!(live(&h), None, "one undo restores the default");
    }
}
