//! #1567: the Browser's "Show work envelope" box. Disabled without a
//! machine; with a mill whose axes state travel it draws the travel box
//! about the work origin; a machine without travel draws nothing and says
//! why; toggling it is view state (no command, no history, same machine).
//! Provenance: our own tests over our own shell; no HSMWorks material.

use super::*;
use camcraft_app::machine::{self, AxisLimits, Machine};
use egui_kittest::kittest::NodeT;

const LABEL: &str = "Show work envelope";

fn ax(name: &str, travel: Option<(f64, f64)>) -> AxisLimits {
    AxisLimits { name: name.into(), travel, ..AxisLimits::default() }
}

fn mill(travel: bool) -> Machine {
    let t = |a, b| travel.then_some((a, b));
    Machine {
        model: "Test mill".into(),
        axes: vec![ax("X", t(-100.0, 200.0)), ax("Y", t(-50.0, 50.0)), ax("Z", t(-30.0, 0.0))],
        ..Machine::default()
    }
}

fn click_box(harness: &mut Harness<'static, ShellUi>) {
    if harness.query_by_label(LABEL).is_none() {
        open_view_menu(harness);
    }
    harness.get_by_label(LABEL).click();
    harness.run();
}

#[test]
fn the_box_is_disabled_without_a_machine() {
    let mut h = shell_harness();
    machine::set(&h.state().state.world, "", None);
    open_view_menu(&mut h);
    assert!(h.get_by_label(LABEL).accesskit_node().is_disabled());
    assert!(!h.state().state.show_work_envelope);
    assert!(crate::work_envelope::envelope_lines(&h.state().state).is_empty());
}

#[test]
fn a_mill_with_travel_draws_its_box_and_toggling_is_view_state() {
    let mut h = shell_harness();
    machine::set(&h.state().state.world, "test.mch", Some(&mill(true)));
    let before = machine::of(&h.state().state.world);
    let undo_before = h.state().state.history.can_undo();
    open_view_menu(&mut h);
    assert!(!h.get_by_label(LABEL).accesskit_node().is_disabled());
    assert!(crate::work_envelope::envelope_lines(&h.state().state).is_empty(), "off by default");
    click_box(&mut h);
    let st = &h.state().state;
    assert!(st.show_work_envelope);
    let lines = crate::work_envelope::envelope_lines(st);
    assert_eq!(lines.len(), 24, "12 edges, two vertices each");
    // Work origin is the world origin in a fresh document: X spans -100..200.
    let xs: Vec<f32> = lines.iter().map(|v| v[0]).collect();
    assert!(xs.contains(&-100.0) && xs.contains(&200.0));
    assert!(!interact::collect_overlay(st).is_empty());
    assert!(st.pending.is_empty(), "a view toggle queues nothing");
    assert_eq!(st.history.can_undo(), undo_before, "no command history entry");
    assert_eq!(machine::of(&st.world), before, "the machine is untouched");
    click_box(&mut h);
    assert!(crate::work_envelope::envelope_lines(&h.state().state).is_empty(), "off again");
}

#[test]
fn a_machine_without_travel_draws_nothing_and_says_why() {
    let mut h = shell_harness();
    machine::set(&h.state().state.world, "test.mch", Some(&mill(false)));
    click_box(&mut h);
    let st = &h.state().state;
    assert!(st.show_work_envelope);
    assert!(crate::work_envelope::envelope_lines(st).is_empty());
    let tip = crate::work_envelope::tooltip(st);
    assert!(tip.contains("no X travel"), "{tip}");
}

#[test]
fn mill_envelope_stands_on_table_for_all_travel_files() {
    let mut h = shell_harness();
    h.state_mut().state.show_work_envelope = true;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines");
    let mut checked = 0;
    for entry in std::fs::read_dir(dir).expect("machine library") {
        let path = entry.expect("machine entry").path();
        if path.extension().and_then(|s| s.to_str()) != Some("mch") { continue; }
        let text = std::fs::read_to_string(&path).expect("machine file");
        let m = machine::parse_mch(&text).expect("valid machine");
        machine::set(&h.state().state.world, path.to_str().expect("path"), Some(&m));
        let Ok(machine::envelope::EnvelopeShape::Box { min, max }) = machine::work_envelope(&h.state().state.world) else { continue };
        let lines = crate::work_envelope::envelope_lines(&h.state().state);
        assert_eq!(lines.len(), 24, "{}", path.display());
        let bottom = lines.iter().map(|v| v[2]).fold(f32::INFINITY, f32::min);
        let top = lines.iter().map(|v| v[2]).fold(f32::NEG_INFINITY, f32::max);
        // #2172: the table is the floor under the seeded block's stock.
        let floor = crate::floor::floor_z(&h.state().state);
        assert_eq!(bottom, floor as f32, "{}: bottom on the floor", path.display());
        assert_eq!(top, (floor + (max[2] - min[2])) as f32, "{}: full Z travel", path.display());
        for k in 0..2 {
            assert_eq!(lines.iter().map(|v| v[k]).fold(f32::INFINITY, f32::min), min[k] as f32);
            assert_eq!(lines.iter().map(|v| v[k]).fold(f32::NEG_INFINITY, f32::max), max[k] as f32);
        }
        checked += 1;
    }
    assert!(checked > 0, "exercise library machines with complete travel");
}

#[test]
fn mill_envelope_stays_on_table_with_shifted_origin_and_inverted_z() {
    let mut st = State::new();
    st.pending.push_back(CommandKind::SetWcs { pos: [10.0, 20.0, 70.0], roll_deg: 0.0, pitch_deg: 0.0, yaw_deg: 0.0 });
    st.drain_actions();
    st.show_work_envelope = true;
    for direction in [machine::AxisDirection::Standard, machine::AxisDirection::Inverted] {
        let mut m = mill(true);
        m.axes[2].direction = direction;
        machine::set(&st.world, "test.mch", Some(&m));
        let before = machine::work_envelope(&st.world);
        let lines = crate::work_envelope::envelope_lines(&st);
        assert_eq!(lines.iter().map(|v| v[2]).fold(f32::INFINITY, f32::min), 0.0);
        assert_eq!(lines.iter().map(|v| v[2]).fold(f32::NEG_INFINITY, f32::max), 30.0);
        assert_eq!(machine::work_envelope(&st.world), before, "program travel stays unchanged");
        assert!(lines.iter().any(|v| v[0] == -90.0 && v[1] == -30.0));
        assert!(crate::work_envelope::tooltip(&st).contains("floor"));
    }
}
