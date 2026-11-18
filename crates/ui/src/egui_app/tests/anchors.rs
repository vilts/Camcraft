//! #1953: pointer targets for behaviour tests that come from the live
//! layout, not from fixed screen numbers, so a UI refit that moves a pane
//! does not break a test that only asserts behaviour.
//! Provenance: our own test helpers over egui_dock's laid-out leaves and
//! the shell's viewport rect; no HSMWorks material involved.

use super::*;

/// The centre of the dock leaf that holds `tab`, as the last frame laid
/// it out. A missing leaf fails with the tab's name.
pub(super) fn pane_center(h: &Harness<'static, ShellUi>, tab: &Tab) -> egui::Pos2 {
    match super::layout::leaf_rect(&h.state().dock, tab) {
        Some(r) => r.center(),
        None => panic!("no laid-out dock leaf holds {tab:?}"),
    }
}

/// A point on the window chrome: the first screen corner (one point in
/// from the edge) that is outside the viewport and outside each rect in
/// `avoid`. A press there is not a pick and hits no listed widget,
/// wherever the refit puts the panes.
pub(super) fn chrome_point(h: &Harness<'static, ShellUi>, avoid: &[egui::Rect]) -> egui::Pos2 {
    let ([x0, y0], [x1, y1]) = h.state().state.viewport_rect;
    let vp = egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1));
    let s = h.ctx.content_rect().shrink(1.0);
    let corners = [s.left_top(), s.right_top(), s.left_bottom(), s.right_bottom()];
    match corners.into_iter().find(|p| !vp.contains(*p) && !avoid.iter().any(|r| r.contains(*p))) {
        Some(p) => p,
        None => panic!("every screen corner is inside the viewport {vp:?} or {avoid:?}"),
    }
}
