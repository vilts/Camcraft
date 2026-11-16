//! M1.1 acceptance: everything is a command, undo/redo is exact, and
//! malformed input never enters the history.
//!
//! Gate map (M4.4): covers app::commands, app::geometry, app::model, app::operation, app::schema, geometry::geometry::stock, tools::tools, app::units, app::world.

use std::path::{Path, PathBuf};

use camcraft_app::commands::{
    CommandError, DeleteEntity, History, ImportModel, SetStock, SetUnits, SetWcsOrigin,
    UpdateModel,
};
use camcraft_app::units::{self, Unit};
use camcraft_app::geometry::{cube, Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::schema::{Location, STOCK_NAME};
use camcraft_app::stock::{CYLINDER_SEGMENTS, StockShape};
use camcraft_app::world::new_world;

fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name)
}

fn cube_mesh() -> Mesh {
    cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0))
}

fn facet_count(world: &flecs_ecs::prelude::World, name: &str) -> Option<usize> {
    model::find_by_name(world, name).and_then(|e| model::facets_of(world, &e).map(|t| t.len()))
}

#[test]
fn import_undo_redo_roundtrip() {
    let world = new_world();
    let mut history = History::default();

    history.execute(&world, ImportModel::from_mesh("cube1", cube_mesh()));
    assert_eq!(facet_count(&world, "cube1"), Some(12));

    assert!(history.undo(&world));
    assert_eq!(facet_count(&world, "cube1"), None);

    assert!(history.redo(&world));
    assert_eq!(facet_count(&world, "cube1"), Some(12));

    assert!(history.can_undo()); // the command is back on the done stack
    assert!(!history.can_redo());
}

#[test]
fn step_import_undo_redo_roundtrip() {
    let world = new_world();
    let mut history = History::default();

    let command = ImportModel::new(&testdata("box.step")).expect("box.step must parse");
    history.execute(&world, command);
    assert_eq!(facet_count(&world, "box"), Some(12));

    assert!(history.undo(&world));
    assert_eq!(facet_count(&world, "box"), None);

    assert!(history.redo(&world));
    assert_eq!(facet_count(&world, "box"), Some(12));
}

#[test]
fn iges_fails_at_construction_with_honest_error() {
    // Construction must refuse by type, not by a failed filesystem read.
    let err = ImportModel::new(Path::new("model.iges")).err();
    assert!(matches!(err, Some(CommandError::UnsupportedType(_))), "{err:?}");
}

#[test]
fn delete_restores_state_exactly() {
    let world = new_world();
    let mut history = History::default();
    let original = cube_mesh().facets;

    history.execute(&world, ImportModel::from_mesh("cube2", cube_mesh()));
    let command = DeleteEntity::new(&world, "cube2").expect("entity exists");
    history.execute(&world, command);
    assert_eq!(facet_count(&world, "cube2"), None);

    history.undo(&world);
    let restored = model::find_by_name(&world, "cube2").and_then(|e| model::facets_of(&world, &e));
    assert_eq!(restored, Some(original));
}

#[test]
fn fresh_command_clears_redo_stack() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("a", cube_mesh()));
    history.undo(&world);
    assert!(history.can_redo());
    history.execute(&world, ImportModel::from_mesh("b", cube_mesh()));
    assert!(!history.can_redo());
}

#[test]
fn empty_history_is_polite() {
    let world = new_world();
    let mut history = History::default();
    assert!(!history.undo(&world));
    assert!(!history.redo(&world));
    assert!(history.is_empty());
}

#[test]
fn delete_missing_entity_fails_at_construction() {
    let world = new_world();
    let result = DeleteEntity::new(&world, "ghost");
    assert!(matches!(result, Err(CommandError::NotFound(_))));
}

#[test]
fn corrupt_import_fails_at_construction() {
    let path = std::env::temp_dir().join(format!("camcraft_corrupt_{}.stl", std::process::id()));
    if std::fs::write(&path, b"garbage not an stl at all \x00\xff").is_err() {
        return; // no temp dir on this machine; the corpus tests cover parsing
    }
    let result = ImportModel::new(&path);
    let _ = std::fs::remove_file(&path);
    match result {
        Err(CommandError::Import(_)) => {}
        Err(other) => panic!("expected Import error, got {other:?}"),
        Ok(_) => panic!("garbage must not construct a command"),
    }
}

/// Everything undo must bring back for stock: parameters, placement, facets.
fn stock_state(
    world: &flecs_ecs::prelude::World,
) -> Option<(StockShape, Location, std::sync::Arc<[camcraft_app::geometry::Triangle]>)> {
    let entity = model::find_by_name(world, STOCK_NAME)?;
    let shape = model::stock_def_of(world, &entity)?;
    Some((
        shape,
        model::transform_of(world, &entity).unwrap_or_default(),
        model::facets_of(world, &entity).unwrap_or_default(),
    ))
}

#[test]
fn stock_create_undo_redo_is_exact() {
    let world = new_world();
    let mut history = History::default();
    let cyl = StockShape::Cylinder {
        diameter: 25.4,
        height: 8.0,
    };

    history.execute(
        &world,
        SetStock::new(&world, cyl.clone(), Location::default()).expect("sane cylinder"),
    );
    assert_eq!(facet_count(&world, STOCK_NAME), Some(4 * CYLINDER_SEGMENTS as usize));
    let after = stock_state(&world).expect("stock exists");
    assert_eq!(after.0, cyl);

    assert!(history.undo(&world));
    assert_eq!(stock_state(&world), None, "undo removes created stock");

    assert!(history.redo(&world));
    // Deterministic meshing makes the redo byte-identical, not just similar.
    assert_eq!(stock_state(&world), Some(after));
}

#[test]
fn stock_replace_undo_restores_previous_params() {
    let world = new_world();
    let mut history = History::default();
    let box_a = StockShape::Box {
        dx: 60.0,
        dy: 60.0,
        dz: 20.0,
    };
    let box_b = StockShape::Box {
        dx: 40.0,
        dy: 40.0,
        dz: 30.0,
    };
    let at = |z: f64| Location {
        pos: Point3::new(0.0, 0.0, z),
        rot: camcraft_app::geometry::IDENTITY_Q,
    };

    history.execute(&world, SetStock::new(&world, box_a.clone(), at(0.0)).unwrap());
    let before = stock_state(&world).expect("stock a");
    history.execute(&world, SetStock::new(&world, box_b.clone(), at(-2.0)).unwrap());
    assert_eq!(stock_state(&world).unwrap().0, box_b);

    assert!(history.undo(&world));
    assert_eq!(stock_state(&world), Some(before));
    assert!(history.redo(&world));
    assert_eq!(stock_state(&world).unwrap().0, box_b);
}

#[test]
fn stock_validates_at_construction() {
    let world = new_world();
    // NaN and absurd dimensions are refused; nothing reaches the world.
    assert!(matches!(
        SetStock::new(
            &world,
            StockShape::Box {
                dx: f64::NAN,
                dy: 1.0,
                dz: 1.0
            },
            Location::default()
        ),
        Err(CommandError::Invalid(_))
    ));
    assert!(matches!(
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 5e6,
                dy: 1.0,
                dz: 1.0
            },
            Location::default()
        ),
        Err(CommandError::Invalid(_))
    ));
    assert_eq!(model::find_by_name(&world, STOCK_NAME), None);
    // A non-finite placement is refused too.
    assert!(matches!(
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 1.0,
                dy: 1.0,
                dz: 1.0
            },
            Location {
                pos: Point3::new(0.0, f64::INFINITY, 0.0),
                rot: camcraft_app::geometry::IDENTITY_Q,
            }
        ),
        Err(CommandError::Invalid(_))
    ));
}

#[test]
fn stock_from_model_copies_geometry_and_placement() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("src", cube_mesh()));
    let src = model::find_by_name(&world, "src").expect("imported");
    let src_tris = model::facets_of(&world, &src).expect("facets");

    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::FromModel { source: "src".into() },
            Location::default(),
        )
        .expect("copyable source"),
    );
    let (shape, _, facets) = stock_state(&world).expect("stock exists");
    assert_eq!(shape, StockShape::FromModel { source: "src".into() });
    assert_eq!(facets, src_tris, "stock geometry mirrors the source");
    // An identity request adopts the source's placement, keeping it put.
    assert_eq!(
        model::transform_of(&world, &model::find_by_name(&world, STOCK_NAME).unwrap()),
        model::transform_of(&world, &src)
    );

    assert!(matches!(
        SetStock::new(
            &world,
            StockShape::FromModel {
                source: "ghost".into()
            },
            Location::default()
        ),
        Err(CommandError::NotFound(_))
    ));
    assert_eq!(facet_count(&world, STOCK_NAME), Some(12), "failed command changed nothing");
}

#[test]
fn wcs_lazy_create_undo_removes() {
    let world = new_world();
    let mut history = History::default();
    let at = Location {
        pos: Point3::new(10.0, -5.0, 2.5),
        rot: camcraft_app::geometry::IDENTITY_Q,
    };

    history.execute(&world, SetWcsOrigin::new(&world, at).unwrap());
    let e = model::find_by_name(&world, "G54").expect("the WCS entity");
    assert!(model::is_wcs(&world, &e));
    assert_eq!(model::transform_of(&world, &e), Some(at));
    assert_eq!(model::facets_of(&world, &e), None, "offsets never have geometry");

    assert!(history.undo(&world));
    assert_eq!(model::find_by_name(&world, "G54"), None);
    assert!(history.redo(&world));
    assert!(model::is_wcs(&world, &model::find_by_name(&world, "G54").unwrap()));
}

#[test]
fn wcs_validates_placement_and_name() {
    let world = new_world();
    let mut history = History::default();
    // #342: no slot to name any more - one WCS per setup; a degenerate
    // rotation is still refused at construction.
    assert!(matches!(
        SetWcsOrigin::new(&world, Location { rot: [0.0; 4], ..Location::default() }),
        Err(CommandError::Invalid(_))
    ));
    // A model squatting on the WCS name is a conflict, not an overwrite.
    history.execute(&world, ImportModel::from_mesh("G54", cube_mesh()));
    assert!(matches!(
        SetWcsOrigin::new(&world, Location::default()),
        Err(CommandError::NameConflict(_))
    ));
}

#[test]
fn delete_stock_undo_restores_kind() {
    let world = new_world();
    let mut history = History::default();
    let box_a = StockShape::Box {
        dx: 10.0,
        dy: 20.0,
        dz: 30.0,
    };
    history.execute(&world, SetStock::new(&world, box_a.clone(), Location::default()).unwrap());
    let before = stock_state(&world).expect("stock a");

    history.execute(&world, DeleteEntity::new(&world, STOCK_NAME).unwrap());
    assert_eq!(model::find_by_name(&world, STOCK_NAME), None);

    assert!(history.undo(&world));
    // The restored entity must be stock again, not a nameless mesh.
    assert_eq!(stock_state(&world), Some(before));
    let e = model::find_by_name(&world, STOCK_NAME).unwrap();
    assert!(model::is_stock(&world, &e));
}

// ===================== M1.5 step 5: SetOperationGeometry =====================

use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationLinks, GenerateOperation, SetOperationGeometry,
};
use camcraft_app::geometry::feature::FaceKind;
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, OpParams, SrcRef};
use camcraft_app::tools::catalog;

fn cat_tool(name: &str) -> camcraft_app::tools::Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

fn face_params() -> OpParams {
    OpParams::Face { holes: Default::default(), tilt: None, blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(4.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// Planar face whose plane sits highest along +z - the box's top. Face
/// normals are sign-canonical, so both horizontal faces report (0,0,1)
/// and only the signed offset tells top from bottom.
fn top_face(world: &flecs_ecs::prelude::World, name: &str) -> FeatureSel {
    let e = model::find_by_name(world, name).expect("entity exists");
    let f = model::features_of(world, &e).expect("features");
    FeatureSel::Face(
        f.faces
            .iter()
            .filter(|fa| matches!(fa.kind, FaceKind::Planar) && fa.normal[2].abs() > 0.9)
            .max_by(|a, b| a.offset.total_cmp(&b.offset))
            .map(|fa| fa.id)
            .expect("a horizontal planar face"),
    )
}

fn op_src(world: &flecs_ecs::prelude::World, name: &str) -> SrcRef {
    let e = model::find_by_name(world, name).expect("op exists");
    model::operation_of(world, &e).expect("operation").src
}

/// Stock → op with a tool, ready to receive geometry picks. Returns the
/// top-face pick of the imported box part.
fn selection_fixture() -> (
    flecs_ecs::prelude::World,
    History,
    FeatureSel,
) {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, AddTool::new(&world, "EM-0600-4FL", cat_tool("EM-0600-4FL")).expect("tool"));
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box { dx: 40.0, dy: 40.0, dz: 20.0 },
            Location::default(),
        )
        .expect("stock"),
    );
    let cmd = CreateOperation::new(
        &world,
        "G1",
        "EM-0600-4FL",
        SrcRef::Stock,
        face_params(),
    )
    .expect("create");
    history.execute(&world, cmd);
    let top = top_face(&world, "part");
    (world, history, top)
}

#[test]
fn geometry_pick_undo_redo_is_byte_exact() {
    let (world, mut history, top) = selection_fixture();
    let pick = GeoSel { entity: "part".into(), feature: top };

    // Undo from any base restores the exact previous component bytes,
    // stamp included - staleness is derived, never stored.
    assert_eq!(op_src(&world, "G1"), SrcRef::Stock);
    history
        .execute(&world, SetOperationGeometry::new(&world, "G1", vec![pick.clone()]).expect("pick"));
    assert_eq!(op_src(&world, "G1"), SrcRef::Selection(vec![pick.clone()]));

    assert!(history.undo(&world));
    assert_eq!(op_src(&world, "G1"), SrcRef::Stock, "undo restores the stock base");
    assert!(history.redo(&world));
    assert_eq!(op_src(&world, "G1"), SrcRef::Selection(vec![pick]));

    // Entity base: the selection replaces a named model, undo returns
    // to that model - not to Stock - byte-exact.
    let other = GeoSel { entity: STOCK_NAME.into(), feature: top_face(&world, STOCK_NAME) };
    history.execute(
        &world,
        EditOperationLinks::new(&world, "G1", None, Some(SrcRef::Entity("part".into())))
            .expect("retarget"),
    );
    history
        .execute(&world, SetOperationGeometry::new(&world, "G1", vec![other.clone()]).expect("stock-top pick"));
    assert!(history.undo(&world));
    assert_eq!(op_src(&world, "G1"), SrcRef::Entity("part".into()), "undo lands on the Entity base");
    assert!(history.redo(&world));
    assert_eq!(op_src(&world, "G1"), SrcRef::Selection(vec![other]));
}

#[test]
fn an_operation_source_is_stock_or_a_part_only() {
    // User review 2026-11-06: G54 showed up as a Source. The command
    // door refuses every named entity that is not one of the user's
    // parts - tool, operation, the stock entity by name, a work offset.
    let (world, _history, _top) = selection_fixture();
    let to = |name: &str| EditOperationLinks::new(&world, "G1", None, Some(SrcRef::Entity(name.into())));
    assert!(to("part").is_ok(), "a part is a source");
    for bad in ["EM-0600-4FL", "G1", STOCK_NAME] {
        assert!(
            matches!(to(bad), Err(CommandError::Invalid(_))),
            "'{bad}' must be refused as a source"
        );
    }
    if let Some(wcs) = model::named_entities(&world)
        .into_iter()
        .find(|(_, e)| model::is_wcs(&world, e))
        .map(|(n, _)| n)
    {
        assert!(matches!(to(&wcs), Err(CommandError::Invalid(_))), "work offset '{wcs}' is no source");
    }
}

#[test]
fn duplicate_picks_collapse_at_construction() {
    let (world, mut history, top) = selection_fixture();
    let a = GeoSel { entity: "part".into(), feature: top };
    let b = GeoSel { entity: STOCK_NAME.into(), feature: top_face(&world, STOCK_NAME) };
    let cmd = SetOperationGeometry::new(&world, "G1", vec![a.clone(), a.clone(), b.clone()])
        .expect("valid picks");
    history.execute(&world, cmd);
    assert_eq!(op_src(&world, "G1"), SrcRef::Selection(vec![a, b]), "order kept, repeats dropped");
}

#[test]
fn reselecting_the_same_geometry_stays_fresh() {
    // The stamp survives the pick command, so re-picking what an
    // already-generated op has is status-OK by derivation - a
    // different pick would flip it to Stale on its own.
    let (world, mut history, top) = selection_fixture();
    let pick = GeoSel { entity: "part".into(), feature: top };
    history.execute(
        &world,
        SetOperationGeometry::new(&world, "G1", vec![pick.clone()]).expect("pick"),
    );
    history.execute(&world, GenerateOperation::new(&world, "G1").expect("generate"));
    let op_before = {
        let e = model::find_by_name(&world, "G1").expect("G1");
        model::operation_of(&world, &e).expect("op")
    };
    history.execute(
        &world,
        SetOperationGeometry::new(&world, "G1", vec![pick.clone(), pick]).expect("re-pick"),
    );
    let e = model::find_by_name(&world, "G1").expect("G1");
    let op_after = model::operation_of(&world, &e).expect("op");
    assert_eq!(op_after, op_before, "same geometry, same stamp, same bytes");
    assert_eq!(operation::status_of(&world, &e), operation::OpStatus::Ok);
}

#[test]
fn set_geometry_refusals_never_write_the_world() {
    let (world, mut history, top) = selection_fixture();
    history.execute(&world, SetWcsOrigin::new(&world, Location::default()).expect("wcs"));
    let before = op_src(&world, "G1");

    // Op that does not exist: NotFound.
    let ghost = GeoSel { entity: "part".into(), feature: top };
    assert!(matches!(
        SetOperationGeometry::new(&world, "no-op", vec![ghost.clone()]),
        Err(CommandError::NotFound(_))
    ));
    // Empty pick list: the clearing command is a links edit, not this.
    assert!(matches!(
        SetOperationGeometry::new(&world, "G1", vec![]),
        Err(CommandError::Invalid(_))
    ));
    // Entity gone: NotFound, not a silent stale pick.
    assert!(matches!(
        SetOperationGeometry::new(
            &world,
            "G1",
            vec![GeoSel { entity: "gone".into(), feature: top }]
        ),
        Err(CommandError::NotFound(_))
    ));
    // A work offset has no geometry to select from.
    assert!(matches!(
        SetOperationGeometry::new(
            &world,
            "G1",
            vec![GeoSel { entity: "G54".into(), feature: top }]
        ),
        Err(CommandError::Invalid(_))
    ));
    // An id that resolves nowhere in the entity: refused at construction.
    assert!(matches!(
        SetOperationGeometry::new(
            &world,
            "G1",
            vec![GeoSel { entity: "part".into(), feature: FeatureSel::Face(7) }]
        ),
        Err(CommandError::Invalid(_))
    ));

    assert_eq!(op_src(&world, "G1"), before, "every refusal left the world alone");
    let _ = &history; // history untouched: nothing constructed entered it
}

#[test]
fn units_set_undo_redo_is_exact_and_no_op_refused() {
    let world = new_world();
    let mut history = History::default();
    assert_eq!(units::of(&world), Unit::Millimeters, "fresh world is mm");

    history.execute(&world, SetUnits::new(&world, Unit::Inches).expect("a real change"));
    assert_eq!(units::of(&world), Unit::Inches);

    assert!(history.undo(&world));
    assert_eq!(units::of(&world), Unit::Millimeters, "undo restores the lens");
    assert!(history.redo(&world));
    assert_eq!(units::of(&world), Unit::Inches);

    // Validate early: switching inches to inches never enters history,
    // so undo after a refused no-op still lands where the user was.
    let err = SetUnits::new(&world, Unit::Inches).expect_err("no-op must refuse");
    assert!(matches!(err, CommandError::Invalid(_)), "{err:?}");
    assert!(history.undo(&world));
    assert_eq!(units::of(&world), Unit::Millimeters);
}

// ---- M3.1 A: the model update command -------------------------------------

/// Same facet count and same AABB as the 0..1 cube, entirely different
/// walls - the exact case the old count+bounds stamp let pass in silence.
fn prism_mesh() -> Mesh {
    let (a, b, c, d) = (
        Point3::new(0.5, 0.0, 0.0),
        Point3::new(1.0, 0.5, 0.0),
        Point3::new(0.5, 1.0, 0.0),
        Point3::new(0.0, 0.5, 0.0),
    );
    let (a1, b1, c1, d1) = (
        Point3::new(0.5, 0.0, 1.0),
        Point3::new(1.0, 0.5, 1.0),
        Point3::new(0.5, 1.0, 1.0),
        Point3::new(0.0, 0.5, 1.0),
    );
    Mesh::from_facets(
        vec![
            Triangle::new(a, b, c),
            Triangle::new(a, c, d), // bottom
            Triangle::new(a1, c1, b1),
            Triangle::new(a1, d1, c1), // top
            Triangle::new(a, b, b1),
            Triangle::new(a, b1, a1), // walls
            Triangle::new(b, c, c1),
            Triangle::new(b, c1, b1),
            Triangle::new(c, d, d1),
            Triangle::new(c, d1, c1),
            Triangle::new(d, a, a1),
            Triangle::new(d, a1, d1),
        ],
    )
}

fn facets(world: &flecs_ecs::prelude::World, name: &str) -> Option<std::sync::Arc<[Triangle]>> {
    model::find_by_name(world, name).and_then(|e| model::facets_of(world, &e))
}

#[test]
fn update_undo_redo_restores_geometry_and_count_exactly() {
    let world = new_world();
    let mut history = History::default();
    let v1 = cube_mesh().facets;

    history.execute(&world, ImportModel::from_mesh("m", cube_mesh()));
    assert_eq!(
        model::find_by_name(&world, "m").map(|e| model::rev_of(&world, &e)),
        Some(1),
        "the first landing is revision 1"
    );

    history.execute(
        &world,
        UpdateModel::new(&world, "m", prism_mesh()).expect("model exists"),
    );
    assert_eq!(
        model::find_by_name(&world, "m").map(|e| model::rev_of(&world, &e)),
        Some(2),
        "geometry landing two counts"
    );
    assert_ne!(facets(&world, "m").unwrap(), v1, "the walls moved");
    assert_eq!(facet_count(&world, "m"), Some(12), "same count, different solid");

    assert!(history.undo(&world));
    assert_eq!(facets(&world, "m").unwrap(), v1, "undo restores revision 1 verbatim");
    assert_eq!(
        model::find_by_name(&world, "m").map(|e| model::rev_of(&world, &e)),
        Some(1),
        "and the counter with it"
    );

    assert!(history.redo(&world));
    assert_eq!(facets(&world, "m").unwrap(), prism_mesh().facets);
    assert_eq!(
        model::find_by_name(&world, "m").map(|e| model::rev_of(&world, &e)),
        Some(2)
    );
}

#[test]
fn update_redoes_gracefully_across_a_deleted_entity() {
    // import, update, undo, undo (the import's invert deletes the entity),
    // redo, redo: the name-keyed apply laws must land the full state -
    // the import re-spawns, the update re-finds and writes revision 2.
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("m", cube_mesh()));
    history.execute(
        &world,
        UpdateModel::new(&world, "m", prism_mesh()).expect("model exists"),
    );
    assert!(history.undo(&world));
    assert!(history.undo(&world));
    assert_eq!(model::find_by_name(&world, "m"), None, "import inverted: gone");
    assert!(history.redo(&world));
    assert!(history.redo(&world));
    assert_eq!(facets(&world, "m").unwrap(), prism_mesh().facets, "full state, not half");
    assert_eq!(
        model::find_by_name(&world, "m").map(|e| model::rev_of(&world, &e)),
        Some(2),
        "prev_rev + 1 held across the interleaving"
    );
}

#[test]
fn update_validates_at_construction() {
    let world = new_world();
    // Missing name: NotFound, like every other entity-keyed command.
    assert!(matches!(
        UpdateModel::new(&world, "ghost", cube_mesh()),
        Err(CommandError::NotFound(_))
    ));
    // Setup furniture is never an update target, whatever it is named.
    let stock = SetStock::new(
        &world,
        StockShape::Box { dx: 10.0, dy: 10.0, dz: 5.0 },
        Location::default(),
    );
    History::default().execute(&world, stock.expect("box stock"));
    assert!(matches!(
        UpdateModel::new(&world, STOCK_NAME, cube_mesh()),
        Err(CommandError::Invalid(_))
    ));
    // Empty geometry cannot land.
    history_import(&world);
    assert!(matches!(
        UpdateModel::new(&world, "m", Mesh::from_facets(vec![])),
        Err(CommandError::Invalid(_))
    ));
}

fn history_import(world: &flecs_ecs::prelude::World) {
    History::default().execute(world, ImportModel::from_mesh("m", cube_mesh()));
}

#[test]
fn update_reports_what_moved() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("m", cube_mesh()));

    // The same geometry re-imported: nothing moved, and the command says
    // so in plain words the status bar can print.
    let same = UpdateModel::new(&world, "m", cube_mesh()).expect("exists");
    assert!(same.changes().is_empty());
    assert_eq!(same.changes().describe(), "geometry unchanged");

    // Cube to prism: walls change, so faces and chains both report.
    let moved = UpdateModel::new(&world, "m", prism_mesh()).expect("exists");
    let diff = moved.changes();
    assert!(!diff.is_empty());
    assert!(!diff.faces.added.is_empty() && !diff.faces.missing.is_empty());
    let text = diff.describe();
    assert!(text.ends_with(" changed") && !text.contains("0 "), "honest counts: {text}");
}

// ===================== M5.2 G: MoveModel - part placement, full rotation =====================

use camcraft_app::commands::{MoveModel, TagFixture};
use camcraft_app::fixture::MACHINE_TABLE;
use camcraft_app::geometry::quat_from_euler;
use camcraft_app::operation::OpStatus;
use std::f64::consts::FRAC_PI_2;

fn op_status(world: &flecs_ecs::prelude::World, name: &str) -> OpStatus {
    let e = model::find_by_name(world, name).expect("entity exists");
    operation::status_of(world, &e)
}

#[test]
fn move_model_rotation_roundtrips_exact_and_the_stamp_follows() {
    let (world, mut history, _top) = selection_fixture();
    // Stand the 10 x 10 x 5 box on its edge (roll 90 deg) at x = 20:
    // the quaternion the world stores is a normalized unit, and the
    // part's own geometry is untouched - only the placement moved.
    let at = Location {
        pos: Point3::new(20.0, 0.0, 0.0),
        rot: quat_from_euler(FRAC_PI_2, 0.0, 0.0),
    };
    let e = model::find_by_name(&world, "part").expect("part");
    history.execute(&world, MoveModel::new(&world, "part", at).expect("placed"));
    let got = model::transform_of(&world, &e).expect("location");
    assert_eq!([got.pos.x, got.pos.y, got.pos.z], [20.0, 0.0, 0.0]);
    for (a, b) in got.rot.iter().zip(at.rot.iter()) {
        assert!((a - b).abs() < 1e-12, "rotation bytes drift: {:?}", got.rot);
    }
    assert!(history.undo(&world));
    let back = model::transform_of(&world, &e).expect("location");
    assert_eq!(back.pos, Point3::ZERO, "undo restores the placement");
    assert!(history.redo(&world));
    let again = model::transform_of(&world, &e).expect("location");
    assert_eq!([again.pos.x, again.rot[0]], [20.0, at.rot[0]], "redo replays bytes");
}

#[test]
fn moving_one_part_carries_existing_stock_and_undo_restores_both() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("part", cube_mesh()));
    let stock_before = Location {
        pos: Point3::new(2.0, 0.0, 0.0),
        ..Location::default()
    };
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 4.0, dy: 4.0, dz: 2.0 }, stock_before)
            .expect("stock"),
    );
    let part_after = Location {
        pos: Point3::new(10.0, 0.0, 0.0),
        rot: quat_from_euler(0.0, 0.0, FRAC_PI_2),
    };
    history.execute(&world, MoveModel::new(&world, "part", part_after).expect("move"));
    // #571: the square box stock follows the turned part but stays square
    // to the machine, re-fitted with the allowance it had on each side
    // (-X 0, +X 3, -Y 2, +Y 2, under 0, top 1): part x 9..10, y 0..1.
    let stock = model::find_by_name(&world, STOCK_NAME).expect("stock remains");
    let moved = model::transform_of(&world, &stock).expect("placement");
    assert!((moved.pos.x - 11.0).abs() < 1e-9 && moved.pos.y.abs() < 1e-9 && moved.pos.z.abs() < 1e-9, "{:?}", moved.pos);
    assert_eq!(moved.rot, stock_before.rot, "the block keeps its orientation");
    assert_eq!(model::stock_def_of(&world, &stock), Some(StockShape::Box { dx: 4.0, dy: 4.0, dz: 2.0 }));

    assert!(history.undo(&world));
    assert_eq!(model::transform_of(&world, &stock), Some(stock_before));
    assert!(history.redo(&world));
    assert_eq!(model::transform_of(&world, &stock), Some(moved));
}

#[test]
fn moving_one_of_two_parts_only_carries_stock_copied_from_it() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("part", cube_mesh()));
    history.execute(&world, ImportModel::from_mesh("other", cube_mesh()));
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 4.0, dy: 4.0, dz: 2.0 }, Location::default())
            .expect("stock"),
    );
    let moved = Location { pos: Point3::new(3.0, 0.0, 0.0), ..Location::default() };
    history.execute(&world, MoveModel::new(&world, "part", moved).expect("move"));
    let stock = model::find_by_name(&world, STOCK_NAME).expect("stock");
    assert_eq!(model::transform_of(&world, &stock), Some(Location::default()));

    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::FromModel { source: "part".into() },
            Location::default(),
        )
        .expect("stock copied from part"),
    );
    history.execute(
        &world,
        MoveModel::new(&world, "part", Location::default()).expect("move back"),
    );
    assert_eq!(model::transform_of(&world, &stock), Some(Location::default()));
}

#[test]
fn a_moved_part_marks_generated_ops_stale_and_undo_clears_it() {
    let (world, mut history, _top) = selection_fixture();
    history.execute(
        &world,
        EditOperationLinks::new(&world, "G1", None, Some(SrcRef::Entity("part".into())))
            .expect("retarget"),
    );
    let cmd = GenerateOperation::new(&world, "G1").expect("generate");
    history.execute(&world, cmd);
    assert_eq!(op_status(&world, "G1"), OpStatus::Ok, "freshly generated");

    // Translation must make it stale; so must PURE ROTATION - the law
    // M3.2 deferred and stage G keeps: the stamp hashes rot, not just pos.
    let slide = Location {
        pos: Point3::new(4.0, 0.0, 0.0),
        rot: [0.0, 0.0, 0.0, 1.0],
    };
    history.execute(&world, MoveModel::new(&world, "part", slide).expect("slide"));
    assert_eq!(op_status(&world, "G1"), OpStatus::Stale, "translation is in the stamp");
    assert!(history.undo(&world));
    assert_eq!(op_status(&world, "G1"), OpStatus::Ok, "undo clears it");

    history.execute(
        &world,
        MoveModel::new(
            &world,
            "part",
            Location {
                pos: Point3::ZERO,
                rot: quat_from_euler(0.0, 0.0, FRAC_PI_2),
            },
        )
        .expect("yaw in place"),
    );
    assert_eq!(op_status(&world, "G1"), OpStatus::Stale, "rotation is in the stamp too");
}

#[test]
fn move_model_refuses_other_furniture_and_garbage() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("block", cube_mesh()));

    // The named verbs: fixtures and offsets are placed by their pages.
    assert!(matches!(
        MoveModel::new(&world, MACHINE_TABLE, Location::default()),
        Err(CommandError::Invalid(_))
    ));
    assert!(matches!(
        MoveModel::new(&world, "ghost", Location::default()),
        Err(CommandError::NotFound(_))
    ));
    assert!(MoveModel::new(&world, "block", Location::default()).is_ok());
    // Garbage never enters history: non-finite and degenerate poses.
    // Checked while the block is still a plain part - a placed
    // fixture's door (below) is the higher truth and would speak
    // first, naming the furniture instead of the pose.
    let nan = Location {
        pos: Point3::new(f64::NAN, 0.0, 0.0),
        rot: [0.0, 0.0, 0.0, 1.0],
    };
    assert!(matches!(MoveModel::new(&world, "block", nan), Err(CommandError::Invalid(_))));
    let zero_q = Location {
        pos: Point3::ZERO,
        rot: [0.0, 0.0, 0.0, 0.0],
    };
    assert!(matches!(
        MoveModel::new(&world, "block", zero_q),
        Err(CommandError::Invalid(m)) if m.contains("quaternion")
    ));
    history.execute(&world, TagFixture::new(&world, "block", None).expect("tagged"));
    let why = MoveModel::new(&world, "block", Location::default()).map(drop).err();
    match why {
        Some(CommandError::Invalid(m)) => assert!(m.contains("fixture"), "names its reason: {m}"),
        other => panic!("expected an invalid refusal, got {other:?}"),
    }
    // The offset exists once a command has placed it; it is not a part.
    history.execute(&world, SetWcsOrigin::new(&world, Location::default()).expect("wcs"));
    assert!(matches!(
        MoveModel::new(&world, "G54", Location::default()),
        Err(CommandError::Invalid(m)) if m.contains("work offset")
    ));

}
