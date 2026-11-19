//! Issue #870: the WCS page's fixed footer - Set origin beside Close, and an "Origin not applied" flag while the fields differ from the placed WCS - and the header line that says where the origin is in words; our own page law, specced from a user's first-cut report (they cut air, then hunted for the button that applies the origin), no vendor material consulted.

use egui::{RichText, Ui};

use camcraft_app::geometry::quat_from_euler;
use camcraft_app::geometry::IDENTITY_Q;
use camcraft_app::{model, setup, units};

use crate::state::State;

/// The flag shown beside the footer's buttons while edits are pending.
pub(super) const NOT_APPLIED: &str = "Origin not applied";

/// The header line: "G54 origin: top of stock, front-left corner (X0 Y0 Z0)".
pub(super) fn origin_line(ui: &mut Ui, state: &State) {
    ui.label(RichText::new(origin_text(state)).strong());
}

pub(super) fn origin_text(state: &State) -> String {
    let active = setup::active(&state.world);
    let label = setup::wcs_label(&state.world, active.as_deref());
    let u = state.units();
    let words = camcraft_app::setup_origin::origin_words(&state.world, active.as_deref(), &|v| units::len(v, u, 2));
    format!("{label} origin: {words} (X0 Y0 Z0)")
}

/// Whether the page's fields hold edits the placed WCS does not carry
/// yet. An unplaced WCS sits at the world origin, square.
pub(super) fn pending(state: &State) -> bool {
    let active = setup::active(&state.world);
    let placed = setup::wcs_entity(&state.world, active.as_deref()).and_then(|e| model::transform_of(&state.world, &e));
    let (pos, rot) = placed.map_or(([0.0; 3], IDENTITY_Q), |l| ([l.pos.x, l.pos.y, l.pos.z], l.rot));
    let d = &state.wcs_ui;
    // A non-finite draft counts as pending: it is not what is placed.
    let moved = [d.x, d.y, d.z].iter().zip(pos).any(|(a, b)| (a - b).abs().partial_cmp(&1e-9).is_none_or(|o| o.is_gt()));
    // Compared as rotations, not as angle triples: two triples can name
    // one rotation (yaw 180 and -180).
    let q = quat_from_euler(d.roll_deg.to_radians(), d.pitch_deg.to_radians(), d.yaw_deg.to_radians());
    let dot: f64 = q.iter().zip(rot).map(|(a, b)| a * b).sum();
    let turned = dot.abs().partial_cmp(&(1.0 - 1e-9)).is_none_or(|o| o.is_lt());
    moved || turned
}

/// The footer row: Set origin, Close, and the pending flag. Returns
/// whether Close was clicked.
pub(super) fn footer(ui: &mut Ui, state: &mut State) -> bool {
    let waiting = pending(state);
    let warn = super::colorvision::text_inks(ui, &state.world).warn;
    let mut close = false;
    ui.horizontal(|ui| {
        if ui.button("Set origin").clicked() {
            super::wcs_page::apply_wcs_drafts(state);
        }
        close = ui.button("Close").clicked();
        if waiting {
            ui.label(RichText::new(NOT_APPLIED).color(warn));
        }
    });
    close
}
