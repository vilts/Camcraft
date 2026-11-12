//! The Setups page's "Set WCS from features" row (#701): arming it
//! makes every viewport feature pick land in the role draft
//! (`crate::wcs_pick`) instead of in the open operation; the role box
//! says what the next pick sets - the origin or an axis. Picked rows
//! show the app's own live check (`pick::align_roles`): an incomplete
//! set greys Apply out and the app's words ride the row, so the why
//! here is the same words the command itself would give. Apply queues
//! the one undoable `SetWcs` - the same command the WCS pane's anchor
//! grid already sends - and only answers on the ACTIVE setup's row:
//! `SetWcsOrigin` places the active setup's WCS, so a greyed Apply on
//! any other setup's row says to make it active first (review round 3).
//!
//! Provenance: our own page law; no vendor material consulted.

use egui::{ComboBox, RichText, Ui};

use camcraft_app::pick::{Role, align_roles};
use camcraft_app::setup;

use crate::state::{CommandKind, State};
use crate::wcs_pick::role_label;

const ROLES: [Role; 4] = [Role::Origin, Role::X, Role::Y, Role::Z];

/// One setup row's control.
pub(super) fn row(ui: &mut Ui, state: &mut State, setup: &str) {
    if !state.setup_pick_rows.iter().any(|s| s == setup) {
        state.setup_pick_rows.push(setup.to_owned()); // #664
    }
    let mine = state.wcs_pick.as_ref().is_some_and(|p| p.setup == setup);
    let armed = mine && crate::wcs_pick::collecting(state);
    ui.horizontal(|ui| {
        ui.label("    Set WCS from features - each pick sets a role");
        let pick_text = if armed { "Stop picking" } else { "Pick for WCS" };
        if ui
            .selectable_label(armed, pick_text)
            .on_hover_text(
                "Click faces, edges, corners or walls in the viewport; each pick sets the \
                 role in the box. Two axes place the WCS, plus an optional origin.",
            )
            .clicked()
        {
            if armed {
                crate::wcs_pick::disarm(state);
            } else {
                crate::wcs_pick::arm(state, setup);
            }
        }
        if armed {
            let cur = state.wcs_pick.as_ref().map_or(Role::Z, |d| d.role);
            ComboBox::from_id_salt(("wcs_role", setup))
                .selected_text(role_label(cur))
                .show_ui(ui, |ui| {
                    if let Some(d) = state.wcs_pick.as_mut().filter(|d| d.picking) {
                        for r in ROLES {
                            ui.selectable_value(&mut d.role, r, role_label(r));
                        }
                    }
                })
                .response
                .on_hover_text("The role the next viewport pick sets: the work origin, or an axis.");
        }
    });
    if mine {
        let picks = state.wcs_pick.as_ref().map(|p| p.picks.clone()).unwrap_or_default();
        // The command's own check, asked per frame: an incomplete or
        // unusable set of picks greys Apply out with the app's words.
        let check = align_roles(&state.world, &picks);
        // Review round 3: `SetWcs` always places the ACTIVE setup's WCS
        // (`SetWcsOrigin::new`, set_wcs.rs), and this row is drawn on
        // every setup's settings page. A row for an inactive setup must
        // not silently move another setup's work zero - Apply waits
        // until the row's setup is the active one, and the greyed button
        // says so in the same plain words the page's radio uses.
        let active = setup::active(&state.world).as_deref() == Some(setup);
        let make_active = format!("Make {setup} the active setup to set its WCS from features");
        let mut landed = false;
        // Wrapped: a long picked-list is exactly when Apply must still be
        // reachable - a plain horizontal row would push it off the pane.
        ui.horizontal_wrapped(|ui| {
            let words: Vec<String> = picks
                .iter()
                .map(|(g, r)| format!("{} of '{}' sets {}", g.feature, g.entity, role_label(*r)))
                .collect();
            let text = if words.is_empty() { "nothing picked yet".to_owned() } else { words.join(", ") };
            ui.label(RichText::new(format!("      Picked: {text}")).weak());
            let tip = if !active {
                make_active.as_str()
            } else {
                match &check {
                    Ok(_) => "Move this setup's work coordinate system onto the picked frame.",
                    Err(why) => why.as_str(),
                }
            };
            if ui
                .add_enabled(check.is_ok() && active, egui::Button::new("Apply"))
                .on_hover_text(tip)
                .on_disabled_hover_text(tip)
                .clicked()
            {
                landed = true;
            }
        });
        match check {
            Ok(a) => {
                if landed {
                    state.pending.push_back(CommandKind::SetWcs {
                        pos: a.pos,
                        roll_deg: a.roll_deg,
                        pitch_deg: a.pitch_deg,
                        yaw_deg: a.yaw_deg,
                    });
                }
            }
            Err(why) => {
                ui.label(RichText::new(format!("      {why}")).weak())
                    .on_hover_text("The picks must make a frame before the WCS can move.");
            }
        }
    }
}
