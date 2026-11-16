//! #543: the Sinker Burn op page's Passes tab shows the electrode pass
//! and, for a rough or finish, the finished cavity size; the finish-order
//! warning reads in the warning wall. Text and roles only (no PNGs).
//! Provenance: our own page law; no vendor material consulted.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use camcraft_app::operation::{OpParams, SinkerPass};
use camcraft_app::verify::Warning;

use super::dialog_tests::{opened, page};
use crate::state::State;

fn sinker_page(pass: SinkerPass) -> Harness<'static, State> {
    let mut state = opened(super::dialog_tests::face());
    let k = camcraft_app::operation::KindInfo::ALL.iter().position(|k| k.label == "Sinker Burn").expect("the kind");
    state.op_ui.params = crate::operations::retype(&state.op_ui.params, k);
    if let OpParams::SinkerBurn(p) = &mut state.op_ui.params {
        p.pass = pass;
        p.cavity_x = 24.5;
        p.cavity_y = 13.25;
    }
    state.op_tab = super::op_tabs::SEC_PASSES;
    let mut harness = page(state);
    harness.run();
    harness
}

fn row_values(h: &Harness<'static, State>, label: &str) -> Vec<String> {
    let lab = h.get_by_label(label).rect();
    h.query_all_by(|n| n.value().is_some())
        .filter(|n| {
            let c = n.rect().center().y;
            c >= lab.top() - 2.0 && c <= lab.bottom() + 2.0 && n.rect().left() > lab.right()
        })
        .filter_map(|n| n.value())
        .collect()
}

#[test]
fn a_rough_op_shows_its_pass_and_the_finished_cavity_size() {
    let h = sinker_page(SinkerPass::Rough);
    let row = h.get_by_label("Electrode pass").rect();
    let combo = h.query_all_by(|n| n.role() == Role::ComboBox).any(|n| {
        let c = n.rect().center().y;
        c >= row.top() - 2.0 && c <= row.bottom() + 2.0 && n.value().as_deref() == Some("Rough (undersized)")
    });
    assert!(combo, "the Electrode pass combo reads Rough (undersized)");
    for (label, want) in [("Finished cavity X", "24.5"), ("Finished cavity Y", "13.25")] {
        let vals = row_values(&h, label);
        assert!(vals.iter().any(|v| v.contains(want)), "{label} shows {want}, row values {vals:?}");
    }
}

#[test]
fn a_finish_op_reads_finish_and_a_single_electrode_hides_the_cavity() {
    let h = sinker_page(SinkerPass::Finish);
    let row = h.get_by_label("Electrode pass").rect();
    let combo = h.query_all_by(|n| n.role() == Role::ComboBox).any(|n| {
        let c = n.rect().center().y;
        c >= row.top() - 2.0 && c <= row.bottom() + 2.0 && n.value().as_deref() == Some("Finish (to size)")
    });
    assert!(combo, "the Electrode pass combo reads Finish (to size)");
    assert!(h.query_by_label("Finished cavity X").is_some() && h.query_by_label("Finished cavity Y").is_some());

    let single = sinker_page(SinkerPass::Single);
    assert!(single.query_by_label("Electrode pass").is_some(), "the pass control always stands");
    assert!(single.query_by_label("Finished cavity X").is_none(), "no cavity X for a single electrode");
    assert!(single.query_by_label("Finished cavity Y").is_none(), "no cavity Y for a single electrode");
}

#[test]
fn the_finish_order_warning_reads_in_the_warning_wall() {
    let w = Warning::SinkerFinishOrder { at: 7, finish_mm: 0.2, rough_mm: 0.1 };
    let lines = crate::operations::fold_warnings(&[w]);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("warning: sinker finish at move 7"), "{lines:?}");
    assert!(lines[0].contains("is not smaller than the rough's 0.100 mm"), "{lines:?}");
}
