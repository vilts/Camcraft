//! Recipe safety regressions for PR #3399.
//! Gate map: covers app::commands::operation::recipe and app::mcp.
//! Provenance: original fixtures and command contracts.

use super::common::solid_part::{circle, plate, rect};
use super::*;
use camcraft_app::commands::ImportModel;
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3};
use camcraft_app::operation::{OpType, seed_params};

#[test]
fn cut_recipe_with_a_named_material_requires_a_destination_material() {
    let (world, mut history) = job();
    history.execute(
        &world,
        SetStockMaterial::new(&world, Some(WorkMaterial::Aluminium6061)).expect("aluminium"),
    );
    let recipe = CutRecipe::capture(&world, "Face1", "Aluminium face").expect("capture");
    history.execute(&world, SetStockMaterial::new(&world, None).expect("clear material"));
    let err = ApplyCutRecipe::new(&world, &recipe, "Face2", SrcRef::Stock)
        .err()
        .expect("refused")
        .to_string();
    assert!(err.contains("Aluminium face") && err.contains("no material"), "{err}");
    assert!(model::find_by_name(&world, "Face2").is_none());

    history.execute(
        &world,
        SetStockMaterial::new(&world, Some(WorkMaterial::Aluminium6061)).expect("matching material"),
    );
    history.execute(
        &world,
        ApplyCutRecipe::new(&world, &recipe, "Face2", SrcRef::Stock).expect("matching material applies"),
    );
    assert!(model::find_by_name(&world, "Face2").is_some());
}

fn saved_hole() -> Hole {
    Hole {
        xy: Point2::new(0.0, 0.0),
        radius: 4.0,
        entry_z: 10.0,
        floor_z: 0.0,
        through: true,
        stepped: false,
    }
}

#[test]
fn cut_recipe_does_not_transfer_holes_to_a_different_source() {
    let (world, mut history) = job();
    for (name, x) in [("old", 0.0), ("new", 10.0)] {
        let mut facets = Vec::new();
        plate(
            &rect(-20.0, -20.0, 20.0, 20.0),
            Some((&circle((x, 0.0), 4.0, 48), 0.0)),
            0.0,
            10.0,
            &mut facets,
        );
        history.execute(&world, ImportModel::from_mesh(name, Mesh::from_facets(facets)));
    }
    let drill = catalog()
        .into_iter()
        .find(|(n, _)| *n == "DR-0600")
        .map(|(_, t)| t)
        .expect("drill");
    history.execute(&world, AddTool::new(&world, "DR-0600", drill).expect("add drill"));
    let mut params = seed_params(OpType::Drill.ordinal());
    if let OpParams::Drill { holes, cycle, .. } = &mut params {
        *holes = vec![saved_hole()];
        *cycle = CycleChoice::Drill;
    }
    history.execute(
        &world,
        CreateOperation::new(&world, "Drill old", "DR-0600", SrcRef::Entity("old".into()), params)
            .expect("create drill"),
    );
    let recipe = CutRecipe::capture(&world, "Drill old", "Plate drilling").expect("capture drill");
    let err = ApplyCutRecipe::new(&world, &recipe, "Drill new", SrcRef::Entity("new".into()))
        .err()
        .expect("saved holes refused")
        .to_string();
    assert!(
        err.contains("Plate drilling") && err.contains("saved geometry"),
        "{err}"
    );
    assert!(model::find_by_name(&world, "Drill new").is_none());
}

#[test]
fn cut_recipe_refuses_other_saved_coordinates_by_name() {
    let (world, _) = job();
    for kind in [
        OpType::Bore,
        OpType::Thread,
        OpType::Circular,
        OpType::Slot,
        OpType::TrochoidalSlot,
        OpType::Probe,
        OpType::SlotUndercut,
        OpType::WireContour,
        OpType::SinkerBurn,
        OpType::SinkerSideBurn,
    ] {
        let mut params = seed_params(kind.ordinal());
        match &mut params {
            OpParams::Bore { holes, .. } | OpParams::Thread { holes, .. } => *holes = vec![saved_hole()],
            _ => {}
        }
        let recipe = CutRecipe {
            name: format!("Saved {kind:?}"),
            tool: TOOL.into(),
            material: None,
            params,
        };
        let err = ApplyCutRecipe::new(&world, &recipe, "New", SrcRef::Stock)
            .err()
            .expect("geometry refused")
            .to_string();
        assert!(
            err.contains(&recipe.name) && err.contains("saved geometry"),
            "{kind:?}: {err}"
        );
        assert!(model::find_by_name(&world, "New").is_none());
    }
}

#[test]
fn cut_recipe_refuses_saved_picks_and_plane_origins() {
    use camcraft_app::geometry::sketch_chains::ChainId;
    use camcraft_app::operation::{ChainOverride, FeatureSel, GeoSel, Tilt};
    let (world, _) = job();
    let mut contour = seed_params(OpType::Contour.ordinal());
    if let OpParams::Contour { chain_overrides, .. } = &mut contour {
        chain_overrides.push(ChainOverride {
            entity: "old".into(),
            chain: ChainId(1),
            side: None,
            reverse: true,
        });
    }
    let mut deburr = seed_params(OpType::Deburr.ordinal());
    if let OpParams::Deburr { exclude_edges, .. } = &mut deburr {
        exclude_edges.push(GeoSel {
            entity: "old".into(),
            feature: FeatureSel::Chain(1),
        });
    }
    let mut tilted = face_params();
    if let OpParams::Face { tilt, .. } = &mut tilted {
        *tilt = Some(Tilt {
            basis_version: 1,
            a_deg: 0.0,
            b_deg: 0.0,
            origin: [10.0, 0.0, 0.0],
        });
    }
    let mut cross_drill = seed_params(OpType::Drill.ordinal());
    if let OpParams::Drill { holes, cross, .. } = &mut cross_drill {
        holes.clear();
        cross.push(camcraft_app::drilling::CrossHole {
            origin: Point3::new(10.0, 0.0, 0.0),
            dir: Point3::new(-1.0, 0.0, 0.0),
            radius: 2.0,
            entry_d: 0.0,
            floor_d: -6.0,
            through: false,
        });
    }
    for params in [contour, deburr, tilted, cross_drill] {
        let recipe = CutRecipe {
            name: "Old geometry".into(),
            tool: TOOL.into(),
            material: None,
            params,
        };
        let err = ApplyCutRecipe::new(&world, &recipe, "New", SrcRef::Stock)
            .err()
            .expect("geometry refused")
            .to_string();
        assert!(err.contains("Old geometry") && err.contains("saved geometry"), "{err}");
        assert!(model::find_by_name(&world, "New").is_none());
    }
}

#[test]
fn cut_recipe_mcp_refuses_an_absent_destination_material() {
    let dir = std::env::temp_dir().join(format!("camcraft-recipe-material-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let lib = dir.join("recipes.camrecipes").to_string_lossy().into_owned();
    let mut server = Server::new();
    ok(
        &mut server,
        "set_stock",
        json!({"box": {"dx": 60.0, "dy": 60.0, "dz": 20.0}}),
    );
    ok(&mut server, "add_tool", json!({"catalog": TOOL}));
    ok(&mut server, "set_stock_material", json!({"material": "aluminium_6061"}));
    ok(
        &mut server,
        "add_operation",
        json!({"kind": "Face", "tool": TOOL, "name": "Face1", "params": {"feed": 900.0}}),
    );
    ok(
        &mut server,
        "save_cut_recipe",
        json!({"operation": "Face1", "name": "Aluminium face", "library": lib}),
    );
    ok(&mut server, "set_stock_material", json!({"material": null}));
    let refused = call(
        &mut server,
        "apply_cut_recipe",
        json!({"recipe": "Aluminium face", "name": "Face2", "library": lib}),
    );
    assert_eq!(refused["isError"], json!(true), "{refused}");
    let text = refused["content"][0]["text"].as_str().expect("error text");
    assert!(
        text.contains("Aluminium face") && text.contains("no material"),
        "{text}"
    );
    let job = ok(&mut server, "get_job", json!({}));
    assert!(
        !job["operations"]
            .as_array()
            .expect("operations")
            .iter()
            .any(|op| op["name"] == json!("Face2")),
        "{job}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
