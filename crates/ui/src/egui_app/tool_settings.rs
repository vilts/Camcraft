//! A selected tool's Properties (#2002) and the blank new-tool form
//! (#2397). The new-tool form is a label-left property grid in collapsible
//! groups (#2014, `tool_settings_grid`); Add tool lands it. #3930: a saved
//! tool's page is a read-only summary (`tool_summary`) with the door
//! "Edit in tool library": the tool library window edits its geometry,
//! holder, body, recipes and details. The drawn profile and the cutter
//! keep their own window (#2397, `tool_profile_page::editor`).
//! Provenance: our own tool editor extracted from pages.rs, retaining its
//! commands. No vendor material involved.

use crate::actions;
use crate::state::{CommandKind, ErrorScope, State, UiTarget};
use camcraft_app::model;
use egui::Ui;

pub(crate) fn page(ui: &mut Ui, state: &mut State, name: &str) {
    if name.is_empty() && !state.new_tool_form {
        // The Tool target with the empty name is the tool destination. It
        // shows the blank draft when New opened one, and the pointer to the
        // library otherwise (#2397).
        let content_h = (ui.available_height() - 40.0).max(80.0);
        egui::ScrollArea::vertical()
            .id_salt("tool_page_scroll")
            .auto_shrink([false, false])
            .max_height(content_h)
            .show(ui, |ui| {
                ui.label("Open the Tool library from Tools to browse or create tools.");
            });
        ui.separator();
        if ui.button("Close").clicked() {
            state.close_property_editor();
        }
        return;
    }
    if !name.is_empty() {
        state.tool_ui.name = name.to_owned();
    }
    state.sync_tool_ui();
    let exists = state.tool_ui.exists;
    // #3930: a saved tool shows its summary; the blank form (and a draft
    // whose typed name matches a saved tool) keeps the grid and Add tool.
    let summary = exists && !name.is_empty();
    let err_preview = state.error_for(ErrorScope::Tool).is_some();
    let block_preview = !exists && !block_reason(state, exists, &state.tool_ui.name).is_empty();
    let footer_h = if err_preview && block_preview {
        96.0
    } else if err_preview || block_preview {
        68.0
    } else {
        40.0
    };
    let scroll_h = (ui.available_height() - footer_h).max(80.0);
    egui::ScrollArea::vertical()
        .id_salt("tool_page_scroll")
        .auto_shrink([false, false])
        .max_height(scroll_h)
        .show(ui, |ui| {
            if summary {
                ui.heading(format!("Tool: {name}"));
                ui.horizontal_wrapped(|ui| {
                    super::tool_summary::edit_button(ui, state, name);
                    // #2397: the drawn profile and the cutter have their own
                    // window, which edits through this page's draft.
                    if ui
                        .button("Edit tool shape")
                        .on_hover_text("open the drawn holder profile, the cutter and the body in their own window")
                        .clicked()
                    {
                        actions::open_tool_shape_editor(state, name);
                    }
                    if ui.button("Delete").clicked() {
                        state
                            .pending
                            .push_back(CommandKind::DeleteTool(name.to_owned()));
                    }
                });
                super::tool_summary::rows(ui, state);
            } else {
                ui.heading("New tool");
                // The property grid takes the pane: the label on the left
                // half, the input on the right half (#2014).
                super::tool_settings_grid::grid(ui, state);
            }
            preview(ui, state);
        });
    footer(ui, state, summary);
}

fn preview(ui: &mut Ui, state: &mut State) {
    let u = state.units();
    let exists = state.tool_ui.exists;
    // One snapshot of the draft drives the picture, so the rows and the
    // preview cannot disagree mid-frame.
    let preview_tool = state.tool_ui.to_tool();
    // #371: the placed body's envelope rides the profile. #2007: the one
    // side preview, under the fields; it draws the profile tab's draft
    // through `preview_inputs`. #2397: the Body / Profile tabs and the
    // shape editor live in their own window ("Edit tool shape").
    let (body, steel) = super::tool_preview_inputs::preview_inputs(state, exists);
    super::tool_preview::draw(ui, &preview_tool, u, body.as_deref(), steel.as_deref(), false);
}

/// Fixed bottom button row (#2007): Apply / Add tool beside Close,
/// always visible without scrolling, with any block or refusal above it.
/// #3930: a saved tool's summary has no draft to Apply, so Close stands
/// alone (the tool library window has the Apply).
fn footer(ui: &mut Ui, state: &mut State, summary: bool) {
    let exists = state.tool_ui.exists;
    let name_now = state.tool_ui.name.clone();
    let block = block_reason(state, exists, &name_now);
    ui.separator();
    if !exists && !block.is_empty() {
        ui.label(&block);
    }
    if let Some(msg) = state.error_for(ErrorScope::Tool).map(str::to_owned) {
        super::error_text::error_text(
            ui,
            super::colorvision::text_inks(ui, &state.world).error,
            &msg,
        );
    }
    ui.horizontal(|ui| {
        if exists && !summary {
            let recipe_refusal = (state.tool_ui.tool_kind() == camcraft_app::tools::ToolKind::Waterjet)
                .then(|| state.tool_ui.to_tool().torch_refusal())
                .flatten();
            if ui
                .add_enabled(recipe_refusal.is_none(), egui::Button::new("Apply"))
                .on_disabled_hover_text(recipe_refusal.as_deref().unwrap_or_default())
                .clicked()
            {
                // #93 slice 3: a landed drawn cutter rides the edit.
                let apply = state.apply_tool_numbers(&name_now);
                state.pending.push_back(apply);
                push_number_if_changed(state, &name_now);
            }
        } else if !exists {
            let add = ui
                .add_enabled(block.is_empty(), egui::Button::new("Add tool"))
                .on_disabled_hover_text(&block);
            if add.clicked() {
                let landed = name_now.trim().to_owned();
                state.pending.push_back(CommandKind::AddTool {
                    name: landed.clone(),
                    tool: state.tool_ui.to_tool(),
                });
                push_number_if_changed(state, &landed);
                // #2397 review: the saved tool becomes the selection, so the
                // page leaves the new-tool form and shows its normal controls,
                // including the Edit tool shape button, without a second
                // browser click. The AddTool command lands in the same frame.
                actions::target(state, UiTarget::Tool(landed));
            }
        }
        if ui.button("Close").clicked() {
            state.close_property_editor();
        }
    });
}

fn block_reason(state: &State, exists: bool, name_now: &str) -> String {
    if name_now.trim().is_empty() {
        "Name the tool first.".to_owned()
    } else if !exists && model::find_by_name(&state.world, name_now).is_some() {
        "That name is already in the document.".to_owned()
    } else {
        String::new()
    }
}

/// Seed the numbers a draft must carry when the kind changes: a kind
/// that READS a field seeds the standard value, so flipping the combo
/// alone never leaves the Add button refusing on a half-built tool.
pub(super) fn seed_kind(state: &mut State, before: usize) {
    let exists = state.tool_ui.exists;
    if state.tool_ui.kind == 3
        && (state.tool_ui.tip_angle.is_nan() || state.tool_ui.tip_angle <= 60.0)
    {
        state.tool_ui.tip_angle = 118.0;
    }
    if state.tool_ui.kind == 4
        && (state.tool_ui.pitch.is_nan() || state.tool_ui.pitch <= 0.0)
    {
        state.tool_ui.pitch = 1.0;
    }
    if state.tool_ui.kind == 6
        && (state.tool_ui.nose_radius.is_nan() || state.tool_ui.nose_radius <= 0.0)
    {
        state.tool_ui.nose_radius = 0.8;
    }
    // The boring bar's nose (M5.2 F): the same corner circle the bore
    // operation reads, so a combo flip alone never leaves the op
    // refusing at the kernel's point-tip door.
    if state.tool_ui.kind == 5
        && (state.tool_ui.nose_radius.is_nan() || state.tool_ui.nose_radius <= 0.0)
    {
        state.tool_ui.nose_radius = 0.4;
    }
    if state.tool_ui.kind == 7 && (state.tool_ui.width.is_nan() || state.tool_ui.width <= 0.0) {
        state.tool_ui.width = 2.0;
    }
    // The threading V (M5.2 D): crest width 0.866 x the seed pitch 2.0,
    // and the 60-degree form angle the generator fits against.
    if state.tool_ui.kind == 8 {
        if state.tool_ui.width.is_nan() || state.tool_ui.width <= 0.0 {
            state.tool_ui.width = 1.732;
        }
        if state.tool_ui.tip_angle.is_nan() || !(0.0..180.0).contains(&state.tool_ui.tip_angle) {
            state.tool_ui.tip_angle = 60.0;
        }
    }
    // The form gauge (M5.2 E2): the house row's 4-wide band on a 6-radius
    // crown - wide enough to state a valley, narrow enough to fit twice.
    if state.tool_ui.kind == 9 {
        if state.tool_ui.width.is_nan() || state.tool_ui.width <= 0.0 {
            state.tool_ui.width = 4.0;
        }
        if state.tool_ui.nose_radius.is_nan() || state.tool_ui.nose_radius <= 0.0 {
            state.tool_ui.nose_radius = 6.0;
        }
    }
    // The chamfer mill (M2.4 B): the shop's 90 - the supplement of the
    // 45-degree face the catalog's chamfer seed stands at.
    if state.tool_ui.kind == 10
        && (state.tool_ui.tip_angle.is_nan()
            || !(0.0..180.0).contains(&state.tool_ui.tip_angle))
    {
        state.tool_ui.tip_angle = 90.0;
    }
    // The thread mills (#894): a form and a pitch.
    super::tool_thread_mill_row::seed(&mut state.tool_ui);
    // The undercut cutters (#1444): head wider than neck, and dovetail flank angle.
    super::tool_undercut_row::seed(&mut state.tool_ui);
    // #3368: only a chamfer mill states a tip; its form keeps sane numbers.
    super::tool_taper_tip_row::seed(&mut state.tool_ui);
    // #3369: only a form mill states a form; a new one starts as a roundover.
    super::tool_form_mill_row::seed(&mut state.tool_ui);
    // The torch (#670): no spindle, no corner - a kerf. The sinker
    // electrode (#541) and EDM wire (#2011) turn none either.
    if matches!(
        state.tool_ui.tool_kind(),
        camcraft_app::tools::ToolKind::Torch
            | camcraft_app::tools::ToolKind::Waterjet
            | camcraft_app::tools::ToolKind::Electrode
            | camcraft_app::tools::ToolKind::Wire
    ) {
        state.tool_ui.rpm = 0.0;
        state.tool_ui.corner_r = 0.0;
    } else if matches!(before, 12 | 13 | 16 | 17)
        && (state.tool_ui.rpm.is_nan() || state.tool_ui.rpm <= 0.0)
    {
        state.tool_ui.rpm = 10000.0;
    }
    if state.tool_ui.tool_kind() == camcraft_app::tools::ToolKind::Wire {
        if state.tool_ui.diameter.is_nan()
            || !(camcraft_app::wire_tools::MIN_WIRE_MM..=camcraft_app::wire_tools::MAX_WIRE_MM)
                .contains(&state.tool_ui.diameter)
        {
            state.tool_ui.diameter = 0.25;
        }
        if state.tool_ui.shank_d.is_nan() || state.tool_ui.shank_d > camcraft_app::wire_tools::MAX_WIRE_MM {
            state.tool_ui.shank_d = state.tool_ui.diameter;
        }
        if !exists
            && (state.tool_ui.material_group.is_empty()
                || state.tool_ui.material_group == "aluminum")
        {
            state.tool_ui.material_group = "brass".into();
        }
        if !exists {
            state.tool_ui.cutting.clear();
        }
    }
}

/// #221 / #2442: queue a `SetToolNumber` when the editor's number differs
/// from the tool's own (or for a new tool, when the user typed a number
/// other than the `next_free` number `AddTool` will assign). Rides the
/// same Apply / Add click as the geometry, as its own undoable command.
/// #3893: the tool library window's Apply calls it too.
pub(super) fn push_number_if_changed(state: &mut State, name: &str) {
    let want = (state.tool_ui.number > 0).then_some(state.tool_ui.number);
    let have = model::find_by_name(&state.world, name)
        .and_then(|e| model::tool_number_of(&state.world, &e));
    if have.is_some() {
        if want != have {
            state.pending.push_back(CommandKind::SetToolNumber {
                name: name.to_owned(),
                number: want,
            });
        }
    } else if let Some(n) = want
        && Some(n) != camcraft_app::tool_numbers::next_free(&state.world)
    {
        state.pending.push_back(CommandKind::SetToolNumber {
            name: name.to_owned(),
            number: Some(n),
        });
    }
}
