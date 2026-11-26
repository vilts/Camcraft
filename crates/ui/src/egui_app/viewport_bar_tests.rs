//! The bottom bar's laws (#1198): the camera buttons, the ribbon size and
//! the combined Move and Rotate handles (#2624).
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

/// #2818: the Display button opens and closes the menu and sits next to
/// the wireframe switch. The switch still gives wireframe in one press from
/// each of the four modes, and one more press gives the default mode.
#[test]
fn display_button_toggles_the_menu_and_wire_works_from_each_mode() {
    let mut state = State::new();
    act(&mut state, Nav::Display);
    assert!(state.display_menu_open);
    act(&mut state, Nav::Display);
    assert!(!state.display_menu_open);
    let at = |k: Nav| NAVS.iter().position(|n| *n == k);
    assert_eq!(at(Nav::Display), at(Nav::Wire).map(|i| i + 1));
    for mode in [DisplayMode::ShadedEdges, DisplayMode::Shaded, DisplayMode::ShadedHiddenEdges] {
        state.display = mode;
        act(&mut state, Nav::Wire);
        assert_eq!(state.display, DisplayMode::Wireframe, "from {mode:?}");
        act(&mut state, Nav::Wire);
        assert_eq!(state.display, DisplayMode::ShadedEdges);
    }
    // #2636: the see-through wireframe is a wireframe: one press leaves it.
    state.display = DisplayMode::WireframeSeeThrough;
    act(&mut state, Nav::Wire);
    assert_eq!(state.display, DisplayMode::ShadedEdges);
    assert!(state.pending.is_empty(), "view state, never a command");
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
fn move_gizmo_shows_arrows_and_rings_together() {
    let mut state = State::new();
    act(&mut state, Nav::Rotate);
    assert!(state.rotate_handles && !state.move_handles);
    assert_eq!(state.status, rotate_label(true));
    act(&mut state, Nav::Move);
    assert!(state.move_handles && state.rotate_handles, "Move shows arrows and rings together");
    act(&mut state, Nav::Rotate);
    assert!(state.move_handles && state.rotate_handles, "Rotate keeps both handle sets while Move is on");
    assert!(state.handle_bar.turn_value, "Rotate selects the angle editor while Move is on");
    act(&mut state, Nav::Move);
    assert!(!state.move_handles && !state.rotate_handles, "Move off clears both handle sets");
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

#[test]
fn move_glyph_shows_a_part_body_while_pan_shows_only_arrows() {
    let has_body = |nav| {
        let ctx = egui::Context::default();
        let state = State::new();
        let rect = egui::Rect::from_min_size(egui::pos2(40.0, 40.0), egui::vec2(NAV_PX, NAV_PX));
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            draw(ui, rect.expand(6.0), &[(nav, rect)], &state, None);
        });
        let body = out.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Rect(r)
                if r.stroke.width > 1.0
                    && r.rect.width() < NAV_PX * 0.65
                    && r.rect.height() < NAV_PX * 0.65)
        });
        out.textures_delta.clear();
        body
    };
    assert!(has_body(Nav::Move), "Move shows the part that its handles move");
    assert!(!has_body(Nav::Pan), "Pan remains a camera navigation glyph");
}

/// Render actual hover text, rather than checking a table that draw might not use.
fn hover_text(kind: Nav, supplied_hover: bool, outside: bool) -> String {
    fn collect(shape: &egui::Shape, text: &mut String) {
        match shape {
            egui::Shape::Text(t) => {
                text.push_str(t.galley.text());
                text.push('\n');
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, text);
                }
            }
            _ => {}
        }
    }
    let ctx = egui::Context::default();
    ctx.all_styles_mut(|style| {
        style.interaction.tooltip_delay = 0.0;
        style.interaction.show_tooltips_only_when_still = false;
    });
    let state = State::new();
    let pane = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 600.0));
    let bar = bar_rect(pane);
    let nav = buttons(bar);
    let target = nav.iter().find(|(button, _)| *button == kind).unwrap().1.center();
    let pointer = if outside { pane.center() } else { target };
    let mut text = String::new();
    for frame in 0..5 {
        let input = egui::RawInput {
            screen_rect: Some(pane),
            time: Some(f64::from(frame)),
            events: vec![egui::Event::PointerMoved(pointer)],
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            draw(ui, bar, &nav, &state, supplied_hover.then_some(pointer));
        });
        text.clear();
        for shape in &output.shapes {
            collect(&shape.shape, &mut text);
        }
        output.textures_delta.clear();
    }
    text
}

#[test]
fn every_bottom_toolbar_button_explains_its_action_on_hover() {
    let expected = [
        (Nav::Frame, "Frame all", "Fit the whole document in the view."),
        (Nav::Iso, "Isometric view", "Look at the model from an isometric angle."),
        (Nav::Wire, "Wireframe", "Switch between shaded edges and wireframe."),
        (Nav::Display, "Display", "Open or close the menu of display modes and graphics."),
        (Nav::Paths, "Toolpaths", "Show or hide all cached toolpaths."),
        (Nav::Axes, "World axes", "Show or hide the world origin axes."),
        (Nav::PickThrough, "Pick through", "Allow selection through the part."),
        (Nav::Move, "Move part", "Show handles to move the selected part."),
        (Nav::Rotate, "Rotate part", "Show rings to rotate the selected part."),
        (Nav::Projection, "Projection", "Switch between perspective and orthographic views."),
        (Nav::Orbit, "Orbit", "Drag to turn the camera around the model."),
        (Nav::Pan, "Pan", "Drag to move the view sideways."),
        (Nav::Zoom, "Zoom", "Drag to move the camera closer or farther."),
    ];
    assert_eq!(expected.len(), NAVS.len());
    let mut missing = Vec::new();
    for (kind, name, action) in expected {
        let text = hover_text(kind, true, false);
        if !text.contains(name) || !text.contains(action) {
            missing.push(format!("{kind:?}: expected {name:?} and {action:?}; painted: {text:?}"));
        }
    }
    assert!(missing.is_empty(), "missing tooltips:\n{}", missing.join("\n"));
}

#[test]
fn bottom_toolbar_does_not_show_tooltips_outside_or_when_hover_is_suppressed() {
    for kind in NAVS {
        assert!(hover_text(kind, true, true).is_empty(), "{kind:?}: pointer outside toolbar");
        assert!(hover_text(kind, false, false).is_empty(), "{kind:?}: hover suppressed during drag");
    }
}

#[test]
fn expanded_bar_contains_controls_without_touching_buttons() {
    let pane = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(900.0, 600.0));
    let base = bar_rect(pane);
    assert_eq!(panel_rect(pane, false), base);
    let panel = panel_rect(pane, true);
    assert!(panel.contains(base.min) && panel.contains(base.max));
    assert!(panel.min.y < base.min.y, "expanded bar grows upward");
    let ctrl = controls_rect(panel);
    assert!(panel.contains(ctrl.min) && panel.contains(ctrl.max));
    for (_, btn) in buttons(base) {
        assert!(!ctrl.intersects(btn), "controls {ctrl:?} must not intersect nav button {btn:?}");
    }
    let ctx = egui::Context::default();
    let mut state = State::new();
    state.move_handles = true;
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        controls::draw(ui, &mut state, ctrl);
    });
    out.textures_delta.clear();
}

#[test]
fn typed_move_and_turn_respect_units_and_refuse_bad_input() {
    let mut state = State::new();
    let mesh = camcraft_app::geometry::cube(
        camcraft_app::geometry::Point3::new(0.0, 0.0, 0.0),
        camcraft_app::geometry::Point3::new(20.0, 20.0, 20.0),
    );
    state.history.execute(&state.world, camcraft_app::commands::ImportModel::from_mesh("part", mesh));
    state.pending.push_back(crate::state::CommandKind::Select(Some("part".into())));
    state.drain_actions();
    state.pending.push_back(crate::state::CommandKind::SetUnits(camcraft_app::units::Unit::Inches));
    state.drain_actions();
    state.history.mark_clean();
    // Typing 0.5 in inch mode moves 12.7 mm exactly along Z.
    state.handle_bar.axis = 2;
    state.handle_bar.move_draft = "0.5".into();
    controls::apply_move_text(&mut state);
    state.drain_actions();
    let e = camcraft_app::model::find_by_name(&state.world, "part").expect("part");
    let pos = camcraft_app::model::transform_of(&state.world, &e).unwrap_or_default().pos;
    assert!((pos.z - 12.7).abs() < 1e-12, "0.5 in = 12.7 mm, got {}", pos.z);
    assert!(state.history.is_dirty());
    // Zero and non-finite inputs queue nothing.
    state.history.mark_clean();
    state.handle_bar.move_draft = "0".into();
    controls::apply_move_text(&mut state);
    state.handle_bar.rotate_draft = "NaN".into();
    controls::apply_turn_text(&mut state);
    state.drain_actions();
    assert!(!state.history.is_dirty());
    assert!(state.status.contains("part not turned"), "{}", state.status);
}

#[path = "viewport_bar_tooltip_tests.rs"]
mod tooltip_tests;

