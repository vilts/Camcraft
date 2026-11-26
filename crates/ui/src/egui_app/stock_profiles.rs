//! The Stock page's "DXF profiles" group (#1863): the "Add DXF profile…"
//! door, and one row per profile placed on the active setup's stock with
//! its placement (X, Y on the stock top and a turn about its normal),
//! Hide / Show and Delete. A placement edit is a draft until "Place
//! profile" queues one `MoveSketch`; Delete queues one `DeleteEntity`.
//! Hide / Show is view state, like the browser eye.
//!
//! Provenance: our own page over this repo's #1862 commands, from the
//! maintainer's words on issue 1863; no vendor material consulted.

use egui::{RichText, Ui};

use camcraft_app::{model, setup, sketch};

use crate::state::{CommandKind, State};

use super::fields::{num, num_plain};
use super::layout::prop_grid;

/// The door's label (also what the structural tests read).
pub(crate) const ADD_LABEL: &str = "Add DXF profile…";

pub(super) fn profiles_group(ui: &mut Ui, state: &mut State) {
    let active = setup::active(&state.world);
    let has_face = sketch::face_frame(&state.world, active.as_deref()).is_some();
    let add = ui
        .add_enabled(has_face, egui::Button::new(ADD_LABEL))
        .on_hover_text("Place a DXF drawing on the top of this stock, as a profile for 2D, plasma and wire EDM operations")
        .on_disabled_hover_text("make a stock first - a DXF profile is placed on the stock top");
    if add.clicked() {
        state.pending.push_back(CommandKind::AddDxfProfileDialog);
    }
    let names = sketch::placed_in(&state.world, active.as_deref());
    if names.is_empty() {
        ui.label(RichText::new("No DXF profiles on this stock.").weak());
        return;
    }
    for name in names {
        profile_row(ui, state, &name);
    }
}

fn profile_row(ui: &mut Ui, state: &mut State, name: &str) {
    let u = state.units();
    let placed = sketch::find(&state.world, name)
        .and_then(|(e, _)| model::transform_of(&state.world, &e))
        .and_then(sketch::face_placement)
        .map_or([0.0; 3], |(x, y, a)| [x, y, a]);
    let mut draft = state.sketch_ui.drafts.get(name).copied().unwrap_or(placed);
    ui.separator();
    ui.label(RichText::new(name).strong());
    prop_grid(ui, &format!("stock_profile_grid_{name}"), |ui| {
        ui.label("Profile X");
        num(ui, u, &mut draft[0]);
        ui.end_row();
        ui.label("Profile Y");
        num(ui, u, &mut draft[1]);
        ui.end_row();
        ui.label("Profile angle (deg)");
        num_plain(ui, &mut draft[2]);
        ui.end_row();
    });
    if draft == placed {
        state.sketch_ui.drafts.remove(name);
    } else {
        state.sketch_ui.drafts.insert(name.to_owned(), draft);
    }
    let hidden = state.hidden_models.iter().any(|n| n == name);
    ui.horizontal(|ui| {
        if ui.add_enabled(draft != placed, egui::Button::new("Place profile")).clicked() {
            let [x, y, angle_deg] = draft;
            state.pending.push_back(CommandKind::MoveSketch { name: name.to_owned(), x, y, angle_deg });
        }
        if ui.button(if hidden { "Show profile" } else { "Hide profile" }).clicked() {
            State::toggle_visibility(&mut state.hidden_models, name);
        }
        if ui.button("Delete profile").clicked() {
            state.pending.push_back(CommandKind::DeleteByName(name.to_owned()));
        }
    });
}
