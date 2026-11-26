//! The gap that a vertical scroll area keeps on its right (#3384, #3390).
//!
//! Provenance: egui's floating scroll bar draws over the right edge of the
//! viewport (`style.spacing.scroll`); our own page law, no other source.

use egui::Ui;

/// Stop the content of a vertical scroll area short of the scroll bar.
///
/// egui draws the bar over the viewport's right edge. A control flush right
/// in the content, such as a check box, then lies under the bar and the bar
/// takes the click. Call this first in the scroll area's closure. The width
/// never falls below `floor`.
pub(crate) fn keep_scroll_gutter(ui: &mut Ui, floor: f32) {
    let s = ui.spacing().scroll;
    let gutter = s.bar_inner_margin + s.bar_width + s.bar_outer_margin;
    ui.set_max_width((ui.available_width() - gutter).max(floor));
}
