//! The bottom bar's laws (#1198): the camera buttons, the ribbon size and
//! the Move / Rotate modes that never share the part (#2047).
//! Provenance: our own tests over our own bar.
//! Gate map: covers ui::egui_app::viewport_bar.

use super::*;

#[test]
fn camera_buttons_flip_the_viewer_and_latch_a_tool() {
    // Projection: flips perspective <-> orthographic, the View
    // door's radios' true twin.
    let mut state = State::new();
    let p0 = state.cam.projection;
    act(&mut state, Nav::Projection);
    assert_ne!(state.cam.projection, p0, "the projection must flip");
    assert!(state.status.contains("orthographic"), "{:?}", state.status);
    act(&mut state, Nav::Projection);
    assert_eq!(state.cam.projection, p0, "one more press flips back");
    // The tool buttons latch once on and once off, and say so.
    for (nav, tool, word) in [
        (Nav::Orbit, DragMode::Orbit, "orbit"),
        (Nav::Pan, DragMode::Pan, "pan"),
        (Nav::Zoom, DragMode::Zoom, "zoom"),
    ] {
        let mut state = State::new();
        act(&mut state, nav);
        assert_eq!(state.nav_tool, Some(tool));
        assert!(state.status.contains(word), "{:?}", state.status);
        // Latching a different tool replaces the first.
        act(&mut state, if matches!(nav, Nav::Zoom) { Nav::Orbit } else { Nav::Zoom });
        assert_ne!(state.nav_tool, Some(tool));
        // The second press on the same tool latches off.
        act(&mut state, nav);
        assert_eq!(state.nav_tool, Some(tool));
        act(&mut state, nav);
        assert_eq!(state.nav_tool, None);
        assert_eq!(state.status, NAV_TOOL_CLEAR);
    }
}

#[test]
fn bar_fits_its_buttons() {
    assert_eq!(NAVS.len(), NAV_COUNT, "NAV_COUNT sizes the ribbon");
    let count = NAVS.len() as f32;
    let width = count * NAV_PX + (count - 1.0) * NAV_GAP + 2.0 * PAD;
    assert!((BAR_W - width).abs() < 1e-6, "the ribbon is as wide as its buttons");
    let pane = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(900.0, 600.0));
    let bar = bar_rect(pane);
    assert!((bar.width() - BAR_W).abs() < 1e-6);
    assert!(
        (bar.center().x - pane.center().x).abs() < 1e-3,
        "the ribbon is centred on the bottom edge"
    );
    assert!(
        (bar.max.y - (pane.max.y - BOTTOM_MARGIN)).abs() < 1e-3,
        "it rests on the bottom edge"
    );
    let kids = buttons(bar);
    assert_eq!(kids.len(), NAVS.len());
    assert!(kids.iter().all(|(_, r)| bar.contains(r.min) && bar.contains(r.max)));
    let last = kids.last().map(|(k, r)| (*k, *r));
    assert_eq!(
        last.map(|(k, _)| k),
        NAVS.last().copied(),
        "the last button is the last in the list"
    );
    assert!(
        last.is_some_and(|(_, r)| bar.contains(r.max)),
        "the last button sits inside the ribbon"
    );
}

#[test]
fn move_and_rotate_modes_never_share_the_part() {
    let mut state = State::new();
    act(&mut state, Nav::Rotate);
    assert!(state.rotate_handles && !state.move_handles);
    assert_eq!(state.status, rotate_label(true));
    act(&mut state, Nav::Move);
    assert!(state.move_handles && !state.rotate_handles, "Move on puts Rotate off");
    act(&mut state, Nav::Rotate);
    assert!(state.rotate_handles && !state.move_handles, "Rotate on puts Move off");
    act(&mut state, Nav::Rotate);
    assert!(!state.rotate_handles && !state.move_handles);
    assert_eq!(state.status, rotate_label(false));
    assert!(state.pending.is_empty(), "a mode is view state, never a command");
    // Rotate sits right after Move in the bar.
    let at = |k: Nav| NAVS.iter().position(|n| *n == k);
    assert_eq!(at(Nav::Rotate), at(Nav::Move).map(|i| i + 1));
}
