//! Recipe per-corner relief regressions for PR #3399.
//! Gate map: covers app::commands::operation::recipe and app::mcp.
//! Provenance: original fixtures and command contracts.

use super::*;
use camcraft_app::geo2d::Point2;
use camcraft_app::machining2d::{CornerReliefChoice, CornerReliefKind, CornerReliefOverride};
use camcraft_app::operation::{OpType, seed_params};

/// The three kinds that carry per-corner relief overrides and are not
/// refused whole.
const KINDS: [OpType; 3] = [OpType::Pocket, OpType::Adaptive2D, OpType::Offset2D];

fn relief_recipe(kind: OpType, global: CornerReliefKind, overrides: Vec<CornerReliefOverride>) -> CutRecipe {
    let mut params = seed_params(kind.ordinal());
    match &mut params {
        OpParams::Pocket { corner_relief, .. }
        | OpParams::Adaptive2D { corner_relief, .. }
        | OpParams::Offset2D { corner_relief, .. } => {
            corner_relief.kind = global;
            corner_relief.overrides = overrides;
        }
        other => panic!("{kind:?} has no corner relief: {other:?}"),
    }
    CutRecipe {
        name: "Corner recipe".into(),
        tool: TOOL.into(),
        material: None,
        params,
    }
}

#[test]
fn cut_recipe_refuses_saved_corner_relief_overrides_by_name() {
    for kind in KINDS {
        let (world, _) = job();
        let saved = CornerReliefOverride {
            at: Point2::new(20.0, 20.0),
            choice: CornerReliefChoice::None,
        };
        let recipe = relief_recipe(kind, CornerReliefKind::None, vec![saved]);
        let err = ApplyCutRecipe::new(&world, &recipe, "New", SrcRef::Stock)
            .err()
            .unwrap_or_else(|| panic!("{kind:?}: override refused"))
            .to_string();
        assert!(err.contains("Corner recipe") && err.contains("saved geometry"), "{kind:?}: {err}");
        assert!(model::find_by_name(&world, "New").is_none(), "{kind:?}");
    }
}

#[test]
fn cut_recipe_applies_global_corner_relief_without_overrides() {
    for kind in KINDS {
        let (world, mut history) = job();
        let recipe = relief_recipe(kind, CornerReliefKind::Dogbone, Vec::new());
        let cmd = ApplyCutRecipe::new(&world, &recipe, "New", SrcRef::Stock)
            .unwrap_or_else(|e| panic!("{kind:?}: global relief applies: {e}"));
        history.execute(&world, cmd);
        let e = model::find_by_name(&world, "New").unwrap_or_else(|| panic!("{kind:?}: op made"));
        let op = model::operation_of(&world, &e).expect("operation");
        assert_eq!(op.params, recipe.params, "{kind:?}: the global relief setting is kept");
    }
}

fn library(tag: &str) -> (std::path::PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("camcraft-recipe-relief-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let lib = dir.join("recipes.camrecipes").to_string_lossy().into_owned();
    (dir, lib)
}

/// A pocket on 40 x 40 stock with `relief`, saved as "Corner recipe";
/// then the stock grows to 60 x 60.
fn saved_pocket_on_new_stock(server: &mut Server, relief: Value, lib: &str) {
    ok(server, "set_stock", json!({"box": {"dx": 40.0, "dy": 40.0, "dz": 10.0}}));
    ok(server, "add_tool", json!({"catalog": TOOL}));
    // Fixed heights: the job has no model for the default references.
    let heights = json!({"top_z": 10.0, "bottom_z": 8.0, "stepdown": 2.0});
    ok(
        server,
        "add_operation",
        json!({"kind": "Pocket", "tool": TOOL, "name": "P", "params": heights}),
    );
    ok(server, "edit_operation", json!({"name": "P", "params": {"corner_relief": relief}}));
    ok(
        server,
        "save_cut_recipe",
        json!({"operation": "P", "name": "Corner recipe", "library": lib}),
    );
    ok(server, "set_stock", json!({"box": {"dx": 60.0, "dy": 60.0, "dz": 10.0}}));
}

fn has_op(server: &mut Server, name: &str) -> bool {
    let job = ok(server, "get_job", json!({}));
    job["operations"]
        .as_array()
        .expect("operations")
        .iter()
        .any(|op| op["name"] == json!(name))
}

#[test]
fn cut_recipe_mcp_refuses_saved_corner_relief_overrides() {
    let (dir, lib) = library("override");
    let mut server = Server::new();
    let relief = json!({"kind": "none", "overrides": [{"at": [20.0, 20.0], "choice": "none"}]});
    saved_pocket_on_new_stock(&mut server, relief, &lib);
    let refused = call(
        &mut server,
        "apply_cut_recipe",
        json!({"recipe": "Corner recipe", "name": "New", "library": lib}),
    );
    assert_eq!(refused["isError"], json!(true), "{refused}");
    let text = refused["content"][0]["text"].as_str().expect("error text");
    assert!(text.contains("Corner recipe") && text.contains("saved geometry"), "{text}");
    assert!(!has_op(&mut server, "New"), "a refused recipe makes no operation");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cut_recipe_mcp_applies_global_corner_relief() {
    let (dir, lib) = library("global");
    let mut server = Server::new();
    saved_pocket_on_new_stock(&mut server, json!({"kind": "dogbone"}), &lib);
    let applied = ok(
        &mut server,
        "apply_cut_recipe",
        json!({"recipe": "Corner recipe", "name": "New", "library": lib}),
    );
    assert_eq!(applied["operation"], json!("New"));
    ok(&mut server, "generate", json!({"name": "New"}));
    let job = ok(&mut server, "get_job", json!({}));
    let ops = job["operations"].as_array().expect("operations");
    let new = ops.iter().find(|op| op["name"] == json!("New")).expect("New in the job");
    assert_eq!(new["params"]["pocket"]["corner_relief"]["kind"], json!("dogbone"), "{new}");
    assert_eq!(new["status"], json!("ok"), "New generated: {new}");
    let _ = std::fs::remove_dir_all(&dir);
}
