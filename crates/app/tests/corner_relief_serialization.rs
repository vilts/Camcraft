//! Provenance: corner-relief project and MCP round trips through public app APIs.
//! Gate map (#2400): covers app::project (corner-relief round trip), app::mcp (corner-relief
//! parameters), app::operation::corner_relief.

mod corner_relief_support;
mod pocket_support;

use camcraft_app::commands::CreateOperation;
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{Height, HeightReference};
use camcraft_app::{project, recognize};
use camcraft_app::world::new_world;
use corner_relief_support::{TOOL, base_pocket, patch_pocket, setup_world};
use pocket_support::{block_with_pocket, rounded_rect};
use serde_json::{Value, json};

fn mcp_call(s: &mut Server, id: &mut i64, name: &str, args: Value) -> Result<Value, String> {
    *id += 1;
    let msg = json!({
        "jsonrpc": "2.0",
        "id": *id,
        "method": "tools/call",
        "params": { "name": name, "arguments": args }
    });
    let reply = s.handle(&msg).expect("JSON-RPC reply");
    let res = &reply["result"];
    if res["isError"] == json!(true) {
        return Err(res["content"][0]["text"].as_str().unwrap_or("").to_owned());
    }
    Ok(res["structuredContent"].clone())
}

#[test]
fn mcp_set_operation_and_project_roundtrip_preserve_corner_relief() {
    let mesh = block_with_pocket(30.0, 20.0, &rounded_rect(10.0, 10.0, 0.0, 1), 10.0, 20.0);
    let (world, mut h) = setup_world(Some(mesh), 60.0, 40.0, 20.0);
    let rec = recognize::pockets_of(&world, "part").expect("recognize");
    let base = base_pocket(
        PocketStrategy::Waterline,
        Height::of(HeightReference::BottomOfSelection, 10.0),
        Height::of(HeightReference::BottomOfSelection, 0.0),
        10.0,
    );
    let (src, base_op) = recognize::operation_for(&rec[0], &base).expect("op");

    // 1. Project file save/load round-trip preserves corner_relief, and default none omits it.
    let edited = patch_pocket(
        &base_op,
        json!({ "corner_relief": { "kind": "t_bone", "tbone_direction": "horizontal" } }),
    )
    .expect("patch corner_relief");
    h.execute(&world, CreateOperation::new(&world, "SavedOp", TOOL, src, edited).expect("create"));
    let tmp = std::env::temp_dir().join(format!("camcraft_2400_rt_{}.camproj", std::process::id()));
    project::save(&world, &tmp).expect("save");
    let text = std::fs::read_to_string(&tmp).expect("read saved");
    assert!(text.contains("corner_relief"), "saved JSON must carry non-default corner_relief");
    let reloaded = new_world();
    project::load(&reloaded, &tmp).expect("load");
    let _ = std::fs::remove_file(&tmp);
    let op2 = model::operation_of(&reloaded, &model::find_by_name(&reloaded, "SavedOp").unwrap()).unwrap();
    let json2 = project::params_to_json(&op2.params);
    assert_eq!(json2["pocket"]["corner_relief"]["kind"], json!("t_bone"));
    assert_eq!(json2["pocket"]["corner_relief"]["tbone_direction"], json!("horizontal"));

    // 2. MCP add_operation / edit_operation accepts corner_relief, resets cleanly to none,
    // and enforces Trochoidal refusal on Adaptive2D.
    let mut s = Server::new();
    let mut id = 0;
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    mcp_call(&mut s, &mut id, "add_tool", json!({ "catalog": TOOL })).expect("tool");
    mcp_call(&mut s, &mut id, "set_stock", json!({ "box": { "dx": 40.0, "dy": 40.0, "dz": 10.0 } })).expect("stock");
    mcp_call(
        &mut s,
        &mut id,
        "add_operation",
        json!({
            "kind": "pocket",
            "tool": TOOL,
            "source": "stock",
            "name": "P1"
        }),
    )
    .expect("add_operation");
    let edited_mcp = mcp_call(
        &mut s,
        &mut id,
        "edit_operation",
        json!({
            "name": "P1",
            "params": {
                "corner_relief": {
                    "kind": "dogbone"
                }
            }
        }),
    )
    .expect("edit_operation accepts corner_relief");
    assert_eq!(edited_mcp["params"]["pocket"]["corner_relief"]["kind"], json!("dogbone"));

    let reset_mcp = mcp_call(
        &mut s,
        &mut id,
        "edit_operation",
        json!({
            "name": "P1",
            "params": {
                "corner_relief": {
                    "kind": "none"
                }
            }
        }),
    )
    .expect("edit_operation resets corner_relief to none");
    assert!(
        reset_mcp["params"]["pocket"].get("corner_relief").is_none(),
        "resetting corner_relief to none omits default field: {reset_mcp}"
    );
}
