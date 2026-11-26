//! #1894 review: the Multi-Axis Deburr page tells the truth about its
//! guards. The shank and holder guard has no switch (the kernel always
//! runs it), so the page shows it as always on and has no "Avoid shank
//! collisions" check box. The Geometry tab says the stock and part law
//! before the user meets the refusal. Behaviour only: labels and text.
use super::*;

const OP: &str = "Multi-Axis Deburr";

fn page(tab: usize) -> Harness<'static, ShellUi> {
    let mut h = Harness::builder()
        .with_size(Vec2::new(1600.0, 1100.0))
        .build_ui_state(frame, ShellUi::seeded());
    let st = &mut h.state_mut().state;
    rack(st, &["CM-0600-90"]);
    let index = camcraft_app::operation::ALL_LABELS.iter().position(|s| *s == OP).expect("the kind lists");
    st.pending.push_back(CommandKind::CreateOperation {
        name: OP.into(),
        tool: "CM-0600-90".into(),
        src: SrcRef::Stock,
        params: crate::operations::quick_params(index),
    });
    st.drain_actions();
    crate::actions::target(st, UiTarget::Operation(OP.into()));
    // The page draws the draft. Set it to the kind's quick-add params, as
    // `every_field_lives_on_exactly_one_section` does for each kind.
    st.op_ui.params = crate::operations::quick_params(index);
    assert!(matches!(st.op_ui.params, OpParams::MultiAxisDeburr { .. }), "the page drafts the kind: {:?}", st.op_ui.params);
    st.op_tab = tab;
    h.run_steps(3);
    h
}

#[test]
fn the_passes_tab_shows_the_shank_guard_as_always_on_with_no_switch() {
    let _g = snapshot_guard();
    let h = page(super::super::op_tabs::SEC_PASSES);
    assert_eq!(h.query_all_by_label("Avoid shank collisions").count(), 0, "no switch that does nothing");
    assert_eq!(h.query_all_by_label("Shank and holder guard").count(), 1, "text {:?}", structural::painted(&h));
    assert!(
        structural::painted(&h).iter().any(|(t, _)| t == "Always on"),
        "the guard reads as always on: {:?}",
        structural::painted(&h)
    );
}

#[test]
fn the_geometry_tab_states_the_stock_and_part_law() {
    let _g = snapshot_guard();
    let h = page(super::super::op_tabs::SEC_GEO);
    let text = structural::painted(&h);
    assert!(
        text.iter().any(|(t, _)| t.contains("Stock must be the source model") && t.contains("no other part")),
        "the page says the law before the refusal: {text:?}"
    );
}
