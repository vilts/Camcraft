//! Recipe start and exit pick regressions for PR #3399.
//! Gate map: covers app::commands::operation::recipe and app::mcp.
//! Provenance: original fixtures and command contracts.

use super::*;
use camcraft_app::geo2d::Point2;
use camcraft_app::operation::{OpType, seed_params};

#[derive(Clone, Copy, Debug)]
enum Pick {
    Start,
    Exit,
}

impl Pick {
    fn key(self) -> &'static str {
        match self {
            Pick::Start => "start",
            Pick::Exit => "exit",
        }
    }
}

fn contour_with(pick: Option<(Pick, Point2)>) -> OpParams {
    let mut params = seed_params(OpType::Contour.ordinal());
    if let (Some(picks), Some((which, at))) = (params.path_picks_mut(), pick) {
        let p = camcraft_app::path_picks::PathPick::xy(at);
        match which {
            Pick::Start => picks.start = vec![p],
            Pick::Exit => picks.exit = vec![p],
        }
    }
    params
}

fn contour_recipe(pick: Option<(Pick, Point2)>) -> CutRecipe {
    CutRecipe {
        name: "Box contour".into(),
        tool: TOOL.into(),
        material: None,
        params: contour_with(pick),
    }
}

fn refuses_command(which: Pick, at: Point2) {
    let (world, _) = job();
    let recipe = contour_recipe(Some((which, at)));
    let err = ApplyCutRecipe::new(&world, &recipe, "New", SrcRef::Stock)
        .err()
        .unwrap_or_else(|| panic!("{which:?} pick refused"))
        .to_string();
    assert!(err.contains("Box contour") && err.contains("saved geometry"), "{which:?}: {err}");
    assert!(model::find_by_name(&world, "New").is_none());
}

#[test]
fn cut_recipe_refuses_a_saved_contour_start_point() {
    refuses_command(Pick::Start, Point2::new(35.0, 0.0));
}

#[test]
fn cut_recipe_refuses_a_saved_contour_exit_point() {
    refuses_command(Pick::Exit, Point2::new(0.0, 25.0));
}

#[test]
fn cut_recipe_applies_a_contour_without_saved_picks() {
    let (world, mut history) = job();
    let recipe = contour_recipe(None);
    let cmd = ApplyCutRecipe::new(&world, &recipe, "New", SrcRef::Stock).expect("no picks: applies");
    history.execute(&world, cmd);
    assert!(model::find_by_name(&world, "New").is_some());
}

fn refuses_mcp(which: Pick, at: [f64; 2]) {
    let dir = std::env::temp_dir().join(format!("camcraft-recipe-pick-{}-{}", which.key(), std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let lib = dir.join("recipes.camrecipes").to_string_lossy().into_owned();
    let mut server = Server::new();
    ok(&mut server, "set_stock", json!({"box": {"dx": 60.0, "dy": 40.0, "dz": 20.0}}));
    ok(&mut server, "add_tool", json!({"catalog": TOOL}));
    ok(&mut server, "add_operation", json!({"kind": "Contour", "tool": TOOL, "name": "C"}));
    let patch = json!({"name": "C", "params": {"picks": {which.key(): [at]}}});
    ok(&mut server, "edit_operation", patch);
    ok(
        &mut server,
        "save_cut_recipe",
        json!({"operation": "C", "name": "Box contour", "library": lib}),
    );
    let refused = call(
        &mut server,
        "apply_cut_recipe",
        json!({"recipe": "Box contour", "name": "New", "library": lib}),
    );
    assert_eq!(refused["isError"], json!(true), "{which:?}: {refused}");
    let text = refused["content"][0]["text"].as_str().expect("error text");
    assert!(text.contains("Box contour") && text.contains("saved geometry"), "{which:?}: {text}");
    let job = ok(&mut server, "get_job", json!({}));
    assert!(
        !job["operations"]
            .as_array()
            .expect("operations")
            .iter()
            .any(|op| op["name"] == json!("New")),
        "{job}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cut_recipe_mcp_refuses_a_saved_contour_start_point() {
    refuses_mcp(Pick::Start, [35.0, 0.0]);
}

#[test]
fn cut_recipe_mcp_refuses_a_saved_contour_exit_point() {
    refuses_mcp(Pick::Exit, [0.0, 25.0]);
}
