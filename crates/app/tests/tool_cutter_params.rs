//! #93 slice 3 acceptance: a milling tool edited by its parameters
//! inside the outline - `ReshapeTool` lands the new numbers and the
//! drawn cutter carried onto them (`cutter_fit::refit`) as one undoable
//! step, refuses by name an outline that disagrees with the NEW numbers
//! (and every `ChangeTool` law), and is driven the same way over MCP
//! (`set_tool_cutter_params`, with the neck as a parameter).
//!
//! Gate map (M4.4): covers app::commands::tool_cutter, app::commands::tool, app::mcp::cutter_tools, tools::cutter_fit.

use camcraft_app::commands::{AddTool, CommandError, History, ReshapeTool, SetToolCutter};
use camcraft_app::cutter_fit::{self, Neck};
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::spun::SpunPoint;
use camcraft_app::tool_cutter;
use camcraft_app::tools::{Tool, ToolKind, catalog};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;
use serde_json::{Value, json};

const TOOL: &str = "EM-1000-4FL";

fn catalog_tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry")
}

/// EM-1000-4FL (Ø10 x 22 flutes, Ø10 x 40 shank) with a Ø8 neck from
/// 22 to 30.
fn necked() -> Vec<SpunPoint> {
    let pt = SpunPoint::line;
    vec![pt(0.0, 0.0), pt(0.0, 5.0), pt(22.0, 5.0), pt(22.0, 4.0), pt(30.0, 4.0), pt(30.0, 5.0), pt(62.0, 5.0), pt(62.0, 0.0)]
}

fn world_with_drawn() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, AddTool::new(&world, TOOL, catalog_tool(TOOL)).expect("tool"));
    history.execute(&world, SetToolCutter::new(&world, TOOL, Some(necked())).expect("drawn"));
    (world, history)
}

fn state(world: &World) -> (Tool, Option<Vec<SpunPoint>>) {
    let e = model::find_by_name(world, TOOL).expect("tool entity");
    let t = model::tool_of(world, &e).expect("a tool");
    (t, tool_cutter::cutter_of(world, &e).map(|s| s.points().to_vec()))
}

fn refusal(r: Result<ReshapeTool, CommandError>) -> String {
    match r {
        Err(CommandError::Invalid(m)) => m,
        Err(e) => panic!("wrong refusal {e}"),
        Ok(_) => panic!("accepted a reshape that should be refused"),
    }
}

#[test]
fn a_parameter_edit_carries_the_drawn_cutter_in_one_undo_step() {
    let (world, mut history) = world_with_drawn();
    let old = catalog_tool(TOOL);
    for new in [
        Tool { diameter: 12.0, shank_d: 12.0, ..old.clone() },
        Tool { kind: ToolKind::BullNose, corner_r: 2.0, flute_len: 28.0, ..old.clone() },
        Tool { kind: ToolKind::Drill, tip_angle: 118.0, shank_len: 50.0, ..old.clone() },
    ] {
        let (was, drawn) = state(&world);
        let points = drawn.map(|d| cutter_fit::refit(&was, &new, &d));
        let cmd = ReshapeTool::new(&world, TOOL, new.clone(), points).unwrap_or_else(|e| panic!("{new:?}: {e}"));
        assert!(cmd.next().is_some());
        history.execute(&world, cmd);
        let (t, drawn) = state(&world);
        assert_eq!(t, new, "the numbers landed");
        let drawn = drawn.expect("the drawn cutter rode along");
        assert_eq!(cutter_fit::neck_of(&t, &drawn), Some(Neck { d: 8.0, len: 8.0 }), "the neck kept its shape");
    }
    // One undo per edit takes numbers and outline back together.
    for _ in 0..3 {
        assert!(history.undo(&world));
    }
    assert_eq!(state(&world), (old.clone(), Some(necked())));
    assert!(history.redo(&world));
    assert_eq!(state(&world).0.diameter, 12.0);
    // None lands the numbers and drops the drawing: the numbers' own.
    history.execute(&world, ReshapeTool::new(&world, TOOL, Tool { diameter: 9.0, ..old }, None).expect("clear"));
    assert_eq!(state(&world).1, None);
    assert!(history.undo(&world));
    assert!(state(&world).1.is_some());
}

#[test]
fn a_reshape_is_refused_by_name_against_the_new_numbers() {
    let (world, _history) = world_with_drawn();
    let old = catalog_tool(TOOL);
    // The old outline, unfitted, disagrees with a Ø12 tool.
    let m = refusal(ReshapeTool::new(&world, TOOL, Tool { diameter: 12.0, ..old.clone() }, Some(necked())));
    assert!(m.starts_with(&format!("tool '{TOOL}': cutter profile:")) && m.contains("Ø 12.000"), "{m}");
    // Every ChangeTool law still holds.
    let m = refusal(ReshapeTool::new(&world, TOOL, Tool { diameter: -1.0, ..old.clone() }, None));
    assert!(m.contains("finite and positive"), "{m}");
    assert!(matches!(ReshapeTool::new(&world, "nope", old.clone(), None), Err(CommandError::NotFound(_))));
    // A turning tool is not a spun profile.
    let m = refusal(ReshapeTool::new(&world, TOOL, Tool { kind: ToolKind::Turning, ..old }, Some(necked())));
    assert!(m.contains("turning tool"), "{m}");
    assert_eq!(state(&world).1, Some(necked()), "nothing landed");
}

#[test]
fn the_tool_is_edited_by_its_parameters_over_mcp() {
    let mut server = Server::new();
    let mut id = 0;
    let mut call = |name: &str, args: Value| {
        id += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
        let reply = server.handle(&msg).expect("a response");
        let r = reply["result"].clone();
        if r["isError"] == json!(true) { Err(r["content"][0]["text"].as_str().unwrap_or_default().to_owned()) } else { Ok(r["structuredContent"].clone()) }
    };
    call("add_tool", json!({ "catalog": TOOL })).expect("add tool");
    // No drawing: the numbers change and the outline stays the numbers' own.
    let got = call("set_tool_cutter_params", json!({ "tool": TOOL, "params": { "flute_len": 25.0 } })).expect("params");
    assert_eq!(got["drawn"], json!(false));
    assert_eq!(got["neck"], Value::Null);
    // A neck is a parameter of the outline.
    let got = call("set_tool_cutter_params", json!({ "tool": TOOL, "neck": { "diameter": 8.0, "length": 8.0 } })).expect("neck");
    assert_eq!(got["drawn"], json!(true));
    assert_eq!(got["neck"], json!({ "diameter": 8.0, "length": 8.0 }));
    // A number edit carries the drawn neck along.
    let got = call("set_tool_cutter_params", json!({ "tool": TOOL, "params": { "diameter": 12.0, "shank_d": 12.0 } })).expect("carry");
    assert_eq!(got["neck"], json!({ "diameter": 8.0, "length": 8.0 }));
    assert_eq!(got["points"][1], json!([0.0, 6.0, 0.0]));
    // Refusals by name.
    let e = call("set_tool_cutter_params", json!({ "tool": TOOL, "neck": { "diameter": 14.0, "length": 8.0 } })).expect_err("wide neck");
    assert!(e.contains("neck:") && e.contains("below the shank"), "{e}");
    let e = call("set_tool_cutter_params", json!({ "tool": TOOL, "params": { "rpm": 1.0 } })).expect_err("not a shape number");
    assert!(e.contains("params.rpm is not a cutter parameter"), "{e}");
    assert!(call("set_tool_cutter_params", json!({ "tool": TOOL, "params": { "diameter": "x" } })).is_err());
    // One undo takes the carried edit back; null drops the drawing.
    call("undo", json!({})).expect("undo");
    let got = call("get_tool_cutter_profile", json!({ "tool": TOOL })).expect("get");
    assert_eq!(got["points"][1], json!([0.0, 5.0, 0.0]));
    assert_eq!(got["neck"], json!({ "diameter": 8.0, "length": 8.0 }));
    let got = call("set_tool_cutter_params", json!({ "tool": TOOL, "neck": null })).expect("drop");
    assert_eq!(got["drawn"], json!(false));
}
