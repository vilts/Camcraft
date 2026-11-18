//! #2257: while a toolpath generates, a progress overlay lies over the
//! viewport with the operation's name, and a Cancel button; it goes when
//! the job lands, is refused or is cancelled. A result whose operation
//! changed while it generated is discarded, named. The worker's result
//! arrives here through a channel the test holds, so no frame races a
//! real generator.
//!
//! Gate map: covers ui::gen_worker, ui::egui_app::gen_overlay, ui::state.
//! Provenance: our own tests over our own UI; no HSMWorks material.

use std::sync::mpsc;

use camcraft_app::commands::{CommandError, GenerateOperation};

use super::super::gen_overlay::CANCEL;
use super::structural::painted;
use super::*;
use crate::gen_worker::{GenProgress, overlay_text};

fn overlay(h: &Harness<'static, ShellUi>) -> Option<(String, egui::Pos2)> {
    painted(h).into_iter().find(|(t, _)| t.starts_with("Generating "))
}

fn ready(h: &Harness<'static, ShellUi>) -> GenerateOperation {
    GenerateOperation::new(&h.state().state.world, "Contour1").expect("the seed generates")
}

fn status(h: &Harness<'static, ShellUi>) -> String {
    h.state().state.status.clone()
}

#[test]
fn the_overlay_names_the_running_operation_and_goes_when_it_lands() {
    let mut h = shell_harness();
    h.run();
    assert!(overlay(&h).is_none(), "no overlay at rest");
    let done = ready(&h);
    let (tx, rx) = mpsc::channel();
    h.state_mut().state.gen_jobs.run_with("Contour1", rx);
    h.run_steps(2);
    let (text, at) = overlay(&h).expect("the overlay shows while the job runs");
    assert!(text.starts_with("Generating Contour1 - "), "{text}");
    let ([x0, y0], [x1, y1]) = h.state().state.viewport_rect;
    assert!(at.x >= x0 && at.x <= x1 && at.y >= y0 && at.y <= y1, "it lies over the viewport: {at:?}");
    assert!(h.query_by_label(CANCEL).is_some(), "the Cancel button is there");
    let _ = tx.send(Ok(done));
    h.run_steps(2);
    assert!(overlay(&h).is_none(), "the overlay goes when the job lands");
    assert!(h.query_by_label(CANCEL).is_none());
    assert!(status(&h).contains("Contour1"), "{}", status(&h));
    assert!(!h.state().state.gen_jobs.busy());
}

#[test]
fn a_refusal_ends_the_overlay_and_keeps_its_message() {
    let mut h = shell_harness();
    h.run();
    let (tx, rx) = mpsc::channel();
    h.state_mut().state.gen_jobs.run_with("Contour1", rx);
    h.run_steps(2);
    assert!(overlay(&h).is_some());
    let _ = tx.send(Err(CommandError::Machining("Contour1: the test refuses it".into())));
    h.run_steps(2);
    assert!(overlay(&h).is_none(), "a refusal ends the overlay");
    let said = status(&h);
    assert!(said.starts_with("generate failed:") && said.contains("Contour1: the test refuses it"), "{said}");
}

#[test]
fn cancel_ends_the_overlay_and_drops_the_result() {
    let mut h = shell_harness();
    h.run();
    let before = h.state().state.history.done_len();
    let done = ready(&h);
    let (tx, rx) = mpsc::channel();
    h.state_mut().state.gen_jobs.run_with("Contour1", rx);
    h.run_steps(2);
    h.get_by_label(CANCEL).click();
    h.run_steps(2);
    assert!(overlay(&h).is_none(), "Cancel ends the overlay");
    assert!(status(&h).contains("generation of Contour1 cancelled"), "{}", status(&h));
    // A worker that still finishes lands nothing.
    assert!(tx.send(Ok(done)).is_err(), "nobody listens after Cancel");
    h.run_steps(2);
    assert_eq!(h.state().state.history.done_len(), before, "no history entry");
}

#[test]
fn a_result_for_an_operation_edited_meanwhile_is_discarded() {
    let mut h = shell_harness();
    h.run();
    let done = ready(&h);
    let (tx, rx) = mpsc::channel();
    h.state_mut().state.gen_jobs.run_with("Contour1", rx);
    let mut params = {
        let st = &h.state().state;
        let e = model::find_by_name(&st.world, "Contour1").expect("the seed's op");
        model::operation_of(&st.world, &e).expect("op").params.clone()
    };
    if let OpParams::Contour { stepdown, .. } = &mut params {
        *stepdown = 2.5;
    }
    h.state_mut().state.pending.push_back(CommandKind::EditOperationParams { name: "Contour1".into(), params: params.clone() });
    h.run();
    let edited = h.state().state.history.done_len();
    let _ = tx.send(Ok(done));
    h.run_steps(2);
    assert!(status(&h).contains("generation of Contour1 discarded"), "{}", status(&h));
    assert_eq!(h.state().state.history.done_len(), edited, "the old result wrote nothing");
    let st = &h.state().state;
    let e = model::find_by_name(&st.world, "Contour1").expect("op");
    assert_eq!(model::operation_of(&st.world, &e).expect("op").params, params, "the edit stands");
}

#[test]
fn background_generation_lands_the_same_toolpath() {
    let mut h = shell_harness();
    h.run();
    let st = &mut h.state_mut().state;
    st.gen_jobs.background = true;
    st.pending.push_back(CommandKind::ClearToolpath("Contour1".into()));
    st.drain_actions();
    st.pending.push_back(CommandKind::GenerateOperation("Contour1".into()));
    st.settle();
    assert!(!st.gen_jobs.busy(), "settle waits for the worker");
    let e = model::find_by_name(&st.world, "Contour1").expect("op");
    assert!(model::toolpath_of(&st.world, &e).is_some(), "the worker's path landed: {}", st.status);
}

#[test]
fn the_overlay_text_counts_a_batch() {
    let one = GenProgress { label: "Adaptive1".into(), index: 1, total: 1, secs: 3.2 };
    assert_eq!(overlay_text(&one), "Generating Adaptive1 - 3 s");
    let many = GenProgress { index: 2, total: 5, ..one };
    assert_eq!(overlay_text(&many), "Generating Adaptive1 (2 of 5) - 3 s");
}

#[test]
fn one_search_for_each_job_and_a_rename_shows_without_another() {
    let mut h = shell_harness();
    h.run();
    let (_tx, rx) = mpsc::channel();
    h.state_mut().state.gen_jobs.run_with("Contour1", rx);
    let before = crate::gen_worker::label_lookups();
    h.run_steps(10);
    for _ in 0..100 {
        let _ = h.state().state.gen_progress();
    }
    assert_eq!(crate::gen_worker::label_lookups() - before, 1, "one search for the job, none for each repaint");
    h.state_mut().state.pending.push_back(CommandKind::RenameOperation { name: "Contour1".into(), label: "Outer wall".into() });
    h.run_steps(10);
    assert_eq!(crate::gen_worker::label_lookups() - before, 1, "a rename needs no new search");
    let (text, _) = overlay(&h).expect("the overlay shows while the job runs");
    assert!(text.starts_with("Generating Outer wall - "), "the rename shows at once: {text}");
}

#[test]
fn a_deleted_operation_shows_its_name_and_no_dead_entity_is_read() {
    let mut h = shell_harness();
    h.run();
    let (_tx, rx) = mpsc::channel();
    h.state_mut().state.gen_jobs.run_with("Contour1", rx);
    h.state_mut().state.pending.push_back(CommandKind::RenameOperation { name: "Contour1".into(), label: "Outer wall".into() });
    h.run_steps(2);
    h.state_mut().state.pending.push_back(CommandKind::DeleteOperation("Contour1".into()));
    h.run_steps(2);
    assert!(model::find_by_name(&h.state().state.world, "Contour1").is_none(), "the op is gone");
    let (text, _) = overlay(&h).expect("the overlay stays while the job runs");
    assert!(text.starts_with("Generating Contour1 - "), "the name, not the dead label: {text}");
}
