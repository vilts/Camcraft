//! #2033: the "Rest stock from" row of the Rest machining group. The
//! previous-operations mode hides the previous tool rows, keeps the
//! allowance, and commits through `EditOperationParams` on Apply.
//! Provenance: our own page law over `camcraft_app::machining2d::RestSource`.

use camcraft_app::machining2d::RestSource;

use super::*;

const TOOL_ROWS: [&str; 3] = ["Prev. tool Ø", "Prev. corner rad.", "Prev. stock left"];

fn entered() -> RestParams {
    RestParams {
        source: RestSource::EnteredTool,
        previous_tool_diameter: 8.0,
        previous_corner_radius: 0.0,
        previous_stock_to_leave: 0.0,
        allowance: 0.0,
    }
}

#[test]
fn the_source_row_switches_the_mode_hides_the_tool_rows_and_commits_on_apply() {
    let mut h = page(opened_pocket(PocketStrategy::Auto, Some(entered())));
    h.run();
    assert_eq!(h.query_all_by_label("Rest stock from").count(), 1, "the mode row shows while rest is on");
    for row in TOOL_ROWS {
        assert!(h.query_by_label(row).is_some(), "{row} shows for the entered tool");
    }

    on_row(&h, "Rest stock from", Role::ComboBox).click();
    h.run();
    h.get_by_label("Previous operations").click();
    h.run();
    assert_eq!(draft_rest(h.state()).map(|r| r.source), Some(RestSource::PreviousOperations));
    assert_eq!(live_rest(h.state()), Some(entered()), "the mode waits for Apply");
    for row in TOOL_ROWS {
        assert!(h.query_by_label(row).is_none(), "{row} hides: the previous operations are read, not a tool");
    }
    assert!(h.query_by_label("Rest allowance").is_some(), "the allowance stays");

    click_apply(&mut h);
    let want = RestParams { source: RestSource::PreviousOperations, ..entered() };
    assert_eq!(live_rest(h.state()), Some(want), "Apply commits the mode");
    let st = h.state_mut();
    assert!(st.history.undo(&st.world));
    assert_eq!(live_rest(st), Some(entered()), "undo restores the entered tool");
}

#[test]
fn the_previous_operations_mode_applies_without_a_previous_tool() {
    // No previous tool at all (0 mm, not larger than the 4 mm tool).
    let stored = RestParams { source: RestSource::PreviousOperations, previous_tool_diameter: 0.0, ..entered() };
    let mut h = page(opened_pocket(PocketStrategy::Auto, Some(stored)));
    h.run();
    for row in TOOL_ROWS {
        assert!(h.query_by_label(row).is_none(), "{row} is hidden on a stored previous-operations pocket");
    }
    set_row_number(&mut h, "Rest allowance", "0.2");
    click_apply(&mut h);
    assert_eq!(live_rest(h.state()), Some(RestParams { allowance: 0.2, ..stored }));
}
