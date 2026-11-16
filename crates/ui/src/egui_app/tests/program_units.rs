//! #902 in the shell: the document's unit is the program's unit, and
//! the Post page says so - a "Program units" row whose Inches click is
//! the same undoable `SetUnits` the Stock page queues.
//! Provenance: our own tests over our own shell; no vendor material.

use super::*;
use crate::state::SetupPage;

#[test]
fn the_post_page_sets_the_program_unit_through_set_units() {
    let mut harness = shell_harness();
    harness.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    harness.run_steps(2);
    crate::actions::target(&mut harness.state_mut().state, UiTarget::SetupNode("Setup1".into(), SetupPage::Post));
    harness.run_steps(2);
    assert!(harness.query_by_label("Program units").is_some(), "the Post page names the program's unit");
    harness.get_by_label("Inches").click();
    harness.run_steps(2);
    assert_eq!(units::of(&harness.state().state.world), Unit::Inches, "the click reaches the world through SetUnits");
    assert_eq!(camcraft_app::posting::program_options(&harness.state().state.world)["Units"], "inch", "and every post hears it");
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run_steps(2);
    assert_eq!(units::of(&harness.state().state.world), Unit::Millimeters, "undo puts mm back");
}
