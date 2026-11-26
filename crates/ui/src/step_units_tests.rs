//! #1874: a STEP file that states no length unit is never imported in a
//! guessed unit. The window asks for the unit; the answer imports the file
//! again in it, as one undoable import; a cancel imports nothing.
//! Provenance: our own tests over our own state machine and fixtures
//! (`box.step` with its unit taken out in memory). No HSMWorks material.
//!
//! Gate map (#1874): covers ui::state (state_step_units, finish_import),
//! app::commands::import_model through the State handoff.

use std::path::{Path, PathBuf};

use camcraft_app::model;
use camcraft_app::units::Unit;

use crate::state::{CommandKind, State};

fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name)
}

fn cut(text: String, from: &str, to: &str) -> String {
    assert!(text.contains(from), "fixture edit no longer applies: {from}");
    text.replacen(from, to, 1)
}

/// `box.step` with no length unit, written to a temp file named `name`.
fn unitless_file(tag: &str) -> PathBuf {
    let t = std::fs::read_to_string(testdata("box.step")).expect("box.step").replace("\r\n", "\n");
    let t = cut(t, "    GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#15))\n", "");
    let t = cut(t, "GLOBAL_UNIT_ASSIGNED_CONTEXT((#12, #13, #14))", "GLOBAL_UNIT_ASSIGNED_CONTEXT((#13, #14))");
    let t = cut(t, "#12 = ( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) );\n", "");
    let t = cut(t, "#15 = UNCERTAINTY_MEASURE_WITH_UNIT(1.0E-6, #12, 'distance_accuracy_value','confusion accuracy');\n", "");
    let dir = std::env::temp_dir().join(format!("camcraft-step-units-ui-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("plate.step");
    std::fs::write(&path, t).expect("write the fixture");
    path
}

fn settle(state: &mut State) {
    state.drain_actions();
    while state.import_in_progress() {
        state.drain_actions();
        std::thread::yield_now();
    }
    state.drain_actions();
}

fn width(state: &State, name: &str) -> f64 {
    let e = model::find_by_name(&state.world, name).expect("the model landed");
    let facets = model::facets_of(&state.world, &e).expect("facets");
    let xs = facets.iter().flat_map(|t| [t.a.x, t.b.x, t.c.x]);
    let (lo, hi) = xs.fold((f64::MAX, f64::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)));
    hi - lo
}

#[test]
fn a_unitless_step_import_asks_for_the_unit_and_imports_nothing_yet() {
    let path = unitless_file("ask");
    let mut state = State::new();
    state.pending.push_back(CommandKind::ImportFile(path.clone()));
    settle(&mut state);
    let ask = state.step_units_ask.clone().expect("the window asks for the unit");
    assert_eq!(ask.path, path);
    assert!(state.status.contains("states no length unit"), "{}", state.status);
    assert!(model::find_by_name(&state.world, "plate").is_none(), "nothing is imported before the answer");
}

#[test]
fn the_answer_imports_the_file_in_that_unit_as_one_undoable_import() {
    let path = unitless_file("answer");
    let mut state = State::new();
    state.pending.push_back(CommandKind::ImportFile(path.clone()));
    settle(&mut state);
    state.pending.push_back(CommandKind::ResolveStepUnits(Some(Unit::Inches)));
    settle(&mut state);
    assert!(state.step_units_ask.is_none(), "the question is answered");
    let inch = width(&state, "plate");
    assert!(state.status.contains("length unit: in (stated at import"), "the status says the unit: {}", state.status);
    state.pending.push_back(CommandKind::Undo);
    settle(&mut state);
    assert!(model::find_by_name(&state.world, "plate").is_none(), "one undo takes the import away");
    // The same file in millimetres is 25.4 times narrower.
    state.pending.push_back(CommandKind::ImportFile(path));
    settle(&mut state);
    state.pending.push_back(CommandKind::ResolveStepUnits(Some(Unit::Millimeters)));
    settle(&mut state);
    let mm = width(&state, "plate");
    assert!((inch / mm - 25.4).abs() < 1e-6, "inch {inch} against mm {mm}");
}

#[test]
fn a_cancel_imports_nothing_and_says_why() {
    let path = unitless_file("cancel");
    let mut state = State::new();
    state.pending.push_back(CommandKind::ImportFile(path));
    settle(&mut state);
    state.pending.push_back(CommandKind::ResolveStepUnits(None));
    settle(&mut state);
    assert!(state.step_units_ask.is_none());
    assert!(model::find_by_name(&state.world, "plate").is_none(), "a cancel imports nothing");
    assert!(state.status.contains("cancelled: the file states no length unit"), "{}", state.status);
}

#[test]
fn a_chosen_unit_never_waits_for_a_later_import_of_the_same_file() {
    // #3166 review: the answer queues the import of the asked file; when
    // another import starts first, the choice is dropped, so a later
    // import of the asked file asks again.
    let path = unitless_file("stale");
    let mut state = State::new();
    state.pending.push_back(CommandKind::ImportFile(path.clone()));
    settle(&mut state);
    state.step_units_next = Some((path.clone(), Unit::Inches));
    state.step_units_ask = None;
    state.pending.push_back(CommandKind::ImportFile(testdata("box.step")));
    settle(&mut state);
    state.pending.push_back(CommandKind::ImportFile(path.clone()));
    settle(&mut state);
    assert!(model::find_by_name(&state.world, "plate").is_none(), "the stale inch is not used");
    assert_eq!(state.step_units_ask.as_ref().map(|a| a.path.clone()), Some(path), "the file asks again");
}

#[test]
fn a_file_that_states_its_unit_imports_without_a_question() {
    let mut state = State::new();
    state.pending.push_back(CommandKind::ImportFile(testdata("box.step")));
    settle(&mut state);
    assert!(state.step_units_ask.is_none(), "no question for a file that states its unit");
    assert!(model::find_by_name(&state.world, "box").is_some());
    assert!(state.status.contains("length unit: mm (stated by the file)"), "{}", state.status);
}
