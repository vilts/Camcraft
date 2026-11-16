//! The Stock page (the active setup's stock), split out of `pages.rs` and
//! laid out like the WCS page (#1440, after #739): named groups (Shape,
//! Size, Position, Fit to models) over the shared `layout` form helpers,
//! label-left rows in one grid column, and the three actions as equal
//! buttons under a divider. The per-side margins are shown only when
//! "Per side" is ticked, and the source rows only for stock from a model.
//! Every change is a command: `SetStock` (Apply), `StockFit` /
//! `StockFitSides` (Fit to Models) and `DeleteByName` - the page only
//! drafts and queues.
//!
//! Provenance: behaviour specced from published CAM stock-setup practice;
//! the layout is our own over the `layout` form helpers; no vendor material.

use egui::{RichText, Ui, Vec2};

use camcraft_app::model;
use camcraft_app::stock::StockShape;

use crate::state::{CommandKind, State};

use super::fields::{combo_owned, num, num_plain};
use super::layout::{group, prop_grid};

/// A button row's height (the WCS page's).
const BUTTON_H: f32 = 24.0;

pub(super) fn stock_page(ui: &mut Ui, state: &mut State) {
    let u = state.units();
    group(ui, "Shape", None, |ui| shape_section(ui, state));
    group(ui, "Size", None, |ui| size_fields(ui, state, u));
    group(ui, "Position and rotation", None, |ui| position_fields(ui, state, u));
    group(ui, "Fit to models", None, |ui| fit_fields(ui, state, u));
    actions(ui, state);
}

/// Units, the shape choice and the models' size it is sized against.
fn shape_section(ui: &mut Ui, state: &mut State) {
    // The document's unit (display and posted program, #902) - the
    // numbers below re-render from the world next pass.
    super::post_options::units_row(ui, state, "Units");
    let u = state.units();
    ui.horizontal_wrapped(|ui| {
        for (i, name) in ["Box", "Cylinder", "From model"].iter().enumerate() {
            ui.radio_value(&mut state.stock_ui.shape, i as i32, *name);
        }
    });
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
        let before = idx;
        combo_owned(ui, "stk_src", &mut idx, &models);
        if idx != before {
            state.stock_ui.source = models[idx.min(models.len() - 1)].clone();
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
        ui.label("Rotation (yaw deg)");
        num_plain(ui, &mut s.yaw_deg);
        ui.end_row();
    });
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
    let labels: &[&str] = if has_stock { &["Apply", "Fit to Models", "Delete"] } else { &["Apply", "Fit to Models"] };
    let avail = ui.available_width();
    let gaps = ui.spacing().item_spacing.x * labels.len().saturating_sub(1) as f32;
    let size = Vec2::new(((avail - gaps) / labels.len() as f32).max(24.0), BUTTON_H);
    match super::wcs_page::even_row(ui, labels, size) {
        Some(0) => apply(state),
        Some(1) => fit(state),
        Some(_) => state.pending.push_back(CommandKind::DeleteByName(stock_name)),
        None => {}
    }
}

fn apply(state: &mut State) {
    let s = &state.stock_ui;
    let shape = match s.shape {
        1 => StockShape::Cylinder { diameter: s.diameter, height: s.height },
        2 => StockShape::FromModel { source: s.source.clone() },
        _ => StockShape::Box { dx: s.dx, dy: s.dy, dz: s.dz },
    };
    let cmd = CommandKind::SetStock { shape, pos: [s.x, s.y, s.z], yaw_deg: s.yaw_deg };
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
