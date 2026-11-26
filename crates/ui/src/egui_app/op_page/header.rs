//! Provenance: original operation header controls from our own UI design.

use egui::Ui;
use camcraft_app::schema::Operation;
use crate::state::{CommandKind, State};
use super::super::fields::combo_owned;

/// Tool, Source and Setup as label | control rows of the header grid.
pub(super) fn header_rows(ui: &mut Ui, state: &mut State, target: &str, live: Option<&Operation>, exists: bool) {
    // #535: a kind that mounts no library tool (the wire EDM's) has no
    // Tool row - and must not adopt the rack's first tool into its draft.
    let toolless = !state.op_ui.params.kind().info().mounts_tool();
    let mut tools = state.tool_names();
    if toolless {
        tools.clear();
    } else if !state.op_ui.tool.is_empty() && !tools.contains(&state.op_ui.tool) {
        tools.push(state.op_ui.tool.clone());
    } else if state.op_ui.tool.is_empty()
        && let Some(first) = tools.first()
    {
        state.op_ui.tool = first.clone();
    }
    if toolless {
        // no row: the wire is the machine's
    } else if tools.is_empty() {
        ui.label("Tool");
        ui.label("none").on_hover_text("no tools in the document - the Tools page has a catalog");
    } else {
        ui.label("Tool");
        let mut t_idx = tools.iter().position(|t| *t == state.op_ui.tool).unwrap_or(0);
        let before = t_idx;
        ui.horizontal(|ui| { super::super::tool_icons::named_glyph(ui, &state.world, &state.op_ui.tool); super::super::tool_icons::tool_combo(ui, &state.world, "op_tool", &mut t_idx, &tools); });
        if t_idx != before
            && let Some(t) = tools.get(t_idx)
        {
            state.op_ui.tool = t.clone();
            if exists {
                state.pending.push_back(CommandKind::EditOperationLinks {
                    name: target.to_owned(),
                    tool: Some(state.op_ui.tool.clone()),
                    src: None,
                });
            }
        }
    }
    if !toolless {
        ui.end_row();
    }
    // #535: a wire EDM flushes with dielectric, which no coolant code
    // of the machine states - no coolant row (the post ignores it).
    if state.op_ui.params.kind().info().family() != camcraft_app::operation::Family::WireEdm {
        coolant_row(ui, state, target, live);
    }

    // Source: the stock, one of the user's parts, or (for 2D milling,
    // plasma and wire EDM via `sketch_model::admits`) a placed DXF
    // profile in this operation's setup - never a work offset, setup or
    // fixture (user review 2026-11-06, #1863, #1864).
    let op_setup = match live {
        Some(o) => o.setup.clone(),
        None => camcraft_app::setup::active(&state.world),
    };
    let mut src_items = super::super::op_source::source_items(&state.world, op_setup.as_deref(), &state.op_ui.params);
    let models: Vec<String> = src_items.iter().skip(1).cloned().collect();
    ui.label("Source");
    let mut s_idx: usize = if state.op_ui.src_kind == 2 {
        src_items.push("Selection".to_owned());
        src_items.len() - 1
    } else if state.op_ui.src_kind != 0 && !state.op_ui.src_name.is_empty() {
        match models.iter().position(|m| *m == state.op_ui.src_name) {
            Some(p) => 1 + p,
            None => {
                state.op_ui.src_kind = 0;
                state.op_ui.src_name.clear();
                0
            }
        }
    } else {
        0
    };
    let before = s_idx;
    combo_owned(ui, "op_src", &mut s_idx, &src_items);
    if s_idx != before {
        match s_idx.checked_sub(1).and_then(|i| models.get(i)) {
            None => {
                state.op_ui.src_kind = 0;
                state.op_ui.src_name.clear();
            }
            Some(m) => {
                state.op_ui.src_kind = 1;
                state.op_ui.src_name = m.clone();
            }
        }
        if exists {
            state.pending.push_back(CommandKind::EditOperationLinks {
                name: target.to_owned(),
                tool: None,
                src: Some(state.op_ui.src()),
            });
        }
    }
    ui.end_row();

    // Setup membership (M3.2 A): moving is never a stamp edit -
    // SetOperationSetup keeps the stamp and lets the resolved-Z digest
    // say whether the op went Stale.
    ui.label("Setup");
    let setups = camcraft_app::setup::names(&state.world);
    if setups.is_empty() {
        ui.label("global (no setups)").on_hover_text("this document has no setups yet - the global stock and offsets apply");
    } else {
        let cur = live.and_then(|o| o.setup.clone()).or_else(|| setups.first().cloned());
        let mut s_idx = setups.iter().position(|n| Some(n) == cur.as_ref()).unwrap_or(0);
        let before = s_idx;
        combo_owned(ui, "op_setup", &mut s_idx, &setups);
        if s_idx != before
            && let Some(n) = setups.get(s_idx)
        {
            state.pending.push_back(CommandKind::SetOperationSetup { name: target.to_owned(), setup: Some(n.clone()) });
        }
    }
    ui.end_row();
}

/// The Coolant row (#112), under the tool it feeds: the fluid the post
/// switches on for this operation. A committed operation only - a draft
/// takes the default (Flood) at creation and offers the row once it
/// exists, so the row never edits a value no command would carry.
fn coolant_row(ui: &mut Ui, state: &mut State, target: &str, live: Option<&Operation>) {
    let Some(op) = live else { return };
    use camcraft_app::operation::CoolantMode;
    ui.label("Coolant").on_hover_text(
        "the cutting fluid the post switches on for this operation - the machine's own code for it; a choice the machine states no code for blocks the post by name",
    );
    let labels: Vec<String> = CoolantMode::ALL.iter().map(|m| m.label().to_owned()).collect();
    let mut idx = CoolantMode::ALL.iter().position(|m| *m == op.coolant).unwrap_or(1);
    let before = idx;
    combo_owned(ui, "op_coolant", &mut idx, &labels);
    if idx != before
        && let Some(m) = CoolantMode::ALL.get(idx)
    {
        state.pending.push_back(CommandKind::EditOperationCoolant { name: target.to_owned(), coolant: *m });
    }
    ui.end_row();
}
