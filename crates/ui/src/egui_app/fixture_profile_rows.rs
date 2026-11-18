//! Fixture row controls for profile-built fixtures and STEP profile derivation (#744).
//! Provenance: clean-room egui row controls over `camcraft_app::fixture_profile`.

use egui::Ui;

use camcraft_app::fixture;
use camcraft_app::fixture_profile::FixtureProfile;
use camcraft_app::model;
use camcraft_app::step_source;
use camcraft_app::units;

use crate::state::{CommandKind, State};

use super::fields::num_by;
use super::fixture_profile_dialog::{axis_picker, extruded_points_editor, spun_points_editor};

/// Render profile edit or STEP derive/export controls on fixture `name`'s row.
pub(super) fn row(ui: &mut Ui, state: &mut State, name: &str) {
    let Some(entity) = model::find_by_name(&state.world, name) else {
        return;
    };
    let step = step_source::of(&state.world, &entity);
    let world_profile = fixture::def_of(&state.world, &entity).and_then(|d| d.profile);
    if step.is_none() && world_profile.is_none() {
        return;
    }
    let u = state.units();
    let export_why = state.in_browser(crate::browser_files::NO_EXPORT);
    if state.profile_edit_draft(name).is_none() {
        return;
    }

    let mut apply_profile: Option<FixtureProfile> = None;
    let mut reset = false;
    let mut derive_axis: Option<Option<camcraft_app::fixture_profile::ExtrudeAxis>> = None;
    let mut export_dxf = false;

    if let Some(step_src) = step {
        ui.label(format!("STEP solid kept as source ({} bytes)", step_src.len()));
        if let Some(d) = state.profile_edits.get_mut(name) {
            ui.horizontal(|ui| {
                if ui.small_button("Derive spun profile").clicked() {
                    derive_axis = Some(None);
                }
                ui.label("Axis");
                axis_picker(ui, &format!("derive-axis-{name}"), &mut d.derive_axis);
                if ui.small_button("Derive extruded profile").clicked() {
                    derive_axis = Some(Some(d.derive_axis));
                }
            });
            if let Some(ref p) = world_profile {
                let summary = match p {
                    FixtureProfile::Spun { points } => format!("Reference spun profile: {} pts", points.len()),
                    FixtureProfile::Extruded { points, axis, depth } => {
                        format!("Reference extruded profile ({}): {} pts, depth {}", axis.label(), points.len(), units::len(*depth, u, 2))
                    }
                };
                ui.horizontal(|ui| {
                    ui.label(summary);
                    if crate::browser_files::button(ui, export_why, "Export DXF…", "Save derived profile as ASCII DXF") {
                        export_dxf = true;
                    }
                });
            }
            if let Some(err) = &d.error {
                super::error_text::error_text(ui, ui.visuals().error_fg_color, err);
            }
        }
    } else if let Some(d) = state.profile_edits.get_mut(name) {
        let kind = d.profile.kind_label();
        ui.label(format!("{kind} fixture profile"));
        match &mut d.profile {
            FixtureProfile::Spun { points } => {
                spun_points_editor(ui, u, &format!("row-spun-{name}"), points);
            }
            FixtureProfile::Extruded { points, axis, depth } => {
                ui.horizontal(|ui| {
                    ui.label("Extrude axis");
                    axis_picker(ui, &format!("row-axis-{name}"), axis);
                    let l = ui.label("Depth");
                    num_by(ui, u, depth, l.id);
                });
                extruded_points_editor(ui, u, &format!("row-ext-{name}"), points);
            }
        }
        ui.horizontal(|ui| {
            if ui.small_button("Apply profile").clicked() {
                apply_profile = Some(d.profile.clone());
            }
            if ui.small_button("Reset").clicked() {
                reset = true;
            }
            if crate::browser_files::button(ui, export_why, "Export DXF…", "Export fixture profile as ASCII DXF") {
                export_dxf = true;
            }
        });
        if let Some(err) = &d.error {
            super::error_text::error_text(ui, ui.visuals().error_fg_color, err);
        }
    }

    if reset
        && let Some(p) = world_profile
        && let Some(d) = state.profile_edits.get_mut(name)
    {
        d.profile = p;
        d.error = None;
    }
    if let Some(profile) = apply_profile {
        state.pending.push_back(CommandKind::SetFixtureProfile { name: name.to_owned(), profile });
    }
    if let Some(axis) = derive_axis {
        state.pending.push_back(CommandKind::DeriveFixtureProfile { name: name.to_owned(), axis });
    }
    if export_dxf {
        state.export_fixture_dxf_dialog(name);
    }
}
