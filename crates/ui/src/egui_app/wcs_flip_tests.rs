//! #1720's Flip X / Flip Y / Flip Z buttons on the WCS page: three equal
//! toggles under the quarter turns, each a live button whose lit state is
//! the stored flag; a click lands one undoable `SetWcsFlip`, the viewport
//! triad draws the flipped axis reversed, and undo clears it again.
//! Structural assertions only (no snapshot PNG changes).
//! Provenance: our own page law; no vendor material consulted.

use egui::accesskit::Toggled;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use super::wcs_page::FLIPS;
use super::wcs_page_tests::page;
use crate::interact::gizmos;
use crate::state::{CommandKind, State};

fn lit(h: &Harness<'static, State>, label: &str) -> bool {
    h.get_by_label(label).accesskit_node().toggled() == Some(Toggled::True)
}

fn flip(state: &State) -> [bool; 3] {
    camcraft_app::setup_wcs_flip::of_setup(&state.world, None)
}

#[test]
fn the_flip_buttons_sit_under_the_turns_as_equal_live_toggles() {
    let h = page(State::new());
    let turn = h.get_by_label("Turn X-").rect();
    let level = h.get_by_label("Level").rect();
    let r: Vec<egui::Rect> = FLIPS.iter().map(|l| h.get_by_label(l).rect()).collect();
    for (i, l) in FLIPS.iter().enumerate() {
        assert!(!h.get_by_label(l).accesskit_node().is_disabled(), "{l} is live");
        assert!(!lit(&h, l), "{l} starts off");
        assert!((r[i].width() - turn.width()).abs() < 0.5, "{l} is a turn's width");
        assert!(r[i].min.y >= turn.max.y && r[i].max.y <= level.min.y, "{l} sits between the turns and Level");
    }
    assert!(h.query_by_label_contains("A flipped axis does not post yet").is_none(), "no note with no flip");
}

#[test]
fn a_flip_click_lands_one_undoable_command_and_reverses_the_triad() {
    let mut h = page(State::new());
    h.get_by_label("Flip Y").click();
    h.run();
    h.run();
    assert_eq!(flip(h.state()), [false, true, false], "{}", h.state().status);
    assert!(lit(&h, "Flip Y") && !lit(&h, "Flip X") && !lit(&h, "Flip Z"));
    assert!(h.query_by_label_contains("A flipped axis does not post yet").is_some(), "the page says it does not post");
    let triad = gizmos(h.state()).into_iter().find(|g| !g.world_axes).expect("the G54 triad");
    assert_eq!(triad.flip, [false, true, false]);
    assert_eq!(triad.axis_dir(1, &[0.0, 1.0, 0.0]), [0.0, -1.0, 0.0], "the Y axis points the other way");
    assert_eq!(triad.axis_dir(0, &[1.0, 0.0, 0.0]), [1.0, 0.0, 0.0], "X is not touched");
    assert!(
        gizmos(h.state()).iter().any(|g| g.world_axes) || !h.state().show_world_axes,
        "a flipped WCS at the origin does not hide the world axes"
    );

    h.state_mut().pending.push_back(CommandKind::Undo);
    h.run();
    h.run();
    assert_eq!(flip(h.state()), [false; 3], "undo clears the flip");
    assert!(!lit(&h, "Flip Y"));
    h.state_mut().pending.push_back(CommandKind::Redo);
    h.run();
    h.run();
    assert_eq!(flip(h.state()), [false, true, false], "redo sets it again");
}

#[test]
fn two_clicks_on_one_axis_clear_it_and_the_others_stack() {
    let mut state = State::new();
    for f in [[true, false, false], [true, false, true], [false, false, true]] {
        state.pending.push_back(CommandKind::SetWcsFlip { flip: f });
        state.drain_actions();
        assert_eq!(flip(&state), f, "{}", state.status);
    }
    state.pending.push_back(CommandKind::SetWcsFlip { flip: [false; 3] });
    state.drain_actions();
    assert_eq!(flip(&state), [false; 3]);
    assert!(state.status.contains("no axis flipped"), "{}", state.status);
}
