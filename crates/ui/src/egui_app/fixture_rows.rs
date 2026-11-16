//! One fixture's row on the Fixtures page (#509): drawn to fit a
//! default-width properties pane - the name and its position wrap on
//! one line, the verbs (Place, Locate part, Unmark, Delete) stand on the next, the
//! Fixed caption wraps instead of truncating, and the place draft is a
//! labelled X / Y / Z / yaw form one click from every row. Every edit
//! still lands through the same commands (`MoveFixture`,
//! `SetFixtureFixed`, `UntagFixture`, `DeleteFixture`); the draft is
//! view state, never history.
//! A built-in vise (#788) adds its jaw opening: Set and "Open to stock",
//! each one `SetVise`. A built-in accessory (#789) adds its figures and
//! "Apply sizes", one `SetAccessory` (`fixture_accessory_rows`).
//! Provenance: our own layout over our own commands; specced from the
//! contributor's hands-on report on #509, no HSMWorks material involved.

use egui::Ui;

use camcraft_app::schema::Location;
use camcraft_app::{fixture, model, units};

use crate::state::{CommandKind, FixtureUi, State, quat_yaw_deg};

use super::layout::prop_row;
use super::fields::{num_plain_row, num_row};

const FIXED_HELP: &str = "On: a table or tailstock - it stands still while a rotary turns the part, \
     so it is collision-checked in the machine frame. Off: a chuck, turning with the part.";

/// The draft that opens `name`'s place form at where it stands now.
fn draft_of(name: &str, loc: &Location) -> FixtureUi {
    FixtureUi {
        name: name.to_owned(),
        x: loc.pos.x,
        y: loc.pos.y,
        z: loc.pos.z,
        yaw_deg: quat_yaw_deg(loc.rot),
    }
}

/// Draw one fixture: `loc` is where it stands, `fixed` its current
/// turns-with-the-part flag (both read by the caller from the world).
pub(super) fn row(ui: &mut Ui, state: &mut State, name: &str, loc: &Location, fixed: bool) {
    let u = state.units();
    let is_draft = state.fixture_ui.name == name;
    // Line 1: the name and where it stands, wrapping inside the pane.
    ui.horizontal_wrapped(|ui| {
        if ui.selectable_label(is_draft, name).clicked() {
            state.fixture_ui = draft_of(name, loc);
        }
        ui.label(format!(
            "{}, {}, {}  yaw {:.1}°",
            units::len(loc.pos.x, u, 2),
            units::len(loc.pos.y, u, 2),
            units::len(loc.pos.z, u, 2),
            quat_yaw_deg(loc.rot)
        ));
    });
    // Line 2: the verbs, left-aligned so none falls off the right edge.
    ui.horizontal_wrapped(|ui| {
        ui.add_space(12.0);
        if ui
            .selectable_label(is_draft, "Place…")
            .on_hover_text("Set this fixture's X / Y / Z and yaw")
            .clicked()
        {
            state.fixture_ui = if is_draft { FixtureUi::default() } else { draft_of(name, loc) };
        }
        // #561: put the part in this fixture by mates.
        super::locate_form::arm_button(ui, state, "Locate part…", name, None);
        if ui.small_button("Unmark").clicked() {
            state.pending.push_back(CommandKind::UntagFixture(name.to_owned()));
        }
        if ui.small_button("Delete").clicked() {
            state.pending.push_back(CommandKind::DeleteFixture(name.to_owned()));
        }
    });
    if is_draft {
        place_form(ui, state, name);
    }
    if super::locate_form::armed_for(state, name, None) {
        super::locate_form::form(ui, state);
    }
    // #339 slice 4: how the gate judges this steel, and why a box.
    if let Some(m) = state.fixture_mesh_state(name) {
        ui.label(format!("Collision: {}", m.detail()));
    }
    // Whether the steel turns with the part (M13.8 (2)): a chuck rides
    // the rotary, a tailstock or a table stands still and is judged in
    // the machine frame. The box edits a copy - the flag lands through
    // `SetFixtureFixed`, one undo unit, never a poke. Its caption is the
    // checkbox's own text, so it wraps in the pane rather than truncating.
    let mut on = fixed;
    if ui.checkbox(&mut on, "Fixed (does not turn with the part)").on_hover_text(FIXED_HELP).changed() {
        state.pending.push_back(CommandKind::SetFixtureFixed { name: name.to_owned(), fixed: on });
    }
    vise_opening(ui, state, name);
    super::fixture_plate_rows::plate_rows(ui, state, name);
    super::fixture_accessory_rows::sizes(ui, state, name);
}

/// A built-in vise's jaw opening (#788): a draft field, Set (one
/// `SetVise`) and "To stock" (the opening from the setup's stock).
fn vise_opening(ui: &mut Ui, state: &mut State, name: &str) {
    let Some(params) = model::find_by_name(&state.world, name)
        .and_then(|e| fixture::def_of(&state.world, &e))
        .and_then(|d| d.vise)
    else {
        return;
    };
    let u = state.units();
    ui.label(format!(
        "Built-in vise {} - opens {} at most",
        params.size,
        units::len(params.max_opening, u, 3)
    ));
    let mut set = None;
    prop_row(ui, "Jaw opening", |ui| {
        num_row(ui, u, state.vise_opening_draft(name, params.opening));
    });
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        if ui.small_button("Set opening").clicked() {
            let opening = *state.vise_opening_draft(name, params.opening);
            set = Some((params.with_opening(opening), false));
        }
        if ui
            .small_button("Open to stock")
            .on_hover_text("Move the moving jaw against the setup's stock, as the vise stands")
            .clicked()
        {
            set = Some((params.clone(), true));
        }
    });
    if let Some((params, from_stock)) = set {
        state.pending.push_back(CommandKind::SetVise { name: name.to_owned(), params, from_stock });
    }
}

/// The place draft: one labelled field per axis, then Apply, which
/// queues `MoveFixture` - the one undoable way a fixture moves.
fn place_form(ui: &mut Ui, state: &mut State, name: &str) {
    let u = state.units();
    let d = &mut state.fixture_ui;
    prop_row(ui, "Place X", |ui| num_row(ui, u, &mut d.x));
    prop_row(ui, "Place Y", |ui| num_row(ui, u, &mut d.y));
    prop_row(ui, "Place Z", |ui| num_row(ui, u, &mut d.z));
    prop_row(ui, "Place yaw", |ui| num_plain_row(ui, &mut d.yaw_deg));
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        if ui.small_button("Apply").clicked() {
            state.pending.push_back(CommandKind::MoveFixture {
                name: name.to_owned(),
                pos: [state.fixture_ui.x, state.fixture_ui.y, state.fixture_ui.z],
                yaw_deg: state.fixture_ui.yaw_deg,
            });
        }
    });
}
