//! #1881: waterjet tool drafts survive an undoable apply.
//! Provenance: our own synthetic recipe and UI command fixture.
//! Gate map (#1881): covers ui::state, ui::egui_app::tool_cutting_rows.

use camcraft_app::{model, tools::WorkMaterial, tool_cutting_rows::CuttingRow};
use camcraft_app::waterjet_tools::{PierceMethod, WaterjetCutData, waterjet_tool};
use camcraft_ui::state::{CommandKind, State, ToolUi};

#[test]
fn waterjet_tool_draft_preserves_recipe_through_apply_and_undo() {
    let mut state = State::new();
    let mut tool = waterjet_tool(0.8);
    tool.cutting.push(CuttingRow {
        material: Some(WorkMaterial::MildSteel), rpm: 0.0, feed: 600.0, stepdown: None, stepover: None, plasma: None,
        waterjet: Some(WaterjetCutData {
            thickness: 5.0, kerf: 0.8, pierce_time: 2.0, abrasive_delay: 0.3,
            re_pierce_time: 1.0, pierce_method: PierceMethod::Wiggle,
        }),
    });
    state.pending.push_back(CommandKind::AddTool { name: "Nozzle".into(), tool: tool.clone() });
    state.drain_actions();
    let read = |s: &State| model::find_by_name(&s.world, "Nozzle").and_then(|e| model::tool_of(&s.world, &e)).expect("Nozzle");
    assert_eq!(read(&state), tool);
    let mut draft = ToolUi::from_tool("Nozzle", &tool, true);
    draft.cutting[0].waterjet.as_mut().unwrap().abrasive_delay = 0.7;
    let edited = draft.to_tool();
    state.pending.push_back(CommandKind::ChangeTool { name: "Nozzle".into(), tool: edited.clone() });
    state.drain_actions();
    assert_eq!(read(&state), edited);
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert_eq!(read(&state), tool);
}
