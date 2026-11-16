//! The setup's own Settings page, laid out like the WCS page (#1440, after
//! #739): titled groups - which setup is active, where its stock comes
//! from, how the part sits, its work offset, the workholding limit (a
//! turning setup only) and the setup's two actions as equal buttons filling
//! the pane's width - then one readout line at the foot. With no setups the
//! page is the one "First setup" group. The groups only frame the pickers
//! the page always had (`setup_stock_pick`, `setup_align_pick`, ...); every
//! change is the same undoable command (`ActivateSetup`,
//! `SetSetupStockSource`, `AlignPart`, `DeleteSetup`, ...) and no command or
//! project field is new.
//!
//! Provenance: our own layout over the `layout` form helpers; no vendor
//! material.

use egui::{RichText, Ui, Vec2};

use camcraft_app::setup;

use crate::state::{CommandKind, SetupPage, State, UiTarget};

use super::layout::group;
use super::setup_pages::{ACTIVE_SETUP_LABEL, WORKHOLDING_POINTER};
use super::wcs_page::{cell_w, even_row};

/// A button row's height (the WCS page's).
const BUTTON_H: f32 = 24.0;
/// The group titles, in page order (the structural twin reads them).
pub(super) const GROUPS: [&str; 5] = ["Active setup", "Stock source", "Part orientation", "Work offset", "This setup"];
/// The workholding group, drawn for a turning setup (or one that states a limit).
pub(super) const HOLDING_GROUP: &str = "Workholding";
/// The group of the page with no setups.
pub(super) const FIRST_GROUP: &str = "First setup";
/// What the page says with no setups.
pub(super) const NO_SETUPS: &str = "No setups in this document: the one stock and the one WCS act as the first setup.";
/// The one line under "Part orientation" saying what the group does.
pub(super) const ORIENT_HELP: &str = "How the part sits in this setup; each way has its own Apply.";

/// The width the pane shows from here: the page's long lines widen
/// `available_width` past the visible pane, and buttons sized by it ran off
/// the edge.
fn pane_w(ui: &Ui) -> f32 {
    (ui.clip_rect().right() - ui.cursor().left()).min(ui.available_width()).max(24.0)
}

/// The groups' body indent. The pick rows are already as wide as the narrow
/// properties pane (#330: Apply, "Point 1 to plane level" and the workholding
/// buttons must stay inside it), so the default indent would push them out. Both are set once for the page.
const INDENT: f32 = 12.0;
/// ... and the gap between a row's items, tightened to pay for it.
const ITEM_GAP: f32 = 4.0;

/// The page body for setup `ctx` (`None` - the implicit context).
pub(super) fn settings_page(ui: &mut Ui, state: &mut State, ctx: Option<&str>) {
    ui.scope(|ui| {
        ui.spacing_mut().indent = INDENT;
        ui.spacing_mut().item_spacing.x = ITEM_GAP;
        groups(ui, state, ctx);
    });
}

fn groups(ui: &mut Ui, state: &mut State, ctx: Option<&str>) {
    let Some(name) = ctx else {
        group(ui, FIRST_GROUP, None, |ui| {
            ui.label(NO_SETUPS);
            ui.label(RichText::new(WORKHOLDING_POINTER).weak());
            if even_row(ui, &["Create first setup"], Vec2::new(cell_w(ui, pane_w(ui), 1), BUTTON_H)).is_some() {
                state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
            }
        });
        return;
    };
    group(ui, GROUPS[0], None, |ui| {
        let is_active = setup::active(&state.world).as_deref() == Some(name);
        if ui.radio(is_active, ACTIVE_SETUP_LABEL).clicked() && !is_active {
            state.pending.push_back(CommandKind::ActivateSetup(Some(name.to_owned())));
        }
    });
    group(ui, GROUPS[1], None, |ui| super::setup_stock_pick::picker(ui, state, name));
    group(ui, GROUPS[2], None, |ui| {
        ui.label(RichText::new(ORIENT_HELP).small().weak());
        // The two dropdowns (#771): plane orientation (axes, points, face,
        // manual, cylinder) and orientation point; they draw the picker
        // rows main keeps (#738).
        super::setup_orient_pick::section(ui, state, name);
        super::setup_relative_pick::picker(ui, state, name);
    });
    group(ui, GROUPS[3], None, |ui| {
        // Main's reading order: set the WCS from features, then name its slot.
        super::wcs_pick_row::row(ui, state, name); // #701
        super::setup_wcs_pick::picker(ui, state, name);
    });
    if super::setup_workholding_row::shown(state, name) {
        group(ui, HOLDING_GROUP, None, |ui| super::setup_workholding_row::row(ui, state, name));
    }
    group(ui, GROUPS[4], None, |ui| actions(ui, state, name));
    readout(ui, state, name);
}

/// Delete and Open fixtures as two equal buttons. Workholding is edited on
/// the Fixtures page only (#1197): the click to it sits here.
fn actions(ui: &mut Ui, state: &mut State, name: &str) {
    let size = Vec2::new(cell_w(ui, pane_w(ui), 2), BUTTON_H);
    match even_row(ui, &[&format!("Delete {name}"), &format!("Open {name} fixtures")], size) {
        Some(0) => state.pending.push_back(CommandKind::DeleteSetup(name.to_owned())),
        Some(_) => {
            // Activate it too, as the browser's Fixtures row does: the
            // fixture page edits only the active setup.
            super::browser_pane::focus(state, name);
            crate::actions::target(state, UiTarget::SetupNode(name.to_owned(), SetupPage::Fixtures));
        }
        None => {}
    }
}

/// The setup's stock entity and datum source, read live off the world.
fn readout(ui: &mut Ui, state: &State, name: &str) {
    if let Some(def) = setup::def_of(&state.world, name) {
        let mut line = format!("Stock entity {}", def.stock);
        if !def.datum_from.is_empty() {
            line.push_str(&format!(", datum from {}", def.datum_from));
        }
        ui.label(RichText::new(line).small().weak());
    }
}
