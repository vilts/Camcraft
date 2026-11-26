//! #1591: 3D Pocket's Passes tab carries the Method choice - Spiral (the
//! default) or Trochoidal - only under the Adaptive strategy, which is
//! the one that reads it. Provenance: our own page law; asserted
//! structurally (labels, the combo's value; no pixels).

use super::*;
use camcraft_app::machining2d::{AdaptiveMethod, PocketStrategy};
use camcraft_app::operation::{KindInfo, OpType};
use egui::accesskit::Role;

/// The Passes tab of a 3D Pocket draft under `strategy`, drawn.
fn passes_under(strategy: PocketStrategy) -> Harness<'static, ShellUi> {
    let mut harness = shell_harness();
    let kind = KindInfo::ALL.iter().position(|k| k.ty == OpType::Pocket3D).unwrap_or(usize::MAX);
    let cur = harness.state().state.op_ui.params.clone();
    harness.state_mut().state.op_ui.params = crate::operations::retype(&cur, kind);
    if let OpParams::Pocket3D { strategy: s, .. } = &mut harness.state_mut().state.op_ui.params {
        *s = strategy;
    } else {
        panic!("the draft is not a 3D Pocket");
    }
    let passes = super::super::op_tabs::SECTIONS.iter().position(|s| *s == "Passes");
    harness.state_mut().state.op_tab = passes.unwrap_or(0);
    harness.run();
    harness
}

fn combos_showing(h: &Harness<'static, ShellUi>, text: &str) -> usize {
    h.query_all_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some(text)).count()
}

#[test]
fn the_3d_adaptive_passes_tab_offers_the_method_and_defaults_to_spiral() {
    let mut harness = passes_under(PocketStrategy::Adaptive);
    assert_eq!(harness.query_all_by_label("Method").count(), 1, "the Method row");
    assert_eq!(combos_showing(&harness, "Spiral"), 1, "a new 3D Adaptive is Spiral");
    assert_eq!(combos_showing(&harness, "Trochoidal"), 0);
    assert!(matches!(harness.state().state.op_ui.params, OpParams::Pocket3D { method: AdaptiveMethod::Spiral, .. }));
    // The draft on Trochoidal: the combo shows it.
    if let OpParams::Pocket3D { method, .. } = &mut harness.state_mut().state.op_ui.params {
        *method = AdaptiveMethod::Trochoidal;
    }
    harness.run();
    assert_eq!(combos_showing(&harness, "Trochoidal"), 1, "the combo shows the draft's method");
    assert_eq!(combos_showing(&harness, "Spiral"), 0);
}

#[test]
fn a_strategy_that_reads_no_method_has_no_method_row() {
    for strategy in [PocketStrategy::Zigzag, PocketStrategy::Waterline, PocketStrategy::Auto, PocketStrategy::OffsetRings] {
        let harness = passes_under(strategy);
        assert_eq!(harness.query_all_by_label("Method").count(), 0, "{strategy:?} has one way of clearing a level");
    }
}
