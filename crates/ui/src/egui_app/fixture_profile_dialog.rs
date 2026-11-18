//! Modal body for "Add profile / DXF fixture…" (#744): create or import
//! revolved spun turning fixtures and extruded milling fixtures.
//! Provenance: clean-room egui dialog over `FixtureProfileDraft`.

use egui::Ui;

use camcraft_app::fixture_profile::ExtrudeAxis;
use camcraft_app::units::Unit;

use crate::state::{CommandKind, FixtureProfileDraft, ProfileKindChoice};

use super::fields::{num_by, num_named, num_plain_by, num_plain_named};

/// Outcome returned by [`body`] to the modal shell.
pub(super) enum ProfileDialogAct {
    LoadDxf,
    Insert(Box<CommandKind>),
    Close,
}

/// Shared axis picker (`X`, `Y`, `Z`) for extruded profiles and STEP derive.
pub(super) fn axis_picker(ui: &mut Ui, id_salt: &str, axis: &mut ExtrudeAxis) {
    ui.horizontal(|ui| {
        ui.push_id(id_salt, |ui| {
            for candidate in [ExtrudeAxis::X, ExtrudeAxis::Y, ExtrudeAxis::Z] {
                ui.selectable_value(axis, candidate, candidate.label());
            }
        });
    });
}

/// Editable table of `(z, r, bulge)` vertices for a spun half-profile.
pub(super) fn spun_points_editor(
    ui: &mut Ui,
    u: Unit,
    id_salt: &str,
    points: &mut Vec<camcraft_app::fixture_profile::SpunPoint>,
) {
    let mut remove_idx: Option<usize> = None;
    egui::Grid::new(id_salt).num_columns(5).show(ui, |ui| {
        ui.strong("#");
        ui.strong("Z");
        ui.strong("R");
        ui.strong("Bulge");
        ui.label("");
        ui.end_row();
        let can_remove = points.len() > 2;
        for (i, pt) in points.iter_mut().enumerate() {
            let n = i + 1;
            ui.label(format!("{n}"));
            num_named(ui, u, &mut pt.z, &format!("{id_salt} Z {n}"));
            num_named(ui, u, &mut pt.r, &format!("{id_salt} R {n}"));
            num_plain_named(ui, &mut pt.bulge, &format!("{id_salt} bulge {n}"));
            if ui.add_enabled(can_remove, egui::Button::new("Remove").small()).clicked() {
                remove_idx = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = remove_idx {
        points.remove(i);
    }
    if ui.small_button("Add point").clicked() {
        let next = points.last().copied().unwrap_or(camcraft_app::fixture_profile::SpunPoint::line(0.0, 0.0));
        points.push(camcraft_app::fixture_profile::SpunPoint::line(next.z + 10.0, next.r));
    }
}

/// Editable table of `(u, v, bulge)` vertices for an extruded closed profile.
pub(super) fn extruded_points_editor(
    ui: &mut Ui,
    u: Unit,
    id_salt: &str,
    points: &mut Vec<camcraft_app::fixture_profile::ProfilePoint>,
) {
    let mut remove_idx: Option<usize> = None;
    egui::Grid::new(id_salt).num_columns(5).show(ui, |ui| {
        ui.strong("#");
        ui.strong("U");
        ui.strong("V");
        ui.strong("Bulge");
        ui.label("");
        ui.end_row();
        let can_remove = points.len() > 3;
        for (i, pt) in points.iter_mut().enumerate() {
            let n = i + 1;
            ui.label(format!("{n}"));
            num_named(ui, u, &mut pt.u, &format!("{id_salt} U {n}"));
            num_named(ui, u, &mut pt.v, &format!("{id_salt} V {n}"));
            num_plain_named(ui, &mut pt.bulge, &format!("{id_salt} bulge {n}"));
            if ui.add_enabled(can_remove, egui::Button::new("Remove").small()).clicked() {
                remove_idx = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = remove_idx {
        points.remove(i);
    }
    if ui.small_button("Add point").clicked() {
        let next = points.last().copied().unwrap_or(camcraft_app::fixture_profile::ProfilePoint::line(0.0, 0.0));
        points.push(camcraft_app::fixture_profile::ProfilePoint::line(next.u + 10.0, next.v));
    }
}

/// Render the dialog contents for `d` and return the chosen action.
pub(super) fn body(
    ui: &mut Ui,
    u: Unit,
    d: &mut FixtureProfileDraft,
    browser_why: Option<&str>,
) -> Option<ProfileDialogAct> {
    let mut act = None;
    ui.horizontal(|ui| {
        ui.selectable_value(&mut d.kind, ProfileKindChoice::Spun, "Spun (revolved)");
        ui.selectable_value(&mut d.kind, ProfileKindChoice::Extruded, "Extruded (closed 2D)");
    });
    ui.horizontal(|ui| {
        let l = ui.label("Fixture name");
        ui.text_edit_singleline(&mut d.name).labelled_by(l.id);
    });
    ui.horizontal(|ui| {
        if crate::browser_files::button(
            ui,
            browser_why,
            "Load DXF…",
            "Read a DXF half-profile (CENTRELINE) or closed profile",
        ) {
            act = Some(ProfileDialogAct::LoadDxf);
        }
        if let Some(src) = &d.source_label {
            ui.label(format!("DXF: {src}"));
        }
    });
    if d.kind == ProfileKindChoice::Extruded {
        ui.horizontal(|ui| {
            ui.label("Extrude axis");
            axis_picker(ui, "profile-dlg-axis", &mut d.axis);
        });
        ui.horizontal(|ui| {
            let l = ui.label("Depth");
            num_by(ui, u, &mut d.depth, l.id);
            ui.checkbox(&mut d.override_dxf_depth, "Override DXF depth");
        });
    }
    ui.add_space(4.0);
    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| match d.kind {
        ProfileKindChoice::Spun => spun_points_editor(ui, u, "profile-dlg-spun", &mut d.spun_points),
        ProfileKindChoice::Extruded => {
            extruded_points_editor(ui, u, "profile-dlg-ext", &mut d.extrude_points)
        }
    });
    ui.add_space(4.0);
    egui::Grid::new("profile-dlg-place").num_columns(2).show(ui, |ui| {
        for (k, axis) in ["Origin X", "Origin Y", "Origin Z"].iter().enumerate() {
            let l = ui.label(*axis);
            num_by(ui, u, &mut d.pos[k], l.id);
            ui.end_row();
        }
        let l = ui.label("Turn about Z (yaw)");
        num_plain_by(ui, &mut d.yaw_deg, l.id);
        ui.end_row();
    });
    if let Some(err) = &d.error {
        super::error_text::error_text(ui, ui.visuals().error_fg_color, err);
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("Insert").clicked() {
            act = Some(ProfileDialogAct::Insert(Box::new(CommandKind::InsertProfileFixture {
                setup: (!d.setup.is_empty()).then(|| d.setup.clone()),
                name: d.name.clone(),
                profile: d.active_profile(),
                pos: d.pos,
                yaw_deg: d.yaw_deg,
            })));
        }
        if ui.button("Cancel").clicked() {
            act = Some(ProfileDialogAct::Close);
        }
    });
    act
}
