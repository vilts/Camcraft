//! Gate map: covers ui::egui_app::viewport_bar.
//! Viewport bar tooltip and label tests, plus visual evidence screenshot generator (#2914).
//! Provenance: our own tests over our own bar.

use super::*;
use crate::state::{DisplayMode, DragMode, State};
use crate::viewport::ProjectionMode;

#[test]
fn nav_buttons_have_distinct_labels_and_descriptive_tooltips() {
    let mut state = State::new();
    assert_eq!(NAVS.len(), NAV_COUNT);
    let mut seen = std::collections::HashSet::new();
    for &kind in &NAVS {
        let lab = label(kind);
        assert!(!lab.is_empty(), "label for {:?} must not be empty", kind);
        assert!(seen.insert(lab), "label {lab:?} of {kind:?} is used by two buttons");
        // The names of other controls in the window: a bar button must not
        // take one of them, or a query by name finds two nodes (#2914).
        for taken in ["Show all toolpaths", "Projection", "Wireframe", crate::egui_app::display_menu::GROUP_MODE] {
            assert_ne!(lab, taken, "the {kind:?} button takes the name of another control");
        }
        let tip = tooltip(kind, &state);
        assert!(!tip.is_empty(), "tooltip for {:?} must not be empty", kind);
    }

    // Toggle states reflect in tooltips:
    // Wireframe toggle
    state.display = DisplayMode::ShadedEdges;
    assert!(tooltip(Nav::Wire, &state).contains("shaded with edges"));
    state.display = DisplayMode::Wireframe;
    assert!(tooltip(Nav::Wire, &state).contains("wireframe"));

    // Display menu toggle
    state.display_menu_open = false;
    assert!(tooltip(Nav::Display, &state).contains("closed"));
    state.display_menu_open = true;
    assert!(tooltip(Nav::Display, &state).contains("open"));

    // Toolpaths toggle
    state.show_all_paths = false;
    assert!(tooltip(Nav::Paths, &state).contains("off"));
    state.show_all_paths = true;
    assert!(tooltip(Nav::Paths, &state).contains("on"));

    // World axes toggle
    state.show_world_axes = false;
    assert!(tooltip(Nav::Axes, &state).contains("off"));
    state.show_world_axes = true;
    assert!(tooltip(Nav::Axes, &state).contains("on"));

    // Pick through toggle
    state.pick_through = false;
    assert!(tooltip(Nav::PickThrough, &state).contains("off"));
    state.pick_through = true;
    assert!(tooltip(Nav::PickThrough, &state).contains("on"));

    // Move handles toggle
    state.move_handles = false;
    assert!(tooltip(Nav::Move, &state).contains("off"));
    state.move_handles = true;
    assert!(tooltip(Nav::Move, &state).contains("on"));

    // Rotate handles toggle
    state.rotate_handles = false;
    assert!(tooltip(Nav::Rotate, &state).contains("off"));
    state.rotate_handles = true;
    assert!(tooltip(Nav::Rotate, &state).contains("on"));

    // Projection toggle
    state.cam.projection = ProjectionMode::Perspective;
    assert!(tooltip(Nav::Projection, &state).contains("perspective"));
    state.cam.projection = ProjectionMode::Orthographic;
    assert!(tooltip(Nav::Projection, &state).contains("orthographic"));

    // Nav tools
    state.nav_tool = None;
    assert!(tooltip(Nav::Orbit, &state).contains("click to latch"));
    assert!(tooltip(Nav::Pan, &state).contains("click to latch"));
    assert!(tooltip(Nav::Zoom, &state).contains("click to latch"));
    state.nav_tool = Some(DragMode::Orbit);
    assert!(tooltip(Nav::Orbit, &state).contains("active"));
    state.nav_tool = Some(DragMode::Pan);
    assert!(tooltip(Nav::Pan, &state).contains("active"));
    state.nav_tool = Some(DragMode::Zoom);
    assert!(tooltip(Nav::Zoom, &state).contains("active"));
}

#[test]
fn nav_buttons_attach_tooltips_on_hover() {
    use egui_kittest::kittest::Queryable;
    use egui_kittest::Harness;

    let state = State::new();
    let pane = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(900.0, 600.0));
    let bar = bar_rect(pane);
    let btns = buttons(bar);
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui(move |ui| {
            let hover = ui.input(|i| i.pointer.hover_pos());
            draw(ui, bar, &btns, &state, hover);
        });

    h.ctx.all_styles_mut(|s| {
        s.interaction.tooltip_delay = 0.0;
        s.interaction.show_tooltips_only_when_still = false;
    });

    h.run_steps(2);

    fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(t) => out.push(t.galley.text().to_owned()),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }

    let all_buttons = buttons(bar);

    // Verify all buttons have accessible labels
    for &(kind, btn_rect) in &all_buttons {
        let lab = label(kind);
        let node = h.get_by_label(lab);
        assert_eq!(node.rect(), btn_rect, "node for {:?} must match button rect", kind);
    }

    // Verify hover tooltip is displayed when hovering over a button
    h.hover_at(all_buttons[0].1.center());
    h.run_steps(2);

    let mut painted = Vec::new();
    for shape in &h.output().shapes {
        walk(&shape.shape, &mut painted);
    }

    assert!(
        painted.iter().any(|s| s.contains("Frame all")),
        "hovering over Frame button must paint a tooltip containing 'Frame all', got: {:?}",
        painted
    );
}

#[test]
#[ignore = "evidence generator: opt-in with CAMCRAFT_EVIDENCE_SCREENSHOTS_OUT"]
fn generate_evidence_screenshots() {
    let _gpu = crate::headless_gpu::gpu_guard(); // #3605: the kittest renderer makes devices
    let Some(out_dir) = std::env::var_os("CAMCRAFT_EVIDENCE_SCREENSHOTS_OUT")
        .map(std::path::PathBuf::from)
    else {
        return;
    };
    let _ = std::fs::create_dir_all(&out_dir);
    let pane = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(600.0, 120.0));
    let bar = bar_rect(pane);
    let all_buttons = buttons(bar);
    let find_btn = |nav: Nav| all_buttons.iter().position(|&(k, _)| k == nav);

    let render_shot = |name: &str, mut setup: Box<dyn FnMut(&mut State)>, hover: Option<usize>| {
        let mut state = State::new();
        setup(&mut state);
        let btns = all_buttons.clone();
        let hover_pos = hover.map(|idx| btns[idx].1.center());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(600.0, 120.0))
            .build_ui(move |ui| {
                ui.painter().rect_filled(pane, 0.0, egui::Color32::from_rgb(0x17, 0x1A, 0x1F));
                draw(ui, bar, &btns, &state, hover_pos);
            });

        h.ctx.all_styles_mut(|s| {
            s.interaction.tooltip_delay = 0.0;
            s.interaction.show_tooltips_only_when_still = false;
        });

        h.run_steps(2);
        if let Some(pos) = hover_pos {
            h.hover_at(pos);
            h.run_steps(3);
        }

        h.render()
            .expect("render")
            .save(out_dir.join(name))
            .expect("save image");
    };

    // 00: At rest (baseline)
    render_shot("00_at_rest.png", Box::new(|_| {}), None);

    // 01: Frame all
    render_shot("01_frame_all.png", Box::new(|_| {}), find_btn(Nav::Frame));

    // 02: Isometric view
    render_shot("02_iso_view.png", Box::new(|_| {}), find_btn(Nav::Iso));

    // 03 & 04: Display mode (Shaded vs Wireframe)
    render_shot(
        "03_wire_shaded.png",
        Box::new(|st| st.display = DisplayMode::ShadedEdges),
        find_btn(Nav::Wire),
    );
    render_shot(
        "04_wire_wireframe.png",
        Box::new(|st| st.display = DisplayMode::Wireframe),
        find_btn(Nav::Wire),
    );

    // 05 & 06: Show all toolpaths (Off vs On)
    render_shot(
        "05_paths_off.png",
        Box::new(|st| st.show_all_paths = false),
        find_btn(Nav::Paths),
    );
    render_shot(
        "06_paths_on.png",
        Box::new(|st| st.show_all_paths = true),
        find_btn(Nav::Paths),
    );

    // 07 & 08: World axes (Off vs On)
    render_shot(
        "07_axes_off.png",
        Box::new(|st| st.show_world_axes = false),
        find_btn(Nav::Axes),
    );
    render_shot(
        "08_axes_on.png",
        Box::new(|st| st.show_world_axes = true),
        find_btn(Nav::Axes),
    );

    // 09 & 10: Pick through (Off vs On)
    render_shot(
        "09_pick_through_off.png",
        Box::new(|st| st.pick_through = false),
        find_btn(Nav::PickThrough),
    );
    render_shot(
        "10_pick_through_on.png",
        Box::new(|st| st.pick_through = true),
        find_btn(Nav::PickThrough),
    );

    // 11 & 12: Move mode (Off vs On)
    render_shot(
        "11_move_mode_off.png",
        Box::new(|st| st.move_handles = false),
        find_btn(Nav::Move),
    );
    render_shot(
        "12_move_mode_on.png",
        Box::new(|st| st.move_handles = true),
        find_btn(Nav::Move),
    );

    // 13 & 14: Rotate mode (Off vs On)
    render_shot(
        "13_rotate_mode_off.png",
        Box::new(|st| st.rotate_handles = false),
        find_btn(Nav::Rotate),
    );
    render_shot(
        "14_rotate_mode_on.png",
        Box::new(|st| st.rotate_handles = true),
        find_btn(Nav::Rotate),
    );

    // 15 & 16: Projection (Perspective vs Orthographic)
    render_shot(
        "15_projection_perspective.png",
        Box::new(|st| st.cam.projection = ProjectionMode::Perspective),
        find_btn(Nav::Projection),
    );
    render_shot(
        "16_projection_ortho.png",
        Box::new(|st| st.cam.projection = ProjectionMode::Orthographic),
        find_btn(Nav::Projection),
    );

    // 17 & 18: Orbit tool (Unlatched vs Latched Active)
    render_shot(
        "17_orbit_latched_off.png",
        Box::new(|st| st.nav_tool = None),
        find_btn(Nav::Orbit),
    );
    render_shot(
        "18_orbit_latched_on.png",
        Box::new(|st| st.nav_tool = Some(DragMode::Orbit)),
        find_btn(Nav::Orbit),
    );

    // 19 & 20: Pan tool (Unlatched vs Latched Active)
    render_shot(
        "19_pan_latched_off.png",
        Box::new(|st| st.nav_tool = None),
        find_btn(Nav::Pan),
    );
    render_shot(
        "20_pan_latched_on.png",
        Box::new(|st| st.nav_tool = Some(DragMode::Pan)),
        find_btn(Nav::Pan),
    );

    // 21 & 22: Zoom tool (Unlatched vs Latched Active)
    render_shot(
        "21_zoom_latched_off.png",
        Box::new(|st| st.nav_tool = None),
        find_btn(Nav::Zoom),
    );
    render_shot(
        "22_zoom_latched_on.png",
        Box::new(|st| st.nav_tool = Some(DragMode::Zoom)),
        find_btn(Nav::Zoom),
    );

    // 23 & 24: Display menu (Closed vs Open)
    render_shot(
        "23_display_menu_closed.png",
        Box::new(|st| st.display_menu_open = false),
        find_btn(Nav::Display),
    );
    render_shot(
        "24_display_menu_open.png",
        Box::new(|st| st.display_menu_open = true),
        find_btn(Nav::Display),
    );
}
