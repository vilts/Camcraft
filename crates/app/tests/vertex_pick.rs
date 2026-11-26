//! M13.4 vertex picks: a feature corner as a geometry selection - its
//! stable id, the pick command and its undo, the file round-trip, and
//! older files that never carried the tag.
//!
//! Gate map (M13.4): covers app::geometry::feature (vertex ids),
//! app::commands::operation (SetOperationGeometry), app::project.

use std::path::PathBuf;

use camcraft_app::commands::{AddTool, CreateOperation, History, ImportModel, SetOperationGeometry, SetStock};
use camcraft_app::geometry::feature;
use camcraft_app::geometry::{IDENTITY_Q, Point3, cube};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel, Height, OpParams, SrcRef};
use camcraft_app::project::{self, FeatureSelDto};
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::world::new_world;

fn catalog_tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry")
}

fn face_params() -> OpParams {
    OpParams::Face { holes: Default::default(),
        tilt: None,
        blend_turns: false, blend_radius: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(4.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

/// A 20 mm block with a Face op on the stock source.
fn block_world() -> (flecs_ecs::prelude::World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", catalog_tool("EM-0400-2FL")).expect("add tool"));
    history.execute(
        &world,
        ImportModel::from_mesh("block", cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 10.0))),
    );
    let stock = StockShape::Box { dx: 24.0, dy: 24.0, dz: 12.0 };
    let at = Location { pos: Point3::new(-2.0, -2.0, 0.0), rot: IDENTITY_Q };
    history.execute(&world, SetStock::new(&world, stock, at).expect("stock"));
    history.execute(
        &world,
        CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face_params()).expect("create"),
    );
    (world, history)
}

/// The id of the block's corner at `at`.
fn corner_id(world: &flecs_ecs::prelude::World, at: [f64; 3]) -> u64 {
    let e = model::find_by_name(world, "block").expect("block");
    let f = model::features_of(world, &e).expect("features");
    feature::corner_verts(&f)
        .into_iter()
        .find(|&vi| f.verts[vi as usize] == at)
        .and_then(|vi| feature::vertex_id(&f, vi))
        .expect("the corner is a feature vertex")
}

#[test]
fn a_box_has_eight_distinct_resolvable_corners() {
    let (world, _) = block_world();
    let e = model::find_by_name(&world, "block").expect("block");
    let f = model::features_of(&world, &e).expect("features");
    let corners = feature::corner_verts(&f);
    assert_eq!(corners.len(), 8, "a box's feature edges meet at its eight corners");
    let ids = feature::vertex_ids(&f);
    assert_eq!(ids.len(), 8, "every corner keys to its own id");
    for vi in corners {
        let id = feature::vertex_id(&f, vi).expect("weldable");
        assert_eq!(feature::resolve_vertex(&f, id), Some(vi), "the id resolves back to its corner");
    }
    assert_eq!(feature::resolve_vertex(&f, 0xdead_beef), None, "a foreign id resolves to nothing");
}

#[test]
fn vertex_pick_command_round_trips_through_undo() {
    let (world, mut history) = block_world();
    let id = corner_id(&world, [20.0, 20.0, 10.0]);
    let pick = GeoSel { entity: "block".into(), feature: FeatureSel::Vertex(id) };
    history.execute(&world, SetOperationGeometry::new(&world, "Face1", vec![pick.clone()]).expect("a live corner is a valid pick"));
    let src = |w: &flecs_ecs::prelude::World| {
        let e = model::find_by_name(w, "Face1").expect("op");
        model::operation_of(w, &e).expect("operation").src
    };
    assert_eq!(src(&world), SrcRef::Selection(vec![pick.clone()]));
    assert!(history.undo(&world), "the pick is undoable");
    assert_eq!(src(&world), SrcRef::Stock, "undo restores the stock source");
    assert!(history.redo(&world));
    assert_eq!(src(&world), SrcRef::Selection(vec![pick]), "redo re-applies the corner");

    // A corner the model does not have is refused at the door.
    let ghost = GeoSel { entity: "block".into(), feature: FeatureSel::Vertex(0xdead_beef) };
    assert!(SetOperationGeometry::new(&world, "Face1", vec![ghost]).is_err(), "a stale vertex id is refused");
}

fn tmp_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("camcraft_vertex_{tag}_{}.camproj", std::process::id()))
}

#[test]
fn vertex_selection_round_trips_through_the_file() {
    let (world, mut history) = block_world();
    let id = corner_id(&world, [0.0, 0.0, 10.0]);
    let pick = GeoSel { entity: "block".into(), feature: FeatureSel::Vertex(id) };
    history.execute(&world, SetOperationGeometry::new(&world, "Face1", vec![pick.clone()]).expect("pick"));
    let path = tmp_path("rt");
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(first.contains("\"vertex\""), "the corner pick is in the file: {first}");

    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "a vertex pick round-trips byte-identically");
    let e = model::find_by_name(&world2, "Face1").expect("op loaded");
    assert_eq!(model::operation_of(&world2, &e).expect("op").src, SrcRef::Selection(vec![pick]));
    // The loaded block recomputes features and the id still resolves.
    let b = model::find_by_name(&world2, "block").expect("block");
    let f = model::features_of(&world2, &b).expect("features");
    assert!(feature::resolve_vertex(&f, id).is_some(), "the saved id names the same corner");
}

#[test]
fn older_feature_tags_still_parse() {
    // Files written before M13.4 only ever carry face/chain/patch: the
    // additive variant leaves their encoding exactly as it was.
    for (text, want) in [
        (r#"{"face":{"id":7}}"#, FeatureSelDto::Face { id: 7 }),
        (r#"{"chain":{"id":8}}"#, FeatureSelDto::Chain { id: 8 }),
        (r#"{"patch":{"id":9}}"#, FeatureSelDto::Patch { id: 9 }),
        (r#"{"vertex":{"id":10}}"#, FeatureSelDto::Vertex { id: 10 }),
    ] {
        let got: FeatureSelDto = serde_json::from_str(text).expect("parses");
        assert_eq!(got, want);
        assert_eq!(serde_json::to_string(&want).expect("writes"), text, "encoding unchanged");
    }
}
