//! Nominal inspection setup form using the saved DXF chain identities and the shared creation command.

use super::layout::{button, hint, prop_row, text_row};
use crate::state::{CommandKind, State};
use camcraft_app::geometry::sketch_chains::ChainId;
use camcraft_app::{operation::wire_op, setup, sketch};
use egui::{ComboBox, Ui};
use flecs_ecs::prelude::World;

#[derive(Default, Clone)]
struct Draft {
    name: String,
    tool: String,
    selected: Option<(String, ChainId, String)>,
    x: String,
    y: String,
}

/// The setup context mounts the Agie wire machine: the test that the
/// inspection export applies to the operation's setup.
pub(super) fn agie_mounted(world: &World, ctx: Option<&str>) -> bool {
    camcraft_app::setup_mount::machine_of(world, ctx).1.is_some_and(|m| {
        m.model == "AGIECUT CLASSIC 2S"
            && camcraft_app::machine::kind(Some(&m)) == camcraft_app::machine::Kind::WireEdm
    })
}

/// The setup context is an Agie job: it mounts the Agie machine, or it
/// posts through the native Agie post. Only then do Agie-only rows show.
pub(super) fn agie_job(world: &World, ctx: Option<&str>) -> bool {
    agie_mounted(world, ctx)
        || camcraft_app::posting::agie::is_native(&camcraft_app::setup_mount::post_of(world, ctx))
}

pub(super) fn section(ui: &mut Ui, state: &mut State, ctx: Option<&str>) {
    // Agie-only: a setup without the Agie machine draws no header, so the
    // Post page of every other job keeps its layout.
    if !agie_mounted(&state.world, ctx) {
        return;
    }
    egui::CollapsingHeader::new("Agie nominal inspection")
        .id_salt(("agie-inspection-section", ctx))
        .show(ui, |ui| fields(ui, state, ctx));
}

fn fields(ui: &mut Ui, state: &mut State, ctx: Option<&str>) {
    hint(
        ui,
        "Choose one closed opening and the centre of a 3 mm start hole. The nominal piece has zero working steps. Physical stock XY is unknown.",
    );
    if crate::browser_files::button(
        ui,
        state.browser_refusal_of(&CommandKind::ImportAgieDrawing),
        "Import DXF drawing",
        "Place a drawing on the active setup",
    ) {
        state.pending.push_back(CommandKind::ImportAgieDrawing);
    }
    let id = ui.make_persistent_id(("agie-inspection-input", ctx));
    let mut draft = ui.data_mut(|d| d.get_temp::<Draft>(id)).unwrap_or_default();
    text_row(
        ui,
        "Operation name",
        &mut draft.name,
        "agie-inspection-name",
        140.0,
    );
    prop_row(ui, "Wire tool", |ui| {
        ComboBox::from_id_salt("agie-inspection-tool")
            .width(ui.available_width())
            .truncate()
            .selected_text(if draft.tool.is_empty() {
                "Choose a 0.25 mm wire"
            } else {
                draft.tool.as_str()
            })
            .show_ui(ui, |ui| {
                for n in state.tool_names() {
                    if wire_op::wire_diameter_of(&state.world, &n)
                        .is_ok_and(|d| (d - 0.25).abs() < 1e-9)
                    {
                        ui.selectable_value(&mut draft.tool, n.clone(), n);
                    }
                }
            });
    });
    prop_row(ui, "DXF opening", |ui| {
        ComboBox::from_id_salt("agie-inspection-chain")
            .width(ui.available_width())
            .truncate()
            .selected_text(
                draft
                    .selected
                    .as_ref()
                    .map_or("Choose a closed chain", |(_, _, s)| s.as_str()),
            )
            .show_ui(ui, |ui| {
                for name in sketch::placed_in(&state.world, ctx) {
                    if let Some((_, profile)) = sketch::find(&state.world, &name) {
                        for ring in &profile.data.chains().closed {
                            let label = format!(
                                "{} · {} · {:.2} mm² · {}",
                                name, ring.layer, ring.area, ring.id
                            );
                            ui.selectable_value(
                                &mut draft.selected,
                                Some((name.clone(), ring.id, label.clone())),
                                label,
                            );
                        }
                    }
                }
            });
    });
    text_row(
        ui,
        "Start X (world mm)",
        &mut draft.x,
        "agie-inspection-x",
        95.0,
    );
    text_row(
        ui,
        "Start Y (world mm)",
        &mut draft.y,
        "agie-inspection-y",
        95.0,
    );
    if button(ui, "Create inspection opening").clicked() {
        match (draft.selected.as_ref(), draft.x.trim().parse::<f64>(),draft.y.trim().parse::<f64>()) {
                (Some((drawing,chain,_)),Ok(x),Ok(y)) if !draft.name.trim().is_empty() && !draft.tool.is_empty() => {
                    state.pending.push_back(CommandKind::CreateAgieInspection {name:draft.name.trim().into(),tool:draft.tool.clone(),
                        drawing:drawing.clone(),chain:*chain,start:[x,y],setup:setup::active(&state.world)});
                }
                _ => state.status="Enter an operation name, choose a wire and a closed chain, and enter both hole coordinates.".into(),
            }
    }
    ui.data_mut(|d| d.insert_temp(id, draft));
}
