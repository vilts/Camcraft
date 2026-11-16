//! #1509: 2D Adaptive's Passes tab carries the Method choice - Spiral
//! (the default) or Trochoidal; 2D Offset, which shares the pane, has
//! no such row. Provenance: our own
//! page law; asserted structurally (labels, the combo's value; no
//! pixels).

use super::*;
use camcraft_app::machining2d::AdaptiveMethod;
use camcraft_app::operation::{KindInfo, OpType};
use egui::accesskit::Role;

use super::super::kind_editors::adaptive_method_names;

fn kind(ty: OpType) -> usize {
    KindInfo::ALL.iter().position(|k| k.ty == ty).unwrap_or(usize::MAX)
}

/// The Passes tab of the draft retyped to `ty`, drawn.
fn passes_of(ty: OpType) -> Harness<'static, ShellUi> {
    let mut harness = shell_harness();
    let cur = harness.state().state.op_ui.params.clone();
    harness.state_mut().state.op_ui.params = crate::operations::retype(&cur, kind(ty));
    let passes = super::super::op_tabs::SECTIONS.iter().position(|s| *s == "Passes");
    harness.state_mut().state.op_tab = passes.unwrap_or(0);
    harness.run();
    harness
}

fn combos_showing(h: &Harness<'static, ShellUi>, text: &str) -> usize {
    h.query_all_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some(text)).count()
}

#[test]
fn the_adaptive_passes_tab_offers_the_method_and_defaults_to_spiral() {
    assert_eq!(adaptive_method_names(), ["Spiral", "Trochoidal"], "one name per method, in the combo's order");
    let mut harness = passes_of(OpType::Adaptive2D);
    assert_eq!(harness.query_all_by_label("Method").count(), 1, "the Method row");
    assert_eq!(combos_showing(&harness, "Spiral"), 1, "a new 2D Adaptive is Spiral");
    assert_eq!(combos_showing(&harness, "Trochoidal"), 0);
    // The draft on Trochoidal: the combo shows it.
    if let OpParams::Adaptive2D { method, .. } = &mut harness.state_mut().state.op_ui.params {
        *method = AdaptiveMethod::Trochoidal;
    } else {
        panic!("the draft is not a 2D Adaptive");
    }
    harness.run();
    assert_eq!(combos_showing(&harness, "Trochoidal"), 1, "the combo shows the draft's method");
    assert_eq!(combos_showing(&harness, "Spiral"), 0);
}

#[test]
fn the_offset_twin_has_no_method_row() {
    let harness = passes_of(OpType::Offset2D);
    assert_eq!(harness.query_all_by_label("Method").count(), 0, "2D Offset has one way of clearing");
}

#[test]
fn trochoidal_method_shows_its_passes_and_feeds_fields_and_spiral_and_offset_hide_them() {
    let passes = super::super::op_tabs::SECTIONS.iter().position(|s| *s == "Passes").unwrap_or(0);
    let feeds = super::super::op_tabs::SECTIONS.iter().position(|s| *s == "Feeds").unwrap_or(0);

    let mut harness = passes_of(OpType::Adaptive2D);
    for label in ["Max engagement", "Loop width", "Return"] {
        assert_eq!(harness.query_all_by_label(label).count(), 0, "Spiral hides {label}");
    }
    harness.state_mut().state.op_tab = feeds;
    harness.run();
    assert_eq!(harness.query_all_by_label("Return feed").count(), 0, "Spiral hides Return feed");

    if let OpParams::Adaptive2D { method, .. } = &mut harness.state_mut().state.op_ui.params {
        *method = AdaptiveMethod::Trochoidal;
    } else {
        panic!("the draft is not a 2D Adaptive");
    }
    harness.state_mut().state.op_tab = passes;
    harness.run();
    for label in ["Max engagement", "Loop width", "Return"] {
        assert_eq!(harness.query_all_by_label(label).count(), 1, "Trochoidal shows {label}");
    }
    harness.state_mut().state.op_tab = feeds;
    harness.run();
    assert_eq!(harness.query_all_by_label("Return feed").count(), 1, "Trochoidal shows Return feed");

    if let OpParams::Adaptive2D { troch, .. } = &mut harness.state_mut().state.op_ui.params {
        troch.ret = Some(camcraft_app::machining2d::TrochoidReturn::Microlift { height: 0.4 });
    }
    harness.state_mut().state.op_tab = passes;
    harness.run();
    assert_eq!(harness.query_all_by_label("Microlift height").count(), 1, "Microlift shows Microlift height");

    for ret in [Some(camcraft_app::machining2d::TrochoidReturn::Retract), None] {
        if let OpParams::Adaptive2D { troch, .. } = &mut harness.state_mut().state.op_ui.params {
            troch.ret = ret;
        }
        harness.run();
        assert_eq!(harness.query_all_by_label("Microlift height").count(), 0, "only explicit Microlift shows its height");
    }

    let mut off = passes_of(OpType::Offset2D);
    for label in ["Max engagement", "Loop width", "Return"] {
        assert_eq!(off.query_all_by_label(label).count(), 0, "Offset2D hides {label}");
    }
    off.state_mut().state.op_tab = feeds;
    off.run();
    assert_eq!(off.query_all_by_label("Return feed").count(), 0, "Offset2D hides Return feed");
}
