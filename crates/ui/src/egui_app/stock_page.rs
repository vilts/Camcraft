//! The Stock page (the active setup's stock), split out of `pages.rs` and
//! laid out like the WCS page (#1440, after #739): named groups (Shape,
//! Size, Position, Fit to models) over the shared `layout` form helpers,
//! label-left rows in one grid column, and the three actions as equal
//! buttons under a divider. The per-side margins are shown only when
//! "Per side" is ticked, and the source rows only for stock from a model.
//! #2015: a Material group states the stock's workpiece material.
//! Every change is a command: `SetStock` (Apply), `SetStockMaterial`, `StockFit` /
//! `StockFitSides` (Fit to Models) and `DeleteByName` - the page only
//! drafts and queues.
//!
//! Provenance: behaviour specced from published CAM stock-setup practice;
//! the layout is our own over the `layout` form helpers; no vendor material.

use egui::{RichText, Ui, Vec2};

use camcraft_app::model;

use crate::state::{CommandKind, ErrorScope, State};

use super::fields::{combo_owned, num};
use super::layout::{group, prop_grid};

/// A button row's height (the WCS page's).
const BUTTON_H: f32 = 24.0;

pub(super) fn stock_page(ui: &mut Ui, state: &mut State) {
    let u = state.units();
    group(ui, "Shape", None, |ui| shape_section(ui, state));
    group(ui, "Material", None, |ui| super::stock_material_row::stock_material_row(ui, state));
    group(ui, "Size", None, |ui| size_fields(ui, state, u));
    group(ui, "Position and rotation", None, |ui| position_fields(ui, state, u));
    if state.stock_ui.shape != 2 {
        group(ui, "Fit to models", None, |ui| fit_fields(ui, state, u));
    }
    actions(ui, state);
    // #1863: DXF profiles placed on this stock's top, under the stock's
    // own actions (those act on the stock, not on its profiles).
    group(ui, "DXF profiles", None, |ui| super::stock_profiles::profiles_group(ui, state));
}

/// Units, the shape choice and the models' size it is sized against.
fn shape_section(ui: &mut Ui, state: &mut State) {
    // The document's unit (display and posted program, #902) - the
    // numbers below re-render from the world next pass.
    super::post_options::units_row(ui, state, "Units");
    let u = state.units();
    let before = state.stock_ui.shape;
    ui.horizontal_wrapped(|ui| {
        for (i, name) in ["Box", "Cylinder", "From model"].iter().enumerate() {
            ui.radio_value(&mut state.stock_ui.shape, i as i32, *name);
        }
    });
    if state.stock_ui.shape != before && state.error_for(ErrorScope::Setup).is_some() {
        // A refusal describes the previous draft, including its status echo.
        if let Some((_, msg)) = state.feedback.take()
            && state.status == msg
        {
            state.status = String::from("ready");
        }
        state.feedback_ctx = None;
        state.feedback_at = None;
        state.feedback_ghost = None;
    }
    // #197: the models' size, right above the numbers it informs.
    match super::stock_info::part_size_lines(&state.world, u) {
        Some([size, extents]) => {
            ui.label(size).on_hover_text("The models' bounding box - what Fit to Models wraps, plus the fit margin");
            ui.label(RichText::new(extents).weak());
        }
        None => {
            ui.label(RichText::new("No models yet - load one to see its size.").weak());
        }
    }
}

/// The size fields of the chosen shape, or the source picker.
fn size_fields(ui: &mut Ui, state: &mut State, u: camcraft_app::units::Unit) {
    match state.stock_ui.shape {
        1 => prop_grid(ui, "stock_size_grid", |ui| {
            ui.label("Diameter");
            num(ui, u, &mut state.stock_ui.diameter);
            ui.end_row();
            ui.label("Height");
            num(ui, u, &mut state.stock_ui.height);
            ui.end_row();
        }),
        2 => source_fields(ui, state),
        _ => prop_grid(ui, "stock_size_grid", |ui| {
            ui.label("Length (X)");
            num(ui, u, &mut state.stock_ui.dx);
            ui.end_row();
            ui.label("Width (Y)");
            num(ui, u, &mut state.stock_ui.dy);
            ui.end_row();
            ui.label("Height (Z)");
            num(ui, u, &mut state.stock_ui.dz);
            ui.end_row();
        }),
    }
}

/// Stock from a model: parts only, as the op Source picker.
fn source_fields(ui: &mut Ui, state: &mut State) {
    let models: Vec<String> = model::model_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| !model::is_fixture(&state.world, e))
        .map(|(n, _)| n)
        .collect();
    if models.is_empty() {
        ui.label("No models to take stock from.");
        // The raw-stock row (and its hint) shows with no models too.
        super::raw_stock_row::raw_stock_row(ui, state);
        return;
    }
    prop_grid(ui, "stock_source_grid", |ui| {
        ui.label("Source");
        let mut idx = models.iter().position(|m| *m == state.stock_ui.source).unwrap_or(models.len() - 1);
        combo_owned(ui, "stk_src", &mut idx, &models);
        // #3261: the draft holds the part the picker shows, also when the
        // user accepts the default (one model: the index never changes).
        if let Some(shown) = models.get(idx).or(models.last())
            && state.stock_ui.source != *shown
        {
            state.stock_ui.source = shown.clone();
        }
        ui.end_row();
    });
    super::raw_stock_row::raw_stock_row(ui, state);
}

fn position_fields(ui: &mut Ui, state: &mut State, u: camcraft_app::units::Unit) {
    prop_grid(ui, "stock_position_grid", |ui| {
        let s = &mut state.stock_ui;
        for (label, v) in [("X center", &mut s.x), ("Y center", &mut s.y), ("Z bottom", &mut s.z)] {
            ui.label(label);
            num(ui, u, v);
            ui.end_row();
        }
    });
    super::stock_axis_rows::axis_rows(ui, &mut state.stock_ui);
}

/// The fit allowance: one margin, or (a box only) one per side.
fn fit_fields(ui: &mut Ui, state: &mut State, u: camcraft_app::units::Unit) {
    // #198: a box can be fitted by a separate allowance on each side.
    if state.stock_ui.shape == 0 {
        ui.checkbox(&mut state.stock_ui.fit_per_side, "Per side")
            .on_hover_text("Fit to Models pads each face of the box by its own allowance");
    }
    let per_side = per_side(state);
    prop_grid(ui, "stock_fit_grid", |ui| {
        if per_side {
            for (i, name) in ["-X", "+X", "-Y", "+Y", "Bottom", "Top"].iter().enumerate() {
                ui.label(format!("Fit margin {name}"));
                num(ui, u, &mut state.stock_ui.fit_sides[i]);
                ui.end_row();
            }
        } else {
            ui.label("Fit margin");
            num(ui, u, &mut state.stock_ui.fit_margin);
            ui.end_row();
        }
    });
}

fn per_side(state: &State) -> bool {
    state.stock_ui.shape == 0 && state.stock_ui.fit_per_side
}

/// Apply / Fit to Models / Delete as equal buttons filling the pane.
fn actions(ui: &mut Ui, state: &mut State) {
    // The stock pane edits the ACTIVE setup's stock entity (M3.2 A):
    // with no setups in the document this resolves to the old global
    // "Stock", so the legacy behavior is the degenerate case, not a
    // branch. sync_setup_ui already mirrored this entity's numbers.
    let active = camcraft_app::setup::active(&state.world);
    let stock_name = camcraft_app::setup::stock_entity_name(&state.world, active.as_deref());
    let has_stock =
        model::find_by_name(&state.world, &stock_name).is_some_and(|e| model::is_stock(&state.world, &e));
    if let Some(s) = active.as_deref() {
        ui.label(RichText::new(format!("Stock of setup {s}.")).small().weak());
    }
    let labels: &[&str] = match (state.stock_ui.shape == 2, has_stock) {
        (true, true) => &["Apply", "Delete"],
        (true, false) => &["Apply"],
        (false, true) => &["Apply", "Fit to Models", "Delete"],
        (false, false) => &["Apply", "Fit to Models"],
    };
    let avail = ui.available_width();
    let gaps = ui.spacing().item_spacing.x * labels.len().saturating_sub(1) as f32;
    let size = Vec2::new(((avail - gaps) / labels.len() as f32).max(24.0), BUTTON_H);
    match super::wcs_page::even_row(ui, labels, size).and_then(|i| labels.get(i)).copied() {
        Some("Apply") => apply(state),
        Some("Fit to Models") => fit(state),
        Some("Delete") => state.pending.push_back(CommandKind::DeleteByName(stock_name)),
        _ => {}
    }
}

fn apply(state: &mut State) {
    let s = &state.stock_ui;
    // #2576: the whole turn from the axis choice, never only a yaw.
    let Some(rot) = s.draft_rot() else {
        state.refuse(ErrorScope::Setup, String::from("stock not set: a turn angle is not a number"));
        return;
    };
    let cmd = CommandKind::SetStockPose { shape: s.draft_shape(), pos: [s.x, s.y, s.z], rot };
    state.pending.push_back(cmd);
}

fn fit(state: &mut State) {
    let s = &state.stock_ui;
    let cmd = if per_side(state) {
        CommandKind::StockFitSides { sides: s.fit_sides }
    } else {
        CommandKind::StockFit { margin: s.fit_margin, shape: s.shape }
    };
    state.pending.push_back(cmd);
}
