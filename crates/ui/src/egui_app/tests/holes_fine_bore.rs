//! #1445: the drill page's fine-bore fields - the two shift rows show only
//! for the fine-bore cycle, they write the draft, and Apply refuses a fine
//! bore with no shift by name. Behaviour only: labels, values, commands.
use super::*;

const OP: &str = "Drill";

fn page() -> Harness<'static, ShellUi> {
    let mut h = Harness::builder()
        .with_size(Vec2::new(1600.0, 1100.0))
        .build_ui_state(frame, ShellUi::seeded());
    let st = &mut h.state_mut().state;
    rack(st, &["DR-0400"]);
    let index = camcraft_app::operation::ALL_LABELS.iter().position(|s| *s == OP).unwrap();
    st.pending.push_back(CommandKind::CreateOperation {
        name: OP.into(),
        tool: "DR-0400".into(),
        src: SrcRef::Stock,
        params: crate::operations::quick_params(index),
    });
    st.drain_actions();
    crate::actions::target(st, UiTarget::Operation(OP.into()));
    st.op_tab = super::super::op_tabs::SEC_PASSES;
    h.run_steps(3);
    h
}

fn set_cycle(h: &mut Harness<'static, ShellUi>, c: camcraft_app::drilling::CycleChoice) {
    match &mut h.state_mut().state.op_ui.params {
        OpParams::Drill { cycle, .. } => *cycle = c,
        _ => panic!("a drill page"),
    }
    h.run_steps(3);
}

#[test]
fn the_shift_rows_show_only_for_a_fine_bore() {
    let _g = snapshot_guard();
    let mut h = page();
    assert_eq!(h.query_all_by_label("Shift X").count(), 0, "no shift row on a plain drill");
    set_cycle(&mut h, crate::operations::cycle_from_index(9));
    for label in ["Shift X", "Shift Y"] {
        assert_eq!(h.query_all_by_label(label).count(), 1, "{label}; text {:?}", structural::painted(&h));
    }
    set_cycle(&mut h, camcraft_app::drilling::CycleChoice::Bore);
    assert_eq!(h.query_all_by_label("Shift Y").count(), 0, "the rows leave with the cycle");
}

#[test]
fn apply_refuses_a_fine_bore_with_no_shift_and_takes_one_with_a_shift() {
    let _g = snapshot_guard();
    let mut h = page();
    set_cycle(&mut h, crate::operations::cycle_from_index(9));
    let st = &mut h.state_mut().state;
    st.apply_operation_params(OP);
    st.drain_actions();
    assert!(st.status.contains("fine bore needs a shift"), "refused by name: {}", st.status);

    let stated = camcraft_app::drilling::CycleChoice::FineBore { shift_x_um: 300, shift_y_um: 0 };
    set_cycle(&mut h, stated);
    let st = &mut h.state_mut().state;
    st.apply_operation_params(OP);
    st.drain_actions();
    assert_eq!(st.status, format!("parameters updated for {OP}"));
    let e = model::find_by_name(&st.world, OP).expect("the operation");
    match model::operation_of(&st.world, &e).expect("operation").params {
        OpParams::Drill { cycle, .. } => assert_eq!(cycle, stated),
        _ => panic!("a drill operation"),
    }
}
