//! The tool editor's Body section (#371): Import body… / Clear body, the
//! placement (axis preset or picked face normal, tip offset), Pick tip /
//! Pick axis on a small orbitable 3D preview of the mesh in its own
//! frame, and Apply body - which queues the one `SetToolBody`. The
//! preview is painted with egui (flat-shaded, depth-sorted triangles),
//! so it needs no GPU and runs under the test harness. Picks and orbit
//! are ephemeral; only Apply body / Clear body reach the history.
//! #3894: the view itself is `tool_model_view`, shared with the tool
//! library window's 3D preview.
//! Provenance: our own pane over `app::tool_body_view` (orthographic
//! projection and ray casting from general graphics practice); no vendor
//! material consulted.

use egui::{Color32, ComboBox, Sense, Shape, Stroke, Ui, Vec2};

use camcraft_app::tool_body_view::{self, AXIS_CHOICES};
use camcraft_app::tool_body;
use camcraft_app::units::{self, Unit};

use super::fields::num_row;
use super::layout::prop_row_hint;
use super::tool_model_view::{self as model_view, BG, View};
use crate::state::{BodyPickMode, State};

/// The preview's side, logical px.
const SIDE: Vec2 = Vec2::new(260.0, 200.0);

/// The envelope bands of the draft's placed body, for the profile
/// preview: `(above, len, r)` per band, or `None` with no usable draft.
/// Re-mirrors the world's body first (a no-op unless it changed), so
/// the preview follows Apply profile, undo and reload whichever tab
/// shows - not only once the Body tab has been drawn. An unchanged
/// machine-default body includes the conservative parameter-cylinder floor.
pub(super) fn envelope(state: &mut State) -> Option<Vec<(f64, f64, f64)>> {
    state.sync_tool_body_ui();
    let tool = state.tool_ui.to_tool();
    let d = &state.tool_body_ui;
    if let Some(b) = state.editor_tool_body()
        && b.machine_default
        && std::sync::Arc::ptr_eq(&b.facets, &d.facets)
        && d.axis == [0.0, 0.0, 1.0]
        && d.offset == 0.0
        && d.lathe.is_none()
    {
        // Use the stored profile, as judging does; a mesh rebuild loses its exact bands.
        return tool_body::checked_bands(&tool, &b);
    }
    match state.tool_body_preview()? {
        Ok(b) => tool_body::checked_bands(&tool, b),
        Err(_) => None,
    }
}

/// The section, the editor's third column beside the profile preview. A new (unsaved) tool
/// takes no body until it exists - the command names an existing tool.
pub(super) fn body_section(ui: &mut Ui, state: &mut State, exists: bool, u: Unit) {
    ui.strong("Tool body");
    if !exists {
        ui.label("Add the tool first, then import its holder or assembly.");
        return;
    }
    state.sync_tool_body_ui();
    let landed = state.editor_tool_body().map(|b| b.source);
    let draft_src = state.tool_body_ui.source.clone();
    let has_draft = !state.tool_body_ui.facets.is_empty();
    ui.label(match (&landed, has_draft) {
        (Some(src), _) if *src == draft_src => format!("In use: {src}"),
        (_, true) => format!("Ready to apply: {draft_src} (not applied yet)"),
        _ if state.tool_body_ui.lathe.is_some() => "In use: no body (the holder is not checked for collisions)".to_owned(),
        _ => "In use: the holder cylinder from the dimensions".to_owned(),
    });
    // #361: a boring bar chooses its frame; a lathe steel's is the
    // turning tool frame.
    super::tool_body_lathe_page::frame_row(ui, state);
    let (import, clear) = ui
        .horizontal(|ui| {
            let import = crate::browser_files::button(
                ui,
                state.in_browser(crate::browser_files::NEEDS_PICKER),
                "Import body…",
                "read a STEP, STL or OBJ holder/assembly; place it, then Apply body",
            );
            let clear = ui
                .add_enabled(landed.is_some() || has_draft, egui::Button::new("Clear body"))
                .on_hover_text("remove the body and check the holder cylinder from the dimensions again (undoable)")
                .clicked();
            (import, clear)
        })
        .inner;
    if import {
        state.import_tool_body_dialog();
    }
    if clear {
        state.clear_tool_body();
    }
    if state.tool_body_ui.facets.is_empty() {
        return;
    }
    if state.tool_body_ui.lathe.is_some() {
        super::tool_body_lathe_page::section(ui, state, u);
        super::tool_body_assembly_page::section(ui, state);
        return;
    }
    let old_axis = state.tool_body_ui.axis;
    let old_offset = state.tool_body_ui.offset;
    prop_row_hint(ui, "Tool axis", "model direction from the tip toward the spindle", |ui| {
        axis_combo(ui, &mut state.tool_body_ui.axis);
    });
    prop_row_hint(ui, "Tip offset", "height of the model origin above the tool tip", |ui| {
        num_row(ui, u, &mut state.tool_body_ui.offset);
    });
    if state.tool_body_ui.axis != old_axis || (state.tool_body_ui.offset - old_offset).abs() > 1e-9 {
        state.tool_body_ui.align_note = None;
    }
    if let Some(ref note) = state.tool_body_ui.align_note {
        ui.label(note);
    }
    let pick = state.tool_body_ui.pick;
    ui.horizontal(|ui| {
        if super::pick_button::pick_button(ui, pick == BodyPickMode::Tip, "Pick tip")
            .on_hover_text("click the body in the preview: the nearest corner becomes the tool tip")
            .clicked()
        {
            state.tool_body_ui.pick = if pick == BodyPickMode::Tip { BodyPickMode::Off } else { BodyPickMode::Tip };
        }
        if super::pick_button::pick_button(ui, pick == BodyPickMode::Axis, "Pick axis")
            .on_hover_text("click a face that looks toward the spindle: its normal becomes the tool axis")
            .clicked()
        {
            state.tool_body_ui.pick = if pick == BodyPickMode::Axis { BodyPickMode::Off } else { BodyPickMode::Axis };
        }
        if ui.button("Fit").on_hover_text("reset the preview's orbit and zoom (or double-click it)").clicked() {
            state.fit_body_preview();
        }
    });
    placement_view(ui, state);
    let note = match state.tool_body_preview() {
        Some(Ok(b)) => match b.profile() {
            Some(p) => format!(
                "Placed from {} to {} {} above the tip",
                units::len(p.bottom(), u, 1),
                units::len(p.top(), u, 1),
                u.suffix()
            ),
            None => String::new(),
        },
        Some(Err(e)) => format!("Placement error: {e}"),
        None => String::new(),
    };
    if !note.is_empty() {
        ui.label(note);
    }
    if ui.button("Apply body").on_hover_text("attach the body with this placement (one undo step)").clicked() {
        state.queue_apply_tool_body();
    }
    if ui
        .button("Convert to profile")
        .on_hover_text("convert this aligned body into an editable conservative half-profile (non-revolved features are enveloped, never cut away)")
        .clicked()
    {
        state.convert_body_draft_to_profile();
    }
    ui.small("Non-revolved features are enveloped conservatively (never cut away).");
    super::tool_body_assembly_page::section(ui, state);
}

fn axis_combo(ui: &mut Ui, axis: &mut [f64; 3]) {
    let text = tool_body_view::axis_label(*axis)
        .map_or_else(|| format!("picked ({:.2}, {:.2}, {:.2})", axis[0], axis[1], axis[2]), str::to_owned);
    ComboBox::from_id_salt("tool_body_axis").selected_text(text).show_ui(ui, |ui| {
        for (name, d) in AXIS_CHOICES {
            if ui.selectable_label(tool_body_view::axis_label(*axis) == Some(name), name).clicked() {
                *axis = d;
            }
        }
    });
}

/// #2017: the tool page's one preview is the side view beside the fields.
/// The 3D view is a placement aid only: it shows while a Pick button is
/// armed (a click on the mesh is the only way to pick), and not otherwise.
pub(super) fn placement_view(ui: &mut Ui, state: &mut State) {
    if state.tool_body_ui.pick != BodyPickMode::Off {
        preview_3d(ui, state);
    }
}

fn preview_3d(ui: &mut Ui, state: &mut State) {
    let (resp, painter) = ui.allocate_painter(SIDE, Sense::click_and_drag());
    let rect = resp.rect;
    painter.rect_filled(rect, 4.0, BG);
    let d = &state.tool_body_ui;
    let Some(view) = View::new(&d.facets, rect, &d.orbit()) else {
        return;
    };
    painter.add(Shape::mesh(model_view::project(&view, &d.facets)));
    // The placement over it: the axis from the tip up (a lathe draft:
    // the tip with its radial and spindle directions), the tip, and
    // the model origin.
    if d.lathe.is_some() {
        super::tool_body_lathe_page::overlay(&painter, d, &|q| view.screen(q), view.reach);
    } else if let Some(a) = model_view::norm(d.axis) {
        let tip = [-d.offset * a[0], -d.offset * a[1], -d.offset * a[2]];
        let top = [0, 1, 2].map(|i| tip[i] + a[i] * view.reach * 2.0);
        painter.line_segment([view.screen(tip), view.screen(top)], Stroke::new(1.5, Color32::from_rgb(90, 200, 230)));
        painter.circle_filled(view.screen(tip), 4.0, Color32::from_rgb(240, 150, 40));
    }
    painter.circle_stroke(view.screen([0.0; 3]), 3.0, Stroke::new(1.0, Color32::from_rgb(160, 160, 170)));
    let mode = d.pick;
    if mode != BodyPickMode::Off {
        painter.text(
            rect.min + Vec2::new(6.0, 6.0),
            egui::Align2::LEFT_TOP,
            super::tool_body_lathe_page::pick_hint(mode),
            egui::FontId::proportional(10.0),
            Color32::from_rgb(240, 200, 120),
        );
    }
    // Zoom about the cursor, fit on a double-click, orbit on a drag.
    // View state only.
    let g = model_view::gesture(ui, &resp, &view);
    if let Some((by, about)) = g.zoom {
        state.zoom_body_preview(by, about);
    }
    if g.fit {
        state.fit_body_preview();
    }
    if let Some(dv) = g.turn {
        let mut o = state.tool_body_ui.orbit();
        o.turn(dv.x, dv.y);
        (state.tool_body_ui.yaw, state.tool_body_ui.pitch) = (o.yaw, o.pitch);
    }
    if resp.clicked()
        && mode != BodyPickMode::Off
        && let Some(at) = resp.interact_pointer_pos()
    {
        let (o, dir) = view.ray(at);
        match tool_body_view::pick(&state.tool_body_ui.facets, o, dir) {
            Some(hit) => state.apply_body_pick(hit),
            None => state.status = "pick: no facet under the cursor".into(),
        }
    }
}

#[cfg(test)]
#[path = "tests/tool_body_floor.rs"]
mod tests;
