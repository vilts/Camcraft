//! #1440: the setup's Tool list page follows the WCS page's layout -
//! titled groups, the readout as a table, the exports last - and keeps
//! what it said with no tools. Provenance: our own page law.

use super::*;
use crate::state::SetupPage;

fn open_tools(harness: &mut Harness<'static, ShellUi>) {
    harness.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    harness.run_steps(2);
    crate::actions::target(&mut harness.state_mut().state, UiTarget::SetupNode("Setup1".into(), SetupPage::Tools));
    harness.run_steps(2);
}

#[test]
fn the_tool_list_page_is_laid_out_in_groups_like_the_wcs_page() {
    let mut harness = shell_harness();
    open_tools(&mut harness);
    let rect = |h: &Harness<'static, ShellUi>, l: &str| h.get_by_label(l).rect();
    let groups = ["Tools in this setup", "Time and travel", "Export"];
    let tops: Vec<f32> = groups.iter().map(|g| rect(&harness, g).top()).collect();
    assert!(tops.windows(2).all(|w| w[0] < w[1]), "groups stack top to bottom: {tops:?}");
    // The tools table (its Size column) sits in the first group, the
    // readout's table in the second.
    assert_header_row(&harness, &["T", "Tool", "Size", "Ops"], tops[0], tops[1]);
    assert_header_row(&harness, &["Tool", "Ops", "Changes", "Cut", "Rapid", "Est (min)"], tops[1], tops[2]);
    assert!(harness.query_by_label_contains("Setup total: est").is_some(), "total line");
    // The exports come last, in order, one row each.
    let (t, s) = (rect(&harness, "Export tool list…"), rect(&harness, "Export setup sheets…"));
    assert!(t.top() > tops[2] && s.top() > t.top(), "export rows under the Export title: {t:?} {s:?}");
    assert!(t.left() == s.left(), "exports share a left edge");
}

/// Every header of a table, once each between two group titles, on one
/// row, in order left to right - so dropping or moving any column is red.
fn assert_header_row(h: &Harness<'static, ShellUi>, headers: &[&str], lo: f32, hi: f32) {
    let rects: Vec<egui::Rect> = headers
        .iter()
        .map(|l| {
            let found: Vec<egui::Rect> =
                h.query_all_by_label(l).map(|n| n.rect()).filter(|r| r.top() > lo && r.top() < hi).collect();
            assert_eq!(found.len(), 1, "header {l:?} once between {lo} and {hi}: {found:?}");
            found[0]
        })
        .collect();
    assert!(rects.windows(2).all(|w| (w[0].top() - w[1].top()).abs() < 2.0), "{headers:?} on one row: {rects:?}");
    assert!(rects.windows(2).all(|w| w[0].left() < w[1].left()), "{headers:?} left to right: {rects:?}");
}

/// With no operation in the setup, the page says what it said before the
/// tidy, in the same places, and still offers the exports.
#[test]
fn the_tool_list_page_keeps_its_empty_messages_with_no_tools() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, ShellUi::blank());
    open_tools(&mut harness);
    let top = |h: &Harness<'static, ShellUi>, l: &str| h.get_by_label(l).rect().top();
    let none_t = top(&harness, "No operation in this setup uses a tool yet.");
    let none_r = top(&harness, "Tool list: no generated operation in this setup yet.");
    assert!(top(&harness, "Tools in this setup") < none_t, "tools message under its group");
    assert!(none_t < top(&harness, "Time and travel") && top(&harness, "Time and travel") < none_r);
    assert!(none_r < top(&harness, "Export tool list…"), "exports last");
    assert!(harness.query_by_label("Est (min)").is_none(), "no table header with no rows");
}
