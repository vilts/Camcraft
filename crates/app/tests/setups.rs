//! M3.2 stage A acceptance: setups are entities like everything else -
//! one clamping, one stock, one datum system - and the whole document
//! resolves through them by the laws in `crate::setup` and the commands'
//! module headers: materialization never renames, furniture is private
//! and lazily spawned, datum transfer clones, a setup with operations
//! does not delete, activation is inert, and moving an operation keeps
//! its stamp and lets the staleness machinery answer.
//!
//! Gate map (M4.4): covers app::commands, app::geometry, app::model, app::operation, app::posting, app::rest, app::schema, app::setup, geometry::geometry::stock, tools::tools, app::world.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    ActivateSetup, AddTool, CommandError, CreateOperation, CreateSetup, DeleteOperation,
    DeleteSetup, GenerateOperation, History, SetOperationSetup, SetStock, SetWcsOrigin,
};
use camcraft_app::geometry::{IDENTITY_Q, Point3};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, HeightReference, OpParams, OpStatus, SrcRef};
use camcraft_app::rest;
use camcraft_app::schema::{Location, STOCK_NAME};
use camcraft_app::setup;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;

fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

fn at(x: f64, y: f64, z: f64) -> Location {
    Location {
        pos: Point3::new(x, y, z),
        rot: IDENTITY_Q,
    }
}

/// Face parameters for the 60x60x20 box stock with a 4 mm cutter - the
/// same fixture the operations tests face on.
fn face_params() -> OpParams {
    OpParams::Face { holes: Default::default(), tilt: None, blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

fn box_stock(world: &World, history: &mut History, dx: f64, dy: f64, dz: f64) {
    let cmd = SetStock::new(world, StockShape::Box { dx, dy, dz }, Location::default());
    history.execute(world, cmd.expect("box stock"));
}

fn add_tool(world: &World, history: &mut History, name: &str) {
    let cmd = AddTool::new(world, name, catalog_tool(name));
    history.execute(world, cmd.expect("catalog tool"));
}

fn create_face(world: &World, history: &mut History, name: &str, cut_z: f64) {
    let mut params = face_params();
    if let OpParams::Face { cut_z: c, .. } = &mut params {
        *c = Height::fixed(cut_z);
    }
    let cmd = CreateOperation::new(world, name, "EM-0400-2FL", SrcRef::Stock, params);
    history.execute(world, cmd.expect("create face op"));
    let cmd = GenerateOperation::new(world, name).expect("generate face op");
    history.execute(world, cmd);
}

fn run(
    world: &World,
    history: &mut History,
    cmd: Result<impl camcraft_app::commands::Command + 'static, CommandError>,
    what: &str,
) {
    history.execute(world, cmd.unwrap_or_else(|e| panic!("{what}: {e:?}")));
}

/// The command refuses at construction; the unit map keeps the
/// assertion from needing Debug on the success type.
fn refused<T>(cmd: Result<T, CommandError>) -> CommandError {
    match cmd {
        Ok(_) => panic!("construction must refuse"),
        Err(e) => e,
    }
}

fn status(world: &World, name: &str) -> OpStatus {
    let e = model::find_by_name(world, name).expect("entity");
    operation::status_of(world, &e)
}

fn op_setup(world: &World, name: &str) -> Option<String> {
    let e = model::find_by_name(world, name).expect("entity");
    model::operation_of(world, &e).expect("operation").setup
}

/// Stock, tool, one materialized setup and one private one, the private
/// stock set to the same 60x60x20 box at the same spot: the pair every
/// cross-setup law is judged against.
fn two_setup_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    run(&world, &mut history, CreateSetup::new(&world, None, None), "materialize");
    run(&world, &mut history, CreateSetup::new(&world, None, None), "second setup");
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    (world, history)
}

#[test]
fn the_first_setup_materializes_and_changes_nothing() {
    // The whole compat law, in one run: a generated path predating setups
    // must still read Ok, byte-identical, after the first CreateSetup -
    // because Setup1 POINTS at the existing entities instead of renaming
    // them.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    create_face(&world, &mut history, "Face1", 19.5);
    let face_e = model::find_by_name(&world, "Face1").expect("entity");
    let before = model::toolpath_of(&world, &face_e).expect("cached path");
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);

    run(&world, &mut history, CreateSetup::new(&world, None, None), "create");
    assert_eq!(setup::names(&world), vec!["Setup1".to_string()]);
    assert_eq!(setup::active(&world).as_deref(), Some("Setup1"));
    let def = setup::def_of(&world, "Setup1").expect("def");
    assert_eq!(def.stock, STOCK_NAME, "materialization points, never renames");
    assert_eq!(def.wcs, "G54", "#342: one WCS - the legacy G54 entity");
    assert_eq!(def.datum_from, "");
    assert!(model::find_by_name(&world, "Setup1/Stock").is_none(), "no private copies");
    assert!(model::find_by_name(&world, "Setup1/WCS").is_none());

    let after = model::toolpath_of(&world, &face_e).expect("cached path");
    assert_eq!(before.path.moves.len(), after.path.moves.len());
    assert_eq!(status(&world, "Face1"), OpStatus::Ok, "the stamp still answers");
    assert_eq!(op_setup(&world, "Face1"), None, "creation never retro-declares");
    assert_eq!(setup::stock_entity_name(&world, Some("Setup1")), STOCK_NAME);
    assert!(setup::stock_entity(&world, None).is_some_and(|e| e == face_stock(&world, None)));

    // Undo drops the setup entity but not the furniture it merely named.
    assert!(history.undo(&world));
    assert!(setup::names(&world).is_empty());
    assert!(model::find_by_name(&world, STOCK_NAME).is_some());
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);
    assert!(history.redo(&world));
    assert_eq!(setup::names(&world), vec!["Setup1".to_string()]);
}

fn face_stock(world: &World, setup: Option<&str>) -> flecs_ecs::prelude::Entity {
    setup::stock_entity(world, setup).expect("stock entity")
}

#[test]
fn new_setups_own_private_furniture_spawned_on_first_use() {
    let (world, mut history) = two_setup_world();
    assert_eq!(setup::names(&world), vec!["Setup1".to_string(), "Setup2".to_string()]);
    let def = setup::def_of(&world, "Setup2").expect("def");
    assert_eq!(def.stock, "Setup2/Stock");
    assert_eq!(def.wcs, "Setup2/WCS", "#342: one WCS per setup");
    // Laziness: Setup2's stock exists (box_stock ran with it active), its
    // WCS does not until placed.
    assert!(setup::stock_entity(&world, Some("Setup2")).is_some());
    assert!(setup::wcs_entity(&world, Some("Setup2")).is_none());
    assert!(setup::wcs_entity(&world, None).is_none(), "legacy G54 never placed");

    run(
        &world,
        &mut history,
        SetWcsOrigin::new(&world, at(10.0, 0.0, 0.0)),
        "place G54 in Setup2",
    );
    let placed = setup::wcs_entity(&world, Some("Setup2"));
    assert!(placed.is_some());
    let entity = model::find_by_name(&world, "Setup2/WCS").expect("furniture");
    assert_eq!(placed, Some(entity));
    assert_eq!(model::transform_of(&world, &entity).expect("loc").pos.x, 10.0);
    // The G-code WORD is the setup's label; only the lookup followed the setup.
    assert_eq!(setup::wcs_entity_name(&world, Some("Setup2")), "Setup2/WCS");
    assert_eq!(setup::wcs_label(&world, Some("Setup2")), "WCS (G54)");
    assert!(model::find_by_name(&world, "G54").is_none());

    // And the STOCK edit lands on the private entity, not the legacy one.
    run(&world, &mut history, SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 30.0 }, Location::default()), "grow Setup2 stock");
    let legacy = model::find_by_name(&world, STOCK_NAME).expect("legacy stock");
    let legacy_shape = model::stock_def_of(&world, &legacy).expect("shape");
    assert_eq!(legacy_shape, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 });
    let priv_e = setup::stock_entity(&world, Some("Setup2")).expect("private stock");
    assert_ne!(priv_e, legacy);
    assert_eq!(
        model::stock_def_of(&world, &priv_e).expect("shape"),
        StockShape::Box { dx: 40.0, dy: 40.0, dz: 30.0 }
    );
}

#[test]
fn heights_resolve_through_the_operations_setup_not_the_cursor() {
    // The same G54 word reads a different plane per setup - and an
    // operation echoes what ITS setup says, wherever the cursor is.
    let (world, mut history) = two_setup_world();
    run(
        &world,
        &mut history,
        SetWcsOrigin::new(&world, at(0.0, 0.0, 5.0)),
        "place G54 in Setup2",
    );
    let h = Height::of(HeightReference::WorkOffset, 0.0);
    assert_eq!(h.z_for(&world, Some("Setup2")), Some(5.0));
    assert_eq!(h.z_for(&world, None), None, "Setup1's G54 was never placed");
    // Switching the cursor cannot switch the reading: the lookup takes
    // the declared setup, and activation is inert data.
    run(&world, &mut history, ActivateSetup::new(&world, Some("Setup1")), "activate Setup1");
    assert_eq!(h.z_for(&world, Some("Setup2")), Some(5.0));
    let top = Height::of(HeightReference::TopOfStock, 0.0);
    assert_eq!(top.z_for(&world, None), Some(20.0));
    assert_eq!(top.z_for(&world, Some("Setup2")), Some(20.0), "same box, same top");
}

#[test]
fn program_order_is_setup_rank_then_name() {
    // Setup2's `A_Op` is alphabetically first but machined second: rank
    // dominates name, and a nameless (None) op shares rank 0 with the
    // first setup - the implicit law ordering exactly as it resolves.
    let (world, mut history) = two_setup_world();
    create_face(&world, &mut history, "Z_Face", 19.5); // joins Setup2 (active)
    run(&world, &mut history, ActivateSetup::new(&world, Some("Setup1")), "back to Setup1");
    create_face(&world, &mut history, "Y_Face", 19.5); // joins Setup1
    let order: Vec<String> = setup::operation_order(&world).into_iter().map(|(n, _)| n).collect();
    assert_eq!(order, vec!["Y_Face".to_string(), "Z_Face".to_string()]);
    assert!(setup::active(&world).as_deref() == Some("Setup1"));
    // The order law is the POST's order law - by call, one function.
    let posted: Vec<String> = camcraft_app::posting::program_ops(&world)
        .expect("program")
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert_eq!(posted, order);
}

#[test]
fn datum_transfer_clones_the_world_and_undoes_exactly() {
    let (world, mut history) = two_setup_world();
    run(&world, &mut history, SetWcsOrigin::new(&world, at(12.0, 0.0, 3.0)), "place offset");
    run(
        &world,
        &mut history,
        CreateSetup::new(&world, None, Some("Setup2")),
        "transfer datum",
    );
    assert_eq!(setup::names(&world).len(), 3);
    assert_eq!(setup::active(&world).as_deref(), Some("Setup3"));
    let def = setup::def_of(&world, "Setup3").expect("def");
    assert_eq!(def.datum_from, "Setup2", "provenance rides the setup");
    assert_eq!(def.stock, "Setup3/Stock");
    // Clone, not alias: own entities, same placement and shape.
    let s3 = setup::stock_entity(&world, Some("Setup3")).expect("cloned stock");
    let s2 = setup::stock_entity(&world, Some("Setup2")).expect("source stock");
    assert_ne!(s3, s2);
    assert_eq!(
        model::stock_def_of(&world, &s3).expect("shape"),
        model::stock_def_of(&world, &s2).expect("shape")
    );
    assert_eq!(
        model::transform_of(&world, &s3).expect("loc"),
        model::transform_of(&world, &s2).expect("loc")
    );
    let w3 = setup::wcs_entity(&world, Some("Setup3")).expect("cloned WCS");
    let w2 = setup::wcs_entity(&world, Some("Setup2")).expect("source WCS");
    assert_ne!(w3, w2);
    assert_eq!(model::transform_of(&world, &w3).expect("loc").pos.z, 3.0);

    assert!(history.undo(&world));
    assert_eq!(setup::names(&world), vec!["Setup1".to_string(), "Setup2".to_string()]);
    assert!(model::find_by_name(&world, "Setup3/Stock").is_none());
    assert!(model::find_by_name(&world, "Setup3/WCS").is_none());
    // The SOURCE's furniture survived the inverse untouched - a clone
    // removes only its own copies.
    assert!(setup::stock_entity(&world, Some("Setup2")).is_some());
    assert!(setup::wcs_entity(&world, Some("Setup2")).is_some());
    assert_eq!(setup::active(&world).as_deref(), Some("Setup2"));
    assert!(history.redo(&world));
    assert!(setup::stock_entity(&world, Some("Setup3")).is_some());
    assert!(setup::wcs_entity(&world, Some("Setup3")).is_some());
}

#[test]
fn a_setup_with_operations_does_not_delete() {
    let (world, mut history) = two_setup_world();
    run(&world, &mut history, ActivateSetup::new(&world, Some("Setup1")), "Setup1 active");
    create_face(&world, &mut history, "Face1", 19.5);
    run(&world, &mut history, ActivateSetup::new(&world, Some("Setup2")), "Setup2 active");
    create_face(&world, &mut history, "Face2", 19.5);

    let err = refused(DeleteSetup::new(&world, "Setup1"));
    match err {
        CommandError::Invalid(msg) => {
            assert!(msg.contains("Face1") && msg.contains("Setup1"), "names both: {msg}");
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
    assert!(setup::exists(&world, "Setup1"), "refusal writes nothing");

    // Move Face1 out and the first setup deletes - its furniture is the
    // SHARED legacy names, which must survive.
    run(&world, &mut history, SetOperationSetup::new(&world, "Face1", Some("Setup2")), "move");
    run(&world, &mut history, DeleteSetup::new(&world, "Setup1"), "delete");
    assert_eq!(setup::names(&world), vec!["Setup2".to_string()]);
    assert!(model::find_by_name(&world, STOCK_NAME).is_some(), "shared furniture stays");
    assert_eq!(
        setup::active(&world).as_deref(),
        Some("Setup2"),
        "a dangling cursor reads as the first that exists"
    );

    // The private setup's OWN furniture goes with it, and undo brings
    // the whole setup back. Emptied first - and note Face1's move to
    // None made it an IMPLICIT member (the first-setup law reads None
    // as the first setup by name, which is now Setup2), so emptying
    // means deleting the operations, not just un-declaring them.
    run(&world, &mut history, DeleteOperation::new(&world, "Face1"), "drop the implicit");
    run(&world, &mut history, DeleteOperation::new(&world, "Face2"), "empty it");
    run(&world, &mut history, DeleteSetup::new(&world, "Setup2"), "delete Setup2");
    assert!(model::find_by_name(&world, "Setup2").is_none());
    assert!(model::find_by_name(&world, "Setup2/Stock").is_none(), "owned furniture goes");
    assert!(model::find_by_name(&world, "Setup2/WCS").is_none());
    assert!(model::find_by_name(&world, STOCK_NAME).is_some());
    assert!(history.undo(&world));
    let priv_stock = model::find_by_name(&world, "Setup2/Stock").expect("restored");
    assert_eq!(
        model::stock_def_of(&world, &priv_stock).expect("shape"),
        StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 },
        "the snapshot restores the stock exactly"
    );
    assert_eq!(setup::names(&world), vec!["Setup2".to_string()]);
}

#[test]
fn activation_is_inert_data() {
    let (world, mut history) = two_setup_world();
    create_face(&world, &mut history, "Face2", 19.5); // joins Setup2
    let face_e = model::find_by_name(&world, "Face2").expect("entity");
    let stamp_before = model::operation_of(&world, &face_e).expect("op").stamp;
    let moves_before = model::toolpath_of(&world, &face_e).expect("path").path.moves.len();

    run(&world, &mut history, ActivateSetup::new(&world, Some("Setup1")), "activate");
    assert_eq!(setup::stored_active(&world).as_deref(), Some("Setup1"));
    let face_e = model::find_by_name(&world, "Face2").expect("entity");
    assert_eq!(
        model::operation_of(&world, &face_e).expect("op").stamp,
        stamp_before,
        "a cursor move digests into nothing"
    );
    assert_eq!(status(&world, "Face2"), OpStatus::Ok);
    assert_eq!(model::toolpath_of(&world, &face_e).expect("path").path.moves.len(), moves_before);
    assert_eq!(op_setup(&world, "Face2"), Some("Setup2".to_string()), "membership is untouched");

    // The one thing activation changes: where NEW things join.
    create_face(&world, &mut history, "Face3", 19.5);
    assert_eq!(op_setup(&world, "Face3"), Some("Setup1".to_string()));
    assert!(history.undo(&world)); // Face3's generate
    assert!(history.undo(&world)); // Face3's create
    assert!(history.undo(&world)); // the activation
    assert_eq!(setup::stored_active(&world).as_deref(), Some("Setup2"));
    assert!(model::find_by_name(&world, "Face3").is_none(), "Face3 gone");
}

#[test]
fn moving_an_operation_keeps_the_stamp_and_the_machinery_answers() {
    let (world, mut history) = two_setup_world();
    run(&world, &mut history, ActivateSetup::new(&world, Some("Setup1")), "Setup1 active");
    create_face(&world, &mut history, "Face1", 19.5);
    let face_e = model::find_by_name(&world, "Face1").expect("entity");
    let stamp_before = model::operation_of(&world, &face_e).expect("op").stamp;

    // Both setups place the same box in the same spot: the move changes
    // nothing the digest reads, so the path is still the current truth.
    run(&world, &mut history, SetOperationSetup::new(&world, "Face1", Some("Setup2")), "move");
    let face_e = model::find_by_name(&world, "Face1").expect("entity");
    let op = model::operation_of(&world, &face_e).expect("op");
    assert_eq!(op.setup.as_deref(), Some("Setup2"));
    assert_eq!(op.stamp, stamp_before, "the command keeps the stamp, never recomputes");
    assert_eq!(status(&world, "Face1"), OpStatus::Ok, "identical placements say Ok");

    // Now Setup2's block moves: the SAME stamp answers Stale, by the
    // resolution machinery, not by anything the move command assumed.
    // SetStock writes the ACTIVE setup's block, so activate into Setup2
    // first - the cursor moves, the op's membership (and stamp) do not.
    run(&world, &mut history, ActivateSetup::new(&world, Some("Setup2")), "raise where Face1 lives");
    run(&world, &mut history, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, at(0.0, 0.0, 1.0)), "raise Setup2 stock");
    assert_eq!(status(&world, "Face1"), OpStatus::Stale);
    assert!(history.undo(&world));
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);

    // Construction refuses before any of this: a nonexistent op, a
    // nonexistent target, and a move that is not a move.
    assert!(
        matches!(refused(SetOperationSetup::new(&world, "Nope", Some("Setup2"))), CommandError::NotFound(_))
    );
    assert!(
        matches!(refused(SetOperationSetup::new(&world, "Face1", Some("Nope"))), CommandError::NotFound(_))
    );
    assert!(
        matches!(refused(SetOperationSetup::new(&world, "Face1", Some("Setup2"))), CommandError::Invalid(_))
    );
}

#[test]
fn setup_names_are_validated_at_construction() {
    let world = new_world();
    // Reserved furniture names and the separator belong to the scheme.
    for bad in ["Stock", "G54", "G59", "Setup1/G54", "", "  "] {
        assert!(
            matches!(refused(CreateSetup::new(&world, Some(bad), None)), CommandError::Invalid(_)),
            "'{bad}' must be refused"
        );
    }
    // Tools cannot squat furniture names either (the law is shared).
    let tool = catalog_tool("EM-0400-2FL");
    assert!(matches!(
        refused(AddTool::new(&world, "Setup9/Stock", tool.clone())),
        CommandError::Invalid(_) | CommandError::NameConflict(_)
    ));
    // One entity, one name: an existing entity's name conflicts.
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    assert!(matches!(
        refused(CreateSetup::new(&world, Some(STOCK_NAME), None)),
        CommandError::NameConflict(_) | CommandError::Invalid(_)
    ));
    // Materialization refuses a datum (there is nothing to transfer);
    // after the first setup, a named source that does not exist is NotFound.
    assert!(matches!(
        refused(CreateSetup::new(&world, None, Some("Setup0"))),
        CommandError::Invalid(_)
    ));
    run(&world, &mut history, CreateSetup::new(&world, None, None), "materialize");
    assert!(
        matches!(refused(CreateSetup::new(&world, None, Some("Nope"))), CommandError::NotFound(_))
    );
    assert!(
        matches!(refused(ActivateSetup::new(&world, Some("Nope"))), CommandError::NotFound(_))
    );
    assert!(matches!(refused(DeleteSetup::new(&world, "Nope")), CommandError::NotFound(_)));
    // `DeleteSetup` on a non-setup entity is Invalid, not a silent no-op.
    assert!(matches!(
        refused(DeleteSetup::new(&world, STOCK_NAME)),
        CommandError::Invalid(_) | CommandError::NotFound(_)
    ));
}

#[test]
fn rest_masks_are_same_setup_only() {
    // An operation's rest digest covers the program AHEAD of it IN ITS
    // OWN SETUP: another setup's cuts belong to another block of metal.
    // The proof is equality: Face2's mask is the same whether or not a
    // first setup machined anything at all.
    let build = |with_face1: bool| {
        let (world, mut history) = two_setup_world();
        if with_face1 {
            run(&world, &mut history, ActivateSetup::new(&world, Some("Setup1")), "active 1");
            create_face(&world, &mut history, "Face1", 19.5);
            run(&world, &mut history, ActivateSetup::new(&world, Some("Setup2")), "active 2");
        }
        create_face(&world, &mut history, "A_Rough", 10.0);
        create_face(&world, &mut history, "Face2", 19.5);
        assert_eq!(status(&world, "Face2"), OpStatus::Ok);
        let d = rest::prior_digest(&world, "Face2", Some("Setup2"));
        assert!(d.is_some(), "A_Rough is ahead in the same setup");
        let d1 = rest::prior_digest(&world, "Face1", Some("Setup1"));
        (world, d, d1)
    };
    let (w0, d_without, d1_without) = build(false);
    let (w1, d_with, d1_with) = build(true);
    assert_eq!(d_with, d_without, "Setup1's program is not Setup2's rest");
    assert_eq!(d1_with, None, "nothing machined ahead of Face1 in Setup1");
    assert_eq!(d1_without, None);
    // And the ORDER law agrees face-to-face: in w1 the post walks
    // Face1 before the Setup2 pair - ordering across setups exists,
    // masking across setups does not.
    let order: Vec<String> = setup::operation_order(&w1).into_iter().map(|(n, _)| n).collect();
    assert_eq!(
        order,
        vec!["Face1".to_string(), "A_Rough".to_string(), "Face2".to_string()]
    );
    let _ = w0;
}

#[test]
fn deleting_the_active_furniture_owner_falls_the_cursor() {
    // DeleteSetup leaves the singleton alone; `active`'s existence
    // fallback is what keeps a document with setups always having one.
    let (world, mut history) = two_setup_world();
    assert_eq!(setup::active(&world).as_deref(), Some("Setup2"));
    run(&world, &mut history, DeleteSetup::new(&world, "Setup2"), "delete active");
    assert_eq!(setup::stored_active(&world).as_deref(), Some("Setup2"), "left dangling");
    assert_eq!(setup::active(&world).as_deref(), Some("Setup1"), "resolves to the first");
    assert!(history.undo(&world));
    assert_eq!(setup::active(&world).as_deref(), Some("Setup2"));
}

/// Guard (#878): the refusals a user reads never say "furniture", our
/// in-code word for a setup's stock, work offsets and fixtures. Each
/// message is built the way its own test builds it.
#[test]
fn setup_refusals_never_say_furniture() {
    let world = new_world();
    let messages = [
        refused(CreateSetup::new(&world, Some("Setup9/Stock"), None)).to_string(),
        refused(CreateSetup::new(&world, Some("Stock"), None)).to_string(),
        refused(CreateSetup::new(&world, Some("G54"), None)).to_string(),
        refused(AddTool::new(&world, "Setup9/Stock", catalog_tool("EM-0400-2FL"))).to_string(),
    ];
    for message in messages {
        assert!(!message.to_lowercase().contains("furniture"), "{message}");
    }
}
