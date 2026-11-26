//! #548: the Machine window's sinker EDM section and the Sinker Burn
//! page's condition readout, through the real widgets (AccessKit labels
//! and roles, no PNGs). Apply rides the undoable `EditMachineApply`; a
//! draft the loader would refuse is named on the page and refused by
//! the command, the machine standing as it was.
//! Provenance: our own page law; no vendor material consulted.

use egui::Vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use camcraft_app::machine::{self, parse_mch};
use camcraft_app::operation::{OpParams, SinkerPass};

use super::dialog_tests::{face, opened, page};
use crate::state::{CommandKind, State};

/// A synthetic sinker machine with one condition, one orbit and no codes.
const SINKER: &str = r#"{
  "general": { "model": "SYNTH SINKER", "vendor": "Camcraft", "kind": "sinker_edm" },
  "kinematics": { "default": {
    "units": { "length": "mm", "angle": "degrees" },
    "parts": [ { "id": "base", "name": "table", "type": "table", "parts": [
      { "id": "x", "name": "X", "type": "linear", "parts": [ { "id": "y", "name": "Y", "type": "linear" } ] },
      { "id": "z", "name": "Z", "type": "linear", "parts": [ { "id": "head", "name": "ram", "type": "head" } ] }
    ] } ] } },
  "edm": {
    "orbits": [ { "kind": "circular", "word": "ORB1" } ],
    "conditions": [ { "id": "S1", "name": "synthetic rough", "gap": 0.05, "role": "rough" } ]
  }
}"#;

pub(super) fn sinker() -> machine::Machine {
    parse_mch(SINKER).expect("the synthetic sinker parses")
}

fn mount(state: &mut State) {
    state.pending.push_back(CommandKind::EditMachineApply { path: "synth.mch".into(), machine: Box::new(sinker()) });
    state.drain_actions();
    assert!(machine::of(&state.world).1.is_some(), "{:?}", state.status);
}

fn window(state: State) -> Harness<'static, State> {
    Harness::builder()
        .with_size(Vec2::new(900.0, 1600.0))
        .build_ui_state(|ui, st: &mut State| super::machine_editor::window(ui.ctx(), st), state)
}

fn open_editor() -> Harness<'static, State> {
    let mut state = State::new();
    mount(&mut state);
    state.pending.push_back(CommandKind::EditMachineDialog);
    state.drain_actions();
    let mut h = window(state);
    h.run();
    h
}

fn conditions(h: &Harness<'static, State>) -> usize {
    machine::of(&h.state().world).1.and_then(|m| m.edm).map_or(0, |e| e.conditions.len())
}

#[test]
fn the_sinker_section_shows_keys_orbits_conditions_and_codes_with_not_stated() {
    let h = open_editor();
    assert!(h.query_all_by_label("Sinker EDM").count() >= 1, "the section header stands");
    for l in ["Max orbit radius", "Electrode pickup", "Orbit 1 word", "Condition 1 id", "Condition 1 gap", "Condition 1 undersize", "Add orbit", "Add condition"] {
        assert!(h.query_by_label(l).is_some(), "the section shows {l}");
    }
    for (role, _) in camcraft_app::machine::SINKER_CODES {
        assert!(h.query_by_label(&format!("Code {role}")).is_some(), "the role {role} stands");
    }
    assert!(h.query_by_label("Code side").is_some(), "the optional lateral burn code is editable");
    // Absent values say so: radius, pickup, the undersize and 8 code roles.
    assert!(h.query_all_by_label("not stated").count() >= 11);
}

#[test]
fn a_machine_that_is_not_a_sinker_has_no_sinker_section() {
    let mut state = State::new();
    state.pending.push_back(CommandKind::LoadBuiltinMachine("Lathe".into()));
    state.drain_actions();
    state.pending.push_back(CommandKind::EditMachineDialog);
    state.drain_actions();
    let mut h = window(state);
    h.run();
    assert!(h.query_by_label("Sinker EDM").is_none() && h.query_by_label("Add condition").is_none());
}

#[test]
fn add_condition_then_apply_is_one_undo_step() {
    let mut h = open_editor();
    h.get_by_label("Add condition").click();
    h.run();
    assert!(h.query_by_label("Condition 2 id").is_some(), "the new row stands in the draft");
    assert_eq!(conditions(&h), 1, "nothing is a command before Apply");
    // The fields scroll and Apply stands below them (#535): bring it in reach.
    h.get_by_label("Apply").scroll_to_me();
    h.run_steps(10);
    h.get_by_label("Apply").click();
    h.run_steps(3);
    h.state_mut().drain_actions();
    h.run();
    assert_eq!(conditions(&h), 2, "Apply mounts the edited machine");
    h.state_mut().pending.push_back(CommandKind::Undo);
    h.state_mut().drain_actions();
    h.run();
    assert_eq!(conditions(&h), 1, "undo restores the previous machine");
}

#[test]
fn a_draft_the_loader_refuses_is_named_and_refused_at_apply() {
    let mut h = open_editor();
    h.get_by_label("Add orbit").click();
    h.run();
    assert!(h.query_all_by_label_contains("Apply will refuse: orbit 2 states no control word").count() >= 1);
    // The fields scroll and Apply stands below them (#535): bring it in reach.
    h.get_by_label("Apply").scroll_to_me();
    h.run_steps(10);
    h.get_by_label("Apply").click();
    h.run_steps(3);
    h.state_mut().drain_actions();
    h.run();
    let orbits = machine::of(&h.state().world).1.and_then(|m| m.edm).map_or(0, |e| e.orbits.len());
    assert_eq!(orbits, 1, "the refused edit left the mounted machine alone");
    assert!(h.state().status.contains("machine edit refused"), "{:?}", h.state().status);
}

/// One edit that breaks a condition rule.
type EdmEdit = fn(&mut camcraft_app::machine::Edm);

#[test]
fn problems_name_each_law_the_loader_keeps() {
    let none = std::collections::BTreeMap::new();
    let mut e = sinker().edm.expect("edm");
    assert!(super::machine_edm_editor::problems(&e, &none).is_empty());
    e.conditions.push(e.conditions[0].clone());
    e.pickup = Some(String::new());
    let mut codes = none.clone();
    codes.insert("depth".into(), " ".into());
    let p = super::machine_edm_editor::problems(&e, &codes).join(" | ");
    assert!(p.contains("stated twice") && p.contains("\"depth\" is blank") && p.contains("pickup is blank"), "{p}");
    assert!(!e.is_sane(), "the loader's own law agrees");
    // A blank code alone: the page names it and the machine law refuses it.
    let mut blank_code = sinker();
    blank_code.edm_codes = codes.clone();
    let p = super::machine_edm_editor::problems(blank_code.edm.as_ref().expect("edm"), &blank_code.edm_codes).join(" | ");
    assert!(p.contains("\"depth\" is blank"), "{p}");
    assert!(!blank_code.is_sane(), "a blank code is refused at Apply as the file loader refuses it");
    // #560's condition rules: the page names each one the loader keeps,
    // and the loader agrees on every case (one law, two doors).
    let cases: [(&str, EdmEdit); 8] = [
        ("max orbit radius", |e| e.max_orbit_radius = Some(0.0)),
        ("gap", |e| e.conditions[0].gap = Some(0.0)),
        ("undersize", |e| e.conditions[0].undersize = Some(0.0)),
        ("undersize", |e| e.conditions[0].undersize = Some(1.5)),
        ("one selector", |e| {
            let mut d = e.conditions[0].clone();
            d.id = "s1".into();
            e.conditions.push(d);
        }),
        ("one selector", |e| {
            e.conditions[0].id = "01".into();
            let mut d = e.conditions[0].clone();
            d.id = "1".into();
            e.conditions.push(d);
        }),
        ("no surrounding spaces", |e| e.conditions[0].id = " S1".into()),
        ("states no id", |e| e.conditions[0].id = String::new()),
    ];
    for (word, edit) in cases {
        let mut bad = sinker().edm.expect("edm");
        edit(&mut bad);
        let p = super::machine_edm_editor::problems(&bad, &none).join(" | ");
        assert!(p.contains(word), "the page names {word:?}: {p}");
        assert!(!bad.is_sane(), "the loader refuses what the page names ({word}): {p}");
    }
    // A wire-only field on a sinker: the page names it, the machine law refuses it.
    let mut wire_field = sinker();
    if let Some(e) = wire_field.edm.as_mut() {
        e.conditions[0].wire_diameter = Some(0.25);
        let p = super::machine_edm_editor::problems(e, &none).join(" | ");
        assert!(p.contains("wire field"), "{p}");
    }
    assert!(!wire_field.edm_sane(), "a sinker condition with a wire diameter is refused");
    // The seeds the page ticks in are ones the loader accepts.
    let mut seeded = sinker().edm.expect("edm");
    seeded.conditions[0].gap = Some(0.01);
    seeded.conditions[0].undersize = Some(0.1);
    assert!(super::machine_edm_editor::problems(&seeded, &none).is_empty() && seeded.is_sane());
    let mut only_pickup = sinker().edm.expect("edm");
    only_pickup.pickup = Some(" ".into());
    assert!(!only_pickup.is_sane(), "a blank pickup is refused at Apply as the file loader refuses it");
}

#[test]
fn the_burn_page_reads_the_conditions_gap_and_undersize() {
    let mut state = opened(face());
    let load = camcraft_app::commands::LoadMachine::new(&state.world, "synth.mch".into(), Some(sinker())).expect("mounts");
    state.history.execute(&state.world, load);
    let k = camcraft_app::operation::KindInfo::ALL.iter().position(|k| k.label == "Sinker Burn").expect("the kind");
    state.op_ui.params = crate::operations::retype(&state.op_ui.params, k);
    if let OpParams::SinkerBurn(p) = &mut state.op_ui.params {
        p.condition = "S1".into();
        p.pass = SinkerPass::Single;
    }
    state.op_tab = super::op_tabs::SEC_GEO;
    let mut h = page(state);
    h.run();
    assert!(h.query_by_label("Burn at X").is_some(), "the Geometry tab is showing");
    assert!(h.query_by_label("Condition gap").is_some() && h.query_by_label("Condition undersize").is_some());
    assert!(h.query_all_by_label_contains("0.050 mm").count() >= 1, "the stated gap reads");
    assert!(h.query_all_by_label("not stated").count() >= 1, "the unstated undersize says so");
}

#[test]
fn the_condition_row_edits_wear_rate_and_notes_through_one_undoable_apply() {
    let mut h = open_editor();
    for l in ["Condition 1 wear rate", "Condition 1 notes"] {
        assert!(h.query_by_label(l).is_some(), "the condition row shows {l}");
    }
    // Tick the wear rate in and type a note; both stay in the draft until Apply.
    h.get_by_label("Condition 1 wear rate").scroll_to_me();
    h.run_steps(10);
    h.get_by_label("Condition 1 wear rate").click();
    h.run_steps(3);
    assert_eq!(
        h.get_by_label("Condition 1 wear rate").accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True),
        "the click ticks the wear rate in"
    );
    h.get_by_label("Condition 1 notes").click();
    h.run_steps(2);
    h.get_by_label("Condition 1 notes").type_text("graphite, 3 A");
    h.run_steps(3);
    let wear = |h: &Harness<'static, State>| machine::of(&h.state().world).1.and_then(|m| m.edm).and_then(|e| e.conditions[0].wear);
    assert_eq!(wear(&h), None, "nothing is a command before Apply");
    h.get_by_label("Apply").scroll_to_me();
    h.run_steps(10);
    h.get_by_label("Apply").click();
    h.run_steps(3);
    h.state_mut().drain_actions();
    h.run();
    let c = machine::of(&h.state().world).1.and_then(|m| m.edm).map(|e| e.conditions[0].clone()).expect("edm");
    assert_eq!(c.wear, Some(0.1), "Apply mounts the seeded wear rate: {:?}", h.state().status);
    assert_eq!(c.notes, "graphite, 3 A", "Apply mounts the note");
    h.state_mut().pending.push_back(CommandKind::Undo);
    h.state_mut().drain_actions();
    h.run();
    assert_eq!(wear(&h), None, "undo restores the machine without the wear rate");
}

#[test]
fn a_bad_wear_rate_is_named_and_refused_like_the_loader_does() {
    let none = std::collections::BTreeMap::new();
    for bad in [-0.1, 1.0, 2.5, f64::NAN, f64::INFINITY] {
        let mut m = sinker();
        if let Some(e) = m.edm.as_mut() {
            e.conditions[0].wear = Some(bad);
            let p = super::machine_edm_editor::problems(e, &none).join(" | ");
            assert!(p.contains("condition 1 wear"), "the page names wear {bad}: {p}");
        }
        assert!(!m.is_sane(), "the loader refuses wear {bad}");
    }
    let mut ok = sinker();
    if let Some(e) = ok.edm.as_mut() {
        e.conditions[0].wear = Some(0.0);
        e.conditions[0].notes = "kept".into();
        assert!(super::machine_edm_editor::problems(e, &none).is_empty());
    }
    assert!(ok.is_sane(), "a wear of 0 and a note pass");
}

/// #1119: every input in the Sinker EDM machine editor carries an accessible name.
#[test]
fn the_sinker_machine_editor_names_every_input() {
    let mut state = State::new();
    let mut m = sinker();
    if let Some(edm) = m.edm.as_mut() {
        edm.max_orbit_radius.get_or_insert(1.0);
        edm.pickup.get_or_insert_with(|| "ER-32".to_owned());
        if let Some(c) = edm.conditions.first_mut() {
            c.gap.get_or_insert(0.01);
            c.undersize.get_or_insert(0.1);
        }
    }
    state.pending.push_back(CommandKind::EditMachineApply { path: "synth.mch".into(), machine: Box::new(m) });
    state.drain_actions();
    state.pending.push_back(CommandKind::EditMachineDialog);
    state.drain_actions();
    let mut h = window(state);
    h.run();
    let (bad, seen) = super::fixture_plate_tests::unnamed_inputs(&h);
    assert!(seen >= 10, "only {seen} inputs found in sinker machine editor");
    assert!(bad.is_empty(), "unnamed inputs in sinker machine editor: {bad:?}");
}
