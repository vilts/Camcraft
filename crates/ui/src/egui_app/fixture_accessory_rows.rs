//! A built-in accessory's figures on its Fixtures-page row (#789): the
//! row's own size draft (one per accessory, so two rows never reset each
//! other) and "Apply sizes", which queues one `SetAccessory`.
//! Provenance: our own layout over our own accessory commands.

use egui::Ui;

use camcraft_app::{fixture, model};

use crate::state::{CommandKind, State};

/// Draw accessory `name`'s figures, if it is a built-in accessory.
pub(super) fn sizes(ui: &mut Ui, state: &mut State, name: &str) {
    let Some(live) = model::find_by_name(&state.world, name)
        .and_then(|e| fixture::def_of(&state.world, &e))
        .and_then(|d| d.accessory)
    else {
        return;
    };
    let u = state.units();
    ui.label(format!("Built-in {}", live.label().to_lowercase()));
    let id = format!("accessory-sizes-{name}");
    super::fixture_accessory_dialog::dims_grid(ui, u, &id, state.accessory_size_draft(name, &live), None);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        if ui.small_button("Apply sizes").clicked() {
            let params = state.accessory_size_draft(name, &live).clone();
            state.pending.push_back(CommandKind::SetAccessory { name: name.to_owned(), params });
        }
    });
}
