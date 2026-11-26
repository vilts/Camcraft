//! Deburr cache inputs follow the selected-edge law and all exclusion models.
//! Provenance: our own project-cache and model-command contracts (#1888).
//! Gate map (#1888): covers app::operation (stamp, deburr_picks), app::project.

use camcraft_app::commands::{
    AddTool, CreateOperation, DeleteEntity, GenerateOperation, History, ImportModel, MoveModel,
    SetStock, UpdateModel,
};
use camcraft_app::geometry::{Point3, cube, feature};
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::{model, posting, project};
use flecs_ecs::prelude::World;
use serde_json::Value;

const CONE: &str = "CM-0600-90";
// Captured with the pre-#1888 selection law, with all filters at rest.
const LEGACY_SELECTED_STAMP: u64 = 10_384_171_768_072_239_596;
// Captured again at the #2901 salt (v3 -> v4): the salt moves every
// stamp once; the automatic deburr's stamp still holds no filter word.
const LEGACY_AUTOMATIC_STAMP: u64 = 8_060_236_086_598_552_561;

fn generated(selected: bool, excluded: bool) -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let mesh = || {
        cube(
            Point3::new(-20.0, -20.0, 0.0),
            Point3::new(20.0, 20.0, 10.0),
        )
    };
    h.execute(&world, ImportModel::from_mesh("plate", mesh()));
    h.execute(
        &world,
        SetStock::new(
            &world,
            camcraft_app::stock::StockShape::FromModel {
                source: "plate".into(),
            },
            Default::default(),
        )
        .unwrap(),
    );
    let tool = camcraft_app::tools::catalog()
        .into_iter()
        .find(|(n, _)| *n == CONE)
        .unwrap()
        .1;
    h.execute(&world, AddTool::new(&world, CONE, tool).unwrap());
    let src = if selected {
        SrcRef::Selection(vec![edge(&world, "plate")])
    } else {
        SrcRef::Entity("plate".into())
    };
    let exclude_edges = if excluded {
        h.execute(&world, ImportModel::from_mesh("exclusion", mesh()));
        vec![edge(&world, "exclusion")]
    } else {
        Vec::new()
    };
    let params = OpParams::Deburr {
        width: 1.0,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(9.5),
        bottom_z: Height::fixed(8.5),
        feed: 500.0,
        plunge_feed: 100.0,
        exclude_edges,
        min_edge_angle_deg: 0.0,
        exclude_holes: false,
        hole_min_diameter: 0.0,
        hole_max_diameter: 0.0,
        unreachable: camcraft_app::finishing3d::UnreachableEdges::SkipWithNote,
        sizing: camcraft_app::finishing3d::DeburrBreak::default(),
        picks: Default::default(),
    };
    h.execute(
        &world,
        CreateOperation::new(&world, "DB", CONE, src, params).unwrap(),
    );
    h.execute(&world, GenerateOperation::new(&world, "DB").unwrap());
    assert_eq!(status(&world), OpStatus::Ok);
    (world, h)
}

fn edge(world: &World, name: &str) -> GeoSel {
    let entity = model::find_by_name(world, name).unwrap();
    let features = model::features_of(world, &entity).unwrap();
    let chain = feature::chains(&features)
        .into_iter()
        .find(|c| {
            feature::chain_points(&features, c)
                .iter()
                .all(|p| (p[1] + 20.0).abs() < 1e-6 && (p[2] - 10.0).abs() < 1e-6)
        })
        .unwrap();
    GeoSel {
        entity: name.into(),
        feature: FeatureSel::Chain(chain.id),
    }
}

fn status(world: &World) -> OpStatus {
    operation::status_of(world, &model::find_by_name(world, "DB").unwrap())
}

fn saved(world: &World) -> Value {
    serde_json::from_str(&project::save_text(world).unwrap()).unwrap()
}

fn op(file: &mut Value) -> &mut Value {
    &mut file["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|e| e["name"] == "DB")
        .unwrap()["operation"]
}

fn refuses_stale(world: &World) {
    assert_eq!(status(world), OpStatus::Stale);
    let error = posting::post_setup_unwritten(world).expect_err("a stale cache must not post");
    assert!(
        error.contains("DB") && error.contains("regenerate"),
        "{error}"
    );
}

#[test]
fn a_legacy_chain_selected_cache_requires_regeneration_before_posting() {
    let (selected, _) = generated(true, false);
    let mut file = saved(&selected);
    let (automatic, _) = generated(false, false);
    let mut old = saved(&automatic);
    // The old selection law cached the automatic ride over all four edges.
    op(&mut file)["cache"] = op(&mut old)["cache"].clone();
    op(&mut file)["stamp"] = LEGACY_SELECTED_STAMP.into();
    let reopened = camcraft_app::world::new_world();
    project::open(&reopened, &file.to_string()).unwrap();
    let entity = model::find_by_name(&reopened, "DB").unwrap();
    assert!(
        model::toolpath_of(&reopened, &entity)
            .unwrap()
            .path
            .breaks
            .iter()
            .flat_map(|b| &b.edge)
            .any(|p| (p.y - 20.0).abs() < 1e-6),
        "the legacy automatic ride also breaks the unselected far edge"
    );
    refuses_stale(&reopened);
    let mut h = History::default();
    h.execute(&reopened, GenerateOperation::new(&reopened, "DB").unwrap());
    assert_eq!(status(&reopened), OpStatus::Ok);
    assert_eq!(
        model::toolpath_of(&reopened, &entity)
            .unwrap()
            .path
            .breaks
            .len(),
        1
    );
    assert!(
        model::toolpath_of(&reopened, &entity)
            .unwrap()
            .path
            .breaks
            .iter()
            .flat_map(|b| &b.edge)
            .all(|p| (p.y + 20.0).abs() < 1e-6)
    );
    posting::post_setup_unwritten(&reopened).expect("the regenerated selected ride can post");
}

#[test]
fn an_automatic_legacy_deburr_cache_keeps_its_stamp() {
    let (world, _) = generated(false, false);
    let mut file = saved(&world);
    assert_eq!(op(&mut file)["stamp"], Value::from(LEGACY_AUTOMATIC_STAMP));
    let reopened = camcraft_app::world::new_world();
    project::open(&reopened, &file.to_string()).unwrap();
    assert_eq!(status(&reopened), OpStatus::Ok);
}

#[test]
fn moving_an_exclusion_model_stales_the_deburr_cache() {
    let (world, mut h) = generated(false, true);
    let at = Location {
        pos: Point3::new(0.0, 5.0, 0.0),
        ..Default::default()
    };
    h.execute(&world, MoveModel::new(&world, "exclusion", at).unwrap());
    refuses_stale(&world);
    assert!(h.undo(&world));
    assert_eq!(status(&world), OpStatus::Ok);
}

#[test]
fn changing_an_exclusion_model_stales_the_deburr_cache() {
    let (world, mut h) = generated(false, true);
    let mesh = cube(
        Point3::new(-20.0, -20.0, 0.0),
        Point3::new(20.0, 20.0, 12.0),
    );
    h.execute(&world, UpdateModel::new(&world, "exclusion", mesh).unwrap());
    refuses_stale(&world);
    assert!(h.undo(&world));
    assert_eq!(status(&world), OpStatus::Ok);
}

#[test]
fn deleting_an_exclusion_model_stales_the_deburr_cache() {
    let (world, mut h) = generated(false, true);
    h.execute(&world, DeleteEntity::new(&world, "exclusion").unwrap());
    refuses_stale(&world);
    assert!(h.undo(&world));
    assert_eq!(status(&world), OpStatus::Ok);
}
