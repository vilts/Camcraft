//! The Panel setups window (#2339): save the dock as it stands under a
//! name, put a saved arrangement back, delete one, and choose whether the
//! last setup returns at the next start. An in-app `egui::Window` over the
//! dock, the shell's one kind of extra surface; closed by default
//! (`state.panel_setups_open` is false on a fresh State), so no panel
//! below it moves until the View door opens it.
//!
//! The dock belongs to the shell, so this window never reads or swaps it:
//! Save, Load and Delete file one `PanelSetupAsk`, and `layout::setups`
//! answers it on the same frame. Only the name field, the auto-load
//! toggle and "Reset to default" act here, and the reset rides the View
//! door's own `LayoutRequest`.
//! Provenance: our own layout, specced from the egui docs; no HSMWorks
//! material involved.

use egui::{Button, Context, Id, RichText, TextEdit, Window};

use crate::state::{LayoutRequest, PanelSetupAsk, State};

pub(crate) fn window(ctx: &Context, state: &mut State) {
    if !state.panel_setups_open {
        return;
    }
    let mut open = true;
    Window::new("Panel setups")
        .id(Id::new("panel-setups"))
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .show(ctx, |ui| body(ui, state));
    state.panel_setups_open = open;
}

fn body(ui: &mut egui::Ui, state: &mut State) {
    ui.set_max_width(400.0);
    ui.label("Save the open panels as they stand, or put a saved setup back.");
    ui.add_space(6.0);
    let named = !state.setup_name_draft.trim().is_empty();
    ui.horizontal(|ui| {
        ui.label("Name");
        ui.add(
            TextEdit::singleline(&mut state.setup_name_draft)
                .hint_text("My setup")
                .desired_width(180.0),
        );
        let hint = if named {
            "Save the open panels with their sides, sizes and tab order"
        } else {
            "Type a name for the setup first"
        };
        if ui
            .add_enabled(named, Button::new("Save current setup"))
            .on_hover_text(hint)
            .clicked()
        {
            let name = state.setup_name_draft.trim().to_owned();
            state.panel_setup_ask = Some(PanelSetupAsk::Save(name));
        }
    });
    ui.add_space(6.0);
    ui.separator();
    ui.label("Saved setups");
    if state.panel_setups.setups.is_empty() {
        ui.label(RichText::new("No saved setups yet.").weak());
    }
    let saved: Vec<String> = state
        .panel_setups
        .setups
        .iter()
        .map(|s| s.name.clone())
        .collect();
    for name in saved {
        let in_use = state.panel_setups.last.as_deref() == Some(name.as_str());
        ui.horizontal(|ui| {
            let label = if in_use {
                format!("{name} (in use)")
            } else {
                name.clone()
            };
            ui.label(label);
            if ui.small_button("Load").clicked() {
                state.panel_setup_ask = Some(PanelSetupAsk::Load(name.clone()));
            }
            if ui.small_button("Delete").clicked() {
                state.panel_setup_ask = Some(PanelSetupAsk::Delete(name.clone()));
            }
        });
    }
    ui.separator();
    let mut auto = state.panel_setups.auto_load;
    if ui
        .checkbox(&mut auto, "Load the last setup when CamCraft starts")
        .changed()
    {
        state.set_setup_auto_load(auto);
    }
    if ui.button("Reset to default").clicked() {
        state.layout_request = Some(LayoutRequest::Reset);
    }
}
