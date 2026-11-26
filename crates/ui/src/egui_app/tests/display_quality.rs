//! The Display menu's Graphics entries.
//!
//! #2834 ("Done means"): each quality entry sets its state field, a preset
//! sets its switches, the menu shows the active preset, the switches reach
//! the view file, and "Look at a work plane" turns the view to face the
//! plane of the clicked origin axis.
//!
//! #2958: the "Line width" and "Anti-aliasing" entries. A pick is a
//! per-user view setting: it goes out to the view file at once, survives a
//! restart, queues no command, and Ctrl+Z does not take it back. Another
//! view setting never resets it. An anti-aliasing count that the graphics
//! device does not offer is a disabled entry, and a pick of it is refused
//! by name and not kept.
//!
//! Provenance: our own tests over our own menu and shell; no vendor material.
//! Gate map: covers ui::egui_app::display_menu, ui::display_quality,
//! ui::look_at_plane, ui::state_view, app::view through the State handoff.

use std::path::PathBuf;

use camcraft_app::view::{self, Antialias, FILE_NAME, LineWidth};
use egui_kittest::kittest::{NodeT, Queryable};

use super::*;
use crate::display_quality::QualityPreset;
use crate::egui_app::display_menu as menu;
use crate::state::DisplayMode;

type Toggled = egui::accesskit::Toggled;

const PERFORMANCE: &str = QualityPreset::Performance.label();
const QUALITY: &str = QualityPreset::Quality.label();

fn open_menu() -> Harness<'static, ShellUi> {
    let mut h = shell_harness();
    h.run();
    let p = hud(h.state()).nav_rect(viewcube::Nav::Display).center();
    click_viewport(&mut h, p);
    assert!(h.state().state.display_menu_open, "a press on Display opens the menu");
    h
}

fn checked(h: &Harness<'static, ShellUi>, label: &str) -> Option<Toggled> {
    h.get_by_label(label).accesskit_node().toggled()
}

fn press(h: &mut Harness<'static, ShellUi>, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(2);
}

/// (ground shadow, ambient occlusion) of the state.
fn switches(h: &Harness<'static, ShellUi>) -> (bool, bool) {
    let st = &h.state().state;
    (st.ground_shadow, st.ambient_occlusion)
}

#[test]
fn each_quality_switch_sets_its_field_and_shows_the_check() {
    let mut h = open_menu();
    for label in [PERFORMANCE, QUALITY, menu::GROUND_SHADOW, menu::AMBIENT_OCCLUSION, menu::LOOK_AT_PLANE] {
        assert!(h.query_by_label(label).is_some(), "entry {label:?}");
    }
    assert_eq!(switches(&h), (false, false), "the two switches are off at the start");
    assert_eq!(checked(&h, menu::GROUND_SHADOW), Some(Toggled::False));
    assert_eq!(checked(&h, menu::AMBIENT_OCCLUSION), Some(Toggled::False));
    press(&mut h, menu::GROUND_SHADOW);
    assert_eq!(switches(&h), (true, false), "Ground shadow sets its field only");
    assert_eq!(checked(&h, menu::GROUND_SHADOW), Some(Toggled::True));
    press(&mut h, menu::AMBIENT_OCCLUSION);
    assert_eq!(switches(&h), (true, true), "Ambient occlusion sets its field only");
    assert_eq!(checked(&h, menu::AMBIENT_OCCLUSION), Some(Toggled::True));
    press(&mut h, menu::GROUND_SHADOW);
    assert_eq!(switches(&h), (false, true));
    assert_eq!(checked(&h, menu::GROUND_SHADOW), Some(Toggled::False));
    assert!(h.state().state.display_menu_open, "a switch keeps the menu open");
    assert!(h.state().state.pending.is_empty(), "a quality switch is view state, never a command");
}

#[test]
fn a_preset_sets_its_switches_and_one_changed_switch_leaves_no_preset() {
    let mut h = open_menu();
    let active = |h: &Harness<'static, ShellUi>| {
        (checked(h, PERFORMANCE), checked(h, QUALITY))
    };
    assert_eq!(active(&h), (Some(Toggled::True), Some(Toggled::False)), "the start is Performance");
    press(&mut h, QUALITY);
    assert_eq!(switches(&h), (true, true), "Quality sets the two switches");
    assert_eq!(active(&h), (Some(Toggled::False), Some(Toggled::True)));
    assert_eq!(QualityPreset::active(h.state().state.effects()), Some(QualityPreset::Quality));
    // One switch changed by hand: no preset is active.
    press(&mut h, menu::AMBIENT_OCCLUSION);
    assert_eq!(switches(&h), (true, false));
    assert_eq!(active(&h), (Some(Toggled::False), Some(Toggled::False)), "no preset is active");
    assert_eq!(QualityPreset::active(h.state().state.effects()), None);
    press(&mut h, PERFORMANCE);
    assert_eq!(switches(&h), (false, false), "Performance clears the two switches");
    assert_eq!(active(&h), (Some(Toggled::True), Some(Toggled::False)));
    assert!(h.state().state.pending.is_empty(), "a preset is view state, never a command");
}

#[test]
fn wireframe_disables_the_two_switches_and_keeps_their_values() {
    let mut h = open_menu();
    press(&mut h, QUALITY);
    assert!(!disabled(&h, menu::GROUND_SHADOW) && !disabled(&h, menu::AMBIENT_OCCLUSION));
    // Both wireframe modes, "Wireframe, see-through" (#2636) included.
    for mode in [DisplayMode::Wireframe, DisplayMode::WireframeSeeThrough] {
        press(&mut h, mode.label());
        assert!(disabled(&h, menu::GROUND_SHADOW), "{mode:?} draws no shadow, and the menu says so");
        assert!(disabled(&h, menu::AMBIENT_OCCLUSION), "{mode:?} draws no lit face, and the menu says so");
        assert_eq!(switches(&h), (true, true), "the values stay for the next shaded mode");
        assert_eq!(checked(&h, menu::GROUND_SHADOW), Some(Toggled::True));
    }
    press(&mut h, DisplayMode::Shaded.label());
    assert!(!disabled(&h, menu::GROUND_SHADOW) && !disabled(&h, menu::AMBIENT_OCCLUSION));
}

#[test]
fn the_quality_switches_are_kept_in_the_view_file() {
    let dir = std::env::temp_dir().join(format!("camcraft_display_quality_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("make the test folder");
    let file = dir.join(view::FILE_NAME);
    let mut h = shell_harness();
    h.state_mut().state.load_view_from(&file);
    h.run();
    let p = hud(h.state()).nav_rect(viewcube::Nav::Display).center();
    click_viewport(&mut h, p);
    press(&mut h, menu::GROUND_SHADOW);
    let (prefs, note) = view::load(&file);
    assert_eq!(note, None);
    assert!(prefs.ground_shadow && !prefs.ambient_occlusion, "one switch is written at once");
    press(&mut h, QUALITY);
    let (prefs, _) = view::load(&file);
    assert!(prefs.ground_shadow && prefs.ambient_occlusion, "a preset is written at once");
    assert_eq!(prefs.display, "shaded-edges", "the other kept settings are not lost");
    // The next run starts with the same view.
    let mut next = State::new();
    next.load_view_from(&file);
    assert!(next.ground_shadow && next.ambient_occlusion);
    // An old file has no such keys: the two switches load off, with no note.
    std::fs::write(&file, r#"{"format":"camcraft-view/1","view":{"display":"shaded"}}"#).expect("write the old file");
    next.load_view_from(&file);
    assert!(!next.ground_shadow && !next.ambient_occlusion, "an old file loads the defaults");
    assert_eq!(next.display, DisplayMode::Shaded);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The camera's forward direction (into the screen).
fn forward(h: &Harness<'static, ShellUi>) -> [f64; 3] {
    let (_, _, (x, y, z)) = h.state().state.cam.view_basis();
    [x, y, z]
}

#[test]
fn look_at_turns_the_view_to_face_a_picked_work_plane() {
    let mut h = open_menu();
    press(&mut h, menu::LOOK_AT_PLANE);
    assert!(h.state().state.look_at_plane_armed, "the entry arms the plane pick");
    assert!(!h.state().state.look_at_armed, "the face pick is not armed");
    assert!(!h.state().state.display_menu_open, "the menu moves away for the click");
    assert_eq!(h.state().state.status, crate::look_at_plane::ARMED);
    // The X axis of the first origin in the view: its plane is YZ, and the
    // house view does not face it.
    let marks = crate::look_at_plane::axis_marks(&h.state().state);
    let mark = marks.iter().find(|m| m.axis == 0).expect("an origin shows its X axis").clone();
    let p = egui::pos2(
        mark.from[0] + (mark.to[0] - mark.from[0]) * 0.7,
        mark.from[1] + (mark.to[1] - mark.from[1]) * 0.7,
    );
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    assert!(dot(forward(&h), mark.normal) > -0.99, "the house view does not face the plane yet");
    let (target, dist) = (h.state().state.cam.target, h.state().state.cam.dist);
    click_viewport(&mut h, p);
    let st = &h.state().state;
    assert!(
        dot(forward(&h), mark.normal) < -0.999_999,
        "the view looks straight at the plane: {:?} vs {:?}",
        forward(&h),
        mark.normal
    );
    assert_eq!((st.cam.target, st.cam.dist), (target, dist), "the view turns; it does not pan or zoom");
    assert!(!st.look_at_plane_armed, "one click, then the pick is off");
    assert_eq!(st.status, crate::look_at_plane::done("YZ", &mark.origin));
    assert!(st.pending.is_empty(), "Look at is view state, never a command");
}

#[test]
fn a_click_on_the_origin_point_gives_the_xy_work_plane() {
    let mut h = shell_harness();
    h.run();
    crate::look_at_plane::arm(&mut h.state_mut().state, true);
    let marks = crate::look_at_plane::axis_marks(&h.state().state);
    let z = marks.iter().find(|m| m.axis == 2).expect("an origin shows its Z axis").clone();
    // Two px from the origin point, on the side away from the Z axis line.
    let along = [z.to[0] - z.from[0], z.to[1] - z.from[1]];
    let len = along[0].hypot(along[1]).max(1.0);
    let p = [z.from[0] - along[0] / len * 2.0, z.from[1] - along[1] / len * 2.0];
    let picked = crate::look_at_plane::pick(&h.state().state, p).expect("the origin point is within reach");
    assert_eq!((picked.axis, picked.origin.as_str()), (2, z.origin.as_str()), "the origin point gives Z");
    click_viewport(&mut h, egui::pos2(p[0], p[1]));
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    assert!(dot(forward(&h), z.normal) < -0.999_999, "the view looks down the Z axis of the origin");
    assert_eq!(h.state().state.status, crate::look_at_plane::done("XY", &z.origin));
}

#[test]
fn look_at_a_plane_away_from_each_axis_changes_nothing_and_says_so() {
    let mut h = shell_harness();
    h.run();
    crate::look_at_plane::arm(&mut h.state_mut().state, true);
    let cam = forward(&h);
    // Just inside the pane's top-left corner: no origin axis is there.
    let ([x0, y0], _) = h.state().state.viewport_rect;
    let p = egui::pos2(x0 + 6.0, y0 + 6.0);
    assert!(crate::look_at_plane::pick(&h.state().state, [p.x, p.y]).is_none());
    click_viewport(&mut h, p);
    let st = &h.state().state;
    assert_eq!(forward(&h), cam, "a miss does not move the view");
    assert_eq!(st.status, crate::look_at_plane::MISS);
    assert!(!st.look_at_plane_armed);
}

#[test]
fn arming_one_look_at_pick_disarms_the_other() {
    let mut h = open_menu();
    press(&mut h, menu::LOOK_AT_FACE);
    assert!(h.state().state.look_at_armed && !h.state().state.look_at_plane_armed);
    let p = hud(h.state()).nav_rect(viewcube::Nav::Display).center();
    click_viewport(&mut h, p);
    press(&mut h, menu::LOOK_AT_PLANE);
    assert!(!h.state().state.look_at_armed && h.state().state.look_at_plane_armed, "one pick at a time");
}

// ---- #2958: the line width and the anti-aliasing ----

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("camcraft-quality-ui-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("make the test folder");
    std::fs::canonicalize(&d).expect("canonical folder")
}

/// A shell that writes `file`, with the Display menu open through a real
/// press on the viewport's Display button.
fn open_menu_kept(file: &std::path::Path, offered: &[u32]) -> Harness<'static, ShellUi> {
    let mut h = shell_harness();
    h.state_mut().state.view_store = Some(file.to_path_buf());
    h.state_mut().state.samples_offered = offered.to_vec();
    h.run();
    let p = hud(h.state()).nav_rect(viewcube::Nav::Display).center();
    click_viewport(&mut h, p);
    assert!(h.state().state.display_menu_open, "a press on Display opens the menu");
    h
}

fn disabled(h: &Harness<'static, ShellUi>, label: &str) -> bool {
    h.get_by_label(label).accesskit_node().is_disabled()
}

#[test]
fn the_defaults_are_one_and_a_half_points_and_four_samples() {
    let state = State::new();
    assert_eq!((state.line_width, state.antialias), (LineWidth::Normal, Antialias::X4));
    assert_eq!(state.samples_offered, vec![1, 4], "headless keeps the WebGPU guaranteed counts");
}

#[test]
fn the_graphics_group_offers_the_line_widths_and_the_antialiasing() {
    let h = open_menu_kept(&temp_dir("labels").join(FILE_NAME), &[1, 4]);
    for label in [menu::LINE_WIDTH, menu::ANTIALIAS, "1 pt", "1.5 pt", "2 pt", "3 pt", "Off", "2x", "4x"] {
        assert!(h.query_by_label(label).is_some(), "entry {label:?}");
    }
    assert!(h.query_by_label("Line width").is_some());
}

#[test]
fn a_line_width_pick_is_kept_survives_a_restart_and_undo_leaves_it() {
    let file = temp_dir("width").join(FILE_NAME);
    let mut h = open_menu_kept(&file, &[1, 4]);
    // The seed document is already unsaved; the pick must not change that.
    let dirty = h.state().state.history.is_dirty();
    h.get_by_label("2 pt").click();
    h.run_steps(2);
    assert_eq!(h.state().state.line_width, LineWidth::Bold);
    assert!(h.state().state.display_menu_open, "a choice keeps the menu open, as the other entries do");
    assert!(h.state().state.pending.is_empty(), "a view setting queues no command");
    assert_eq!(view::load(&file).0.line_width, LineWidth::Bold, "the pick goes out to the file at once");
    assert_eq!(h.state().state.history.is_dirty(), dirty, "a view setting is not an unsaved change");
    h.state_mut().state.pending.push_back(CommandKind::Undo);
    h.run_steps(3);
    assert_eq!(h.state().state.line_width, LineWidth::Bold, "undo leaves the width");
    // The gizmo-name switch writes the same file: it keeps the width.
    h.state_mut().state.set_gizmo_names(false);
    assert_eq!(view::load(&file).0.line_width, LineWidth::Bold, "another view setting never resets it");
    let mut restarted = State::new();
    restarted.load_view_from(&file);
    assert_eq!(restarted.line_width, LineWidth::Bold, "the width survives a restart");
}

#[test]
fn an_offered_antialias_pick_is_kept_and_an_unoffered_one_is_disabled() {
    let file = temp_dir("aa").join(FILE_NAME);
    let mut h = open_menu_kept(&file, &[1, 4]);
    assert!(disabled(&h, "2x"), "2x is not offered on WebGPU's guaranteed counts");
    assert!(!disabled(&h, "Off") && !disabled(&h, "4x"));
    h.get_by_label("Off").click();
    h.run_steps(2);
    assert_eq!(h.state().state.antialias, Antialias::Off);
    assert_eq!(view::load(&file).0.antialias, Antialias::Off, "kept at once");
    // A device that offers 2x enables the entry.
    let h = open_menu_kept(&file, &[1, 2, 4]);
    assert!(!disabled(&h, "2x"), "an offered 2x is live");
}

#[test]
fn a_pick_of_an_unoffered_count_is_refused_by_name_and_not_kept() {
    let file = temp_dir("refuse").join(FILE_NAME);
    let mut state = State::new();
    state.view_store = Some(file.clone());
    state.select_antialias(Antialias::X2);
    assert_eq!(state.antialias, Antialias::X4, "the refused pick is not worn");
    assert!(state.status.contains("anti-aliasing 2x not selected"), "refused by name: {}", state.status);
    assert!(!file.exists(), "nothing is written for a refusal");
    state.samples_offered = vec![1, 2, 4];
    state.select_antialias(Antialias::X2);
    assert_eq!(state.antialias, Antialias::X2, "an offered count is kept");
    assert_eq!(view::load(&file).0.antialias, Antialias::X2);
}
