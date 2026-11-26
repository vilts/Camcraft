//! Saved cut recipes (#3073): an operation's settings saved under a name,
//! then applied to new geometry as one undoable step - by the command and
//! by the MCP tools. A recipe whose tool or material is gone is refused
//! by name.
//!
//! Gate map (M4.4): covers app::cut_recipe, app::commands
//! (ApplyCutRecipe), app::mcp.
//! Provenance: our own command and MCP contract.

mod common;
#[path = "cut_recipe/picks.rs"]
mod picks;
#[path = "cut_recipe/relief.rs"]
mod relief;
#[path = "cut_recipe/safety.rs"]
mod safety;
#[path = "cut_recipe/waterjet.rs"]
mod waterjet;

use camcraft_app::commands::{
    AddTool, ApplyCutRecipe, CreateOperation, GenerateOperation, History, SetStock, SetStockMaterial,
};
use camcraft_app::cut_recipe::{self, CutRecipe};
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{WorkMaterial, catalog};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;
use serde_json::{Value, json};

const TOOL: &str = "EM-0600-4FL";

fn face_params() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        blend_radius: None,
        tilt: None,
        stepover: 2.5,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.0),
        top_z: None,
        stepdown: Some(0.5),
        feed: 900.0,
        plunge_feed: 250.0,
        climb: false,
        picks: Default::default(),
    }
}

/// A box stock, one tool, and one face op made by hand.
fn job() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    history.execute(&world, stock.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, TOOL, tool).expect("catalog tool"));
    let cmd = CreateOperation::new(&world, "Face1", TOOL, SrcRef::Stock, face_params());
    history.execute(&world, cmd.expect("create"));
    (world, history)
}

#[test]
fn cut_recipe_apply_generates_and_undo_removes_it_in_one_step() {
    let (world, mut history) = job();
    let recipe = CutRecipe::capture(&world, "Face1", "Sign face").expect("capture");
    assert_eq!(recipe.tool, TOOL);
    assert_eq!(recipe.params, face_params());

    // The file round trip keeps every field.
    let text = cut_recipe::to_text(std::slice::from_ref(&recipe)).expect("write");
    let back = cut_recipe::parse(&text).expect("read");
    assert_eq!(back, vec![recipe.clone()]);

    let cmd = ApplyCutRecipe::new(&world, &back[0], "Face2", SrcRef::Stock).expect("apply");
    history.execute(&world, cmd);
    let e = model::find_by_name(&world, "Face2").expect("the recipe made an op");
    let op = model::operation_of(&world, &e).expect("operation");
    assert_eq!(op.tool, TOOL);
    assert_eq!(op.params, face_params());

    history.execute(&world, GenerateOperation::new(&world, "Face2").expect("generate"));
    assert!(model::toolpath_of(&world, &e).is_some_and(|t| !t.path.moves.is_empty()), "a generated path");

    history.undo(&world); // the generation
    history.undo(&world); // the recipe: one step
    assert!(model::find_by_name(&world, "Face2").is_none(), "one undo removes the applied op");
    assert!(model::find_by_name(&world, "Face1").is_some(), "the source op stays");
}

#[test]
fn cut_recipe_with_a_missing_tool_is_refused_by_name() {
    let (world, _) = job();
    let mut recipe = CutRecipe::capture(&world, "Face1", "Box pocket").expect("capture");
    recipe.tool = "Gone-3mm".into();
    let err = ApplyCutRecipe::new(&world, &recipe, "Face2", SrcRef::Stock).err().expect("refused").to_string();
    assert!(err.contains("Box pocket") && err.contains("Gone-3mm"), "{err}");
    assert!(model::find_by_name(&world, "Face2").is_none());
}

#[test]
fn cut_recipe_with_an_unknown_or_other_material_is_refused_by_name() {
    let (world, mut history) = job();
    let mut recipe = CutRecipe::capture(&world, "Face1", "Alu face").expect("capture");
    recipe.material = Some("unobtainium".into());
    let err = ApplyCutRecipe::new(&world, &recipe, "Face2", SrcRef::Stock).err().expect("refused").to_string();
    assert!(err.contains("Alu face") && err.contains("unobtainium"), "{err}");

    recipe.material = Some(WorkMaterial::Aluminium6061.word().into());
    history.execute(&world, SetStockMaterial::new(&world, Some(WorkMaterial::MildSteel)).expect("steel"));
    let err = ApplyCutRecipe::new(&world, &recipe, "Face2", SrcRef::Stock).err().expect("refused").to_string();
    assert!(err.contains("Alu face") && err.contains(WorkMaterial::Aluminium6061.word()), "{err}");
    assert!(model::find_by_name(&world, "Face2").is_none());
}

fn call(server: &mut Server, name: &str, arguments: Value) -> Value {
    let reply = server
        .handle(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        }))
        .expect("a request gets a response");
    reply["result"].clone()
}

fn ok(server: &mut Server, name: &str, arguments: Value) -> Value {
    let result = call(server, name, arguments);
    assert_ne!(result["isError"], json!(true), "{name}: {result}");
    result["structuredContent"].clone()
}

#[test]
fn cut_recipe_mcp_save_list_apply_generate_and_undo() {
    let dir = std::env::temp_dir().join(format!("camcraft-recipes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let library = dir.join("recipes.camrecipes");
    let lib = library.to_string_lossy().into_owned();

    let mut s = Server::new();
    ok(&mut s, "set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }));
    ok(&mut s, "add_tool", json!({ "catalog": TOOL }));
    ok(&mut s, "add_operation", json!({ "kind": "Face", "tool": TOOL, "name": "Face1", "params": { "stepover": 2.5 } }));
    let saved = ok(&mut s, "save_cut_recipe", json!({ "operation": "Face1", "name": "Sign face", "library": lib }));
    assert_eq!(saved["recipe"], json!("Sign face"));
    let again = call(&mut s, "save_cut_recipe", json!({ "operation": "Face1", "name": "Sign face", "library": lib }));
    assert_eq!(again["isError"], json!(true), "a second save under one name asks for replace: {again}");

    let listed = ok(&mut s, "list_cut_recipes", json!({ "library": lib }));
    assert_eq!(listed["recipes"][0]["name"], json!("Sign face"));
    assert_eq!(listed["recipes"][0]["tool"], json!(TOOL));

    let applied = ok(&mut s, "apply_cut_recipe", json!({ "recipe": "Sign face", "name": "Face2", "library": lib }));
    assert_eq!(applied["operation"], json!("Face2"));
    ok(&mut s, "generate", json!({ "name": "Face2" }));
    let job = ok(&mut s, "get_job", json!({}));
    let ops = job["operations"].as_array().expect("operations");
    let face2 = ops.iter().find(|o| o["name"] == json!("Face2")).expect("Face2 in the job");
    assert_eq!(face2["params"]["face"]["stepover"], json!(2.5));
    assert_eq!(face2["status"], json!("ok"), "Face2 generated: {face2}");

    ok(&mut s, "undo", json!({})); // the generation
    ok(&mut s, "undo", json!({})); // the recipe
    let job = ok(&mut s, "get_job", json!({}));
    let names: Vec<_> = job["operations"].as_array().expect("ops").iter().map(|o| o["name"].clone()).collect();
    assert!(!names.contains(&json!("Face2")), "one undo removes the applied op: {names:?}");

    let missing = call(&mut s, "apply_cut_recipe", json!({ "recipe": "No such", "library": lib }));
    assert_eq!(missing["isError"], json!(true));
    assert!(missing["content"][0]["text"].as_str().is_some_and(|t| t.contains("No such")), "{missing}");
    let _ = std::fs::remove_dir_all(&dir);
}
