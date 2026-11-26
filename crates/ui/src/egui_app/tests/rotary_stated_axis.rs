//! #2772 parts 1-2, the operation page (the review of PR #3674, item 5):
//! the "Stated rotation axis" group on the rotary kinds' Passes tab,
//! asserted structurally per crates/ui/AGENTS.md - labels, roles,
//! enabled state, the parameter commit and its undo - across the
//! three rotary kinds. The help text follows the resolver's own order
//! (item 6): the machine's stated line first, the model's fit only
//! when nothing is stated (`rotary_stated::lines_of`). No pixels; the
//! release PNGs stay unchanged.
//! Provenance: our own page law; no vendor material.

use super::*;
use crate::operations::quick_params;
use camcraft_app::commands::{AddTool, CreateOperation, ImportModel};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::operation::{OpParams, OpType, SrcRef};
use camcraft_app::stock::StockShape;
use egui::accesskit::{Role, Toggled};
use egui_kittest::kittest::NodeT;

use super::super::dialog_tests::page;

const GROUP: &str = "Stated rotation axis";
const HELP_ORDER: &str = "the machine's stated rotary line, else fitted from the model";

/// Every painted text of the last frame, as `structural::painted` walks
/// it, on the properties-page harness (`painted` is typed on the shell).
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

/// The dialog's `opened` shape with the ball the rotary kinds cut with
/// (BN-0600): stock, part, tool, one operation "Op" of the given kind.
fn opened_rotary(kind: usize) -> State {
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
        CreateOperation::new(&state.world, "Op", "BN-0600", SrcRef::Stock, quick_params(kind)).expect("operation"),
    );
    state.pending.push_back(CommandKind::Select(Some("Op".into())));
    state.drain_actions();
    state.ui_target = UiTarget::Operation("Op".into());
    state.sync_ops_ui();
    state.op_tab = super::super::op_tabs::SEC_PASSES;
    state
}

/// The group's unnamed header checkbox: the one on the title's row.
fn group_checkbox<'a>(h: &'a Harness<'static, State>) -> egui_kittest::Node<'a> {
    let row = h.get_by_label(GROUP).rect();
    h.query_all_by(move |n| n.role() == Role::CheckBox)
        .find(|n| {
            let c = n.rect().center().y;
            c >= row.top() - 2.0 && c <= row.bottom() + 2.0
        })
        .expect("the group's checkbox on the title's row")
}

/// The draft's stated axis, whatever rotary kind the op is.
fn draft_axis(h: &Harness<'static, State>) -> Option<[f64; 3]> {
    match &h.state().op_ui.params {
        OpParams::RotaryContour { stated_axis, .. }
        | OpParams::RotaryParallel { stated_axis, .. }
        | OpParams::RotaryOffset { stated_axis, .. } => *stated_axis,
        _ => panic!("the draft is not a rotary kind"),
    }
}

/// The document's stated axis behind the draft, through the command
/// history's own record.
fn live_axis(state: &State) -> Option<[f64; 3]> {
    let e = model::find_by_name(&state.world, "Op").expect("the operation exists");
    match &model::operation_of(&state.world, &e).expect("operation parameters").params {
        OpParams::RotaryContour { stated_axis, .. }
        | OpParams::RotaryParallel { stated_axis, .. }
        | OpParams::RotaryOffset { stated_axis, .. } => *stated_axis,
        _ => panic!("the operation is not a rotary kind"),
    }
}

/// The control on `label`'s row: the node of `role` whose vertical
/// centre falls inside the label's rect (the dialog's own `on_row`).
fn on_row<'a>(h: &'a Harness<'static, State>, label: &str, role: Role) -> egui_kittest::Node<'a> {
    let lab = h.get_by_label(label).rect();
    h.query_all_by(move |n| n.role() == role)
        .find(|n| {
            let c = n.rect().center().y;
            c >= lab.top() - 2.0 && c <= lab.bottom() + 2.0
        })
        .expect("a control on the label's row")
}

#[test]
fn the_stated_axis_group_arms_edits_and_undoes_on_every_rotary_kind() {
    for kind in [
        OpType::RotaryContour as usize,
        OpType::RotaryParallel as usize,
        OpType::RotaryOffset as usize,
    ] {
        let mut h = page(opened_rotary(kind));
        h.run();
        // Disarmed: the group is named, its checkbox is enabled and
        // off, and no direction row is drawn.
        assert!(h.query_by_label(GROUP).is_some(), "kind {kind}: the group is drawn");
        let toggle = group_checkbox(&h);
        assert_eq!(toggle.accesskit_node().toggled(), Some(Toggled::False), "kind {kind}: off by default");
        assert!(!toggle.accesskit_node().is_disabled(), "kind {kind}: the axis can be stated");
        for row in ["Direction X", "Direction Y", "Direction Z"] {
            assert!(h.query_by_label(row).is_none(), "kind {kind}: no {row} while unstated");
        }
        assert_eq!(draft_axis(&h), None, "kind {kind}: the draft is unstated");
        // Arm: the seed direction appears with editable rows.
        group_checkbox(&h).click();
        h.run_steps(2);
        assert_eq!(draft_axis(&h), Some([1.0, 0.0, 0.0]), "kind {kind}: arming seeds X");
        assert_eq!(group_checkbox(&h).accesskit_node().toggled(), Some(Toggled::True));
        for row in ["Direction X", "Direction Y", "Direction Z"] {
            let spin = on_row(&h, row, Role::SpinButton);
            assert!(!spin.accesskit_node().is_disabled(), "kind {kind}: {row} is editable");
        }
        // Edit: Direction Y reads 1 - a staged draft, the document waits.
        on_row(&h, "Direction Y", Role::SpinButton).click();
        h.run();
        on_row(&h, "Direction Y", Role::SpinButton).type_text("1");
        h.run_steps(2);
        assert_eq!(draft_axis(&h), Some([1.0, 1.0, 0.0]), "kind {kind}: the draft took the typed Y");
        assert_eq!(live_axis(h.state()), None, "kind {kind}: a staged edit must not write the document");
        // Apply writes one command; one undo restores the unstated op.
        h.get_by_label("Apply").click();
        h.run();
        h.state_mut().drain_actions();
        assert_eq!(live_axis(h.state()), Some([1.0, 1.0, 0.0]), "kind {kind}: Apply commits the stated axis");
        let st = h.state_mut();
        assert!(st.history.undo(&st.world), "kind {kind}: Apply must add an undoable command");
        assert_eq!(live_axis(h.state()), None, "kind {kind}: one undo restores the unstated op");
    }
}

#[test]
fn the_help_text_follows_the_resolver_machine_line_first() {
    // The resolver reads the machine's stated line before the fit
    // (`rotary_stated::lines_of`); the visible help says the same
    // order, on the armed group where the hint rides the last
    // direction row's hover.
    let mut h = page(opened_rotary(OpType::RotaryParallel as usize));
    h.run();
    group_checkbox(&h).click();
    h.run_steps(2);
    assert!(h.query_by_label("Direction Z").is_some(), "the armed group draws its rows");
    h.ctx
        .style_mut_of(h.ctx.theme(), |s| s.interaction.tooltip_delay = 0.0);
    h.hover_at(h.get_by_label("Direction Z").rect().center());
    h.run_steps(2);
    let texts = painted_texts(&h);
    assert!(
        texts.iter().any(|t| t.contains("absent: ") && t.contains(HELP_ORDER)),
        "the help names the machine's line before the fit; painted: {texts:?}"
    );
    // And the painted order is never the reversed one.
    assert!(
        !texts.iter().any(|t| t.contains("absent: fitted from the model, else the machine")),
        "the retired order is gone from the page; painted: {texts:?}"
    );
}
