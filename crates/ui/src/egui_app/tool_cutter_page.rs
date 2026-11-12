//! The tool editor's drawn cutter (#93 slice 2): the Profile tab's Draw
//! cutter view - a milling tool's own cutter, neck and shank as a spun
//! half-outline, Parametric (drawn from its numbers) or Custom (a table
//! of points with optional arcs that must keep agreeing with them), the
//! shared flat side view that follows every keystroke, the live refusal
//! by name, and Apply cutter, which queues the one command. #93
//! slice 3: the kind's parameters sit above the points
//! (`tool_cutter_params`) and drive the outline.
//! Provenance: our own pane over `app::cutter_profile` and
//! `app::tool_cutter`; the side view is the handbook's tool drawing. No
//! vendor material consulted.

use egui::Ui;

use camcraft_app::cutter_profile;
use camcraft_app::spun::SpunProfile;
use camcraft_app::units::{self, Unit};

use super::tool_profile_page::{BAD, points_grid, side_view};
use crate::state::State;

pub(super) fn cutter_section(ui: &mut Ui, state: &mut State, exists: bool, u: Unit) {
    ui.strong("Cutter profile");
    if !exists {
        ui.label("Add the tool first, then draw its cutter.");
        return;
    }
    state.sync_tool_cutter_ui();
    let landed = state.editor_tool_cutter();
    ui.label(if landed.is_some() { "landed: a drawn cutter" } else { "landed: the cutter its numbers draw" });
    let custom = state.tool_cutter_ui.custom;
    ui.horizontal(|ui| {
        if ui
            .selectable_label(!custom, "Parametric")
            .on_hover_text("the cutter, neck and shank its numbers draw, redrawn as they change")
            .clicked()
        {
            state.set_cutter_custom(false);
        }
        if ui
            .selectable_label(custom, "Custom")
            .on_hover_text("edit the outline point by point; it must keep the stated diameter, flute and shank lengths")
            .clicked()
        {
            state.set_cutter_custom(true);
        }
    });
    // #93 slice 3: the kind's parameters, editing the draft numbers -
    // re-synced at once so outline and numbers agree this very frame.
    super::tool_cutter_params::params_block(ui, state, u);
    state.sync_tool_cutter_ui();
    let custom = state.tool_cutter_ui.custom;
    let (insert, remove) = points_grid(ui, u, &mut state.tool_cutter_ui.points, custom, "tool_cutter_points");
    if let Some(i) = insert {
        state.insert_cutter_point(i);
    }
    if let Some(i) = remove {
        state.remove_cutter_point(i);
    }
    let tool = state.tool_ui.to_tool();
    let points = state.tool_cutter_ui.points.clone();
    let check = cutter_profile::check(&tool, points.clone());
    let cutter: Vec<(f64, f64)> = check.as_ref().map_or_else(|_| points.iter().map(|p| (p.z, p.r)).collect(), SpunProfile::outline);
    let holder = state
        .editor_tool_body()
        .and_then(|b| b.spun)
        .or_else(|| SpunProfile::holder_of(&tool))
        .map(|h| h.outline())
        .unwrap_or_default();
    side_view(ui, &tool, (&cutter, check.is_ok()), (&holder, true), u);
    match &check {
        Ok(s) => {
            ui.label(format!(
                "cutter Ø {} {}, flutes {} {}, overall {} {}",
                units::len(tool.diameter, u, 2),
                u.suffix(),
                units::len(tool.flute_len, u, 1),
                u.suffix(),
                units::len(s.points().last().map_or(0.0, |p| p.z), u, 1),
                u.suffix()
            ));
        }
        Err(e) => {
            ui.colored_label(BAD, e.to_string());
        }
    }
    // #93 slice 3: edited numbers make an Apply worth landing too.
    let moved = state.editor_tool_changed();
    let can = custom || landed.is_some() || moved;
    if ui
        .add_enabled(can, egui::Button::new("Apply cutter"))
        .on_hover_text(if custom {
            "land the drawn cutter, and any edited numbers with it, on the tool (one undo step); toolpaths and checks keep reading the numbers"
        } else {
            "drop the drawn cutter: the outline its numbers draw again (one undo step)"
        })
        .clicked()
    {
        state.queue_apply_tool_cutter();
    }
}
