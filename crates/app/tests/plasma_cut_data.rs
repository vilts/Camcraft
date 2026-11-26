//! #1521 slice 1: a torch's plasma recipes at the command, project file and tool library doors.
//! Provenance: our own synthetic fixtures from the issue's data rules; no cut chart was copied.
//! Gate map (#1521): covers app::commands (tool), app::project (cutting_rows_dto, file),
//! app::tool_library, app::cutting_rows.

use camcraft_app::commands::{AddTool, ChangeTool, History, ImportTools};
use camcraft_app::model;
use camcraft_app::project;
use camcraft_app::tool_cutting_rows::{CuttingRow, plasma_row_for};
use camcraft_app::tool_library::{export_tool_library, parse_tool_library};
use camcraft_app::tools::{Tool, ToolKind, WorkMaterial, torch_tool};
use camcraft_app::world::new_world;
use camcraft_app::plasma_cut_tools::{PlasmaCutData, PuddleJump};
use flecs_ecs::prelude::World;

fn recipe(thickness: f64) -> CuttingRow {
    CuttingRow {
        plasma: Some(PlasmaCutData {
            thickness,
            kerf: 1.5,
            cut_height: Some(1.5),
            pierce_height: Some(3.8),
            pierce_delay: Some(0.5),
            transfer_height: Some(2.5),
            puddle_jump: Some(PuddleJump { percent: 50.0, dwell: 0.2 }),
            amperage: Some(45.0),
            arc_voltage: None,
            max_pierce_thickness: Some(12.0),
            edge_start: false,
        }),
        ..CuttingRow::plain(Some(WorkMaterial::MildSteel), 0.0, 2400.0)
    }
}

fn torch() -> Tool {
    Tool { cutting: vec![recipe(3.0), recipe(6.0)], ..torch_tool(1.5) }
}

fn read_tool(world: &World) -> Tool {
    model::find_by_name(world, "Torch").and_then(|e| model::tool_of(world, &e)).expect("Torch")
}

fn edit(row: CuttingRow, f: impl FnOnce(&mut PlasmaCutData)) -> CuttingRow {
    let mut p = row.plasma.expect("recipe");
    f(&mut p);
    CuttingRow { plasma: Some(p), ..row }
}

#[test]
fn plasma_recipe_commands_refuse_bad_rows_by_tool_name_and_undo_restores() {
    let w = new_world();
    let mut h = History::default();
    h.execute(&w, AddTool::new(&w, "Torch", torch()).expect("a sane torch with two recipes"));
    assert_eq!(read_tool(&w), torch());
    for (rows, word) in [
        (vec![edit(recipe(16.0), |_| ())], "pierce capacity"),
        (vec![recipe(3.0), recipe(3.0)], "twice"),
        (vec![edit(recipe(3.0), |p| p.pierce_height = Some(-1.0))], "pierce height"),
        (vec![edit(recipe(3.0), |p| p.pierce_height = None)], "puddle jump"),
        (vec![CuttingRow { feed: 0.0, ..recipe(3.0) }], "feed"),
    ] {
        let bad = Tool { cutting: rows, ..torch() };
        for said in [
            AddTool::new(&w, "Other", bad.clone()).err().expect("refused").to_string(),
            ChangeTool::new(&w, "Torch", bad).err().expect("refused").to_string(),
        ] {
            assert!(said.contains(word) && said.contains("tool '"), "{word}: {said}");
        }
    }
    let mill = Tool { cutting: vec![recipe(3.0)], ..camcraft_app::tools::catalog().remove(0).1 };
    assert!(AddTool::new(&w, "Mill", mill).err().expect("refused").to_string().contains("requires a plasma torch"));
    assert_eq!(read_tool(&w), torch(), "a refused command changes nothing");

    let edge = Tool { cutting: vec![recipe(3.0), edit(recipe(16.0), |p| p.edge_start = true)], ..torch() };
    h.execute(&w, ChangeTool::new(&w, "Torch", edge.clone()).expect("an edge-start recipe above the capacity is kept"));
    assert_eq!(read_tool(&w), edge);
    h.undo(&w);
    assert_eq!(read_tool(&w), torch(), "undo restores the previous recipes");
    h.redo(&w);
    assert_eq!(read_tool(&w), edge);
}

#[test]
fn plasma_recipes_roundtrip_project_and_library_and_never_seed_by_material_alone() {
    let w = new_world();
    let mut h = History::default();
    h.execute(&w, AddTool::new(&w, "Torch", torch()).unwrap());
    let original = read_tool(&w);
    assert_eq!(original.kind, ToolKind::Torch);
    // The recipe's feed is for one thickness: the material alone never picks it.
    let in_force = camcraft_app::cutting_rows::row_in(&original, Some(WorkMaterial::MildSteel)).expect("Unknown row");
    assert!(in_force.is_unknown() && in_force.feed == original.feed, "{in_force:?}");
    assert!(camcraft_app::cutting_rows::seeded_for(&original, Some(WorkMaterial::MildSteel)).is_none());
    assert_eq!(plasma_row_for(&original.cutting, WorkMaterial::MildSteel, 6.0), Some(&recipe(6.0)));

    let data = project::tool_to_json(&original);
    let written = &data["cutting_data"][1]["plasma"];
    assert_eq!(written["thickness"], 6.0);
    assert_eq!(written["puddle_jump"]["percent"], 50.0);
    assert!(written.get("arc_voltage").is_none(), "a blank value writes no key: {written}");
    assert_eq!(project::tool_from_json(&data).unwrap(), original);

    let saved = project::save_text(&w).unwrap();
    let again = new_world();
    project::open(&again, &saved).unwrap();
    assert_eq!(read_tool(&again), original);

    let library = export_tool_library(&w).unwrap();
    let imported = new_world();
    let command = ImportTools::new(&imported, parse_tool_library(&library).unwrap()).unwrap();
    History::default().execute(&imported, command);
    assert_eq!(read_tool(&imported), original);
}

#[test]
fn plasma_recipe_files_refuse_a_misspelt_key_and_an_unpierceable_plate() {
    let mut data = project::tool_to_json(&torch());
    data["cutting_data"][0]["plasma"]["peirce_height"] = serde_json::json!(3.0);
    assert!(project::tool_from_json(&data).is_err(), "a misspelt recipe key is refused, never dropped");

    let w = new_world();
    History::default().execute(&w, AddTool::new(&w, "Torch", torch()).unwrap());
    let saved = project::save_text(&w).unwrap();
    assert!(saved.contains("\"max_pierce_thickness\": 12.0"), "the fixture states its capacity as the edit below expects");
    let thick = saved.replace("\"max_pierce_thickness\": 12.0", "\"max_pierce_thickness\": 2.0");
    let said = project::open(&new_world(), &thick).expect_err("refused").to_string();
    assert!(said.contains("pierce capacity"), "{said}");
}
