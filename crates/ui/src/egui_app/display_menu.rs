//! The viewport's Display menu (#2818): the toolbar's Display button opens
//! it above the ribbon. Three groups: the display mode (one choice), Look
//! at, and the graphics switches (what the view shows, and the projection).
//! Each entry is view state: a direct field write, never a command. The
//! display mode and the grid switch are kept preferences (`state_view`).
//! #2958 adds the overlay line width and the anti-aliasing to Graphics,
//! also kept in the view file. An anti-aliasing count that the graphics
//! device does not offer is a disabled entry that says why on hover.
//!
//! Each setting here draws through the one wgpu pass that the native window
//! and the browser shell share (`view_pass`), so no entry is hidden on wasm.
//! That holds for the ground shadow and the ambient occlusion too (#2834):
//! they are vertex buffers and alpha blending, with no texture (`shade_pass`).
//! The two wireframe modes draw neither, so the two switches are disabled
//! there and say so.
//!
//! Provenance: our own menu, written from the group list of issue #2818 and
//! published CAD viewport practice; our own wording. No vendor material.

use camcraft_app::view::{Antialias, LineWidth};

use crate::display_quality::QualityPreset;
use crate::state::{DisplayMode, State};
use crate::viewport::ProjectionMode;

/// The group titles and the entry labels: the tests read the same words.
pub(crate) const GROUP_MODE: &str = "Display mode";
pub(crate) const GROUP_LOOK_AT: &str = "Look at";
pub(crate) const GROUP_GRAPHICS: &str = "Graphics";
pub(crate) const LOOK_AT_FACE: &str = "Look at a face";
pub(crate) const SHOW_GRID: &str = "Grid";
pub(crate) const SHOW_AXES: &str = "Origin axes";
pub(crate) const PERSPECTIVE: &str = "Perspective view";
pub(crate) const ORTHOGRAPHIC: &str = "Orthographic view";
pub(crate) const LOOK_AT_PLANE: &str = "Look at a work plane";
pub(crate) const GROUND_SHADOW: &str = "Ground shadow";
pub(crate) const AMBIENT_OCCLUSION: &str = "Ambient occlusion";
pub(crate) const LINE_WIDTH: &str = "Line width";
pub(crate) const ANTIALIAS: &str = "Anti-aliasing";

/// The entry label of a line width.
#[must_use]
pub(crate) fn line_label(w: LineWidth) -> String {
    format!("{} pt", w.word())
}

/// Why an anti-aliasing entry cannot be picked on this device, if it cannot.
pub(crate) fn antialias_refusal(state: &State, a: Antialias) -> Option<String> {
    (!state.samples_offered.contains(&a.samples()))
        .then(|| format!("This graphics device does not offer {} anti-aliasing.", a.word()))
}

/// The gap between the menu and the button that opened it.
const GAP: f32 = 8.0;

/// Draw the menu while it is open. `button` is the Display button's rect:
/// the menu stands above it, and a press on the button is the button's own
/// (it closes the menu there), not a "click elsewhere".
pub(crate) fn draw(ui: &egui::Ui, state: &mut State, button: egui::Rect, pane: egui::Rect) {
    if !state.display_menu_open {
        return;
    }
    let area = egui::Area::new(egui::Id::new("viewport-display-menu"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(egui::pos2(button.center().x, button.min.y - GAP))
        .constrain_to(pane)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| entries(ui, state));
        });
    let on_button = ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| button.contains(p));
    let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
    if escape || (area.response.clicked_elsewhere() && !on_button) {
        state.display_menu_open = false;
    }
}

/// The three groups. Radios and check boxes carry the check mark of the
/// active entry and its AccessKit checked state.
fn entries(ui: &mut egui::Ui, state: &mut State) {
    ui.strong(GROUP_MODE);
    let mut kept = false;
    for mode in DisplayMode::ALL {
        kept |= ui
            .radio_value(&mut state.display, mode, mode.label())
            .on_hover_text(mode.tooltip())
            .changed();
    }
    ui.separator();
    ui.strong(GROUP_LOOK_AT);
    let look = ui
        .selectable_label(state.look_at_armed, LOOK_AT_FACE)
        .on_hover_text("Click a face in the view. The view turns to face it.");
    if look.clicked() {
        crate::look_at::arm(state, !state.look_at_armed);
        // The next click belongs to the view, so the menu moves away.
        state.display_menu_open = !state.look_at_armed;
    }
    let plane = ui
        .selectable_label(state.look_at_plane_armed, LOOK_AT_PLANE)
        .on_hover_text("Click an axis of an origin in the view. The view turns to face the plane of that axis. A click on Z gives the XY work plane.");
    if plane.clicked() {
        crate::look_at_plane::arm(state, !state.look_at_plane_armed);
        state.display_menu_open = !state.look_at_plane_armed;
    }
    ui.separator();
    ui.strong(GROUP_GRAPHICS);
    kept |= ui
        .checkbox(&mut state.show_grid, SHOW_GRID)
        .on_hover_text("Show the ground grid below the model. The origin axes do not change it.")
        .changed();
    ui.checkbox(&mut state.show_world_axes, SHOW_AXES)
        .on_hover_text("Show the axes of the world origin. The ground grid stays as it is.");
    ui.radio_value(&mut state.cam.projection, ProjectionMode::Perspective, PERSPECTIVE)
        .on_hover_text("Far objects look smaller, as the eye sees them.");
    ui.radio_value(&mut state.cam.projection, ProjectionMode::Orthographic, ORTHOGRAPHIC)
        .on_hover_text("Parallel edges stay parallel. Sizes do not change with distance.");
    lines(ui, state);
    ui.separator();
    kept |= quality(ui, state);
    if kept {
        state.persist_view();
    }
}

/// #2958: the overlay line width and the anti-aliasing. Each pick is kept
/// at once by its `State` method (which also refuses an unoffered count).
fn lines(ui: &mut egui::Ui, state: &mut State) {
    ui.label(LINE_WIDTH).on_hover_text("The width of toolpaths, height planes, outlines and axes.");
    ui.horizontal(|ui| {
        for w in LineWidth::ALL {
            if ui.radio(state.line_width == w, line_label(w)).clicked() && state.line_width != w {
                state.select_line_width(w);
            }
        }
    });
    ui.label(ANTIALIAS).on_hover_text("Smooths the edges of models and stock. 4x uses the most graphics memory.");
    ui.horizontal(|ui| {
        for a in Antialias::ALL {
            let why = antialias_refusal(state, a);
            let r = ui.add_enabled(why.is_none(), egui::RadioButton::new(state.antialias == a, a.word()));
            let r = match &why {
                Some(why) => r.on_disabled_hover_text(why),
                None => r,
            };
            if r.clicked() && state.antialias != a {
                state.select_antialias(a);
            }
        }
    });
}

/// The quality rows of the Graphics group (#2834): the two presets (one
/// choice, or none after a switch changed by hand) and the two shading
/// switches. True when a kept value changed.
fn quality(ui: &mut egui::Ui, state: &mut State) -> bool {
    let mut kept = false;
    let active = QualityPreset::active(state.effects());
    for preset in QualityPreset::ALL {
        if ui.radio(active == Some(preset), preset.label()).on_hover_text(preset.tooltip()).clicked() {
            preset.apply(state);
            kept = true;
        }
    }
    // The two wireframe modes draw no lit face and no shadow of one. The
    // switches keep their values for the next shaded mode.
    let shaded = !state.display.is_wireframe();
    const NOT_IN_WIREFRAME: &str = "The wireframe modes do not draw this. Select a shaded mode.";
    kept |= ui
        .add_enabled(shaded, egui::Checkbox::new(&mut state.ground_shadow, GROUND_SHADOW))
        .on_hover_text("Draw a soft shadow of the model on the floor.")
        .on_disabled_hover_text(NOT_IN_WIREFRAME)
        .changed();
    kept |= ui
        .add_enabled(shaded, egui::Checkbox::new(&mut state.ambient_occlusion, AMBIENT_OCCLUSION))
        .on_hover_text("Make the faces of the model darker in their corners.")
        .on_disabled_hover_text(NOT_IN_WIREFRAME)
        .changed();
    kept
}
