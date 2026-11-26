//! Structural pins for #2400: relief modes, T-bone directions, and Apply/undo.
//! Gate map (M4.4): covers ui::egui_app::kind_editors, ui::egui_app::mill2d_page.
//! Provenance: our own UI laws over the shipping shell and undoable commands.

use super::*;
use camcraft_app::machining2d::{CornerRelief, CornerReliefKind, TBoneDirection};
use camcraft_app::operation::OpType;
use egui::accesskit::Role;

fn passes(ty: OpType) -> Harness<'static, ShellUi> {
    let mut h = Harness::builder()
        .with_size(Vec2::new(1280.0, 1600.0))
        .build_ui_state(frame, ShellUi::seeded());
    crate::actions::add_operation(&mut h.state_mut().state, ty.ordinal());
    h.run_steps(2);
    assert_eq!(h.state().state.op_ui.params.kind(), ty);
    h.get_by_label("Passes").click();
    h.run_steps(2);
    h
}

fn relief(params: &OpParams) -> &CornerRelief {
    match params {
        OpParams::Pocket { corner_relief, .. }
        | OpParams::Adaptive2D { corner_relief, .. }
        | OpParams::Offset2D { corner_relief, .. } => corner_relief,
        _ => panic!("expected a corner-relief operation"),
    }
}

fn live(h: &Harness<'static, ShellUi>) -> CornerRelief {
    let st = &h.state().state;
    let e = model::find_by_name(&st.world, &st.op_ui.name).expect("operation");
    relief(&model::operation_of(&st.world, &e).expect("parameters").params).clone()
}

fn choose(h: &mut Harness<'static, ShellUi>, row: &str, current: &str, next: &str) {
    let y = h.get_by_label(row).rect().center().y;
    let control = h
        .query_all_by_role(Role::ComboBox)
        .find(|n| (n.rect().center().y - y).abs() < 3.0)
        .expect("the row has a combo");
    assert_eq!(control.value().as_deref(), Some(current));
    assert!(!control.accesskit_node().is_disabled(), "{row} is editable");
    control.click();
    h.run_steps(2);
    h.get_by_label(next).click();
    h.run_steps(2);
}

#[test]
fn relief_modes_and_direction_controls_edit_each_supported_passes_page() {
    let _guard = snapshot_guard();
    for ty in [OpType::Pocket, OpType::Adaptive2D, OpType::Offset2D] {
        let mut h = passes(ty);
        assert_eq!(h.query_all_by_label("Corner relief").count(), 1);
        assert!(h.query_by_label("T-bone direction").is_none());
        assert_eq!(relief(&h.state().state.op_ui.params).kind, CornerReliefKind::None);
        choose(&mut h, "Corner relief", "None", "Dogbone");
        assert_eq!(relief(&h.state().state.op_ui.params).kind, CornerReliefKind::Dogbone);
        assert!(h.query_by_label("T-bone direction").is_none());
        choose(&mut h, "Corner relief", "Dogbone", "T-bone");
        assert_eq!(relief(&h.state().state.op_ui.params).kind, CornerReliefKind::TBone);
        assert_eq!(h.query_all_by_label("T-bone direction").count(), 1);
        let mut before = "Long edge";
        for (label, direction) in [
            ("Short edge", TBoneDirection::ShortEdge),
            ("Horizontal", TBoneDirection::Horizontal),
            ("Vertical", TBoneDirection::Vertical),
            ("Long edge", TBoneDirection::LongEdge),
        ] {
            choose(&mut h, "T-bone direction", before, label);
            assert_eq!(relief(&h.state().state.op_ui.params).tbone_direction, direction);
            before = label;
        }
        assert!(live(&h).is_off(), "draft edits do not change the document");
        choose(&mut h, "Corner relief", "T-bone", "None");
        assert!(relief(&h.state().state.op_ui.params).is_off());
        assert!(h.query_by_label("T-bone direction").is_none());
    }
}

/// #2410: corner relief is a router option. A mill hides the row; a router
/// shows it; a relief already stated stays shown on the mill.
#[test]
fn relief_row_shows_on_a_router_and_not_on_a_mill() {
    let _guard = snapshot_guard();
    let mount = |h: &mut Harness<'static, ShellUi>, name: &str| {
        let st = &mut h.state_mut().state;
        let cmd = camcraft_app::commands::LoadMachine::builtin(&st.world, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        st.history.execute(&st.world, cmd);
        h.run_steps(2);
    };
    for ty in [OpType::Pocket, OpType::Adaptive2D, OpType::Offset2D] {
        let mut h = passes(ty);
        mount(&mut h, "VMC 3-axis");
        assert!(h.query_by_label("Corner relief").is_none(), "{ty:?}: a mill offers no corner relief");
        mount(&mut h, "Tormach 24R");
        assert_eq!(h.query_all_by_label("Corner relief").count(), 1, "{ty:?}: a router offers it");
        choose(&mut h, "Corner relief", "None", "Dogbone");
        mount(&mut h, "VMC 3-axis");
        assert_eq!(h.query_all_by_label("Corner relief").count(), 1, "{ty:?}: a stated relief stays shown");
    }
}

#[test]
fn relief_mode_and_direction_apply_as_one_undoable_command() {
    let _guard = snapshot_guard();
    for ty in [OpType::Pocket, OpType::Adaptive2D, OpType::Offset2D] {
        let mut h = passes(ty);
        let original = live(&h);
        choose(&mut h, "Corner relief", "None", "T-bone");
        choose(&mut h, "T-bone direction", "Long edge", "Vertical");
        assert_eq!(live(&h), original, "the edit stays in the draft");
        h.get_by_label("Apply").click();
        h.run_steps(2);
        let applied = live(&h);
        assert_eq!(applied.kind, CornerReliefKind::TBone);
        assert_eq!(applied.tbone_direction, TBoneDirection::Vertical);
        let st = &mut h.state_mut().state;
        assert!(st.history.undo(&st.world));
        assert_eq!(live(&h), original, "one undo restores both fields");
    }
}
