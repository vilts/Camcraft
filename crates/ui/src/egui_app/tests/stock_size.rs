//! #197: the Stock page shows the models' size through the real shell.
//! Provenance: our own page law.

use super::*;

/// #197: the Stock page shows the models' size above the stock fields -
/// the seeded 60 x 60 x 30 block - so stock can be sized against it.
#[test]
fn the_stock_page_shows_the_models_size() {
    let mut harness = shell_harness();
    harness.state_mut().state.ui_target = UiTarget::Stock;
    harness.run();
    assert!(
        harness.query_by_label("Models: 60.000 x 60.000 x 30.000 mm").is_some(),
        "the stock page names the seeded block's size"
    );
}

/// #1440: the Stock page follows the WCS page's layout - titled groups,
/// the fit allowance in its own group, and the actions as one row of
/// equal buttons below them all.
#[test]
fn the_stock_page_is_laid_out_in_groups_like_the_wcs_page() {
    let mut harness = shell_harness();
    harness.state_mut().state.ui_target = UiTarget::Stock;
    harness.run();
    let rect = |h: &Harness<'static, ShellUi>, l: &str| h.get_by_label(l).rect();
    let groups = ["Shape", "Size", "Position and rotation", "Fit to models"];
    let tops: Vec<f32> = groups.iter().map(|g| rect(&harness, g).top()).collect();
    assert!(tops.windows(2).all(|w| w[0] < w[1]), "groups stack top to bottom: {tops:?}");
    // Each field sits in its group, under the group's title.
    assert!(rect(&harness, "Length (X)").top() > tops[1] && rect(&harness, "Length (X)").top() < tops[2]);
    assert!(rect(&harness, "X center").top() > tops[2] && rect(&harness, "X center").top() < tops[3]);
    assert!(rect(&harness, "Fit margin").top() > tops[3]);
    // The three actions: equal width, one row, under the last group.
    let (a, f) = (rect(&harness, "Apply"), rect(&harness, "Fit to Models"));
    assert!(a.top() > rect(&harness, "Fit margin").bottom(), "actions come last");
    assert_eq!(a.top(), f.top(), "one row");
    assert!((a.width() - f.width()).abs() < 3.0, "equal buttons {a:?} {f:?}");
    // Labels share one column: the fields line up.
    assert_eq!(rect(&harness, "X center").left(), rect(&harness, "Length (X)").left());
}

/// #1440 review: with no models, the from-model Source group still shows
/// the raw-stock row (here its hint), as it did before the layout tidy.
#[test]
fn the_stock_page_keeps_the_raw_stock_row_with_no_models() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, ShellUi::blank());
    {
        let st = &mut harness.state_mut().state;
        st.stock_ui.shape = 2;
        st.ui_target = UiTarget::Stock;
    }
    harness.run();
    assert!(harness.query_by_label("No models to take stock from.").is_some(), "no models");
    assert!(
        harness
            .query_by_label("Apply a stock from a model to say whether its source is raw stock only.")
            .is_some(),
        "the raw-stock row shows with no models"
    );
}
