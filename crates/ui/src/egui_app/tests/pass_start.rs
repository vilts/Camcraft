//! #3316 / #3354 / #3355 / #3361 / #3362: the Passes tabs of 2D Offset,
//! 2D Pocket, 2D Adaptive, 3D Pocket and 3D Finish carry the shared
//! "Pass starts at" row (Auto / Inside /
//! Outside, Auto by default). Provenance:
//! our own page law; asserted structurally (labels, the combo's value;
//! no pixels).

use super::*;
use camcraft_app::machining2d::PassStart;
use camcraft_app::operation::{KindInfo, OpType};
use egui::accesskit::Role;

use super::super::pass_start_row::{
    FACE_NO_PASS_ORDER, PASS_START_LABEL, TROCH_PASS_START_REFUSED, pass_start_names,
};

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
fn the_offset_passes_tab_offers_pass_start_and_shows_the_draft() {
    assert_eq!(pass_start_names(), ["Auto", "Inside", "Outside"], "one name per value, in the combo's order");
    let mut harness = passes_of(OpType::Offset2D);
    assert_eq!(harness.query_all_by_label(PASS_START_LABEL).count(), 1, "the Pass starts at row");
    let outside_before = combos_showing(&harness, "Outside");
    if let OpParams::Offset2D { pass_start, .. } = &mut harness.state_mut().state.op_ui.params {
        assert_eq!(*pass_start, PassStart::Auto, "a new 2D Offset starts Auto");
        *pass_start = PassStart::Outside;
    } else {
        panic!("the draft is not a 2D Offset");
    }
    harness.run();
    assert_eq!(combos_showing(&harness, "Outside"), outside_before + 1, "the combo shows the draft's value");
}

/// #3354: 2D Pocket's Passes tab carries the same row, Auto by default.
#[test]
fn the_pocket_passes_tab_offers_pass_start_and_shows_the_draft() {
    let mut harness = passes_of(OpType::Pocket);
    assert_eq!(harness.query_all_by_label(PASS_START_LABEL).count(), 1, "the Pass starts at row");
    let inside_before = combos_showing(&harness, "Inside");
    if let OpParams::Pocket { pass_start, .. } = &mut harness.state_mut().state.op_ui.params {
        assert_eq!(*pass_start, PassStart::Auto, "a new 2D Pocket starts Auto");
        *pass_start = PassStart::Inside;
    } else {
        panic!("the draft is not a 2D Pocket");
    }
    harness.run();
    assert_eq!(combos_showing(&harness, "Inside"), inside_before + 1, "the combo shows the draft's value");
}

/// #3360: Bore's Passes tab has the same row, Auto by default, and the
/// combo shows the draft's value.
#[test]
fn the_bore_passes_tab_offers_pass_start_and_shows_the_draft() {
    let mut harness = passes_of(OpType::Bore);
    assert_eq!(harness.query_all_by_label(PASS_START_LABEL).count(), 1, "the Pass starts at row");
    let outside_before = combos_showing(&harness, "Outside");
    if let OpParams::Bore { pass_start, .. } = &mut harness.state_mut().state.op_ui.params {
        assert_eq!(*pass_start, PassStart::Auto, "a new Bore starts Auto");
        *pass_start = PassStart::Outside;
    } else {
        panic!("the draft is not a Bore");
    }
    harness.run();
    assert_eq!(combos_showing(&harness, "Outside"), outside_before + 1, "the combo shows the draft's value");
}

/// #3356: Face has no pass order (its rows run edge to edge). Its row is
/// there, disabled, and says why by name.
#[test]
fn the_face_passes_tab_says_it_has_no_pass_order() {
    let harness = passes_of(OpType::Face);
    let rows: Vec<_> = harness.query_all_by_label(PASS_START_LABEL).collect();
    assert_eq!(rows.len(), 1, "the Pass starts at row on Face");
    let locked = harness
        .query_all_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some("Auto") && n.is_disabled())
        .count();
    assert_eq!(locked, 1, "the combo shows Auto and Face cannot pick an order");
    assert_eq!(
        harness
            .query_all_by(|n| [n.label(), n.value()].into_iter().flatten().any(|l| l.contains(FACE_NO_PASS_ORDER)))
            .count(),
        1,
        "the row names why Face has no order, in view (not only on hover)"
    );
}

#[test]
fn the_slot_passes_tab_locks_auto_and_names_the_continuous_pass() {
    let harness = passes_of(OpType::Slot);
    assert_eq!(harness.query_all_by_label(PASS_START_LABEL).count(), 1, "the Pass starts at row");
    let locked = harness
        .query_all_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some("Auto") && n.is_disabled())
        .count();
    assert_eq!(locked, 1, "Slot shows Auto and cannot pick an order");
    let reason = "Slot has no pass order. Each depth has one continuous pass. A tool-width cut follows the centerline. A wider cut connects rows.";
    assert_eq!(
        harness
            .query_all_by(|n| [n.label(), n.value()].into_iter().flatten().any(|text| text.contains(reason)))
            .count(),
        1,
        "the reason is visible without hovering"
    );
}

#[test]
fn the_trochoidal_slot_passes_tab_locks_auto_and_explains_the_loop_order() {
    let harness = passes_of(OpType::TrochoidalSlot);
    assert_eq!(harness.query_all_by_label(PASS_START_LABEL).count(), 1, "the Pass starts at row");
    let locked = harness
        .query_all_by(|n| {
            n.role() == Role::ComboBox && n.value().as_deref() == Some("Auto") && n.is_disabled()
        })
        .count();
    assert_eq!(locked, 1, "Trochoidal Slot shows Auto and cannot pick an order");
    let reason = harness
        .query_all_by(|n| {
            [n.label(), n.value()].into_iter().flatten().any(|text| {
                text.contains("entry bore")
                    && text.contains("advances along the slot")
                    && text.contains("clearance from the previous cut")
            })
        })
        .count();
    assert_eq!(reason, 1, "the reason is visible without hovering");
}

/// #3355: 2D Adaptive has the same row; under the Trochoidal method a
/// forced order is said refused by name, never dropped in silence.
#[test]
fn the_adaptive_twin_offers_pass_start_and_says_the_trochoidal_refusal() {
    let mut harness = passes_of(OpType::Adaptive2D);
    assert_eq!(harness.query_all_by_label(PASS_START_LABEL).count(), 1, "the Pass starts at row");
    let inside_before = combos_showing(&harness, "Inside");
    if let OpParams::Adaptive2D { pass_start, .. } = &mut harness.state_mut().state.op_ui.params {
        assert_eq!(*pass_start, PassStart::Auto, "a new 2D Adaptive starts Auto");
        *pass_start = PassStart::Inside;
    } else {
        panic!("the draft is not a 2D Adaptive");
    }
    harness.run();
    assert_eq!(combos_showing(&harness, "Inside"), inside_before + 1, "the combo shows the draft's value");
    assert_eq!(harness.query_all_by_label(TROCH_PASS_START_REFUSED).count(), 0, "Spiral takes the order");
    if let OpParams::Adaptive2D { method, .. } = &mut harness.state_mut().state.op_ui.params {
        *method = camcraft_app::machining2d::AdaptiveMethod::Trochoidal;
    }
    harness.run();
    assert_eq!(harness.query_all_by_label(TROCH_PASS_START_REFUSED).count(), 1, "Trochoidal says the refusal");
}

/// #3745: the Lead in / Lead out tab of each 2D kind with a pass order
/// shows the "Approach distance" row, with its hover text, only when Pass
/// starts at is Inside or Outside; Auto hides it.
#[test]
fn the_leads_tab_shows_the_approach_distance_only_for_inside_and_outside() {
    use super::super::approach_page::{APPROACH_DISTANCE_HINT, APPROACH_DISTANCE_LABEL};
    let leads = super::super::op_tabs::SECTIONS.iter().position(|s| s.starts_with("Lead"));
    assert!(leads.is_some(), "the op page has a Lead in / Lead out tab");
    for ty in [OpType::Pocket, OpType::Offset2D, OpType::Adaptive2D] {
        let mut harness = shell_harness();
        let cur = harness.state().state.op_ui.params.clone();
        harness.state_mut().state.op_ui.params = crate::operations::retype(&cur, kind(ty));
        harness.state_mut().state.op_tab = leads.unwrap_or(0);
        harness.run();
        let rows = |h: &Harness<'static, ShellUi>| h.query_all_by_label(APPROACH_DISTANCE_LABEL).count();
        assert_eq!(rows(&harness), 0, "{ty:?}: Auto shows no approach distance");
        for start in [PassStart::Inside, PassStart::Outside, PassStart::Auto] {
            match &mut harness.state_mut().state.op_ui.params {
                OpParams::Pocket { pass_start, .. }
                | OpParams::Offset2D { pass_start, .. }
                | OpParams::Adaptive2D { pass_start, .. } => *pass_start = start,
                _ => panic!("{ty:?}: the draft has no pass order"),
            }
            harness.run();
            let want = usize::from(start != PassStart::Auto);
            assert_eq!(rows(&harness), want, "{ty:?} {start:?}: the Approach distance row");
            // The hover text rides the row's label (`prop_row_hint`), so
            // it is not a visible node: no dimmed line stands in for it.
            assert_eq!(
                harness.query_all_by(|n| n.label().is_some_and(|l| l.contains(APPROACH_DISTANCE_HINT))).count(),
                0,
                "{ty:?} {start:?}: the help is a tooltip, not a line"
            );
        }
    }
}

/// #3316: a pass-start warning on the toolpath is a warning line of the
/// op page's status, word for word.
#[test]
fn a_pass_start_warning_is_a_status_line() {
    let side = camcraft_app::machining2d::StartSide::from(PassStart::Outside);
    let note = side.wall_first_note().unwrap_or_default();
    let lines = super::super::approach_page::entry_status_lines(&[note.clone(), "a quiet note".to_owned()]);
    assert_eq!(lines, vec![format!("warning: {note}")]);
    assert!(note.starts_with(camcraft_app::machining2d::PASS_START_NOTE), "{note}");
}

#[test]
fn circular_passes_tab_offers_all_three_pass_starts() {
    let mut harness = passes_of(OpType::Circular);
    assert_eq!(harness.query_all_by_label(PASS_START_LABEL).count(), 1, "the Circular Passes tab has the shared row");
    let mut current = "Auto";
    for (label, choice) in [("Inside", Some("inside")), ("Outside", Some("outside")), ("Auto", None)] {
        harness.get_by(|node| node.role() == Role::ComboBox && node.value().as_deref() == Some(current)).click();
        harness.run_steps(2);
        harness.get_by_label(label).click();
        harness.run_steps(2);
        let wire = camcraft_app::project::params_to_json(&harness.state().state.op_ui.params);
        assert_eq!(wire["circular"].get("pass_start").and_then(serde_json::Value::as_str), choice);
        assert_eq!(combos_showing(&harness, label), 1, "the combo displays the new choice");
        current = label;
    }
}

/// #3361: 3D Pocket's Passes tab carries the shared row, Auto by default.
/// Under the default strategy (Auto, which cuts rows) and under Adaptive
/// the line under it says, in view, that the value changes no move; the
/// ring strategies take it without that line. The wire writes no key for
/// Auto and the word for Inside / Outside.
#[test]
fn the_pocket3d_passes_tab_offers_pass_start_and_names_the_row_strategies() {
    use super::super::pass_start_row::POCKET3D_NO_RING_ORDER;
    use camcraft_app::machining2d::PocketStrategy;
    let shown = |h: &Harness<'static, ShellUi>| {
        h.query_all_by(|n| [n.label(), n.value()].into_iter().flatten().any(|l| l.contains(POCKET3D_NO_RING_ORDER)))
            .count()
    };
    let mut harness = passes_of(OpType::Pocket3D);
    assert_eq!(harness.query_all_by_label(PASS_START_LABEL).count(), 1, "the Pass starts at row");
    assert_eq!(shown(&harness), 1, "the default strategy cuts rows: the line says the value is not used");
    let wire = camcraft_app::project::params_to_json(&harness.state().state.op_ui.params);
    assert_eq!(wire["pocket3d"].get("pass_start"), None, "Auto writes no key");
    let outside_before = combos_showing(&harness, "Outside");
    if let OpParams::Pocket3D { pass_start, strategy, .. } = &mut harness.state_mut().state.op_ui.params {
        assert_eq!(*pass_start, PassStart::Auto, "a new 3D Pocket starts Auto");
        *pass_start = PassStart::Outside;
        *strategy = PocketStrategy::Waterline;
    } else {
        panic!("the draft is not a 3D Pocket");
    }
    harness.run();
    assert_eq!(combos_showing(&harness, "Outside"), outside_before + 1, "the combo shows the draft's value");
    assert_eq!(shown(&harness), 0, "Waterline uses the value: no line");
    let wire = camcraft_app::project::params_to_json(&harness.state().state.op_ui.params);
    assert_eq!(wire["pocket3d"].get("pass_start").and_then(serde_json::Value::as_str), Some("outside"));
    if let OpParams::Pocket3D { strategy, .. } = &mut harness.state_mut().state.op_ui.params {
        *strategy = PocketStrategy::Adaptive;
    }
    harness.run();
    assert_eq!(shown(&harness), 1, "the Adaptive spiral has no ring order: the line is back");
}

/// #3362: 3D Finish's Passes tab carries the shared row, Auto by default.
/// Under the default pattern (Parallel, rows) the line under it names the
/// pattern and says the value is not used; Contour and Flat take it
/// without that line. The wire writes no key for Auto and the word for
/// Inside.
#[test]
fn the_finish3d_passes_tab_offers_pass_start_and_names_the_unused_patterns() {
    use super::super::pass_start_row::finish3d_no_ring_order;
    use camcraft_app::finishing3d::FinishPattern;
    let shown = |h: &Harness<'static, ShellUi>, text: &str| {
        h.query_all_by(|n| [n.label(), n.value()].into_iter().flatten().any(|l| l.contains(text))).count()
    };
    let mut harness = passes_of(OpType::Finish3D);
    assert_eq!(harness.query_all_by_label(PASS_START_LABEL).count(), 1, "the Pass starts at row");
    assert_eq!(shown(&harness, &finish3d_no_ring_order("Parallel")), 1, "Parallel rows: the line names the pattern");
    let wire = camcraft_app::project::params_to_json(&harness.state().state.op_ui.params);
    assert_eq!(wire["finish3d"].get("pass_start"), None, "Auto writes no key");
    let inside_before = combos_showing(&harness, "Inside");
    if let OpParams::Finish3D { pass_start, pattern, .. } = &mut harness.state_mut().state.op_ui.params {
        assert_eq!(*pass_start, PassStart::Auto, "a new 3D Finish starts Auto");
        *pass_start = PassStart::Inside;
        *pattern = FinishPattern::Contour;
    } else {
        panic!("the draft is not a 3D Finish");
    }
    harness.run();
    assert_eq!(combos_showing(&harness, "Inside"), inside_before + 1, "the combo shows the draft's value");
    assert_eq!(shown(&harness, "Not used with"), 0, "Contour uses the value: no line");
    let wire = camcraft_app::project::params_to_json(&harness.state().state.op_ui.params);
    assert_eq!(wire["finish3d"].get("pass_start").and_then(serde_json::Value::as_str), Some("inside"));
    for (pattern, name, line) in [
        (FinishPattern::Flat, "Flat", 0),
        (FinishPattern::Waterline, "Waterline", 1),
        (FinishPattern::Geodesic, "Geodesic", 1),
    ] {
        if let OpParams::Finish3D { pattern: p, .. } = &mut harness.state_mut().state.op_ui.params {
            *p = pattern;
        }
        harness.run();
        assert_eq!(shown(&harness, &finish3d_no_ring_order(name)), line, "{name}");
    }
}

/// #3748: the Lead in / Lead out tab of 2D Offset keeps Helix, Ramp and
/// Plunge selectable for Inside and Outside (the Type combo is never
/// greyed out), and shows the advice - the selected type, the recommended
/// Arc and the reason - next to the selector when one of them is
/// selected. Arc and Auto show none; the 2D Adaptive twin, which has no
/// Arc entry from the air, shows none either.
#[test]
fn the_leads_tab_advises_arc_next_to_the_entry_type_for_inside_and_outside() {
    use camcraft_app::machining2d::entry_advice;
    use camcraft_app::machining3d::Entry3D;
    use egui_kittest::kittest::NodeT;
    let leads = super::super::op_tabs::SECTIONS.iter().position(|s| s.starts_with("Lead"));
    for ty in [OpType::Offset2D, OpType::Adaptive2D] {
        let mut harness = shell_harness();
        let cur = harness.state().state.op_ui.params.clone();
        harness.state_mut().state.op_ui.params = crate::operations::retype(&cur, kind(ty));
        harness.state_mut().state.op_tab = leads.unwrap_or(0);
        for start in [PassStart::Inside, PassStart::Outside, PassStart::Auto] {
            for kind_of in [Entry3D::Helix, Entry3D::Ramp, Entry3D::Plunge, Entry3D::Arc] {
                match &mut harness.state_mut().state.op_ui.params {
                    OpParams::Offset2D { pass_start, entry, .. } | OpParams::Adaptive2D { pass_start, entry, .. } => {
                        *pass_start = start;
                        *entry = kind_of;
                    }
                    _ => panic!("{ty:?}: the draft has no pass order"),
                }
                harness.run();
                let name = format!("{kind_of:?}");
                let combos: Vec<_> =
                    harness.query_all_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some(name.as_str())).collect();
                assert!(!combos.is_empty(), "{ty:?} {start:?}: the Type combo shows {name}");
                let greyed = combos.into_iter().filter(|n| n.accesskit_node().is_disabled()).count();
                assert_eq!(greyed, 0, "{ty:?} {start:?}: the Type combo is greyed out");
                let advice = entry_advice(start, kind_of);
                let want = usize::from(ty == OpType::Offset2D && advice.is_some());
                let shown = advice.map_or(0, |a| harness.query_all_by_label(a).count());
                assert_eq!(shown, want, "{ty:?} {start:?} {kind_of:?}: the advice next to the selector");
            }
        }
    }
}
