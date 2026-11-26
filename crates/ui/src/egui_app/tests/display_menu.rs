//! The viewport's Display menu (#2818 "Done means"): the Display button opens
//! the menu; each entry sets its state field and shows its checked state;
//! the labels exist; no entry queues a command; "Look at a face" turns the
//! view to face the clicked face; the kept settings reach the view file.
//! Provenance: our own tests over our own menu; no vendor material.
//! Gate map: covers ui::egui_app::display_menu, ui::look_at, ui::state_view.

use egui_kittest::kittest::{NodeT, Queryable};

use super::*;
use crate::egui_app::display_menu as menu;
use crate::state::DisplayMode;
use crate::viewport::ProjectionMode;

type Toggled = egui::accesskit::Toggled;

/// A shell with the Display menu open, through a real press on the button.
fn open_menu() -> Harness<'static, ShellUi> {
    let mut h = shell_harness();
    h.run();
    assert!(!h.state().state.display_menu_open, "closed at the start");
    let p = hud(h.state()).nav_rect(viewcube::Nav::Display).center();
    click_viewport(&mut h, p);
    assert!(h.state().state.display_menu_open, "a press on Display opens the menu");
    h
}

fn checked(h: &Harness<'static, ShellUi>, label: &str) -> Option<Toggled> {
    h.get_by_label(label).accesskit_node().toggled()
}

#[test]
fn the_display_button_opens_and_closes_the_menu_and_the_labels_exist() {
    let mut h = open_menu();
    for group in [menu::GROUP_MODE, menu::GROUP_LOOK_AT, menu::GROUP_GRAPHICS] {
        assert!(h.query_by_label(group).is_some(), "group title {group:?}");
    }
    for label in ["Shaded, with edges", "Shaded", "Wireframe", "Wireframe, see-through", "Shaded, with hidden edges"] {
        assert!(h.query_by_label(label).is_some(), "display mode {label:?}");
    }
    for label in [menu::LOOK_AT_FACE, menu::SHOW_GRID, menu::SHOW_AXES, menu::PERSPECTIVE, menu::ORTHOGRAPHIC] {
        assert!(h.query_by_label(label).is_some(), "entry {label:?}");
    }
    // A second press on the button closes the menu, and it stays closed.
    let p = hud(h.state()).nav_rect(viewcube::Nav::Display).center();
    click_viewport(&mut h, p);
    assert!(!h.state().state.display_menu_open);
    assert!(h.query_by_label("Shaded, with hidden edges").is_none(), "a closed menu draws no entry");
    assert!(h.state().state.pending.is_empty(), "the menu is view state, never a command");
}

#[test]
fn each_display_mode_entry_sets_the_mode_and_shows_the_check() {
    let mut h = open_menu();
    assert_eq!(checked(&h, "Shaded, with edges"), Some(Toggled::True), "the default mode is checked");
    for mode in [DisplayMode::Shaded, DisplayMode::Wireframe, DisplayMode::ShadedHiddenEdges, DisplayMode::ShadedEdges] {
        h.get_by_label(mode.label()).click();
        h.run_steps(2);
        assert_eq!(h.state().state.display, mode, "{:?} sets its mode", mode.label());
        for other in DisplayMode::ALL {
            let want = if other == mode { Toggled::True } else { Toggled::False };
            assert_eq!(checked(&h, other.label()), Some(want), "check of {:?} in {mode:?}", other.label());
        }
        assert!(h.state().state.display_menu_open, "a choice keeps the menu open");
    }
    assert!(h.state().state.pending.is_empty(), "a display mode is view state, never a command");
}

#[test]
fn the_graphics_entries_set_their_fields() {
    let mut h = open_menu();
    assert!(h.state().state.show_grid && h.state().state.show_world_axes, "both show at the start");
    assert_eq!(checked(&h, menu::SHOW_GRID), Some(Toggled::True));
    let grid_lines = |h: &Harness<'static, ShellUi>| crate::interact::ground_grid_lines(&h.state().state).len();
    assert!(grid_lines(&h) > 0, "the grid draws at the start");
    h.get_by_label(menu::SHOW_GRID).click();
    h.run_steps(2);
    assert!(!h.state().state.show_grid);
    assert_eq!(checked(&h, menu::SHOW_GRID), Some(Toggled::False));
    assert_eq!(grid_lines(&h), 0, "the Grid box removes the grid lines from the draw list");
    assert!(h.state().state.show_world_axes, "the axes stay");
    h.get_by_label(menu::SHOW_GRID).click();
    h.run_steps(2);
    assert!(grid_lines(&h) > 0, "and the grid comes back");
    h.get_by_label(menu::SHOW_AXES).click();
    h.run_steps(2);
    assert!(!h.state().state.show_world_axes);
    assert_eq!(checked(&h, menu::SHOW_AXES), Some(Toggled::False));
    assert!(h.state().state.show_grid, "the Origin axes box leaves the grid switch alone");
    assert!(grid_lines(&h) > 0, "the grid still draws with the axes off (#3322)");
    // Projection: one choice of two.
    assert_eq!(h.state().state.cam.projection, ProjectionMode::Perspective);
    assert_eq!(checked(&h, menu::PERSPECTIVE), Some(Toggled::True));
    h.get_by_label(menu::ORTHOGRAPHIC).click();
    h.run_steps(2);
    assert_eq!(h.state().state.cam.projection, ProjectionMode::Orthographic);
    assert_eq!(checked(&h, menu::ORTHOGRAPHIC), Some(Toggled::True));
    assert_eq!(checked(&h, menu::PERSPECTIVE), Some(Toggled::False));
    h.get_by_label(menu::PERSPECTIVE).click();
    h.run_steps(2);
    assert_eq!(h.state().state.cam.projection, ProjectionMode::Perspective);
    assert!(h.state().state.pending.is_empty(), "graphics switches are view state");
}

/// The camera's forward direction (into the screen).
fn forward(h: &Harness<'static, ShellUi>) -> [f64; 3] {
    let (_, _, (x, y, z)) = h.state().state.cam.view_basis();
    [x, y, z]
}

#[test]
fn look_at_turns_the_view_to_face_the_clicked_face() {
    let mut h = open_menu();
    h.get_by_label(menu::LOOK_AT_FACE).click();
    h.run_steps(2);
    assert!(h.state().state.look_at_armed, "the entry arms the pick");
    assert!(!h.state().state.display_menu_open, "the menu moves away for the click");
    assert_eq!(h.state().state.status, crate::look_at::ARMED);
    // The seeded block is a box: the face under the pane centre has an
    // axis direction, and the house view does not face it yet.
    let c = vp_center(h.state());
    let d = crate::look_at::face_direction(&h.state().state, [c.x, c.y]).expect("the centre ray hits the block");
    let axis = d.iter().filter(|v| v.abs() > 0.999).count() == 1 && d.iter().filter(|v| v.abs() < 1e-9).count() == 2;
    assert!(axis, "a box face has an axis direction: {d:?}");
    let before = forward(&h);
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    assert!(dot(before, d) > -0.99, "the house view does not face the face yet");
    assert!(dot(before, d) < 0.0, "the face direction points toward the eye");
    let (target, dist) = (h.state().state.cam.target, h.state().state.cam.dist);
    click_viewport(&mut h, c);
    let st = &h.state().state;
    assert!(dot(forward(&h), d) < -0.999_999, "the view looks straight at the face: {:?} vs {d:?}", forward(&h));
    assert_eq!((st.cam.target, st.cam.dist), (target, dist), "the view turns; it does not pan or zoom");
    assert!(!st.look_at_armed, "one click, then the pick is off");
    assert_eq!(st.status, crate::look_at::DONE);
    assert!(st.pending.is_empty(), "Look at is view state, never a command");
    let block = model::find_by_name(&st.world, "Block").expect("seed has Block");
    assert!(!model::is_selected(&st.world, &block), "the armed click selects nothing");
}

#[test]
fn look_at_uses_the_face_normal_in_the_active_setup_frame() {
    use camcraft_app::commands::{ActivateSetup, AlignPartToSetup, CreateSetup};

    let mut h = shell_harness();
    h.run();
    let st = &mut h.state_mut().state;
    st.history.execute(
        &st.world,
        CreateSetup::new(&st.world, None, None).expect("Setup1"),
    );
    st.history.execute(
        &st.world,
        ActivateSetup::new(&st.world, Some("Setup1")).expect("activate Setup1"),
    );
    st.history.execute(
        &st.world,
        AlignPartToSetup::angles(&st.world, "Setup1", [90.0, 0.0, 0.0])
            .expect("rotate the part about X"),
    );
    // The seeded block's local top centre [30, 30, 30] is shown at
    // [30, -30, 30]. Its local +Z face has a shown normal of -Y.
    st.cam.target = [30.0, -30.0, 30.0];
    st.cam = st.cam.toward([0.2, -1.0, 0.3]);
    crate::look_at::arm(st, true);
    h.run();
    let c = vp_center(h.state());
    let d = crate::look_at::face_direction(&h.state().state, [c.x, c.y])
        .expect("the ray hits the rotated top face");
    for (actual, expected) in d.into_iter().zip([0.0, -1.0, 0.0]) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "the face normal is in the shown frame: {d:?}"
        );
    }
    click_viewport(&mut h, c);
    for (actual, expected) in forward(&h).into_iter().zip([0.0, 1.0, 0.0]) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "the view faces the rotated top face"
        );
    }
    assert_eq!(h.state().state.status, crate::look_at::DONE);
}

#[test]
fn look_at_on_empty_space_changes_nothing_and_says_so() {
    let mut h = shell_harness();
    h.run();
    crate::look_at::arm(&mut h.state_mut().state, true);
    let cam = forward(&h);
    // Just inside the pane's top-left corner: no body is there.
    let ([x0, y0], _) = h.state().state.viewport_rect;
    let p = egui::pos2(x0 + 6.0, y0 + 6.0);
    assert!(crate::look_at::face_direction(&h.state().state, [p.x, p.y]).is_none());
    click_viewport(&mut h, p);
    let st = &h.state().state;
    assert_eq!(forward(&h), cam, "a miss does not move the view");
    assert_eq!(st.status, crate::look_at::MISS);
    assert!(!st.look_at_armed);
}

#[test]
fn the_display_mode_and_the_grid_are_kept_in_the_view_file() {
    use camcraft_app::view;
    let dir = std::env::temp_dir().join(format!("camcraft_display_menu_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("make the test folder");
    let file = dir.join(view::FILE_NAME);
    assert_eq!(view::DEFAULT_DISPLAY, DisplayMode::default().key(), "the two crates agree on the default key");
    let mut h = shell_harness();
    h.state_mut().state.load_view_from(&file);
    h.run();
    let p = hud(h.state()).nav_rect(viewcube::Nav::Display).center();
    click_viewport(&mut h, p);
    h.get_by_label("Shaded, with hidden edges").click();
    h.run_steps(2);
    h.get_by_label(menu::SHOW_GRID).click();
    h.run_steps(2);
    let (prefs, note) = view::load(&file);
    assert_eq!(note, None);
    assert_eq!(prefs.display, "shaded-hidden-edges");
    assert!(!prefs.show_grid);
    assert!(prefs.show_gizmo_names, "the other kept setting is not lost");
    // The next run starts with the same view.
    let mut next = State::new();
    next.load_view_from(&file);
    assert_eq!(next.display, DisplayMode::ShadedHiddenEdges);
    assert!(!next.show_grid);
    // An old file has no such keys: the defaults load, with no note.
    std::fs::write(&file, r#"{"format":"camcraft-view/1","view":{"show_gizmo_names":false}}"#).expect("write the old file");
    let mut old = State::new();
    old.display = DisplayMode::Wireframe;
    old.load_view_from(&file);
    assert_eq!(old.display, DisplayMode::ShadedEdges, "an old file loads the default mode");
    assert!(old.show_grid && !old.show_gizmo_names);
    // A key from a newer build is read as the default mode.
    std::fs::write(&file, r#"{"format":"camcraft-view/1","view":{"display":"x-ray"}}"#).expect("write the newer file");
    old.load_view_from(&file);
    assert_eq!(old.display, DisplayMode::ShadedEdges);
    let _ = std::fs::remove_dir_all(&dir);
}
