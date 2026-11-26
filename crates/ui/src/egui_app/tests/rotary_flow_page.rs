//! #2827, the property panel: the Rotary Flow band's Passes tab draws
//! the stated rotation axis group (#2772's editor), the stepover row,
//! the pose trio worded off the radial (`rotary_axis_editor`) and the
//! tolerance row - and the guide hint says the picked edge chains span
//! the band. Asserted structurally per crates/ui/AGENTS.md: labels,
//! roles, enabled state, painted text. No pixels; the release PNGs stay
//! unchanged.
//! Provenance: our own page law; no vendor material.

use super::*;
use crate::operations::quick_params;
use camcraft_app::commands::{AddTool, CreateOperation, ImportModel};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::operation::OpParams;
use camcraft_app::stock::StockShape;
use egui::accesskit::Role;
use egui_kittest::kittest::NodeT;

use super::super::dialog_tests::page;

/// The dialog's `opened` shape with the ball the flow band cuts with
/// (BN-0600): stock, part, tool, one Rotary Flow "Op".
fn opened_flow() -> State {
    let mut state = State::new();
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    state.drain_actions();
    let ball = camcraft_app::tools::catalog()
        .into_iter()
        .find(|(n, _)| *n == "BN-0600")
        .map(|(_, t)| t)
        .expect("the catalog's ball-nose entry");
    state
        .history
        .execute(&state.world, AddTool::new(&state.world, "BN-0600", ball).expect("tool"));
    state.history.execute(
        &state.world,
        ImportModel::from_mesh("part", cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 10.0))),
    );
    state.history.execute(
        &state.world,
        CreateOperation::new(
            &state.world,
            "Op",
            "BN-0600",
            camcraft_app::operation::SrcRef::Stock,
            quick_params(camcraft_app::operation::OpType::RotaryFlow as usize),
        )
        .expect("operation"),
    );
    state.pending.push_back(CommandKind::Select(Some("Op".into())));
    state.drain_actions();
    state.ui_target = UiTarget::Operation("Op".into());
    state.sync_ops_ui();
    state.op_tab = super::super::op_tabs::SEC_PASSES;
    state
}

/// Every painted text of the last frame, as `structural::painted` walks
/// it, on the properties-page harness.
fn painted_texts(h: &Harness<'static, State>) -> Vec<String> {
    fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(t) => out.push(t.galley.text().to_owned()),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for s in &h.output().shapes {
        walk(&s.shape, &mut out);
    }
    out
}

#[test]
fn the_flow_passes_tab_draws_the_axis_group_the_band_rows_and_the_guide_hint() {
    let mut h = page(opened_flow());
    h.run();
    // The stated rotation axis group, armed and off like the twins'.
    assert!(h.query_by_label("Stated rotation axis").is_some(), "the axis group is drawn");
    // The band rows are named and editable.
    for label in ["Stepover", "Tool lead angle", "Tool lag angle", "Max tilt", "Tolerance"] {
        let row = h.get_by_label(label);
        assert!(!row.accesskit_node().is_disabled(), "{label} is enabled");
    }
    h.get_by_label("Stepover");
    let spin = h.query_all_by(|n| n.role() == Role::SpinButton).count();
    // Stepover is Auto by default (no spin), so 4: the pose trio and the
    // tolerance row.
    assert!(spin >= 4, "the number rows draw spin buttons: {spin}");
    // The guide hint rides the preceding row's hover, like every hint.
    h.ctx
        .style_mut_of(h.ctx.theme(), |s| s.interaction.tooltip_delay = 0.0);
    h.hover_at(h.get_by_label("Tolerance").rect().center());
    h.run_steps(2);
    let texts = painted_texts(&h);
    assert!(
        texts.iter().any(|t| t.contains("picked edge chains") && t.contains("Geometry tab")),
        "the guide hint is painted; painted: {texts:?}"
    );
}

/// The draft's optional retract radius, the flow's own field.
fn draft_retract(h: &Harness<'static, State>) -> Option<camcraft_app::operation::Height> {
    match &h.state().op_ui.params {
        OpParams::RotaryFlow { retract_z, .. } => *retract_z,
        _ => panic!("the draft is not a Rotary Flow"),
    }
}

/// The document's retract radius behind the draft.
fn live_retract(state: &State) -> Option<camcraft_app::operation::Height> {
    let e = model::find_by_name(&state.world, "Op").expect("the operation exists");
    match &model::operation_of(&state.world, &e).expect("operation parameters").params {
        OpParams::RotaryFlow { retract_z, .. } => *retract_z,
        _ => panic!("the operation is not a Rotary Flow"),
    }
}

/// The draft's stated axis, the flow's own field.
fn draft_axis(h: &Harness<'static, State>) -> Option<[f64; 3]> {
    match &h.state().op_ui.params {
        OpParams::RotaryFlow { stated_axis, .. } => *stated_axis,
        _ => panic!("the draft is not a Rotary Flow"),
    }
}

fn live_axis(state: &State) -> Option<[f64; 3]> {
    let e = model::find_by_name(&state.world, "Op").expect("the operation exists");
    match &model::operation_of(&state.world, &e).expect("operation parameters").params {
        OpParams::RotaryFlow { stated_axis, .. } => *stated_axis,
        _ => panic!("the operation is not a Rotary Flow"),
    }
}

/// The group's unnamed header checkbox: the one on the title's row.
fn group_checkbox<'a>(h: &'a Harness<'static, State>, title: &str) -> egui_kittest::Node<'a> {
    let row = h.get_by_label(title).rect();
    h.query_all_by(move |n| n.role() == Role::CheckBox)
        .find(|n| {
            let c = n.rect().center().y;
            c >= row.top() - 2.0 && c <= row.bottom() + 2.0
        })
        .expect("the group's checkbox on the title's row")
}

#[test]
fn the_retract_hop_seeds_a_radius_below_the_clearance_radius() {
    let mut h = page(opened_flow());
    h.state_mut().op_tab = super::super::op_tabs::SEC_HEIGHTS;
    h.run();
    // Off: no radial row, the draft unstated.
    assert!(h.query_by_label("Retract radius").is_none(), "no radial row while off");
    assert_eq!(draft_retract(&h), None);
    // On: the row is named for the radius, the hint describes the
    // cylinder, and the seed sits at or below the clearance radius -
    // never above it (#4005 review item 3).
    h.get_by_label("Hop below clearance").click();
    h.run_steps(2);
    let row = h.get_by_label("Retract radius");
    assert!(!row.accesskit_node().is_disabled(), "the radius is editable");
    // The toggle and the displayed text: the row reads as a radius on
    // the shared reference, its readout one step of radius below the
    // clearance's (= Z 45.000) readout above.
    let (draft_c, draft_r) = match &h.state().op_ui.params {
        OpParams::RotaryFlow { clearance_z, retract_z, .. } => (*clearance_z, *retract_z),
        _ => panic!("the draft is not a Rotary Flow"),
    };
    let r = draft_r.expect("the toggle seeded the radius");
    assert_eq!(r.reference, draft_c.reference, "the seed shares the clearance's reference");
    assert!(r.offset <= draft_c.offset, "the seed is at or below clearance: {r:?} vs {draft_c:?}");
    let texts = painted_texts(&h);
    assert!(
        texts.iter().any(|t| t.contains("Retract radius")),
        "the radial row is named for the radius; painted: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t.contains("(= Z 44.000)")),
        "the seeded readout sits below the clearance's readout; painted: {texts:?}"
    );
    // Apply commits the hop; one undo restores the open band.
    h.get_by_label("Apply").click();
    h.run();
    h.state_mut().drain_actions();
    assert!(live_retract(h.state()).is_some(), "Apply commits the retract radius");
    let st = h.state_mut();
    assert!(st.history.undo(&st.world), "Apply must add an undoable command");
    assert_eq!(live_retract(h.state()), None, "one undo restores the open band");
}

#[test]
fn the_stated_axis_arms_edits_applies_and_undoes_on_the_flow() {
    let mut h = page(opened_flow());
    h.run();
    assert!(h.query_by_label("Stated rotation axis").is_some(), "the group is drawn");
    let toggle = group_checkbox(&h, "Stated rotation axis");
    assert_eq!(toggle.accesskit_node().toggled(), Some(egui::accesskit::Toggled::False), "off by default");
    assert!(!toggle.accesskit_node().is_disabled(), "the axis can be stated");
    // Arm: the seed direction appears in the draft, not the document.
    toggle.click();
    h.run_steps(2);
    assert_eq!(draft_axis(&h), Some([1.0, 0.0, 0.0]), "arming seeds X");
    assert_eq!(live_axis(h.state()), None, "a staged arm must not write the document");
    // Apply commits one command; one undo restores the unstated op.
    h.get_by_label("Apply").click();
    h.run();
    h.state_mut().drain_actions();
    assert_eq!(live_axis(h.state()), Some([1.0, 0.0, 0.0]), "Apply commits the stated axis");
    let st = h.state_mut();
    assert!(st.history.undo(&st.world), "Apply must add an undoable command");
    assert_eq!(live_axis(h.state()), None, "one undo restores the unstated op");
}
