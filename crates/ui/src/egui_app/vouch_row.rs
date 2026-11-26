//! The model page's vouch row (#1339): beside the "Fidelity unknown" line
//! of a model from a project saved before v59, a person types their name
//! and vouches for the mesh, so its operations can post again without a
//! re-import. The button queues `VouchModelFidelity`; a vouched model
//! shows who vouched instead. An incomplete model (#1610) and a recorded
//! reduced fidelity (#864) get no row: neither can be vouched away.
//! Provenance: our own layout over our own `app::model_vouch`; no vendor
//! material.

use egui::{TextEdit, Ui};
use flecs_ecs::prelude::Entity;

use camcraft_app::{mesh_open, model_fidelity, model_vouch};

use crate::state::{CommandKind, State};

/// What the button says.
pub(crate) const BUTTON: &str = "Vouch for this mesh";
/// What the name field is called.
pub(crate) const FIELD: &str = "Vouched by";

pub(super) fn row(ui: &mut Ui, state: &mut State, entity: &Entity, name: &str) {
    if let Some(by) = model_vouch::by_of(&state.world, entity) {
        ui.label(model_vouch::summary(&by));
        return;
    }
    let unknown = model_fidelity::chord_of(&state.world, entity).is_some_and(model_fidelity::is_unknown);
    if !unknown || mesh_open::hole_of(&state.world, entity).is_some() {
        return;
    }
    ui.label(
        egui::RichText::new(
            "If this mesh is exact (an STL or OBJ import), or you checked it yourself, you can \
             vouch for it in place of a re-import. Gouge checks then read the mesh as exact. \
             Your name is kept with the model; undo puts the mark back.",
        )
        .small()
        .weak(),
    );
    // The button has a row of its own: the Properties pane is narrow.
    ui.horizontal(|ui| {
        let l = ui.label(FIELD);
        ui.add(TextEdit::singleline(&mut state.vouch_by).desired_width(140.0)).labelled_by(l.id);
    });
    let named = model_vouch::checked_by(&state.vouch_by).is_ok();
    if ui.add_enabled(named, egui::Button::new(BUTTON)).clicked() {
        state.pending.push_back(CommandKind::VouchModelFidelity { name: name.to_string(), by: state.vouch_by.clone() });
    }
}
