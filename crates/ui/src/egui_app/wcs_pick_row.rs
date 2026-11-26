//! WCS feature rows keep one ephemeral pick per role and apply one SetWcs command.
//! Provenance: our own UI over the app alignment command; no vendor material consulted.

use super::layout;
use crate::state::{CommandKind, State};
use camcraft_app::pick::{Role, align_roles};
use camcraft_app::setup;
use egui::Ui;

pub(super) const GROUP: &str = "Set WCS from features";
const ROLES: [(Role, &str, &str, &str); 4] = [
    (Role::Origin, "Origin", "Pick origin", "Clear origin"),
    (Role::X, "X axis", "Pick X axis", "Clear X axis"),
    (Role::Y, "Y axis", "Pick Y axis", "Clear Y axis"),
    (Role::Z, "Z axis", "Pick Z axis", "Clear Z axis"),
];

pub(super) fn row(ui: &mut Ui, state: &mut State, setup: &str) {
    if !state.setup_pick_rows.iter().any(|s| s == setup) {
        state.setup_pick_rows.push(setup.to_owned());
    }
    layout::prop_grid(ui, &format!("wcs_features_{setup}"), |ui| {
        for (role, label, pick_label, clear_label) in ROLES {
            layout::list_row(ui, label);
            ui.horizontal_wrapped(|ui| {
                let draft = state.wcs_pick.as_ref().filter(|d| d.setup == setup);
                let armed = draft.is_some_and(|d| d.picking && d.role == role);
                let picked = draft.and_then(|d| d.picks.iter().find(|(_, r)| *r == role));
                let name = picked.map(|(g, _)| camcraft_app::pick::geo_label(&state.world, g));
                if super::pick_button::pick_button(ui, armed, pick_label)
                    .on_hover_text(
                        "Pick a face, edge, corner or wall in the viewport for this row.",
                    )
                    .clicked()
                {
                    crate::wcs_pick::arm(state, setup);
                    if let Some(d) = state.wcs_pick.as_mut() {
                        d.role = role;
                    }
                }
                if ui
                    .add_enabled_ui(name.is_some(), |ui| layout::small_button(ui, clear_label))
                    .inner
                    .clicked()
                    && let Some(d) = state.wcs_pick.as_mut().filter(|d| d.setup == setup)
                {
                    d.picks.retain(|(_, r)| *r != role);
                }
                layout::list_row(ui, name.as_deref().unwrap_or("not set"));
            });
            ui.end_row();
        }
    });
    let picks = state
        .wcs_pick
        .as_ref()
        .filter(|d| d.setup == setup)
        .map(|d| d.picks.as_slice())
        .unwrap_or_default();
    let check = align_roles(&state.world, picks);
    let active = setup::active(&state.world).as_deref() == Some(setup);
    let why = if !active {
        format!("Make {setup} the active setup to set its WCS from features")
    } else {
        check.as_ref().err().cloned().unwrap_or_default()
    };
    ui.horizontal(|ui| {
        if ui
            .add_enabled_ui(active && check.is_ok(), |ui| layout::button(ui, "Apply"))
            .inner
            .on_hover_text("Place this setup WCS on the picked frame.")
            .on_disabled_hover_text(&why)
            .clicked()
            && let Ok(a) = &check
        {
            state.pending.push_back(CommandKind::SetWcs {
                pos: a.pos,
                roll_deg: a.roll_deg,
                pitch_deg: a.pitch_deg,
                yaw_deg: a.yaw_deg,
            });
        }
        if layout::button(ui, "Clear")
            .on_hover_text("Empty all four feature rows.")
            .clicked()
            && state.wcs_pick.as_ref().is_some_and(|d| d.setup == setup)
        {
            crate::wcs_pick::disarm(state);
        }
    });
    if !why.is_empty() {
        layout::list_row(ui, &why);
    }
}
