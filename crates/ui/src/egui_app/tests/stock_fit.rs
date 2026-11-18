//! Structural pins for stock fitting and draft feedback (#1683).

use egui_kittest::kittest::NodeT;

use super::*;

fn disabled(h: &Harness<'static, ShellUi>, label: &str) -> bool {
    h.get_by_label(label).accesskit_node().is_disabled()
}

/// The Delete button of the Stock page. The ribbon has its own Delete
/// button (#1988), so the lookup keeps only the node inside the Properties pane.
fn stock_pane_delete<'h>(h: &'h Harness<'static, ShellUi>) -> egui_kittest::Node<'h> {
    let pane = h
        .state()
        .dock
        .iter_leaves()
        .find(|(_, l)| l.tabs.contains(&Tab::Properties))
        .map(|(_, l)| l.viewport)
        .expect("the properties leaf");
    let mut nodes = h.get_all_by_label("Delete").filter(|n| pane.contains(n.rect().center()));
    let node = nodes.next().expect("a Delete button on the Stock page");
    assert!(nodes.next().is_none(), "one Delete button on the Stock page");
    node
}

/// #1683: only parametric stock can be fitted around the models.
#[test]
fn stock_fit_controls_follow_the_selected_shape() {
    let mut h = shell_harness();
    h.state_mut().state.ui_target = UiTarget::Stock;
    h.run_steps(2);
    assert!(!disabled(&h, "Fit to Models"), "box stock can fit");
    assert!(h.query_by_label("Fit margin").is_some());
    h.get_by_label("Cylinder").click();
    h.run_steps(2);
    assert!(!disabled(&h, "Fit to Models"), "cylinder stock can fit");
    assert!(h.query_by_label("Fit margin").is_some());
    h.get_by_label("From model").click();
    h.run_steps(2);
    assert!(h.query_by_label("Fit margin").is_none(), "source stock has no fit margin");
    assert!(h.query_by_label("Fit to Models").is_none(), "source stock offers no fit action");
    assert!(!disabled(&h, "Apply"), "source stock can still be applied");
    let active = camcraft_app::setup::active(&h.state().state.world);
    let stock = camcraft_app::setup::stock_entity_name(&h.state().state.world, active.as_deref());
    assert!(model::find_by_name(&h.state().state.world, &stock).is_some());
    stock_pane_delete(&h).click();
    h.run_steps(2);
    assert!(model::find_by_name(&h.state().state.world, &stock).is_none(), "Delete still deletes stock");
    h.get_by_label("Box").click();
    h.run_steps(2);
    assert!(!disabled(&h, "Fit to Models"), "returning to box restores fit");
    assert!(h.query_by_label("Fit margin").is_some());
}

/// #1683: changing the draft shape retires an actual fit refusal.
#[test]
fn stock_shape_change_clears_the_fit_refusal() {
    let mut h = shell_harness();
    h.state_mut().state.ui_target = UiTarget::Stock;
    h.run_steps(2);
    h.get_by_label("From model").click();
    h.run_steps(2);
    h.state_mut().state.pending.push_back(CommandKind::StockFit { margin: 2.0, shape: 2 });
    h.run_steps(2);
    let refusal = "from-model stock follows its source: choose Box or Cylinder to fit";
    assert_eq!(h.state().state.error_for(crate::state::ErrorScope::Setup), Some(refusal));
    assert!(h.query_by_label(refusal).is_some(), "the command refusal is visible");
    h.get_by_label("Box").click();
    h.run_steps(2);
    assert_eq!(h.state().state.stock_ui.shape, 0);
    assert!(h.state().state.error_for(crate::state::ErrorScope::Setup).is_none(), "the old fit refusal retires");
    assert!(h.query_by_label(refusal).is_none(), "the page no longer shows the old refusal");
    assert_ne!(h.state().state.status, refusal, "the status strip retires the refusal too");
}

/// A stock draft change must leave another page's refusal intact.
#[test]
fn stock_shape_change_preserves_operation_feedback() {
    use crate::state::ErrorScope;
    let mut h = shell_harness();
    h.state_mut().state.ui_target = UiTarget::Stock;
    h.run_steps(2);
    let message = "operation generation refused";
    h.state_mut().state.feedback = Some((ErrorScope::Operation, message.into()));
    h.state_mut().state.status = message.into();
    h.state_mut().state.feedback_at = Some([1.0, 2.0, 3.0]);
    h.get_by_label("Cylinder").click();
    h.run_steps(2);
    let state = &h.state().state;
    assert_eq!(state.stock_ui.shape, 1);
    assert_eq!(state.error_for(ErrorScope::Operation), Some(message));
    assert_eq!(state.status, message);
    assert_eq!(state.feedback_at, Some([1.0, 2.0, 3.0]));
}

/// Retiring a fit refusal must preserve a newer status message.
#[test]
fn stock_shape_change_preserves_newer_status() {
    use crate::state::ErrorScope;
    let mut h = shell_harness();
    h.state_mut().state.ui_target = UiTarget::Stock;
    h.run_steps(2);
    h.get_by_label("From model").click();
    h.run_steps(2);
    h.state_mut().state.pending.push_back(CommandKind::StockFit { margin: 2.0, shape: 2 });
    h.run_steps(2);
    assert!(h.state().state.error_for(ErrorScope::Setup).is_some());
    let newer = "newer status after the fit refusal";
    h.state_mut().state.status = newer.into();
    h.get_by_label("Box").click();
    h.run_steps(2);
    let state = &h.state().state;
    assert_eq!(state.stock_ui.shape, 0);
    assert!(state.error_for(ErrorScope::Setup).is_none());
    assert_eq!(state.status, newer);
}
