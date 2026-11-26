//! #1874: the length-unit prompt. It shows while a STEP import waits for
//! its unit, offers millimetres, inches and Cancel, and its answer goes
//! through the command queue. Escape is Cancel.
//! Provenance: our own tests over our own shell; no HSMWorks material.
//!
//! Gate map (#1874): covers ui::egui_app::step_units_prompt,
//! ui::state (state_step_units).

use std::path::PathBuf;

use super::*;
use crate::egui_app::step_units_prompt as prompt;
use crate::state::StepUnitsAsk;

fn asking() -> Harness<'static, ShellUi> {
    let mut h = shell_harness();
    h.state_mut().state.step_units_ask = Some(StepUnitsAsk { path: PathBuf::from("no-such-dir/plate.step"), as_fixture: None });
    h.run();
    h
}

#[test]
fn the_prompt_names_the_file_and_offers_the_two_units_and_cancel() {
    let h = asking();
    for label in [prompt::TITLE, prompt::MILLIMETERS, prompt::INCHES, prompt::CANCEL] {
        assert!(h.query_by_label(label).is_some(), "entry {label:?}");
    }
    assert!(h.query_by_label_contains("plate.step states no length unit").is_some(), "the file is named");
}

#[test]
fn no_prompt_shows_without_a_question() {
    let mut h = shell_harness();
    h.run();
    assert!(h.query_by_label(prompt::TITLE).is_none());
}

#[test]
fn a_unit_button_answers_through_the_queue_and_closes_the_prompt() {
    let mut h = asking();
    h.get_by_label(prompt::INCHES).click();
    h.run_steps(3);
    assert!(h.state().state.step_units_ask.is_none(), "the answer clears the question");
    assert!(!h.state().state.status.contains("cancelled"), "a unit is no cancel: {}", h.state().state.status);
    assert!(h.query_by_label(prompt::TITLE).is_none(), "the prompt is gone");
}

#[test]
fn cancel_and_escape_import_nothing() {
    let mut h = asking();
    h.get_by_label(prompt::CANCEL).click();
    h.run_steps(3);
    assert!(h.state().state.step_units_ask.is_none());
    assert!(h.state().state.status.contains("cancelled"), "{}", h.state().state.status);
    let mut h = asking();
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(h.state().state.step_units_ask.is_none(), "Escape answers Cancel");
    assert!(h.state().state.status.contains("cancelled"), "{}", h.state().state.status);
}
