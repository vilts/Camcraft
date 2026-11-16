//! #1512: the Passes tab shows the one radial-step field the pocket's
//! strategy reads - Stepover under Zigzag / Waterline / Auto, Optimal load
//! under Adaptive / Offset rings - so no field on the page is inert.
//! Provenance: our own page law; asserted structurally (no pixels).

use super::*;
use camcraft_app::machining2d::PocketStrategy;

const STEPOVER: &str = "Stepover";
const LOAD: &str = "Optimal load (% of D)";

fn passes_with(harness: &mut Harness<'static, ShellUi>, strategy: PocketStrategy) -> (usize, usize) {
    if let OpParams::Pocket { strategy: s, .. } = &mut harness.state_mut().state.op_ui.params {
        *s = strategy;
    } else {
        panic!("the draft is not a Pocket");
    }
    let passes = super::super::op_tabs::SECTIONS.iter().position(|s| *s == "Passes");
    harness.state_mut().state.op_tab = passes.unwrap_or(0);
    harness.run();
    (harness.query_all_by_label(STEPOVER).count(), harness.query_all_by_label(LOAD).count())
}

#[test]
fn the_pocket_passes_tab_shows_only_the_step_its_strategy_reads() {
    let mut harness = shell_harness();
    let cur = harness.state().state.op_ui.params.clone();
    // Kind index 2 is Pocket (KindInfo::ALL order, the structural table).
    harness.state_mut().state.op_ui.params = crate::operations::retype(&cur, 2);
    for s in [PocketStrategy::Zigzag, PocketStrategy::Waterline, PocketStrategy::Auto] {
        assert_eq!(passes_with(&mut harness, s), (1, 0), "{s:?} spaces its rows by the stepover");
    }
    for s in [PocketStrategy::Adaptive, PocketStrategy::OffsetRings] {
        assert_eq!(passes_with(&mut harness, s), (0, 1), "{s:?} steps by the load, never the stepover");
    }
}
