//! Structural UI check for #3349: an Engrave operation shows the
//! "Do not touch" controls, takes face and edge picks, and highlights them.
//! Provenance: our own tests over our own shell; no vendor material.
//! Gate map (M4.4): covers ui::egui_app::boundary, ui::avoid_viz, ui::boundary_draft.

use super::do_not_touch::{expand_boundary, seed_part_and_offset};
use super::*;
use camcraft_app::operation::OpType;

#[test]
fn engrave_boundaries_shows_do_not_touch_controls_and_takes_picks() {
    let _g = snapshot_guard();
    let mut h = Harness::builder()
        .with_size(Vec2::new(1400.0, 1200.0))
        .build_ui_state(frame, ShellUi::seeded());
    h.run_steps(2);
    let (_face_top, face_other, edge) = {
        let st = &mut h.state_mut().state;
        seed_part_and_offset(st, OpType::Engrave)
    };
    h.run_steps(3);
    expand_boundary(&mut h);

    assert!(h.query_by_label_contains("Do not touch").is_some());
    assert!(h.query_by_label("Pick protected").is_some(), "Engrave must expose Pick protected");
    assert!(h.query_by_label_contains("operations only so far").is_none());

    h.get_by_label("Pick protected").click();
    h.run_steps(2);
    assert_eq!(h.state().state.boundary_pick, Some(crate::state::BoundaryPick::DoNotTouch));

    {
        let st = &mut h.state_mut().state;
        let pick = crate::state::BoundaryPick::DoNotTouch;
        let mods = crate::boundary_draft::PickMods::default();
        crate::boundary_draft::take_pick(st, pick, face_other.clone(), mods);
        crate::boundary_draft::take_pick(st, pick, edge.clone(), mods);
        let marks = crate::avoid_viz::avoid_marks(st, 1.4);
        assert!(!marks.faces.is_empty(), "protected face must highlight for Engrave");
        assert!(!marks.edges.is_empty(), "protected edge must highlight for Engrave");
    }
    h.run_steps(2);
    assert!(h.query_by_label(&camcraft_app::operation::describe_sel(&face_other)).is_some());
    assert!(h.query_by_label(&camcraft_app::operation::describe_sel(&edge)).is_some());

    h.get_by_label("Apply").click();
    h.run_steps(2);
    let saved = camcraft_app::model::find_by_name(&h.state().state.world, "OpTest")
        .and_then(|e| camcraft_app::model::operation_of(&h.state().state.world, &e))
        .expect("saved engrave op");
    assert_eq!(saved.area.do_not_touch.picks, vec![face_other, edge]);
}
