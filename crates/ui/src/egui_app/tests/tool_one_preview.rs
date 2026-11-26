//! #2017: the tool page has one preview - the side view - and it draws the
//! holder shape the collision check reads; the 3D view is a placement aid
//! that shows only while a Pick button is armed. Provenance: our own page
//! law; the test holder is our own geometry.

use super::tool_body::{TOOL, editor, prism_stl, shank_top, temp_stl};
use egui::{Color32, Shape};

use crate::state::BodyPickMode;
use camcraft_app::tool_body;
use super::*;

/// Background rects of the previews: the side view is 260 x 260, the 3D view 260 x 200.
fn previews(harness: &Harness<'static, ShellUi>) -> (usize, usize) {
    let (mut side, mut view3d) = (0, 0);
    fn walk(s: &Shape, side: &mut usize, view3d: &mut usize) {
        match s {
            Shape::Vec(v) => v.iter().for_each(|s| walk(s, side, view3d)),
            Shape::Rect(r) if r.fill == Color32::from_rgb(24, 26, 31) => {
                if (r.rect.width() - 260.0).abs() < 0.5 && (r.rect.height() - 260.0).abs() < 0.5 {
                    *side += 1;
                } else if (r.rect.width() - 260.0).abs() < 0.5 && (r.rect.height() - 200.0).abs() < 0.5 {
                    *view3d += 1;
                }
            }
            _ => {}
        }
    }
    for c in &harness.output().shapes {
        walk(&c.shape, &mut side, &mut view3d);
    }
    (side, view3d)
}

#[test]
fn the_tool_page_shows_one_preview_and_no_idle_3d_view() {
    let mut harness = editor();
    let path = temp_stl("one");
    prism_stl(&path, 15.0, 0.0, 30.0, 24);
    harness.state_mut().state.import_tool_body_from(&path);
    harness.run();
    harness.run();
    assert_eq!(previews(&harness), (1, 0), "one side view, no 3D view at rest");
    harness.get_by_label("Pick tip").click();
    harness.run();
    harness.run();
    assert_eq!(previews(&harness), (1, 1), "the armed pick shows the 3D placement view");
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn the_side_preview_bands_are_the_collision_stack_of_an_explicit_body() {
    let mut harness = editor();
    let path = temp_stl("bands");
    prism_stl(&path, 15.0, 0.0, 30.0, 24);
    let top = shank_top(&harness.state().state);
    {
        let st = &mut harness.state_mut().state;
        st.import_tool_body_from(&path);
        st.tool_body_ui.offset = top + 5.0;
    }
    harness.run();
    harness.get_by_label("Apply body").click();
    harness.run();
    harness.run();
    let st = &mut harness.state_mut().state;
    let e = model::find_by_name(&st.world, TOOL).expect("tool");
    let tool = model::tool_of(&st.world, &e).expect("tool record");
    let (_, profile) = tool_body::judged(&st.world, &e, tool.clone()).expect("body fits");
    let swept: Vec<(f64, f64, f64)> = tool_body::stack(&tool.geom(), profile.as_ref())
        .bands
        .iter()
        .map(|b| (b.above, b.len, b.r))
        .collect();
    assert!(swept.len() > 1, "the body gives bands: {swept:?}");
    assert_eq!(super::super::tool_body_page::envelope(st), Some(swept), "one shape for check and preview");
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

/// The background rect of the 3D placement view, when it shows.
fn view3d_rect(harness: &Harness<'static, ShellUi>) -> Option<egui::Rect> {
    fn walk(s: &Shape) -> Option<egui::Rect> {
        match s {
            Shape::Vec(v) => v.iter().find_map(walk),
            Shape::Rect(r) if r.fill == Color32::from_rgb(24, 26, 31) && (r.rect.height() - 200.0).abs() < 0.5 => Some(r.rect),
            _ => None,
        }
    }
    harness.output().shapes.iter().find_map(|c| walk(&c.shape))
}

#[test]
fn dragging_the_placement_preview_rotates_without_consuming_the_pick() {
    for (label, mode) in [("Pick tip", BodyPickMode::Tip), ("Pick axis", BodyPickMode::Axis)] {
        let mut harness = editor();
        let path = temp_stl("orbit");
        prism_stl(&path, 15.0, 0.0, 30.0, 24);
        harness.state_mut().state.import_tool_body_from(&path);
        harness.run();
        harness.get_by_label(label).click();
        harness.run();
        harness.run();
        let rect = view3d_rect(&harness).expect("the armed pick shows the 3D view");
        let before = {
            let d = &harness.state().state.tool_body_ui;
            (d.yaw, d.pitch, d.offset, d.axis)
        };
        let from = rect.center();
        set_pos(&mut harness, from);
        button_at(&mut harness, egui::PointerButton::Primary, from, true);
        for step in 1..=4 {
            set_pos(&mut harness, from + egui::vec2(10.0 * step as f32, 6.0 * step as f32));
        }
        button_at(&mut harness, egui::PointerButton::Primary, from + egui::vec2(40.0, 24.0), false);
        harness.run();
        let d = &harness.state().state.tool_body_ui;
        assert!(d.yaw != before.0 && d.pitch != before.1, "{label}: the drag turns the view: {before:?} -> ({}, {})", d.yaw, d.pitch);
        assert_eq!(d.pick, mode, "{label}: the drag keeps the pick armed");
        assert_eq!((d.offset, d.axis), (before.2, before.3), "{label}: the drag picks nothing");
        // A plain click still picks, and that ends the pick.
        let at = view3d_rect(&harness).expect("still armed").center();
        set_pos(&mut harness, at);
        button_at(&mut harness, egui::PointerButton::Primary, at, true);
        button_at(&mut harness, egui::PointerButton::Primary, at, false);
        harness.run();
        assert_eq!(harness.state().state.tool_body_ui.pick, BodyPickMode::Off, "{label}: the click picks");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}

#[test]
fn without_a_body_the_side_preview_has_no_envelope_bands() {
    let mut harness = editor();
    assert_eq!(super::super::tool_body_page::envelope(&mut harness.state_mut().state), None);
}

/// #2007: the Profile tab adds no second side view. The one preview draws
/// the drawn holder (as the bands the check sweeps) and the drawn cutter
/// (as the steel outline) while they validate, before any Apply.
#[test]
fn the_profile_tab_draws_its_drafts_in_the_one_preview() {
    use super::super::tool_preview_inputs::preview_inputs;
    let mut harness = editor();
    harness.get_by_label("Profile").click();
    harness.run();
    harness.run();
    assert_eq!(previews(&harness), (1, 0), "Draw holder: one preview");
    harness.get_by_label("Custom").click();
    harness.run();
    {
        let st = &mut harness.state_mut().state;
        st.tool_profile_ui.points[1].r = 40.0;
        st.tool_profile_ui.points[2].r = 40.0;
    }
    harness.run();
    let (bands, steel) = preview_inputs(&mut harness.state_mut().state, true);
    let reach = bands.expect("the drawn holder shows").iter().fold(0.0f64, |m, b| m.max(b.2));
    assert!((reach - 40.0).abs() < 1e-6, "the preview reaches r {reach}, the drawn holder r 40");
    assert!(steel.is_none(), "Draw holder keeps the cutter from the dimensions");
    harness.get_by_label("Draw cutter").click();
    harness.run();
    harness.run();
    assert_eq!(previews(&harness), (1, 0), "Draw cutter: one preview");
    let (bands, steel) = preview_inputs(&mut harness.state_mut().state, true);
    assert!(steel.is_some_and(|s| s.len() > 2), "the drawn cutter is the steel outline");
    assert!(bands.is_some_and(|b| !b.is_empty()), "the holder cylinder stands on it as bands");
}
