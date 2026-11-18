//! #1557: the hole kinds expose the stated allowance and the document unit.
use super::*;
use egui_kittest::kittest::NodeT;

fn page(kind: &str, unit: Unit) -> Harness<'static, ShellUi> {
    let mut h = Harness::builder()
        .with_size(Vec2::new(1600.0, 1100.0))
        .build_ui_state(frame, ShellUi::seeded());
    let st = &mut h.state_mut().state;
    if unit != Unit::Millimeters {
        st.pending.push_back(CommandKind::SetUnits(unit));
        st.drain_actions();
    }
    rack(st, &["DR-0400", "TM-0600-P125"]);
    let index = camcraft_app::operation::ALL_LABELS
        .iter()
        .position(|s| *s == kind)
        .unwrap();
    let tool = match kind {
        "Drill" => "DR-0400",
        "Thread" => "TM-0600-P125",
        _ => "EM-0600-4FL",
    };
    st.pending.push_back(CommandKind::CreateOperation {
        name: kind.into(),
        tool: tool.into(),
        src: SrcRef::Stock,
        params: crate::operations::quick_params(index),
    });
    st.drain_actions();
    crate::actions::target(st, UiTarget::Operation(kind.into()));
    st.op_tab = super::super::op_tabs::SEC_HEIGHTS;
    h.run_steps(3);
    h
}

fn allowance(params: &OpParams) -> Option<f64> {
    match params {
        OpParams::Drill { break_through, .. }
        | OpParams::Bore { break_through, .. }
        | OpParams::Thread { break_through, .. } => *break_through,
        _ => panic!("hole kind"),
    }
}

#[test]
fn hole_break_through_default_controls_and_document_units() {
    let _g = snapshot_guard();
    for kind in ["Drill", "Bore", "Thread"] {
        for (unit, derived, entered) in [
            (Unit::Millimeters, "1.000 mm", "1.0 mm"),
            (Unit::Inches, "0.03937 in", "0.0394 in"),
        ] {
            let mut h = page(kind, unit);
            assert_eq!(
                h.query_all_by_label("Break-through").count(),
                1,
                "{kind}; text {:?}",
                structural::painted(&h)
            );
            let toggle = h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Default");
            assert_eq!(
                toggle.accesskit_node().toggled(),
                Some(egui::accesskit::Toggled::True)
            );
            let expected = if kind == "Thread" {
                "one pitch"
            } else {
                derived
            };
            assert!(
                h.query_by_label(expected).is_some(),
                "{kind} default {expected}"
            );
            assert_eq!(allowance(&h.state().state.op_ui.params), None);
            toggle.click();
            h.run_steps(3);
            assert_eq!(
                allowance(&h.state().state.op_ui.params),
                Some(1.0),
                "{kind} default toggle states length"
            );
            let label_y = h.get_by_label("Break-through").rect().center().y;
            assert!(
                h.query_all_by_role(egui::accesskit::Role::SpinButton)
                    .any(|n| n.value().as_deref() == Some(entered)
                        && (n.rect().center().y - label_y).abs() < 15.0),
                "{kind} numeric allowance uses document units {entered}; values {:?}",
                h.query_all_by_role(egui::accesskit::Role::SpinButton)
                    .map(|n| n.value())
                    .collect::<Vec<_>>()
            );
            h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Default")
                .click();
            h.run_steps(3);
            assert_eq!(
                allowance(&h.state().state.op_ui.params),
                None,
                "{kind} restores kind default"
            );
            h.state_mut().state.op_tab = super::super::op_tabs::SEC_PASSES;
            h.run_steps(3);
            assert!(
                h.query_by_label("Break-through").is_none(),
                "{kind}: Heights only"
            );
        }
    }
}

#[test]
fn external_thread_has_no_hole_break_through_control() {
    let _g = snapshot_guard();
    let mut h = page("Thread", Unit::Millimeters);
    let OpParams::Thread { external, .. } = &mut h.state_mut().state.op_ui.params else {
        unreachable!()
    };
    *external = true;
    h.run_steps(3);
    assert!(h.query_by_label("Break-through").is_none());
}
