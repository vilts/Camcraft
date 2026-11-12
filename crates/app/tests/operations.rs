//! M1.3 acceptance: tools and operations are entities like everything
//! else - commands own their fields, undo/redo is exact, generation runs
//! at command construction (validate early), and staleness is derived,
//! never stored.
//!
//! Gate map (M4.4): covers cl::cl, app::commands, motion::drilling, motion::finishing3d, geometry::geo2d, app::geometry, motion::machining2d, app::model, app::operation, app::schema, geometry::geometry::stock, tools::tools, sim::verify, app::world.
//!
//! Lane shape (M13.8): the debug ladder runs every test here in seconds.
//! The six slow generate-and-verify laws (swarf, the swarf ladder,
//! contour5d, the shank-flag twins, the ramp and ladder stamps) run over
//! a narrower Z band of the same fixture in debug, and their full-band
//! twins (`*_full_band`) carry the identical assertions in the heavy lane
//! (`#[ignore]`, `cargo test --release -p camcraft-app --test operations
//! -- --ignored`).

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, ChangeTool, CommandError, CreateOperation, DeleteEntity, DeleteOperation,
    EditOperationLinks, EditOperationParams, GenerateOperation, History, ImportModel,
    RegenerateAll, SetStock, SetWcsOrigin, UpdateModel,
};
use camcraft_app::geometry::{IDENTITY_Q, Mesh, Point3, cube};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::{Location, Operation, ToolDef, STOCK_NAME, Toolpath};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, catalog};
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

/// Face parameters for the 60x60x20 box stock with a 4 mm cutter.
fn face_params() -> OpParams {
    OpParams::Face { holes: Default::default(), blend_turns: false,
        tilt: None,
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

fn contour_params() -> OpParams {
    OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(10.0),
        retract_z: Height::fixed(8.0),
        top_z: Height::fixed(5.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 2.5,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    }
}

/// Slot1's fixture (M8.2 C): a 40 mm channel 12 wide, cut to z = -6 in
/// three levels. With the 6 mm EM the cutter-center stadium stands 3 mm
/// either side of the axis - room for a 2 mm orbit, none for a 4 mm one.
fn slot_params() -> OpParams {
    OpParams::Slot {
        links: camcraft_app::links::Links::PLANAR,
        a: camcraft_app::geo2d::Point2::new(0.0, 10.0),
        b: camcraft_app::geo2d::Point2::new(40.0, 10.0),
        width: 12.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(5.0),
        retract_z: None,
        top_z: Height::fixed(0.0),
        bottom_z: Height::fixed(-6.0),
        stepdown: 3.0,
        stepover: 4.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        tilt: None,
    }
}

/// Stock bought from the model itself: the finishing world, where the
/// rough already took the box away and nothing but the part stands
/// for a leaning holder to meet.
fn model_stock(world: &World, history: &mut History, source: &str) {
    let cmd = SetStock::new(world, StockShape::FromModel { source: source.into() }, Location::default());
    history.execute(world, cmd.expect("from-model stock"));
}

/// The swarf tool: the catalog's 6 mm ball under the same name, but
/// with a flute longer than the blade and a shank no wider than the
/// ball - a tool that LIES along a ruling must be all flute where it
/// lies (the catalog stub's 8 mm shank, lying a radius off the flank,
/// sinks a millimetre into it: the verifier's honest ShankHit).
fn add_swarf_ball(world: &World, history: &mut History) {
    let mut t = catalog_tool("BN-0600");
    t.flute_len = 60.0;
    t.shank_d = t.diameter;
    let cmd = AddTool::new(world, "BN-0600", t);
    history.execute(world, cmd.expect("swarf ball"));
}

fn box_stock(world: &World, history: &mut History, dx: f64, dy: f64, dz: f64) {
    let cmd = SetStock::new(world, StockShape::Box { dx, dy, dz }, Location::default());
    history.execute(world, cmd.expect("box stock"));
}

fn add_tool(world: &World, history: &mut History, name: &str) {
    let cmd = AddTool::new(world, name, catalog_tool(name));
    history.execute(world, cmd.expect("catalog tool"));
}

/// Stock + a 4 mm end mill + a generated face operation: the fixture most
/// tests start from.
fn face_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face_params());
    history.execute(&world, cmd.expect("create face op"));
    let cmd = GenerateOperation::new(&world, "Face1").expect("generate face op");
    history.execute(&world, cmd);
    (world, history)
}

fn status(world: &World, name: &str) -> OpStatus {
    let e = model::find_by_name(world, name).expect("entity exists");
    operation::status_of(world, &e)
}

fn cache(world: &World, name: &str) -> Option<Toolpath> {
    model::find_by_name(world, name).and_then(|e| model::toolpath_of(world, &e))
}

#[test]
fn edit_operation_params_refuses_a_type_change() {
    let (world, _history) = face_world();
    // The `OpParams` variant IS the operation's kind: an "edit" carrying a
    // foreign variant would retype the operation in place - the hand-test
    // report's "generating this operation replaced the face operation".
    // The command refuses it by name instead of writing the ghost, and
    // points at the door that does retype: the browser row's menu (M9.1).
    let Err(err) = EditOperationParams::new(&world, "Face1", contour_params()) else {
        panic!("a kind change is not a parameter edit and must be refused");
    };
    assert!(matches!(err, CommandError::Invalid(_)), "{err:?}");
    assert!(err.to_string().contains("retype menu"), "{err}");
    // Nothing was written: the operation keeps its exact params and its
    // fresh stamp - a refusal that silently half-applied would be worse
    // than the retype it replaces.
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);
    let e = model::find_by_name(&world, "Face1").expect("entity");
    assert_eq!(
        model::operation_of(&world, &e).expect("op").params,
        face_params()
    );
}

#[test]
fn tools_are_entities_and_undoable() {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0800-4FL");

    let e = model::find_by_name(&world, "EM-0800-4FL").expect("tool entity");
    assert!(model::is_tool(&world, &e));
    assert_eq!(model::tool_of(&world, &e).map(|t| t.diameter), Some(8.0));
    assert_eq!(model::facets_of(&world, &e), None, "tools have no geometry");

    assert!(history.undo(&world));
    assert!(model::find_by_name(&world, "EM-0800-4FL").is_none());
    assert!(history.redo(&world));
    let e = model::find_by_name(&world, "EM-0800-4FL").expect("restored");
    assert_eq!(
        model::tool_of(&world, &e),
        Some(catalog_tool("EM-0800-4FL"))
    );
}

#[test]
fn add_tool_refusals_at_construction() {
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 10.0, 10.0, 10.0);
    let cmd = AddTool::new(&world, "T1", catalog_tool("EM-0400-2FL"));
    history.execute(&world, cmd.expect("named tool"));

    // reserved setup names and the empty name
    assert!(matches!(
        AddTool::new(&world, STOCK_NAME, catalog_tool("EM-0400-2FL")),
        Err(CommandError::Invalid(_))
    ));
    assert!(matches!(
        AddTool::new(&world, "G54", catalog_tool("EM-0400-2FL")),
        Err(CommandError::Invalid(_))
    ));
    assert!(matches!(
        AddTool::new(&world, "", catalog_tool("EM-0400-2FL")),
        Err(CommandError::Invalid(_))
    ));
    // duplicate name
    assert!(matches!(
        AddTool::new(&world, "T1", catalog_tool("EM-0400-2FL")),
        Err(CommandError::NameConflict(_))
    ));
    // insane geometry
    assert!(matches!(
        AddTool::new(
            &world,
            "T2",
            Tool {
                diameter: 0.0,
                ..catalog_tool("EM-0400-2FL")
            }
        ),
        Err(CommandError::Invalid(_))
    ));
    assert!(
        model::find_by_name(&world, "T2").is_none(),
        "refused names never spawn"
    );
}

#[test]
fn change_tool_is_exact_and_marks_dependents_stale() {
    let (world, mut history) = face_world();
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);

    let mut fatter = catalog_tool("EM-0400-2FL");
    fatter.diameter = 6.0;
    history.execute(
        &world,
        ChangeTool::new(&world, "EM-0400-2FL", fatter.clone()).expect("edit"),
    );
    assert_eq!(
        status(&world, "Face1"),
        OpStatus::Stale,
        "the cache predates the edit"
    );
    assert!(
        cache(&world, "Face1").is_some(),
        "stale keeps the old path on screen"
    );

    assert!(history.undo(&world));
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);
    assert!(history.redo(&world));
    assert_eq!(status(&world, "Face1"), OpStatus::Stale);

    // re-generating with the fatter tool clears the staleness
    history.execute(
        &world,
        GenerateOperation::new(&world, "Face1").expect("regenerate"),
    );
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);
    let e = model::find_by_name(&world, "EM-0400-2FL").expect("tool");
    assert_eq!(model::tool_of(&world, &e).map(|t| t.diameter), Some(6.0));
}

#[test]
fn create_operation_validates_every_reference() {
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 20.0, 20.0, 10.0);
    add_tool(&world, &mut history, "EM-0400-2FL");
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(2.0, 2.0, 2.0))),
    );
    history.execute(
        &world,
        SetWcsOrigin::new(&world, at(0.0, 0.0, 0.0)).expect("wcs"),
    );

    // unknown tool, and a name that is a model rather than a tool
    assert!(matches!(
        CreateOperation::new(&world, "O", "no-such", SrcRef::Stock, face_params()),
        Err(CommandError::NotFound(_))
    ));
    assert!(matches!(
        CreateOperation::new(&world, "O", "part", SrcRef::Stock, face_params()),
        Err(CommandError::Invalid(_))
    ));
    // sources must exist and not be offsets
    assert!(matches!(
        CreateOperation::new(
            &world,
            "O",
            "EM-0400-2FL",
            SrcRef::Entity("gone".into()),
            face_params()
        ),
        Err(CommandError::NotFound(_))
    ));
    assert!(matches!(
        CreateOperation::new(
            &world,
            "O",
            "EM-0400-2FL",
            SrcRef::Entity("G54".into()),
            face_params()
        ),
        Err(CommandError::Invalid(_))
    ));
    // parameters gate at construction (no struct-update on enum variants:
    // write the whole variant)
    assert!(matches!(
        CreateOperation::new(
            &world,
            "O",
            "EM-0400-2FL",
            SrcRef::Stock,
            OpParams::Face { holes: Default::default(), blend_turns: false,
                tilt: None,
                stepover: f64::NAN,
                clearance_z: Height::fixed(25.0),
                cut_z: Height::fixed(19.5),
                top_z: None,
                stepdown: None,
                feed: 1200.0,
                plunge_feed: 300.0,
                climb: true,
            },
        ),
        Err(CommandError::Invalid(_))
    ));
    // the positive case: a model source with sane params is accepted
    assert!(
        CreateOperation::new(
            &world,
            "O",
            "EM-0400-2FL",
            SrcRef::Entity("part".into()),
            contour_params()
        )
        .is_ok()
    );
}

#[test]
fn no_stock_refused_for_stock_source() {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0400-2FL");
    assert!(matches!(
        CreateOperation::new(&world, "O", "EM-0400-2FL", SrcRef::Stock, face_params()),
        Err(CommandError::Invalid(_))
    ));
}

#[test]
fn generate_undo_redo_replays_the_identical_path() {
    let (world, mut history) = face_world();
    let first = cache(&world, "Face1").expect("generated");
    assert!(!first.path.is_empty(), "a 60x60 face pass is a real path");
    assert!(first.path.is_chained() && first.path.all_finite());
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);

    assert!(history.undo(&world));
    assert_eq!(
        status(&world, "Face1"),
        OpStatus::Never,
        "undo restores the pre-generate state"
    );
    assert!(
        cache(&world, "Face1").is_none(),
        "no component, not an empty one"
    );

    assert!(history.redo(&world));
    assert_eq!(
        cache(&world, "Face1").map(|c| c.path),
        Some(first.path.clone()),
        "redo replays bytes, never re-runs"
    );
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);
}

#[test]
fn edit_params_clears_the_stamp_not_the_cache() {
    let (world, mut history) = face_world();
    let first = cache(&world, "Face1").expect("generated").path;

    let lower = OpParams::Face { holes: Default::default(), blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    history.execute(
        &world,
        EditOperationParams::new(&world, "Face1", lower).expect("edit"),
    );
    assert_eq!(status(&world, "Face1"), OpStatus::Stale);
    assert_eq!(
        cache(&world, "Face1").map(|c| c.path),
        Some(first.clone()),
        "cache survives the edit"
    );

    assert!(history.undo(&world));
    assert_eq!(
        status(&world, "Face1"),
        OpStatus::Ok,
        "the stamp comes back with the params"
    );
    assert!(history.redo(&world));
    assert_eq!(status(&world, "Face1"), OpStatus::Stale);
}

#[test]
fn stock_resize_makes_generated_ops_stale() {
    // The law SetStock's facet count cannot enforce (12 tris either way),
    // the stamp can: stock params are in the digest.
    let (world, mut history) = face_world();
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);

    box_stock(&world, &mut history, 60.0, 60.0, 25.0);
    assert_eq!(status(&world, "Face1"), OpStatus::Stale);

    history.execute(
        &world,
        GenerateOperation::new(&world, "Face1").expect("regenerate"),
    );
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);
    // undoing the regenerate returns the stock-resize staleness, cache and all
    assert!(history.undo(&world));
    assert_eq!(status(&world, "Face1"), OpStatus::Stale);
    assert!(cache(&world, "Face1").is_some());
}

#[test]
fn stock_move_makes_generated_ops_stale() {
    let (world, mut history) = face_world();
    let e = model::find_by_name(&world, STOCK_NAME).expect("stock");
    // A command-less move is the model case; use SetStock to relocate
    // exactly (same parameters, new placement).
    let cmd = SetStock::new(
        &world,
        StockShape::Box {
            dx: 60.0,
            dy: 60.0,
            dz: 20.0,
        },
        at(5.0, 0.0, 0.0),
    );
    history.execute(&world, cmd.expect("relocate"));
    assert_eq!(model::transform_of(&world, &e).map(|l| l.pos.x), Some(5.0));
    assert_eq!(
        status(&world, "Face1"),
        OpStatus::Stale,
        "the stamp reads stock Location"
    );
}

#[test]
fn retarget_links_clear_the_stamp() {
    let (world, mut history) = face_world();
    add_tool(&world, &mut history, "EM-0600-4FL");

    history.execute(
        &world,
        EditOperationLinks::new(&world, "Face1", Some("EM-0600-4FL"), None).expect("retarget"),
    );
    assert_eq!(status(&world, "Face1"), OpStatus::Stale);
    assert_eq!(
        model::operation_of(&world, &model::find_by_name(&world, "Face1").expect("op"))
            .map(|o| o.tool),
        Some("EM-0600-4FL".to_string())
    );
    assert!(history.undo(&world));
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);
    assert_eq!(
        model::operation_of(&world, &model::find_by_name(&world, "Face1").expect("op"))
            .map(|o| o.tool),
        Some("EM-0400-2FL".to_string())
    );
}

#[test]
fn generation_failure_refuses_the_command_and_writes_nothing() {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    let cmd = CreateOperation::new(
        &world,
        "Contour1",
        "EM-0600-4FL",
        SrcRef::Entity("part".into()),
        contour_params(),
    );
    history.execute(&world, cmd.expect("create"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Contour1").expect("generate"),
    );
    let first_path = cache(&world, "Contour1").expect("cache").path;
    let stored_stamp = model::operation_of(
        &world,
        &model::find_by_name(&world, "Contour1").expect("op"),
    )
    .expect("op")
    .stamp;

    // Move the source out of the section plane (setup poke, not a command).
    let e = model::find_by_name(&world, "part").expect("part");
    model::set_location(&world, &e, Point3::new(0.0, 0.0, 200.0));

    let Err(err) = GenerateOperation::new(&world, "Contour1") else {
        panic!("must fail");
    };
    assert!(matches!(err, CommandError::Machining(_)), "{err:?}");
    // Nothing half-wrote: the old cache and its stamp stand.
    assert_eq!(cache(&world, "Contour1").map(|c| c.path), Some(first_path));
    let op = model::operation_of(
        &world,
        &model::find_by_name(&world, "Contour1").expect("op"),
    )
    .expect("op");
    assert_eq!(op.stamp, stored_stamp, "the stored stamp is untouched");
    assert_eq!(
        status(&world, "Contour1"),
        OpStatus::Stale,
        "the moved source reads stale"
    );
}

/// The cube's silent twin at the same footprint: twelve facets, the same
/// 10 x 10 x 5 AABB, every wall a different plane. This is what the old
/// count+bounds stamp could not see.
fn prism_10x10x5() -> Mesh {
    let (a, b, c, d) = (
        Point3::new(5.0, 0.0, 0.0),
        Point3::new(10.0, 5.0, 0.0),
        Point3::new(5.0, 10.0, 0.0),
        Point3::new(0.0, 5.0, 0.0),
    );
    let (a1, b1, c1, d1) = (
        Point3::new(5.0, 0.0, 5.0),
        Point3::new(10.0, 5.0, 5.0),
        Point3::new(5.0, 10.0, 5.0),
        Point3::new(0.0, 5.0, 5.0),
    );
    Mesh::from_facets(
        vec![
            Triangle::new(a, b, c),
            Triangle::new(a, c, d),
            Triangle::new(a1, c1, b1),
            Triangle::new(a1, d1, c1),
            Triangle::new(a, b, b1),
            Triangle::new(a, b1, a1),
            Triangle::new(b, c, c1),
            Triangle::new(b, c1, b1),
            Triangle::new(c, d, d1),
            Triangle::new(c, d1, c1),
            Triangle::new(d, a, a1),
            Triangle::new(d, a1, d1),
        ],
    )
}

#[test]
fn a_model_update_marks_entity_sourced_operations_stale() {
    // M3.1 A, the milestone's whole point in one assertion: geometry
    // landing on a model's name must invalidate the operations that read
    // that model - even geometry with the same facet count and the same
    // box, which the pre-M3.1 stamp saw as nothing happening at all.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    let cmd = CreateOperation::new(
        &world,
        "Contour1",
        "EM-0600-4FL",
        SrcRef::Entity("part".into()),
        contour_params(),
    );
    history.execute(&world, cmd.expect("create"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Contour1").expect("generate"),
    );
    assert_eq!(status(&world, "Contour1"), OpStatus::Ok);

    let cmd = UpdateModel::new(&world, "part", prism_10x10x5()).expect("model exists");
    history.execute(&world, cmd);
    assert_eq!(
        status(&world, "Contour1"),
        OpStatus::Stale,
        "different walls under the same count and box is a different model"
    );

    // The undo law of the whole milestone: revision 1 restores the stamp,
    // so the operation reads Ok without a regenerate - staleness is
    // derived, and derivation is exact in both directions.
    assert!(history.undo(&world));
    assert_eq!(status(&world, "Contour1"), OpStatus::Ok, "the cube is back");
    assert!(history.redo(&world));
    assert_eq!(status(&world, "Contour1"), OpStatus::Stale);
}

#[test]
fn an_entry_edit_marks_the_contour_stale_and_the_wall_regen_moves_nothing() {
    // The M8.2 A digest law in one run: `entry` and the radius feed the
    // contour stamp, so the family edit reads Stale once and clears with
    // a regen - and on the wall side the regen cuts the SAME path,
    // because the Plunge default IS that law. Staleness is derived, so
    // walking the edit back re-derives it without asking the generator.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Contour1",
            "EM-0600-4FL",
            SrcRef::Entity("part".into()),
            contour_params(),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Contour1").expect("generate"),
    );
    assert_eq!(status(&world, "Contour1"), OpStatus::Ok);

    let mut edited = contour_params();
    let OpParams::Contour {
        ref mut entry,
        ref mut helix_radius,
        ..
    } = edited
    else {
        unreachable!("contour_params is a Contour");
    };
    *entry = camcraft_app::machining3d::Entry3D::Helix;
    *helix_radius = 2.0;
    history.execute(
        &world,
        EditOperationParams::new(&world, "Contour1", edited).expect("edit"),
    );
    assert_eq!(
        status(&world, "Contour1"),
        OpStatus::Stale,
        "the entry bytes ride the digest"
    );
    let before = cache(&world, "Contour1").expect("cached").path;
    history.execute(
        &world,
        GenerateOperation::new(&world, "Contour1").expect("regen"),
    );
    assert_eq!(status(&world, "Contour1"), OpStatus::Ok);
    let after = cache(&world, "Contour1").expect("cached").path;
    assert_eq!(
        before.moves, after.moves,
        "the wall-side graze is unchanged by the stated arm"
    );
    // The only delta is what the refusal law obliges: the stated arm is
    // said on the notes channel, once, and the dive stands in unchanged.
    assert!(
        before.notes.iter().all(|n| !n.starts_with("entry:")),
        "the dive names nothing: {:?}",
        before.notes
    );
    assert_eq!(
        after
            .notes
            .iter()
            .filter(|n| n.starts_with("entry:") && n.contains("needs bounded air"))
            .count(),
        1,
        "the refused helix is said exactly once: {:?}",
        after.notes
    );

    // Undo re-derives: the params digest walks back to the Plunge stamp,
    // which no cached path carries any more (the regen stamped it with
    // the Helix digest) - Stale again, honestly, until a regen.
    assert!(history.undo(&world));
    assert_eq!(status(&world, "Contour1"), OpStatus::Stale, "derived, both ways");
    assert!(history.redo(&world));
    assert_eq!(status(&world, "Contour1"), OpStatus::Ok, "and exact");
}

#[test]
fn an_entry_edit_marks_the_slot_stale_and_the_orbit_proves_itself() {
    // The M8.2 C digest law on the slot's own stamp: flipping the entry
    // makes the op Stale once, a regen clears it - and the regen really
    // moved, because this channel has room the orbit stands in (12 wide,
    // 6 tool: the stadium reaches 3 mm off-axis, the orbit 2).
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Slot1",
            "EM-0600-4FL",
            SrcRef::Stock,
            slot_params(),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Slot1").expect("generate"),
    );
    assert_eq!(status(&world, "Slot1"), OpStatus::Ok);
    let before = cache(&world, "Slot1").expect("cached").path;
    assert!(before.notes.len() == 1 && before.notes.iter().all(|n| n.starts_with(camcraft_app::operation::TOP_STOCK_WARNING)), "no entry fallback, only the independent Top/stock warning: {:?}", before.notes);
    assert_eq!(
        before.moves.iter().filter(|m| m.arc.is_some()).count(),
        0,
        "Plunge means no orbit in the file"
    );

    let mut edited = slot_params();
    let OpParams::Slot {
        ref mut entry,
        ref mut helix_radius,
        ..
    } = edited
    else {
        unreachable!("slot_params is a Slot");
    };
    *entry = camcraft_app::machining3d::Entry3D::Helix;
    *helix_radius = 2.0;
    history.execute(
        &world,
        EditOperationParams::new(&world, "Slot1", edited).expect("edit"),
    );
    assert_eq!(
        status(&world, "Slot1"),
        OpStatus::Stale,
        "the entry bytes ride the digest"
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Slot1").expect("regen"),
    );
    assert_eq!(status(&world, "Slot1"), OpStatus::Ok);
    let after = cache(&world, "Slot1").expect("cached").path;
    assert!(
        after.moves.iter().any(|m| m.arc.is_some()),
        "a channel this wide certifies the orbit - it must land"
    );
    assert!(after.notes.len() == 1 && after.notes.iter().all(|n| n.starts_with(camcraft_app::operation::TOP_STOCK_WARNING)), "a certified orbit adds no entry note; only the Top/stock warning remains: {:?}", after.notes);
    assert_ne!(
        before.moves, after.moves,
        "the entry edit changed the path, not just the stamp"
    );

    // Undo re-derives in both directions, the contour's law unchanged.
    assert!(history.undo(&world));
    assert_eq!(status(&world, "Slot1"), OpStatus::Stale, "derived, both ways");
    assert!(history.redo(&world));
    assert_eq!(status(&world, "Slot1"), OpStatus::Ok, "and exact");
}

#[test]
fn a_reimport_with_unchanged_geometry_keeps_operations_ok() {
    // The other half of the promise: re-running the same file over a model
    // must not churn the job. The signature is content, not a counter, so
    // identical geometry keeps every dependent operation Ok even as rev
    // ticks.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    let cube_v1 = cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0));
    history.execute(&world, ImportModel::from_mesh("part", cube_v1.clone()));
    let cmd = CreateOperation::new(
        &world,
        "Contour1",
        "EM-0600-4FL",
        SrcRef::Entity("part".into()),
        contour_params(),
    );
    history.execute(&world, cmd.expect("create"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Contour1").expect("generate"),
    );

    history.execute(
        &world,
        UpdateModel::new(&world, "part", cube_v1).expect("model exists"),
    );
    assert_eq!(
        status(&world, "Contour1"),
        OpStatus::Ok,
        "same content, same stamp - no churn from a re-import that changed nothing"
    );
}

/// Face parameters for the 10 x 10 x 5 solid the stock tests below cut.
fn face_params_5() -> OpParams {
    OpParams::Face { holes: Default::default(), blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(10.0),
        cut_z: Height::fixed(4.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

fn stock_facets(world: &World) -> std::sync::Arc<[camcraft_app::geometry::Triangle]> {
    let stock = model::find_by_name(world, STOCK_NAME).expect("stock exists");
    model::facets_of(world, &stock).expect("stock carries geometry")
}

#[test]
fn a_model_update_flows_through_a_frommodel_stock() {
    // M3.1 B, the propagation law: stock bought from a model is a copy,
    // so an update to the model must land on the copy inside the same
    // command - and the operations that mill that stock must see it.
    // The path here reads the stock, never the model directly: the only
    // way the update can reach this operation is through the material.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0400-2FL");
    let cube_v1 = cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0));
    history.execute(&world, ImportModel::from_mesh("block", cube_v1.clone()));
    let cmd = SetStock::new(
        &world,
        StockShape::FromModel {
            source: "block".into(),
        },
        Location::default(),
    );
    history.execute(&world, cmd.expect("from-model stock"));
    assert_eq!(
        stock_facets(&world),
        cube_v1.facets,
        "the copy starts honest"
    );
    let cmd = CreateOperation::new(
        &world,
        "Face1",
        "EM-0400-2FL",
        SrcRef::Stock,
        face_params_5(),
    );
    history.execute(&world, cmd.expect("create"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Face1").expect("generate"),
    );
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);

    // Same facet count, same box, different walls - the pre-M3.1 blind
    // spot, arriving this time through the stock.
    let cmd = UpdateModel::new(&world, "block", prism_10x10x5()).expect("model exists");
    history.execute(&world, cmd);
    assert_eq!(
        stock_facets(&world),
        prism_10x10x5().facets,
        "the material was re-derived with the model"
    );
    assert_eq!(
        status(&world, "Face1"),
        OpStatus::Stale,
        "the stock the operation mills is different metal now"
    );

    // One undo unit: model and material trade back together, and the
    // operation is Ok again without a regenerate - propagation is exact
    // in both directions.
    assert!(history.undo(&world));
    assert_eq!(stock_facets(&world), cube_v1.facets, "the old copy is back");
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);
    assert!(history.redo(&world));
    assert_eq!(stock_facets(&world), prism_10x10x5().facets);
    assert_eq!(status(&world, "Face1"), OpStatus::Stale);
}

#[test]
fn a_frommodel_stock_survives_its_source_being_deleted() {
    // The copy is the material, so the stamp digests the copy (M3.1 B):
    // with the source model gone the stock's facets have not moved, the
    // path would mill identically, and the operation is honestly Ok.
    let (world, mut history) = {
        let world = new_world();
        let mut history = History::default();
        add_tool(&world, &mut history, "EM-0400-2FL");
        history.execute(
            &world,
            ImportModel::from_mesh("block", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
        );
        history.execute(
            &world,
            SetStock::new(
                &world,
                StockShape::FromModel {
                    source: "block".into(),
                },
                Location::default(),
            )
            .expect("from-model stock"),
        );
        let cmd = CreateOperation::new(
            &world,
            "Face1",
            "EM-0400-2FL",
            SrcRef::Stock,
            face_params_5(),
        );
        history.execute(&world, cmd.expect("create"));
        history.execute(
            &world,
            GenerateOperation::new(&world, "Face1").expect("generate"),
        );
        (world, history)
    };
    let before = stock_facets(&world);
    let cmd = DeleteEntity::new(&world, "block").expect("delete");
    history.execute(&world, cmd);
    assert_eq!(
        stock_facets(&world),
        before,
        "the material kept its geometry"
    );
    assert_eq!(
        status(&world, "Face1"),
        OpStatus::Ok,
        "nothing a stock-based path reads has changed"
    );
    assert!(history.undo(&world));
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);
}

#[test]
fn a_model_update_does_not_touch_a_box_stock() {
    // The gate on the other side: propagation is only for stock that was
    // bought from the model being updated. A parametric box is no one's
    // copy, and an import must never rewrite it.
    let (world, mut history) = face_world();
    let before = stock_facets(&world);
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    let cmd = UpdateModel::new(&world, "part", prism_10x10x5()).expect("model exists");
    history.execute(&world, cmd);
    assert_eq!(stock_facets(&world), before, "the box is its own material");
    assert_eq!(
        status(&world, "Face1"),
        OpStatus::Ok,
        "the stock op never read the model"
    );
}

// ------------------------------------------------------------ M3.1 stage C:
// why an operation reads Stale (the live questions), and the batch fix
// that regenerates the whole job in one undo unit.

use camcraft_app::operation::HeightReference;

fn reason(world: &World, name: &str) -> Option<String> {
    let e = model::find_by_name(world, name).expect("entity exists");
    operation::stale_reason(world, &e)
}

#[test]
fn stale_reason_is_none_unless_the_operation_is_stale() {
    let (world, mut history) = face_world();
    assert_eq!(
        reason(&world, "Face1"),
        None,
        "Ok - there is nothing to explain"
    );
    // Never is not stale either: the status line already says "never".
    let cmd = CreateOperation::new(&world, "Face2", "EM-0400-2FL", SrcRef::Stock, face_params());
    history.execute(&world, cmd.expect("create"));
    assert_eq!(reason(&world, "Face2"), None);
}

#[test]
fn stale_reason_names_the_vanished_tool() {
    let (world, mut history) = face_world();
    let cmd = DeleteEntity::new(&world, "EM-0400-2FL").expect("delete");
    history.execute(&world, cmd);
    assert_eq!(status(&world, "Face1"), OpStatus::Stale);
    assert_eq!(
        reason(&world, "Face1").as_deref(),
        Some("its tool 'EM-0400-2FL' no longer exists")
    );
}

#[test]
fn stale_reason_names_a_deleted_geometry_source() {
    let (world, mut history) = box_part_world();
    let cmd = CreateOperation::new(
        &world,
        "Contour1",
        "EM-0600-4FL",
        SrcRef::Entity("part".into()),
        contour_params(),
    );
    history.execute(&world, cmd.expect("create"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Contour1").expect("generate"),
    );
    history.execute(&world, DeleteEntity::new(&world, "part").expect("delete"));
    assert_eq!(status(&world, "Contour1"), OpStatus::Stale);
    assert_eq!(
        reason(&world, "Contour1").as_deref(),
        Some("its geometry source 'part' was deleted")
    );
}

#[test]
fn stale_reason_asks_for_a_re_pick_when_the_feature_moved() {
    // The successor law of "operations reference features, not copies":
    // the id is a live question, and when it no longer resolves the
    // message names the action - re-pick, not re-import blindly.
    let (world, mut history) = box_part_world();
    let top = FeatureSel::Face(top_face_of(&world, "part"));
    let cmd = CreateOperation::new(
        &world,
        "C1",
        "EM-0600-4FL",
        sel("part", top),
        contour_params(),
    );
    history.execute(&world, cmd.expect("create"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "C1").expect("generate"),
    );
    let cmd = UpdateModel::new(&world, "part", prism_10x10x5()).expect("model exists");
    history.execute(&world, cmd);
    assert_eq!(status(&world, "C1"), OpStatus::Stale);
    assert_eq!(
        reason(&world, "C1").as_deref(),
        Some("the picked face of 'part' no longer exists - the geometry moved")
    );
}

#[test]
fn stale_reason_names_a_height_whose_source_is_gone() {
    // The tool and the geometry source all survive; what moved is the
    // SETUP - a plane measured from stock that is no longer there.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0400-2FL");
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 60.0,
                dy: 60.0,
                dz: 20.0,
            },
            Location::default(),
        )
        .expect("box stock"),
    );
    let params = OpParams::Face { holes: Default::default(), blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 5.0),
        cut_z: Height::fixed(4.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    let cmd = CreateOperation::new(
        &world,
        "Face1",
        "EM-0400-2FL",
        SrcRef::Entity("part".into()),
        params,
    );
    history.execute(&world, cmd.expect("create"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Face1").expect("generate"),
    );
    history.execute(
        &world,
        DeleteEntity::new(&world, STOCK_NAME).expect("delete stock"),
    );
    assert_eq!(status(&world, "Face1"), OpStatus::Stale);
    let why = reason(&world, "Face1").expect("stale");
    assert!(
        why.starts_with("a height is measured from") && why.contains("no stock yet"),
        "the message names the missing plane's source: {why}"
    );
}

#[test]
fn stale_reason_falls_back_when_every_input_survives() {
    // A tool edit moves no entity and no id - the numbers inside the
    // stamp changed, and the honest answer is the general one.
    let (world, mut history) = face_world();
    let mut fatter = catalog_tool("EM-0400-2FL");
    fatter.diameter = 8.0;
    history.execute(
        &world,
        ChangeTool::new(&world, "EM-0400-2FL", fatter).expect("edit"),
    );
    assert_eq!(status(&world, "Face1"), OpStatus::Stale);
    assert_eq!(
        reason(&world, "Face1").as_deref(),
        Some("the geometry or tool inputs changed since it was generated")
    );
}

#[test]
fn regenerate_all_is_empty_when_nothing_is_stale() {
    let (world, _history) = face_world();
    let cmd = RegenerateAll::new(&world);
    assert!(cmd.is_empty() && cmd.regenerated() == 0 && cmd.skipped().is_empty());
}

#[test]
fn regenerate_all_fixes_the_whole_job_in_one_undo_unit() {
    // The M3.1 payoff: one revision, two stale operations, one batch and
    // one Ctrl+Z back. Never and Ok operations are not the batch's
    // business - it regenerates what staleness marked, nothing else.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0400-2FL");
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    let cmd = CreateOperation::new(
        &world,
        "Face1",
        "EM-0400-2FL",
        SrcRef::Entity("part".into()),
        face_params_5(),
    );
    history.execute(&world, cmd.expect("create face"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Face1").expect("generate face"),
    );
    let cmd = CreateOperation::new(
        &world,
        "Contour1",
        "EM-0400-2FL",
        SrcRef::Entity("part".into()),
        contour_params(),
    );
    history.execute(&world, cmd.expect("create contour"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Contour1").expect("generate contour"),
    );

    history.execute(
        &world,
        UpdateModel::new(&world, "part", prism_10x10x5()).expect("update"),
    );
    assert_eq!(status(&world, "Face1"), OpStatus::Stale);
    assert_eq!(status(&world, "Contour1"), OpStatus::Stale);

    let cmd = RegenerateAll::new(&world);
    assert_eq!(cmd.regenerated(), 2);
    assert!(cmd.skipped().is_empty());
    history.execute(&world, cmd);
    // #145: Contour1 sorts ahead of Face1 and the face's verdict reads
    // it (#109) - the batch's second round re-runs the face against
    // the contour's new path, so one click converges.
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);
    assert_eq!(status(&world, "Contour1"), OpStatus::Ok);

    // One undo unit - the batch trades back together, both STALE again.
    assert!(history.undo(&world));
    assert_eq!(status(&world, "Face1"), OpStatus::Stale);
    assert_eq!(status(&world, "Contour1"), OpStatus::Stale);
    assert!(history.redo(&world));
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);
    assert_eq!(status(&world, "Contour1"), OpStatus::Ok);
}

#[test]
fn regenerate_all_skips_what_it_cannot_generate() {
    // Skip, not fatal: one operation whose tool vanished must not block
    // the rest of the job. The failed op keeps its old cache and its
    // STALE tag - validate early means a refusal writes nothing - and
    // the batch lists it by name with the refusal text.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0400-2FL");
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    let cmd = CreateOperation::new(
        &world,
        "Face1",
        "EM-0400-2FL",
        SrcRef::Entity("part".into()),
        face_params_5(),
    );
    history.execute(&world, cmd.expect("create face"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Face1").expect("generate face"),
    );
    let cmd = CreateOperation::new(
        &world,
        "Contour1",
        "EM-0600-4FL",
        SrcRef::Entity("part".into()),
        contour_params(),
    );
    history.execute(&world, cmd.expect("create contour"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Contour1").expect("generate contour"),
    );

    let face_cache = cache(&world, "Face1").expect("cache").path;
    history.execute(
        &world,
        UpdateModel::new(&world, "part", prism_10x10x5()).expect("update"),
    );
    history.execute(
        &world,
        DeleteEntity::new(&world, "EM-0400-2FL").expect("delete tool"),
    );

    let cmd = RegenerateAll::new(&world);
    assert_eq!(
        cmd.regenerated(),
        1,
        "Contour1 regenerates; Face1's generator refuses"
    );
    assert_eq!(cmd.skipped().len(), 1);
    assert_eq!(cmd.skipped()[0].0, "Face1");
    history.execute(&world, cmd);
    assert_eq!(status(&world, "Contour1"), OpStatus::Ok);
    assert_eq!(
        status(&world, "Face1"),
        OpStatus::Stale,
        "the skip stays honestly marked"
    );
    assert_eq!(
        cache(&world, "Face1").expect("cache").path,
        face_cache,
        "a refusal wrote nothing"
    );

    // Undo trades back the one thing the batch touched.
    assert!(history.undo(&world));
    assert_eq!(status(&world, "Contour1"), OpStatus::Stale);
    assert_eq!(status(&world, "Face1"), OpStatus::Stale);
}

/// Pocket3D parameters for the fixture's 30 x 30 x 5 cube: a ball-nose
/// plunge ladder down the middle of the model's own silhouette.
fn pocket3d_params_5() -> OpParams {
    let mut p = multiaxis_rough_pocket_twin();
    let OpParams::Pocket3D {
        top_z,
        bottom_z,
        stepdown,
        clearance_z,
        ..
    } = &mut p
    else {
        unreachable!()
    };
    *top_z = Height::fixed(5.0);
    *bottom_z = Height::fixed(0.0);
    *stepdown = 2.5;
    *clearance_z = Height::fixed(10.0);
    p
}

#[test]
fn the_shell_switch_sends_the_pocket_to_the_offset_engine() {
    // M2.5 F B, the app seam: one OpParams bool is the whole wire —
    // generate dispatches pocket3d_offset over the plain ladder with
    // nothing else changed. The cube's walls are vertical, so the
    // shell's tips fall exactly on the planes (the roof over every
    // decision disc is the open column above); the proof the flag
    // moved is the XY plan disagreeing - offset rings against the
    // zigzag - while BOTH roads verify clean. Off staying byte-for-byte
    // the old ladder is the .nc goldens' claim; this is the ON half.
    let run = |shell: bool| {
        let world = new_world();
        let mut history = History::default();
        // The stock stands to the cube's top, Z 5 (#96): the pocket's
        // top is 5, so its rapids under clearance 10 are air - the old
        // 20 mm block put them into uncut stock.
        box_stock(&world, &mut history, 60.0, 60.0, 5.0);
        add_tool(&world, &mut history, "BN-0600");
        history.execute(
            &world,
            ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(30.0, 30.0, 5.0))),
        );
        let mut p = pocket3d_params_5();
        let OpParams::Pocket3D { shell: s, .. } = &mut p else {
            unreachable!()
        };
        *s = shell;
        let cmd =
            CreateOperation::new(&world, "P", "BN-0600", SrcRef::Entity("part".into()), p)
                .expect("create op");
        history.execute(&world, cmd);
        let cmd = GenerateOperation::new(&world, "P").expect("generate op");
        history.execute(&world, cmd);
        assert_eq!(status(&world, "P"), OpStatus::Ok, "shell={shell}");
        let t = cache(&world, "P").expect("cache");
        assert!(t.warnings.is_empty(), "shell={shell}: {:?}", t.warnings);
        t.path
    };
    let ladder = run(false);
    let shells = run(true);
    assert!(!ladder.is_empty() && !shells.is_empty());
    assert_ne!(
        ladder, shells,
        "the flag must move the XY plan, not just its name"
    );
}

#[test]
fn threaded_batch_matches_the_serial_loop_bit_for_bit() {
    // M5.3 B's determinism claim, tested as a law: `RegenerateAll`
    // stages each round's stale jobs against the world as it stands,
    // then FINISHES them on rayon workers; its rounds (#145) re-run an
    // op whose program ahead was regenerated in the same batch. The
    // serial loop it must equal generates and applies op by op in
    // program order: both roads must write down identical bytes -
    // paths, warnings, stamps - in identical tree order.
    let build = || {
        let world = new_world();
        let mut history = History::default();
        // Box stock: what every Entity-source op reads at collect
        // time as the swarf column around the smaller model.
        box_stock(&world, &mut history, 60.0, 60.0, 20.0);
        add_tool(&world, &mut history, "EM-0400-2FL");
        add_tool(&world, &mut history, "BN-0600");
        history.execute(
            &world,
            ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(30.0, 30.0, 5.0))),
        );
        // Names encode the program order on purpose (name order IS
        // post order), and the three jobs read three different
        // corners of the world at collect time: the pocket's prior
        // state (nothing ahead of it - the empty-mask branch), the
        // contour's section height, and the face's From-referenced
        // heights. The batch/serial law this test proves is about
        // the seam; the masked rest paths ride it through the
        // remachining suite, whose ops generate through this same
        // collect_job/run_job pair.
        let mut rim = contour_params();
        if let OpParams::Contour { bottom_z, .. } = &mut rim {
            *bottom_z = Height::fixed(4.0);
        }
        for (name, tool, params) in [
            ("APocket3D", "BN-0600", pocket3d_params_5()),
            ("BContour", "EM-0400-2FL", rim),
            ("CFrame", "EM-0400-2FL", face_params_5()),
        ] {
            let cmd =
                CreateOperation::new(&world, name, tool, SrcRef::Entity("part".into()), params);
            history.execute(&world, cmd.expect("create op"));
            let cmd = GenerateOperation::new(&world, name).expect("generate op");
            history.execute(&world, cmd);
        }
        // The revision that drives everything Stale: a cube one mm
        // narrower in x. A geometry-kind change (a diamond prism)
        // would read the pocket's ladder off its envelope law too -
        // this is the same kind of model, honestly moved.
        history.execute(
            &world,
            UpdateModel::new(
                &world,
                "part",
                cube(Point3::ZERO, Point3::new(29.0, 30.0, 5.0)),
            )
            .expect("update"),
        );
        for name in ["APocket3D", "BContour", "CFrame"] {
            assert_eq!(status(&world, name), OpStatus::Stale);
        }
        (world, history)
    };

    // The batch: stage all (world reads, tree order), finish on rayon, apply once.
    let (world_a, mut history_a) = build();
    let batch = RegenerateAll::new(&world_a);
    assert_eq!(batch.regenerated() + batch.skipped().len(), 3);
    assert!(batch.skipped().is_empty(), "{:?}", batch.skipped());
    history_a.execute(&world_a, batch);

    // The serial loop: each stale op generated and applied in program
    // order, so every op reads the program ahead as regenerated (#109,
    // #145) - the job the batch's rounds converge to.
    let (world_b, mut history_b) = build();
    let stale: Vec<String> = camcraft_app::setup::operation_order(&world_b)
        .into_iter()
        .filter(|(_, e)| operation::status_of(&world_b, e) == OpStatus::Stale)
        .map(|(n, _)| n)
        .collect();
    assert_eq!(stale.len(), 3);
    for name in &stale {
        let part = GenerateOperation::new(&world_b, name).expect("serial generate");
        history_b.execute(&world_b, part);
    }
    for name in &stale {
        assert_eq!(status(&world_a, name), OpStatus::Ok, "{name}: one batch converges");
        assert_eq!(status(&world_b, name), OpStatus::Ok, "{name}: the ordered loop converges");
    }

    // Bit-for-bit: the path bytes, the warnings, and the stamps agree.
    for name in &stale {
        assert_eq!(
            cache(&world_a, name),
            cache(&world_b, name),
            "{name}: the batch's path must be the serial loop's bytes"
        );
        let e_a = model::find_by_name(&world_a, name).expect("op exists");
        let e_b = model::find_by_name(&world_b, name).expect("op exists");
        assert_eq!(
            model::operation_of(&world_a, &e_a),
            model::operation_of(&world_b, &e_b),
            "{name}: the batch's stamp must be the serial loop's stamp"
        );
    }
}

#[test]
fn delete_operation_round_trips_with_its_cache() {
    let (world, mut history) = face_world();
    let op = model::operation_of(&world, &model::find_by_name(&world, "Face1").expect("op"))
        .expect("definition");
    let first = cache(&world, "Face1").expect("cache").path;

    history.execute(
        &world,
        DeleteOperation::new(&world, "Face1").expect("delete"),
    );
    assert!(
        model::find_by_name(&world, "Face1").is_none(),
        "entity and cache are gone together"
    );

    assert!(history.undo(&world));
    let e = model::find_by_name(&world, "Face1").expect("restored");
    assert!(model::is_operation(&world, &e));
    assert_eq!(
        model::operation_of(&world, &e),
        Some(op),
        "definition restored exactly"
    );
    assert_eq!(
        cache(&world, "Face1").map(|c| c.path),
        Some(first.clone()),
        "cache restored too - not a hollow entity"
    );

    // regenerating now replays the identical path (deterministic generators)
    history.execute(
        &world,
        GenerateOperation::new(&world, "Face1").expect("regenerate"),
    );
    assert_eq!(cache(&world, "Face1").map(|c| c.path), Some(first));
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);
}

#[test]
fn delete_entity_restores_camparts() {
    // The generic delete must carry ToolDef/Operation/Toolpath through its
    // snapshot, or undo recreates a hollow entity and re-adding conflicts.
    let (world, mut history) = face_world();
    history.execute(
        &world,
        DeleteEntity::new(&world, "Face1").expect("generic delete"),
    );
    assert!(model::find_by_name(&world, "Face1").is_none());
    assert!(history.undo(&world));
    let e = model::find_by_name(&world, "Face1").expect("restored");
    assert!(model::is_operation(&world, &e), "kind survives");
    assert!(model::toolpath_of(&world, &e).is_some(), "cache survives");
    assert_eq!(status(&world, "Face1"), OpStatus::Ok);

    history.execute(
        &world,
        DeleteEntity::new(&world, "EM-0400-2FL").expect("delete tool"),
    );
    assert!(history.undo(&world));
    let t = model::find_by_name(&world, "EM-0400-2FL").expect("tool restored");
    assert!(model::is_tool(&world, &t));
    assert_eq!(
        model::tool_of(&world, &t),
        Some(catalog_tool("EM-0400-2FL"))
    );
}

#[test]
fn tools_and_operations_stay_out_of_the_models_list() {
    let (world, _history) = face_world();
    let names: Vec<String> = model::model_entities(&world)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert!(
        names.is_empty(),
        "setup furniture and CAM objects are not models: {names:?}"
    );
    assert_eq!(
        model::named_entities(&world).len(),
        3,
        "Stock, tool, operation"
    );
}

#[test]
fn contour_on_a_model_box_generates() {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    let cmd = CreateOperation::new(
        &world,
        "Contour1",
        "EM-0600-4FL",
        SrcRef::Entity("part".into()),
        contour_params(),
    );
    history.execute(&world, cmd.expect("create"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "Contour1").expect("generate"),
    );
    let c = cache(&world, "Contour1").expect("cache");
    assert!(!c.path.is_empty());
    assert!(c.path.is_chained());
    // world-space Aabb is in the digest: moving the model goes stale
    let e = model::find_by_name(&world, "part").expect("part");
    model::set_transform(&world, &e, at(3.0, 0.0, 0.0));
    assert_eq!(status(&world, "Contour1"), OpStatus::Stale);
}

#[test]
fn region_of_is_the_preview_of_what_generate_would_cut() {
    // The viewport's region outline: the generators' own region
    // construction, minus tool and path - so it answers "what gets cut"
    // for operations that have never been generated.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);

    let trace = Operation {
        tool: String::new(),
        src: SrcRef::Stock,
        params: OpParams::Trace {
            tilt: None,
            clearance_z: Height::fixed(25.0),
            retract_z: None,
            top_z: Height::fixed(10.0),
            bottom_z: Height::fixed(5.0),
            stepdown: 5.0,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
        },
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    assert_eq!(operation::region_z(&world, None, None, &trace.params), Some(10.0));
    let region = operation::region_of(&world, &trace).expect("a box sections cleanly at 10 mm");
    assert_eq!(region.loops().len(), 1);
    // The section keeps the mesh's per-face triangle splits, so the ring
    // is 8 collinear points tracing the same 60x60 rectangle.
    let pts = region.loops()[0].points();
    assert!(pts.len() >= 4);
    let extent = |get: fn(&camcraft_app::geo2d::Point2) -> f64| -> f64 {
        let lo = pts.iter().map(get).fold(f64::MAX, f64::min);
        let hi = pts.iter().map(get).fold(f64::MIN, f64::max);
        hi - lo
    };
    assert!((extent(|p| p.x) - 60.0).abs() < 1e-9, "60 mm across X");
    assert!((extent(|p| p.y) - 60.0).abs() < 1e-9, "60 mm across Y");
    assert!(
        pts.iter()
            .all(|p| p.x.abs() <= 30.0 + 1e-9 && p.y.abs() <= 30.0 + 1e-9)
    );

    // Face: the silhouette footprint of the same box (no tool consulted).
    let face = Operation {
        tool: String::new(),
        src: SrcRef::Stock,
        params: OpParams::Face { holes: Default::default(), blend_turns: false,
            tilt: None,
            stepover: 3.0,
            clearance_z: Height::fixed(25.0),
            cut_z: Height::fixed(19.5),
            top_z: None,
            stepdown: None,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
        },
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    assert_eq!(operation::region_z(&world, None, None, &face.params), Some(19.5));
    assert!(
        !operation::region_of(&world, &face)
            .expect("footprint")
            .is_empty()
    );

    // A vanished source previews NOTHING, not a stale outline.
    let ghost = Operation {
        tool: String::new(),
        src: SrcRef::Entity("gone".into()),
        params: trace.params.clone(),
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    assert_eq!(operation::region_of(&world, &ghost), None);
}

// ===================== M1.5 step 4: selection sources =====================

use camcraft_app::geometry::Triangle;
use camcraft_app::geometry::feature::{self, FaceKind};
use camcraft_app::operation::{FeatureSel, GeoSel};

/// Planar face whose plane sits highest along +z - the box's top. Face
/// normals are sign-canonical, so both horizontal faces report (0,0,1)
/// and only the signed offset tells top from bottom.
fn top_face_of(world: &World, name: &str) -> u64 {
    let e = model::find_by_name(world, name).expect("part exists");
    let f = model::features_of(world, &e).expect("features");
    f.faces
        .iter()
        .filter(|fa| matches!(fa.kind, FaceKind::Planar) && fa.normal[2].abs() > 0.9)
        .max_by(|a, b| a.offset.total_cmp(&b.offset))
        .map(|fa| fa.id)
        .expect("a planar top face")
}

/// Planar face id of one vertical wall.
fn wall_face_of(world: &World, name: &str) -> u64 {
    let e = model::find_by_name(world, name).expect("part exists");
    let f = model::features_of(world, &e).expect("features");
    f.faces
        .iter()
        .find(|fa| matches!(fa.kind, FaceKind::Planar) && fa.normal[2].abs() < 0.5)
        .map(|fa| fa.id)
        .expect("a vertical wall face")
}

/// The rim chain of a flat n-gon disc: closed at 16 chords.
fn rim_chain_of(world: &World, name: &str) -> u64 {
    let e = model::find_by_name(world, name).expect("part exists");
    let f = model::features_of(world, &e).expect("features");
    feature::chains(&f)
        .into_iter()
        .find(|c| c.closed)
        .map(|c| c.id)
        .expect("the rim chains closed at this tessellation")
}

/// Any chain of an entity - a box's are all single open edges, which is
/// fine when only the chain's identity (not its region) is under test.
fn any_chain_of(world: &World, name: &str) -> u64 {
    let e = model::find_by_name(world, name).expect("part exists");
    let f = model::features_of(world, &e).expect("features");
    feature::chains(&f)
        .into_iter()
        .next()
        .map(|c| c.id)
        .expect("a faceted model has chains")
}

/// A 16-chord flat disc of radius r at z = 0, as an importable mesh.
fn disc_mesh(r: f64) -> camcraft_app::geometry::Mesh {
    let tau = std::f64::consts::TAU;
    let p = |i: usize| {
        let a = tau * i as f64 / 16.0;
        Point3::new(r * a.cos(), r * a.sin(), 0.0)
    };
    let facets: Vec<Triangle> = (0..16)
        .map(|i| Triangle::new(Point3::new(0.0, 0.0, 0.0), p(i), p(i + 1)))
        .collect();
    camcraft_app::geometry::Mesh::from_facets(facets)
}

fn sel(entity: &str, feature: FeatureSel) -> SrcRef {
    SrcRef::Selection(vec![GeoSel {
        entity: entity.to_string(),
        feature,
    }])
}

fn face_sel_params() -> OpParams {
    OpParams::Face { holes: Default::default(), blend_turns: false,
        tilt: None,
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

fn box_part_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    (world, history)
}

#[test]
fn a_selected_face_bounds_facing_and_contour() {
    let (world, mut history) = box_part_world();
    let top = FeatureSel::Face(top_face_of(&world, "part"));

    // Contour: the face's outline is the boundary; generation is the
    // normal command path end to end.
    let cmd = CreateOperation::new(
        &world,
        "C1",
        "EM-0600-4FL",
        sel("part", top),
        contour_params(),
    );
    history.execute(&world, cmd.expect("create contour on selection"));
    let op =
        model::operation_of(&world, &model::find_by_name(&world, "C1").expect("C1")).expect("op");
    let region = operation::region_of(&world, &op).expect("the picked face bounds a region");
    assert_eq!(region.loops().len(), 1, "one square boundary, nothing else");
    let xs: Vec<f64> = region.loops()[0].points().iter().map(|p| p.x).collect();
    let ys: Vec<f64> = region.loops()[0].points().iter().map(|p| p.y).collect();
    assert!(xs.iter().all(|x| (0.0..=10.0).contains(x)) && xs.iter().any(|x| *x > 9.0));
    assert!(ys.iter().all(|y| (0.0..=10.0).contains(y)) && ys.iter().any(|y| *y > 9.0));
    history.execute(
        &world,
        GenerateOperation::new(&world, "C1").expect("generate"),
    );
    assert!(!cache(&world, "C1").expect("cache").path.is_empty());
    assert_eq!(status(&world, "C1"), OpStatus::Ok);

    // Face: same region, cut plane from the parameter.
    let cmd = CreateOperation::new(
        &world,
        "F1",
        "EM-0600-4FL",
        sel("part", top),
        face_sel_params(),
    );
    history.execute(&world, cmd.expect("create face on selection"));
    let op =
        model::operation_of(&world, &model::find_by_name(&world, "F1").expect("F1")).expect("op");
    assert!(
        !operation::region_of(&world, &op)
            .expect("footprint")
            .is_empty()
    );
}

#[test]
fn a_vertical_or_tilted_pick_refuses_with_the_fix_in_the_message() {
    let (world, _history) = box_part_world();
    let wall = FeatureSel::Face(wall_face_of(&world, "part"));
    let op = |params: OpParams| Operation {
        tool: "EM-0600-4FL".into(),
        src: sel("part", wall),
        params,
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    // Facing a wall: refused, and the message says what to pick instead.
    let face = op(face_sel_params());
    assert_eq!(operation::region_of(&world, &face), None);
    let e = operation::generate(&world, &face, "").expect_err("wall cannot be faced");
    assert!(matches!(e, camcraft_app::machining2d::GenError::Invalid(_)));
    assert!(e.to_string().contains("not horizontal"), "{e}");

    // Contouring a whole wall face: its rim collapses to a line from
    // above; the honest boundary is the top edge chain, and the refusal
    // says so.
    let contour = op(contour_params());
    assert_eq!(operation::region_of(&world, &contour), None);
    let e =
        operation::generate(&world, &contour, "").expect_err("a wall face bounds no area in XY");
    assert!(e.to_string().contains("projects to a line"), "{e}");
}

#[test]
fn closed_chain_rims_bound_and_double_picks_collapse_to_one_loop() {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("disc", disc_mesh(12.0)));

    let rim = FeatureSel::Chain(rim_chain_of(&world, "disc"));
    let op = Operation {
        tool: "EM-0600-4FL".into(),
        src: sel("disc", rim),
        params: contour_params(),
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    let region = operation::region_of(&world, &op).expect("the rim chain bounds the disc");
    assert_eq!(region.loops().len(), 1);
    assert!(
        region.loops()[0].points().len() >= 16,
        "the chord ring survives"
    );

    // The same rim picked twice is one boundary, not a zero-wide sliver.
    let twice = Operation {
        src: SrcRef::Selection(vec![
            GeoSel {
                entity: "disc".into(),
                feature: rim,
            },
            GeoSel {
                entity: "disc".into(),
                feature: rim,
            },
        ]),
        ..op.clone()
    };
    let r2 = operation::region_of(&world, &twice).expect("deduped");
    assert_eq!(r2.loops().len(), 1, "coincident rims count once");

    // The same rim picked as the sheet's face is the same ring: a face's
    // boundary loop and its chain agree, and together they stay one loop.
    // A sheet face whose ring-walk failed to close (LoopKind::Open) is
    // the case the "open sheet" refusal serves; this rim closes cleanly.
    let sheet = FeatureSel::Face({
        let e = model::find_by_name(&world, "disc").expect("disc");
        let f = model::features_of(&world, &e).expect("features");
        f.faces
            .iter()
            .find(|fa| matches!(fa.kind, FaceKind::Planar))
            .map(|fa| fa.id)
            .expect("the disc is one planar face")
    });
    let sheet_op = Operation {
        src: sel("disc", sheet),
        ..op.clone()
    };
    let r3 = operation::region_of(&world, &sheet_op).expect("the face ring bounds the disc");
    assert_eq!(r3.loops().len(), 1);
    let both = Operation {
        src: SrcRef::Selection(vec![
            GeoSel {
                entity: "disc".into(),
                feature: sheet,
            },
            GeoSel {
                entity: "disc".into(),
                feature: rim,
            },
        ]),
        ..op.clone()
    };
    let r4 = operation::region_of(&world, &both).expect("face+rim dedupe");
    assert_eq!(
        r4.loops().len(),
        1,
        "one ring whether picked as face, chain, or both"
    );
}

#[test]
fn pocket_refuses_an_open_run() {
    let (world, _history) = box_part_world();
    // A box edge is its own chain at this tessellation (90-degree
    // corners never join) - exactly the open run a Pocket must refuse.
    let e = model::find_by_name(&world, "part").expect("part");
    let f = model::features_of(&world, &e).expect("features");
    let open = feature::chains(&f)
        .into_iter()
        .find(|c| !c.closed)
        .map(|c| FeatureSel::Chain(c.id))
        .expect("box chains are open runs");
    let op = Operation {
        tool: "EM-0600-4FL".into(),
        src: sel("part", open),
        params: OpParams::Pocket {
            links: camcraft_app::links::Links::PLANAR,
            tilt: None,
            stepover: 3.0,
            stock_to_leave: 0.0,
            clearance_z: Height::fixed(10.0),
            retract_z: None,
            top_z: Height::fixed(5.0),
            bottom_z: Height::fixed(0.0),
            stepdown: 2.5,
            feed: 800.0,
            plunge_feed: 200.0,
            climb: true,
            strategy: camcraft_app::machining2d::PocketStrategy::Zigzag,
            entry: camcraft_app::machining3d::Entry3D::Plunge,
            helix_radius: 1.0,
            optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
            blend_turns: false,
        },
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    assert_eq!(operation::region_of(&world, &op), None, "nothing to draw");
    let err = operation::generate(&world, &op, "").expect_err("pocket refuses");
    assert!(err.to_string().contains("closed chain"), "{err}");
}

#[test]
fn selection_staleness_tracks_content_not_counts() {
    let (world, mut history) = box_part_world();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    let top_a = top_face_of(&world, "part");
    let src_a = sel("part", FeatureSel::Face(top_a));
    let params = face_sel_params();
    let cmd = CreateOperation::new(&world, "S1", "EM-0600-4FL", src_a.clone(), params.clone());
    history.execute(&world, cmd.expect("create"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "S1").expect("generate"),
    );
    assert_eq!(status(&world, "S1"), OpStatus::Ok);

    // Swapping the picked face - same entity, same item count - moves
    // the digest. A stamp keyed on counts would miss exactly this.
    let wall = wall_face_of(&world, "part");
    let src_b = sel("part", FeatureSel::Face(wall));
    let sa = operation::current_stamp(&world, "EM-0600-4FL", &src_a, &params, "S1", None);
    let sb = operation::current_stamp(&world, "EM-0600-4FL", &src_b, &params, "S1", None);
    assert_ne!(sa, sb, "a different face is different geometry");

    // Moving the part moves it too (the region swings out from under
    // the cut), and moving it back lands on the stored stamp again.
    let part = model::find_by_name(&world, "part").expect("part");
    model::set_transform(&world, &part, at(4.0, 0.0, 0.0));
    assert_eq!(status(&world, "S1"), OpStatus::Stale);
    model::set_transform(&world, &part, at(0.0, 0.0, 0.0));
    assert_eq!(status(&world, "S1"), OpStatus::Ok);

    // Face vs Chain on one entity are distinguishable, and so is item
    // order (the pick list is the user's, not a bag).
    let chain = FeatureSel::Chain(any_chain_of(&world, "part"));
    let sc = operation::current_stamp(
        &world,
        "EM-0600-4FL",
        &sel("part", chain),
        &params,
        "S1",
        None,
    );
    assert_ne!(sa, sc);
    let ab = SrcRef::Selection(vec![
        GeoSel {
            entity: "part".into(),
            feature: FeatureSel::Face(top_a),
        },
        GeoSel {
            entity: "part".into(),
            feature: FeatureSel::Face(wall),
        },
    ]);
    let ba = SrcRef::Selection(vec![
        GeoSel {
            entity: "part".into(),
            feature: FeatureSel::Face(wall),
        },
        GeoSel {
            entity: "part".into(),
            feature: FeatureSel::Face(top_a),
        },
    ]);
    assert_ne!(
        operation::current_stamp(&world, "EM-0600-4FL", &ab, &params, "S1", None),
        operation::current_stamp(&world, "EM-0600-4FL", &ba, &params, "S1", None),
    );
}

#[test]
fn a_vanished_selection_previews_nothing_and_goes_stale() {
    let (world, mut history) = box_part_world();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    let top = FeatureSel::Face(top_face_of(&world, "part"));
    let cmd = CreateOperation::new(
        &world,
        "V1",
        "EM-0600-4FL",
        sel("part", top),
        face_sel_params(),
    );
    history.execute(&world, cmd.expect("create"));
    history.execute(
        &world,
        GenerateOperation::new(&world, "V1").expect("generate"),
    );
    assert_eq!(status(&world, "V1"), OpStatus::Ok);

    history.execute(
        &world,
        DeleteEntity::new(&world, "part").expect("delete part"),
    );
    let op =
        model::operation_of(&world, &model::find_by_name(&world, "V1").expect("V1")).expect("op");
    assert_eq!(
        operation::region_of(&world, &op),
        None,
        "a dead pick draws nothing"
    );
    assert_eq!(status(&world, "V1"), OpStatus::Stale);
}

#[test]
fn selection_commands_validate_at_construction() {
    let (world, mut history) = box_part_world();
    add_tool(&world, &mut history, "EM-0400-2FL");
    // Empty picks, vanished entities and offsets never reach the world.
    assert!(
        CreateOperation::new(
            &world,
            "B",
            "EM-0400-2FL",
            SrcRef::Selection(vec![]),
            face_params()
        )
        .is_err()
    );
    let bad = sel("gone", FeatureSel::Face(7));
    assert!(matches!(
        CreateOperation::new(&world, "B", "EM-0400-2FL", bad, face_params()),
        Err(CommandError::NotFound(_))
    ));
    let offset = sel("G54", FeatureSel::Face(7));
    assert!(CreateOperation::new(&world, "B", "EM-0400-2FL", offset, face_params()).is_err());
    let blank = SrcRef::Selection(vec![GeoSel {
        entity: String::new(),
        feature: FeatureSel::Face(7),
    }]);
    assert!(CreateOperation::new(&world, "B", "EM-0400-2FL", blank, face_params()).is_err());
    // A real pick creates cleanly (the id itself is only checked at
    // generation - it is a geometry hash, not a pointer).
    let top = FeatureSel::Face(top_face_of(&world, "part"));
    assert!(
        CreateOperation::new(&world, "OK", "EM-0400-2FL", sel("part", top), face_params()).is_ok()
    );
}

// ============== M1.5 step 8: selection-aware generation ==============

use camcraft_app::cl::MoveKind;
use camcraft_app::verify::Warning;

/// An open chain whose two ends sit at distinct XY points - a horizontal
/// rim edge, the one kind of box run a centerline pass is about.
fn open_run_of(world: &World, name: &str) -> u64 {
    let e = model::find_by_name(world, name).expect("part exists");
    let f = model::features_of(world, &e).expect("features");
    feature::chains(&f)
        .into_iter()
        .find(|c| {
            !c.closed && {
                let p = feature::chain_points(&f, c);
                p.len() >= 2 && (p[0][0] != p[1][0] || p[0][1] != p[1][1])
            }
        })
        .map(|c| c.id)
        .expect("a box has horizontal rim edges")
}

#[test]
fn an_open_run_contours_on_its_centerline() {
    let (world, mut history) = box_part_world();
    let run = open_run_of(&world, "part");
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Edge1",
            "EM-0600-4FL",
            sel("part", FeatureSel::Chain(run)),
            contour_params(),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Edge1").expect("generate"),
    );

    // Preview: the run bounds no area, so no region outline - the
    // centerline stands in for it, and it is exactly what the path rides.
    let op = model::operation_of(&world, &model::find_by_name(&world, "Edge1").expect("op"))
        .expect("op");
    assert!(
        operation::region_of(&world, &op).is_none(),
        "a run bounds no area"
    );
    assert_eq!(
        operation::chains_of(&world, &op).len(),
        1,
        "one run to draw"
    );

    let path = cache(&world, "Edge1").expect("cache").path;
    assert!(path.is_chained(), "one continuous path");
    assert!(path.all_finite());
    let e = model::find_by_name(&world, "part").expect("part");
    let f = model::features_of(&world, &e).expect("features");
    let c = feature::chains(&f)
        .into_iter()
        .find(|c| c.id == run)
        .expect("chain");
    let pts = feature::chain_points(&f, &c);
    // contour_params: 5.0 -> 0.0 at 2.5 - three levels, one plunge each,
    // Lead::None so the plunge lands exactly on the stored start vertex.
    let levels = [5.0, 2.5, 0.0];
    let plunges: Vec<_> = path
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Plunge)
        .collect();
    assert_eq!(plunges.len(), levels.len());
    for (lvl, m) in plunges.iter().enumerate() {
        assert_eq!(
            (m.to.x, m.to.y),
            (pts[0][0], pts[0][1]),
            "plunge {lvl} enters at the run start, not short of it"
        );
        assert_eq!(m.to.z, levels[lvl]);
    }
    for (lvl, &z) in levels.iter().enumerate() {
        let feeds: Vec<_> = path
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Feed && m.to.z == z)
            .collect();
        assert_eq!(
            feeds.len(),
            pts.len() - 1,
            "one feed per segment, level {lvl}"
        );
        let last = feeds.last().expect("a level feeds somewhere");
        assert_eq!(
            (last.to.x, last.to.y),
            (pts[pts.len() - 1][0], pts[pts.len() - 1][1]),
            "the run stops at its end, never beyond"
        );
    }
}

// ============ M1.5.1 step 4: picked runs stitch into rings ============

/// A flat disc whose rim carries two hard creases - the seam vertices at
/// 0 and 180 degrees sit 15% proud of the circle, putting a 61-degree
/// chord turn past the 30-degree merge angle there while every other
/// turn stays under it. The chain walk cannot close it, so the rim
/// arrives as exactly two open half-runs whose ends meet end to end:
/// the split-rim shape the M1.5.1 hand-test hit on the STEP bore.
fn split_rim_disc(r: f64) -> camcraft_app::geometry::Mesh {
    let tau = std::f64::consts::TAU;
    let p = |i: usize| {
        let a = tau * i as f64 / 16.0;
        let rr = if i.is_multiple_of(8) { r * 1.15 } else { r };
        Point3::new(rr * a.cos(), rr * a.sin(), 0.0)
    };
    let facets: Vec<Triangle> = (0..16)
        .map(|i| Triangle::new(Point3::new(0.0, 0.0, 0.0), p(i), p(i + 1)))
        .collect();
    camcraft_app::geometry::Mesh::from_facets(facets)
}

/// The open runs an entity's chains carry, in chain order.
fn open_runs_of(world: &World, name: &str) -> Vec<u64> {
    let e = model::find_by_name(world, name).expect("part exists");
    let f = model::features_of(world, &e).expect("features");
    feature::chains(&f)
        .into_iter()
        .filter(|c| !c.closed)
        .map(|c| c.id)
        .collect()
}

#[test]
fn two_open_runs_stitch_into_one_boundary() {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("disc", split_rim_disc(12.0)));
    let runs = open_runs_of(&world, "disc");
    assert_eq!(runs.len(), 2, "the creased rim arrives as two open runs");

    let op = Operation {
        tool: "EM-0600-4FL".into(),
        src: SrcRef::Selection(
            runs.iter()
                .map(|&id| GeoSel {
                    entity: "disc".into(),
                    feature: FeatureSel::Chain(id),
                })
                .collect(),
        ),
        params: face_sel_params(),
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    // The hand-test's exact failure ("nothing in the selection bounds an
    // area") is gone: two picked runs that spell one loop bound one area.
    let region = operation::region_of(&world, &op).expect("stitched runs bound the disc");
    assert_eq!(region.loops().len(), 1, "one ring, not a pair of arcs");
    let ring = &region.loops()[0];
    assert!(
        ring.points().len() >= 16,
        "all 16 rim vertices survive the seam dedupe"
    );
    let want = std::f64::consts::PI * 12.0 * 12.0;
    let got = ring.area().abs();
    assert!(
        (got - want).abs() / want < 0.05,
        "the stitched ring is the disc: {got} vs {want}"
    );
}

#[test]
fn stitching_is_order_independent() {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("disc", split_rim_disc(12.0)));
    let runs = open_runs_of(&world, "disc");
    assert_eq!(runs.len(), 2);
    let pick = |order: [usize; 2]| -> Operation {
        Operation {
            tool: "EM-0600-4FL".into(),
            src: SrcRef::Selection(
                order
                    .iter()
                    .map(|&k| GeoSel {
                        entity: "disc".into(),
                        feature: FeatureSel::Chain(runs[k]),
                    })
                    .collect(),
            ),
            params: face_sel_params(),
            stamp: None,
            setup: None,
            area: Default::default(),
            coolant: Default::default(),
            after: Default::default(),
            suppressed: false,
        }
    };
    let ab = operation::region_of(&world, &pick([0, 1])).expect("first, then second");
    let ba = operation::region_of(&world, &pick([1, 0])).expect("second, then first");
    assert_eq!(ab.loops().len(), 1);
    assert_eq!(ba.loops().len(), 1);
    // The same ring, walked from either end: identical area, and a
    // pick-order shuffle must never produce two loops or zero.
    assert!(
        (ab.loops()[0].area() - ba.loops()[0].area()).abs() < 1e-9,
        "the ring is the ring whatever the pick order"
    );
}

#[test]
fn a_single_arc_stays_open() {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("disc", split_rim_disc(12.0)));
    let runs = open_runs_of(&world, "disc");
    assert_eq!(runs.len(), 2);
    // One half-rim alone has nothing to close against - it remains the
    // open run it always was: no boundary, and a Contour still
    // centerlines it end to end.
    let op = Operation {
        tool: "EM-0600-4FL".into(),
        src: sel("disc", FeatureSel::Chain(runs[0])),
        params: contour_params(),
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    assert!(
        operation::region_of(&world, &op).is_none(),
        "an unpaired run bounds nothing"
    );
    let cs = operation::chains_of(&world, &op);
    assert_eq!(cs.len(), 1, "one run to draw");
    assert_eq!(cs[0].len(), 9, "the half-rim centerline, seam to seam");
}

#[test]
fn duplicate_seam_points_do_not_break_push_loop() {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("disc", split_rim_disc(12.0)));
    let runs = open_runs_of(&world, "disc");
    assert_eq!(runs.len(), 2);
    // The stitched ring concatenates runs that share their seam vertex,
    // so repeats ride in at every seam. Picking one run twice adds a
    // repeat the stitcher must ignore and the routing must dedupe:
    // still exactly one boundary, never a sliver or a second pass.
    let op = Operation {
        tool: "EM-0600-4FL".into(),
        src: SrcRef::Selection(vec![
            GeoSel {
                entity: "disc".into(),
                feature: FeatureSel::Chain(runs[0]),
            },
            GeoSel {
                entity: "disc".into(),
                feature: FeatureSel::Chain(runs[0]),
            },
            GeoSel {
                entity: "disc".into(),
                feature: FeatureSel::Chain(runs[1]),
            },
        ]),
        params: face_sel_params(),
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    let region = operation::region_of(&world, &op).expect("the ring survives the doubled seam");
    assert_eq!(
        region.loops().len(),
        1,
        "one ring, whatever the pick repeats"
    );
}

#[test]
fn a_closed_chain_routes_the_boundary_path_not_a_centerline() {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("disc", disc_mesh(12.0)));
    // The disc is one planar face inside a rim that chains closed - step 4
    // proved the two picks dedupe to a single identical loop, so the two
    // operations below must cut byte-comparable paths, and neither may
    // ride the rim itself: a closed chain bounds, it does not run.
    let e = model::find_by_name(&world, "disc").expect("disc");
    let f = model::features_of(&world, &e).expect("features");
    let face = FeatureSel::Face(
        f.faces
            .iter()
            .find(|fa| matches!(fa.kind, FaceKind::Planar))
            .map(|fa| fa.id)
            .expect("the disc is one planar face"),
    );
    let rim_id = rim_chain_of(&world, "disc");
    let params = OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(10.0),
        retract_z: Height::fixed(8.0),
        top_z: Height::fixed(0.0),
        bottom_z: Height::fixed(-2.0),
        stepdown: 2.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    };
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "R-face",
            "EM-0600-4FL",
            sel("disc", face),
            params.clone(),
        )
        .expect("create face pick"),
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "R-rim",
            "EM-0600-4FL",
            sel("disc", FeatureSel::Chain(rim_id)),
            params.clone(),
        )
        .expect("create rim pick"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "R-face").expect("generate face"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "R-rim").expect("generate rim"),
    );
    assert_eq!(
        status(&world, "R-rim"),
        OpStatus::Ok,
        "a tangential boundary cut"
    );

    let pf = cache(&world, "R-face").expect("cache").path;
    let pr = cache(&world, "R-rim").expect("cache").path;
    // Structural: same budget, same kinds, same feed footprint. The seam
    // (which vertex a loop starts at) is an input-order artifact, so the
    // positions compare as sets, not as sequences.
    assert_eq!(pf.moves.len(), pr.moves.len(), "equal move budget");
    let xy = |p: &camcraft_app::cl::Toolpath| -> Vec<[f64; 2]> {
        let mut v: Vec<[f64; 2]> = p
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Feed)
            .map(|m| [m.to.x, m.to.y])
            .collect();
        v.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
        v
    };
    let (fe, fs) = (xy(&pf), xy(&pr));
    assert_eq!(fe.len(), fs.len(), "equal feed counts");
    for (a, b) in fe.iter().zip(&fs) {
        assert!(
            (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9,
            "footprint point {a:?} vs {b:?}"
        );
    }

    // And it is a boundary path, not a centerline: with a 6 mm cutter at
    // zero leave the tool center stands ~3 mm off the rim vertices, never
    // on them.
    let c = feature::chains(&f)
        .into_iter()
        .find(|c| c.id == rim_id)
        .expect("rim chain");
    let ring = feature::chain_points(&f, &c);
    let on_rim = pr
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .any(|m| {
            ring.iter()
                .any(|q| (m.to.x - q[0]).abs() < 1e-6 && (m.to.y - q[1]).abs() < 1e-6)
        });
    assert!(
        !on_rim,
        "a closed chain bounds - the tool never rides its vertices"
    );
    let min_standoff = pr
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Plunge)
        .map(|m| {
            ring.iter()
                .map(|q| ((m.to.x - q[0]).powi(2) + (m.to.y - q[1]).powi(2)).sqrt())
                .fold(f64::INFINITY, f64::min)
        })
        .fold(f64::INFINITY, f64::min);
    assert!(
        (min_standoff - 3.0).abs() < 0.25,
        "plunges stand a cutter radius off the rim, got {min_standoff}"
    );
}

#[test]
fn a_mixed_pick_list_cuts_boundary_then_run_and_says_what_overlap_means() {
    let (world, mut history) = box_part_world();
    let top = FeatureSel::Face(top_face_of(&world, "part"));
    let run = FeatureSel::Chain(open_run_of(&world, "part"));
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Mix1",
            "EM-0600-4FL",
            SrcRef::Selection(vec![
                GeoSel {
                    entity: "part".into(),
                    feature: top,
                },
                GeoSel {
                    entity: "part".into(),
                    feature: run,
                },
            ]),
            contour_params(),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Mix1").expect("generate"),
    );
    let op =
        model::operation_of(&world, &model::find_by_name(&world, "Mix1").expect("op")).expect("op");
    assert!(
        operation::region_of(&world, &op).is_some(),
        "the face still bounds"
    );
    assert_eq!(
        operation::chains_of(&world, &op).len(),
        1,
        "the run still runs"
    );

    let out = cache(&world, "Mix1").expect("cache");
    assert!(out.path.is_chained(), "stitched seams stay continuous");
    assert_eq!(
        out.path
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Plunge)
            .count(),
        6,
        "three levels of boundary, three of run"
    );
    // The run's edge lies on the face's own wall, so a full-width cutter
    // centered there half-cuts air and half-cuts the finished boundary -
    // the verifier sees exactly that and says so, instead of pretending
    // the pick list was coherent. (Pick the run WITHOUT the face, or use
    // a chamfer tool, for a clean edge-of-part pass.)
    assert!(
        out.warnings
            .iter()
            .any(|w| matches!(w, Warning::Gouge { .. })),
        "a centerline inside a picked wall is flagged, never silent: {:?}",
        out.warnings
    );
}

// ====== M1.5.1 step 6: open runs bind face regions, out to stock ======

use camcraft_app::geo2d::Point2;

/// A flat 16-chord disc whose rim carries a hard crease every `every`
/// vertices - the seam bulge puts the chord turn past the 30-degree
/// merge angle there, so the rim walks as `16 / every` open runs: four
/// quarter rims at `every = 4`, two half rims at `every = 8` (the
/// `split_rim_disc` shape the step-4 tests stitch closed.
fn seamed_sheet(r: f64, every: usize) -> camcraft_app::geometry::Mesh {
    let tau = std::f64::consts::TAU;
    let p = |i: usize| {
        let a = tau * i as f64 / 16.0;
        let rr = if i.is_multiple_of(every) { r * 1.15 } else { r };
        Point3::new(rr * a.cos(), rr * a.sin(), 0.0)
    };
    let facets: Vec<Triangle> = (0..16)
        .map(|i| Triangle::new(Point3::new(0.0, 0.0, 0.0), p(i), p(i + 1)))
        .collect();
    camcraft_app::geometry::Mesh::from_facets(facets)
}

/// The hand-test's model in flat form: a 40x40 plate with a bore whose
/// rim carries the same 8-apart seams as `seamed_sheet` - STEP-adaptive
/// tessellation of a cylindrical wall in a face, reduced to the shape
/// the region logic must answer. The bore walks as exactly two open
/// half-runs; the square outer rim as four short ones.
fn bored_plate() -> camcraft_app::geometry::Mesh {
    let tau = std::f64::consts::TAU;
    let bore = |i: usize| {
        let a = tau * (i % 16) as f64 / 16.0;
        let r = if (i % 16).is_multiple_of(8) {
            12.0 * 1.15
        } else {
            12.0
        };
        (r * a.cos(), r * a.sin())
    };
    // The bore vertices pushed radially onto the plate's square edge;
    // the diagonal ones land exactly on the corners, so the outer ring
    // is the 40x40 square with collinear splits.
    let outer = |i: usize| {
        let (x, y) = bore(i);
        let s = x.abs().max(y.abs());
        (x * 20.0 / s, y * 20.0 / s)
    };
    let bp = |i: usize| {
        let (x, y) = bore(i);
        Point3::new(x, y, 0.0)
    };
    let op = |i: usize| {
        let (x, y) = outer(i);
        Point3::new(x, y, 0.0)
    };
    let mut facets = Vec::new();
    for i in 0..16 {
        facets.push(Triangle::new(op(i), op(i + 1), bp(i + 1)));
        facets.push(Triangle::new(op(i), bp(i + 1), bp(i)));
    }
    camcraft_app::geometry::Mesh::from_facets(facets)
}

/// A 60x20 sheet scribbled once down its middle: two planar faces, and
/// the long straight top and bottom runs a face op must resolve to a
/// side by the material score, not by a coin flip.
fn scribed_sheet() -> camcraft_app::geometry::Mesh {
    let p = |x: f64, y: f64| Point3::new(x, y, 0.0);
    camcraft_app::geometry::Mesh::from_facets(
        vec![
            Triangle::new(p(-30.0, -10.0), p(0.0, -10.0), p(0.0, 10.0)),
            Triangle::new(p(-30.0, -10.0), p(0.0, 10.0), p(-30.0, 10.0)),
            Triangle::new(p(0.0, -10.0), p(30.0, -10.0), p(30.0, 10.0)),
            Triangle::new(p(0.0, -10.0), p(30.0, 10.0), p(0.0, 10.0)),
        ],
    )
}

/// The open half-runs of a `bored_plate` bore rim: long runs that hug
/// the origin, unlike the square rim's short, far ones.
fn bore_runs_of(world: &World, name: &str) -> Vec<u64> {
    let e = model::find_by_name(world, name).expect("part exists");
    let f = model::features_of(world, &e).expect("features");
    feature::chains(&f)
        .into_iter()
        .filter(|c| {
            !c.closed && {
                let p = feature::chain_points(&f, c);
                p.len() >= 5 && p.iter().all(|q| (q[0] * q[0] + q[1] * q[1]).sqrt() < 15.0)
            }
        })
        .map(|c| c.id)
        .collect()
}

/// The bore test in solid form: a 40x40x8 block drilled through by a
/// sixteen-facet bore (22.5-degree turns - inside the crease bound, so
/// the wall tessellates into strips that weld, exactly what STEP hands
/// back for a cylindrical face). Unlike `bored_plate` this has metal to
/// section: the silhouette keeps the bore as a through-hole.
fn bored_block() -> camcraft_app::geometry::Mesh {
    let tau = std::f64::consts::TAU;
    let bp = |i: usize, z: f64| {
        let a = tau * (i % 16) as f64 / 16.0;
        Point3::new(12.0 * a.cos(), 12.0 * a.sin(), z)
    };
    // Bore vertices pushed radially onto the 40x40 square: the diagonal
    // ones carry a corner, so each side's wall quads share its plane.
    let op = |i: usize, z: f64| {
        let a = tau * (i % 16) as f64 / 16.0;
        let (x, y) = (a.cos(), a.sin());
        let s = x.abs().max(y.abs());
        Point3::new(x * 20.0 / s, y * 20.0 / s, z)
    };
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, facets: &mut Vec<Triangle>| {
        facets.push(Triangle::new(a, b, c));
        facets.push(Triangle::new(a, c, d));
    };
    let mut facets = Vec::new();
    for i in 0..16 {
        let j = i + 1;
        // Caps: annuli wound outward (+z top, -z bottom).
        quad(bp(j, 8.0), bp(i, 8.0), op(i, 8.0), op(j, 8.0), &mut facets);
        quad(bp(i, 0.0), bp(j, 0.0), op(j, 0.0), op(i, 0.0), &mut facets);
        // Bore wall faces the axis; the block wall faces away.
        quad(bp(i, 8.0), bp(j, 8.0), bp(j, 0.0), bp(i, 0.0), &mut facets);
        quad(op(j, 8.0), op(i, 8.0), op(i, 0.0), op(j, 0.0), &mut facets);
    }
    camcraft_app::geometry::Mesh::from_facets(facets)
}

/// Picks for a list of chain ids on one entity, in the given order.
fn chain_picks(entity: &str, ids: &[u64]) -> Vec<GeoSel> {
    ids.iter()
        .map(|&id| GeoSel {
            entity: entity.to_string(),
            feature: FeatureSel::Chain(id),
        })
        .collect()
}

fn sel_op(world: &World, items: Vec<GeoSel>, params: OpParams) -> Operation {
    let _ = world;
    Operation {
        tool: "EM-0600-4FL".into(),
        src: SrcRef::Selection(items),
        params,
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    }
}

/// Shoelace area of `seamed_sheet`'s 16 rim vertices - the disc's own
/// polygon, computed without going through any code under test.
fn seamed_area(r: f64, every: usize) -> f64 {
    let tau = std::f64::consts::TAU;
    let v = |i: usize| {
        let a = tau * (i % 16) as f64 / 16.0;
        let rr = if (i % 16).is_multiple_of(every) {
            r * 1.15
        } else {
            r
        };
        (rr * a.cos(), rr * a.sin())
    };
    (0..16)
        .map(|i| {
            let (ax, ay) = v(i);
            let (bx, by) = v(i + 1);
            (ax * by - bx * ay).abs() * 0.5
        })
        .sum()
}

#[test]
fn face_on_split_bore_rim_machines_annulus() {
    // The hand-test's exact scenario (issue C): a Face op over the two
    // open rim runs of a bore. Step 4 proved they spell one closed
    // ring; the ring's interior is air, so the material-side rule takes
    // its complement - the annulus, what facing the rim actually means.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("plate", bored_plate()));
    let runs = bore_runs_of(&world, "plate");
    assert_eq!(runs.len(), 2, "the seamed bore rim walks as two runs");

    let items = chain_picks("plate", &runs);
    let op = sel_op(&world, items.clone(), face_sel_params());
    let region = operation::region_of(&world, &op).expect("the rim's complement in the plate");
    assert_eq!(
        region.loops().len(),
        2,
        "plate boundary and bore, not a half-open rim"
    );
    let want = 1600.0 - seamed_area(12.0, 8);
    let got = region.area().abs();
    assert!(
        (got - want).abs() / want < 0.01,
        "the annulus, to the polygon's own area: {got} vs {want}"
    );
    assert!(
        !region.contains(Point2::new(0.0, 0.0)),
        "the bore is left standing"
    );
    assert!(
        region.contains(Point2::new(15.0, 0.0)),
        "material by the rim is taken"
    );
    assert!(region.contains(Point2::new(0.0, -16.0)), "and across it");

    // And it generates: the same walk the preview read, start to finish.
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Bore1",
            "EM-0600-4FL",
            SrcRef::Selection(items),
            face_sel_params(),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Bore1").expect("generate"),
    );
    let path = cache(&world, "Bore1").expect("cache").path;
    assert!(
        !path.moves.is_empty(),
        "facing rows across the annulus bounds"
    );
    assert!(path.all_finite());
}

#[test]
fn a_patch_pick_bounds_the_same_annulus_as_its_strips() {
    // The welded-face answer to the same bore problem the rim-chain
    // route solved: pick the wall, not a strip under the cursor. The
    // wall's rims are one ring (the twin drops at `push_loop`), the ring
    // circles air, so the material-side rule complements it - and the
    // annulus must be the one the passing chain pick already produces,
    // to the last bit the walk can agree on.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("plate", bored_block()));
    let e = model::find_by_name(&world, "plate").expect("plate");
    let f = model::features_of(&world, &e).expect("features");
    let ps = feature::patches(&f);
    assert_eq!(ps.len(), 1, "only the bore wall welds: 22.5-degree strips");
    assert_eq!(ps[0].faces.len(), 16);

    let items = vec![GeoSel {
        entity: "plate".into(),
        feature: FeatureSel::Patch(ps[0].id),
    }];
    let op = sel_op(&world, items.clone(), face_sel_params());
    let region = operation::region_of(&world, &op).expect("bore wall bounds a region");
    assert_eq!(
        region.loops().len(),
        2,
        "block outline and bore, not the wall's two rims doubled"
    );
    let tau = std::f64::consts::TAU;
    let want = 1600.0 - 8.0 * 12.0f64.powi(2) * (tau / 16.0).sin();
    let got = region.area().abs();
    assert!(
        (got - want).abs() / want < 0.01,
        "the annulus, to the polygon's own area: {got} vs {want}"
    );
    assert!(
        !region.contains(Point2::new(0.0, 0.0)),
        "the bore stays standing"
    );
    assert!(
        region.contains(Point2::new(15.0, 0.0)),
        "material by the wall is taken"
    );

    // Equivalence oracle: the closed rim chains answer the same cut.
    let rims: Vec<u64> = feature::chains(&f)
        .into_iter()
        .filter(|c| {
            c.closed && {
                let p = feature::chain_points(&f, c);
                p.iter().all(|q| (q[0] * q[0] + q[1] * q[1]).sqrt() < 15.0)
            }
        })
        .map(|c| c.id)
        .collect();
    assert_eq!(rims.len(), 2, "both bore rims loop closed");
    let strip_op = sel_op(&world, chain_picks("plate", &rims), face_sel_params());
    let strip_region = operation::region_of(&world, &strip_op).expect("rims bound the same");
    let (a, b) = (region.area().abs(), strip_region.area().abs());
    assert!(
        (a - b).abs() / a.max(f64::EPSILON) < 1e-6,
        "wall pick and rim picks are the same annulus: {a} vs {b}"
    );

    // And it generates.
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Wall1",
            "EM-0600-4FL",
            SrcRef::Selection(items),
            face_sel_params(),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Wall1").expect("generate"),
    );
    let path = cache(&world, "Wall1").expect("cache").path;
    assert!(
        !path.moves.is_empty(),
        "facing rows across the annulus bounds"
    );
    assert!(path.all_finite());
}

#[test]
fn face_on_one_bore_run_machines_outside_to_stock() {
    // One half of the rim, unpaired: the run acts as the limit, and the
    // stock beyond it is the far edge. Material between run and stock
    // machines; material on the run's other side stays standing - the
    // user's "generate to the outside of the stock box, the open loop
    // selected acts as a limit", at unit scale.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    history.execute(&world, ImportModel::from_mesh("plate", bored_plate()));
    let runs = bore_runs_of(&world, "plate");
    assert_eq!(runs.len(), 2);
    // Take the upper half-rim so the sides of the assertions are fixed.
    let e = model::find_by_name(&world, "plate").expect("plate");
    let f = model::features_of(&world, &e).expect("features");
    let upper = runs
        .iter()
        .copied()
        .find(|&id| {
            let c = feature::chains(&f)
                .into_iter()
                .find(|x| x.id == id)
                .expect("chain");
            let p = feature::chain_points(&f, &c);
            p.iter().map(|q| q[1]).sum::<f64>() > 0.0
        })
        .expect("one half-rim lies above the axis");

    let items = chain_picks("plate", &[upper]);
    let op = sel_op(&world, items.clone(), face_sel_params());
    let region = operation::region_of(&world, &op).expect("the run-out to the stock");
    assert!(
        region.contains(Point2::new(0.0, 16.0)),
        "material beyond the rim, up to stock"
    );
    assert!(
        region.contains(Point2::new(18.0, 6.0)),
        "and by the extension, still past the rim"
    );
    assert!(
        !region.contains(Point2::new(0.0, -16.0)),
        "the run limits: below it nothing moves"
    );
    assert!(
        !region.contains(Point2::new(0.0, 6.0)),
        "and the bore air is never in the region"
    );
    let got = region.area().abs();
    assert!(
        got > 400.0 && got < 1600.0,
        "a band of the plate, not all of it: {got}"
    );

    let notes = operation::selection_notes(&world, None, &items, &face_sel_params());
    assert!(
        notes
            .iter()
            .any(|s| s.contains("1 open run extended to the stock boundary")),
        "the list says what the run-out did: {notes:?}"
    );
    assert!(
        !notes.iter().any(|s| s.contains("outside")),
        "nothing flipped - the strip already faced metal: {notes:?}"
    );
}

#[test]
fn one_open_run_on_a_boss_faces_the_whole_boss() {
    // A convex rim rings all air on its OUTSIDE: a quarter run extended
    // to stock bounds only air, so the material-side rule takes the
    // complement inside the material - the whole boss. Facing an arc
    // that runs past nothing metal is facing the part, never nothing.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    history.execute(
        &world,
        ImportModel::from_mesh("disc", seamed_sheet(12.0, 4)),
    );
    let runs = open_runs_of(&world, "disc");
    assert_eq!(runs.len(), 4, "the quarter-seamed rim walks as four runs");

    let items = chain_picks("disc", &[runs[0]]);
    let op = sel_op(&world, items.clone(), face_sel_params());
    let region = operation::region_of(&world, &op).expect("the boss complement");
    let want = seamed_area(12.0, 4);
    let got = region.area().abs();
    assert!(
        (got - want).abs() / want < 0.01,
        "the whole boss, to its own polygon: {got} vs {want}"
    );
    assert!(region.contains(Point2::new(0.0, 0.0)));
    assert!(
        region.contains(Point2::new(2.0, 2.0)),
        "the interior is the answer"
    );

    let notes = operation::selection_notes(&world, None, &items, &face_sel_params());
    assert!(
        notes
            .iter()
            .any(|s| s.contains("1 open run extended to the stock boundary")),
        "{notes:?}"
    );
    assert!(
        notes
            .iter()
            .any(|s| s.contains("machining outside the run")),
        "air on the far side is said, not swallowed: {notes:?}"
    );
}

#[test]
fn straight_run_machines_the_material_side() {
    // A scribble line laces to nothing, so no side is "outward" - both
    // normals are tried and the one that meets metal wins. The sheet's
    // top run therefore faces the sheet below it, and nothing above.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    history.execute(&world, ImportModel::from_mesh("sheet", scribed_sheet()));
    let e = model::find_by_name(&world, "sheet").expect("sheet");
    let f = model::features_of(&world, &e).expect("features");
    let top = feature::chains(&f)
        .into_iter()
        .find(|c| {
            !c.closed && {
                let p = feature::chain_points(&f, c);
                p.len() >= 3 && p.iter().all(|q| (q[1] - 10.0).abs() < 1e-9)
            }
        })
        .map(|c| c.id)
        .expect("the sheet's top run");

    let op = sel_op(&world, chain_picks("sheet", &[top]), face_sel_params());
    let region = operation::region_of(&world, &op).expect("the run's material side");
    let want = 1200.0;
    let got = region.area().abs();
    assert!(
        (got - want).abs() / want < 0.01,
        "the whole sheet under the run: {got} vs {want}"
    );
    assert!(
        region.contains(Point2::new(0.0, -9.0)),
        "metal below the scribble is in"
    );
    assert!(
        region.contains(Point2::new(0.0, 9.0)),
        "metal above it is in - the run is a limit, not a side"
    );
    assert!(
        !region.contains(Point2::new(0.0, 15.0)),
        "air up to the stock stays air"
    );
}

#[test]
fn ambiguous_extension_refuses_by_name() {
    // Same split rim at r = 30 in 60 mm stock: the seam vertices at
    // +/-34.5 stand OUTSIDE the stock, and the run's single honest
    // outward ray hits nothing. That is genuinely undecidable - so the
    // refusal names it, and nothing is cut in silence.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    history.execute(
        &world,
        ImportModel::from_mesh("disc", seamed_sheet(30.0, 8)),
    );
    let runs = open_runs_of(&world, "disc");
    assert_eq!(runs.len(), 2);

    let op = sel_op(&world, chain_picks("disc", &[runs[0]]), face_sel_params());
    assert!(
        operation::region_of(&world, &op).is_none(),
        "no preview from undecidable geometry"
    );
    let items = chain_picks("disc", &[runs[0]]);
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Bad1",
            "EM-0600-4FL",
            SrcRef::Selection(items),
            face_sel_params(),
        )
        .expect("create"),
    );
    let CommandError::Machining(msg) = GenerateOperation::new(&world, "Bad1")
        .err()
        .expect("generate must fail, not decide by coin")
    else {
        panic!("expected a machining refusal, got something else");
    };
    assert!(
        msg.contains("ambiguous") && msg.contains("closed chain"),
        "{msg}"
    );
}

#[test]
fn straight_runs_face_their_material_and_trace_refuses_by_name() {
    // Every open run on a facing op now answers something. A box edge
    // is straight - it closes no lens, so the material score decides,
    // and the only side that meets metal meets ALL of it: the whole
    // top face, the scribble rule at box scale, never the silent drop
    // the old routing committed. (Pocket refusal and contour centerline
    // keep their step-3/4 tests.)
    let (world, mut history) = box_part_world();
    let run = open_run_of(&world, "part");
    let items = chain_picks("part", &[run]);
    let op = sel_op(&world, items.clone(), face_sel_params());
    let region = operation::region_of(&world, &op).expect("the run's material side");
    let got = region.area().abs();
    assert!(
        (got - 100.0).abs() / 100.0 < 0.02,
        "a straight edge run faces the whole 10x10 top: {got}"
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Face9",
            "EM-0600-4FL",
            SrcRef::Selection(items.clone()),
            face_sel_params(),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Face9").expect("generate the whole face"),
    );
    assert!(
        !cache(&world, "Face9").expect("cache").path.moves.is_empty(),
        "rows across the top face"
    );

    // Trace machines a closed boundary; a run inside it is a named
    // error, never the silent drop the old routing committed.
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Tr9",
            "EM-0600-4FL",
            SrcRef::Selection(items),
            OpParams::Trace {
                tilt: None,
                clearance_z: Height::fixed(25.0),
                retract_z: None,
                top_z: Height::fixed(5.0),
                bottom_z: Height::fixed(2.5),
                stepdown: 2.5,
                feed: 500.0,
                plunge_feed: 100.0,
                climb: true,
            },
        )
        .expect("create"),
    );
    let CommandError::Machining(msg) = GenerateOperation::new(&world, "Tr9")
        .err()
        .expect("trace must refuse a run")
    else {
        panic!("expected a machining refusal");
    };
    assert!(msg.contains("Trace machines a closed boundary"), "{msg}");
}

#[test]
fn selection_notes_report_stitching_and_extension() {
    // The Geometry section's small grey rows, end to end: what the
    // stitcher and the run-out decided, computed on read so a stale
    // project can never lie about a pick list it no longer holds.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("plate", bored_plate()));
    let runs = bore_runs_of(&world, "plate");
    let items = chain_picks("plate", &runs);
    let notes = operation::selection_notes(&world, None, &items, &face_sel_params());
    assert!(
        notes
            .iter()
            .any(|s| s.contains("2 open runs stitched into 1 ring")),
        "{notes:?}"
    );
    assert!(
        notes
            .iter()
            .any(|s| s.contains("machining outside the run")),
        "{notes:?}"
    );

    // A Contour over the same picks stitches into one boundary pass, so
    // the stitch is reported; nothing was extended, so that stays unsaid.
    let contour = OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: Height::fixed(12.0),
        top_z: Height::fixed(1.0),
        bottom_z: Height::fixed(-1.0),
        stepdown: 1.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    };
    let notes = operation::selection_notes(&world, None, &items, &contour);
    assert!(
        notes
            .iter()
            .any(|s| s.contains("2 open runs stitched into 1 ring")),
        "{notes:?}"
    );
    assert!(!notes.iter().any(|s| s.contains("extended")), "{notes:?}");
    // The stitched ring circles the bore's air, so Auto answers Inside
    // (M1.5.4) - what used to be a silent outside cut is now said.
    assert!(
        notes
            .iter()
            .any(|s| s.contains("machining inside the ring")),
        "{notes:?}"
    );
}

// ---------------------------------------------------------------- M1.5.4 B:
// contour Side. Auto answers the fill-ratio question on picked rings;
// Inside/Outside are the user's ask, honored verbatim; whole-section
// sources refuse Inside by name at every command door.

/// Contour at the band z±1, one level pair deep enough for the fixtures.
fn contour_at(side: Side, z: f64) -> OpParams {
    OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(10.0),
        retract_z: Height::fixed(8.0),
        top_z: Height::fixed(z + 1.0),
        bottom_z: Height::fixed(z - 1.0),
        stepdown: 8.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    }
}

/// The welded bore-wall patch of a `bored_block`, as a pick list.
fn bore_patch_pick(world: &World, name: &str) -> Vec<GeoSel> {
    let e = model::find_by_name(world, name).expect("part exists");
    let f = model::features_of(world, &e).expect("features");
    let ps = feature::patches(&f);
    assert_eq!(ps.len(), 1, "only the bore wall welds: 22.5-degree strips");
    vec![GeoSel {
        entity: name.to_string(),
        feature: FeatureSel::Patch(ps[0].id),
    }]
}

/// Distance of every cut point (plunges and feeds) from the XY origin.
fn feed_radii(world: &World, name: &str) -> Vec<f64> {
    // The ring walk itself: the XY landing radius of every cutting move.
    cache(world, name)
        .expect("cache")
        .path
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed)
        .map(|m| (m.to.x * m.to.x + m.to.y * m.to.y).sqrt())
        .collect()
}

fn dive_radii(world: &World, name: &str) -> Vec<f64> {
    // Where each level stands up the tool: the XY radius of the plunges.
    // Since M8.2 A an Inside contour answers this from the floor's room,
    // while a wall graze answers it from the ring itself.
    cache(world, name)
        .expect("cache")
        .path
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Plunge)
        .map(|m| (m.to.x * m.to.x + m.to.y * m.to.y).sqrt())
        .collect()
}

fn world_with_bore() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("plate", bored_block()));
    (world, history)
}

#[test]
fn contour_bore_wall_pick_defaults_inside() {
    // The hand-test's defect 2, fixed: contouring a bore's wall from the
    // material side cuts OUTSIDE the wall. The picked ring bounds air, so
    // Auto machines inside it, says so, and the path rides the eroded
    // ring - a Ø6 cutter around an Ø24 bore stands ~3 mm off the wall,
    // on the axis side.
    let (world, mut history) = world_with_bore();
    let items = bore_patch_pick(&world, "plate");
    let op = sel_op(&world, items.clone(), contour_at(Side::Auto, 4.0));
    let region = operation::region_of(&world, &op).expect("the picked wall rings");
    assert_eq!(
        region.loops().len(),
        1,
        "contour machines the ring, not the annulus"
    );
    // 1e-5: Region::new round-trips the vertices through clipper's int
    // grid; the law under test is that nothing was COMPLEMENTED, not the
    // last bit of the coordinate.
    assert!(
        region.loops()[0]
            .points()
            .iter()
            .all(|p| (p.x.hypot(p.y) - 12.0).abs() < 1e-5),
        "the picked ring is untouched: contour never rewrites loops; got {:?}",
        region.loops()[0].points(),
    );

    let notes = operation::selection_notes(&world, None, &items, &contour_at(Side::Auto, 4.0));
    assert!(
        notes
            .iter()
            .any(|s| s.contains("machining inside the ring")),
        "{notes:?}"
    );

    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "BoreC",
            "EM-0600-4FL",
            SrcRef::Selection(items),
            contour_at(Side::Auto, 4.0),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "BoreC").expect("generate"),
    );
    assert_eq!(status(&world, "BoreC"), OpStatus::Ok);
    let radii = feed_radii(&world, "BoreC");
    assert!(
        radii.iter().all(|d| (8.0..=10.0).contains(d)),
        "inside the bore, on the eroded ring: {radii:?}"
    );
    let dives = dive_radii(&world, "BoreC");
    assert!(
        dives.iter().all(|d| *d < 8.0),
        "a bore bores in through its floor's room, not its wall: {dives:?}"
    );
}

#[test]
fn contour_boss_pick_defaults_outside() {
    // The other half of the law: a ring that bounds the material itself
    // (a boss's OD) keeps today's outside walk and earns no flip note.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("disc", disc_mesh(12.0)));
    let items = chain_picks("disc", &[rim_chain_of(&world, "disc")]);
    let notes = operation::selection_notes(&world, None, &items, &contour_at(Side::Auto, 0.0));
    assert!(
        !notes
            .iter()
            .any(|s| s.contains("machining inside the ring")),
        "a boss's ring bounds material: {notes:?}"
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "BossC",
            "EM-0600-4FL",
            SrcRef::Selection(items),
            contour_at(Side::Auto, 0.0),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "BossC").expect("generate"),
    );
    assert_eq!(status(&world, "BossC"), OpStatus::Ok);
    let radii = feed_radii(&world, "BossC");
    assert!(
        radii.iter().all(|d| (14.0..=16.0).contains(d)),
        "outside the boss, on the grown ring: {radii:?}"
    );
    let dives = dive_radii(&world, "BossC");
    assert!(
        dives.iter().all(|d| (14.0..=16.0).contains(d)),
        "the wall's dive grazes at the lead-in, unchanged: {dives:?}"
    );
}

#[test]
fn contour_outside_forces_outside_on_bore() {
    // The escape hatch: an explicit Outside on a bore reproduces the old
    // path and stays silent (the note belongs to Auto's verdict).
    let (world, mut history) = world_with_bore();
    let items = bore_patch_pick(&world, "plate");
    let notes = operation::selection_notes(&world, None, &items, &contour_at(Side::Outside, 4.0));
    assert!(
        !notes.iter().any(|s| s.contains("inside the ring")),
        "{notes:?}"
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "BoreO",
            "EM-0600-4FL",
            SrcRef::Selection(items),
            contour_at(Side::Outside, 4.0),
        )
        .expect("create"),
    );
    // The explicit Outside stands as asked - and on a bore it puts the
    // Ø6 cutter on the ring at r 15, IN the plate around the Ø24 bore:
    // the old path cut that groove silently. The part-solid depth law
    // refuses it by name instead (the silent-gouge fix).
    let err = GenerateOperation::new(&world, "BoreO")
        .err()
        .expect("an Outside ring around a bore stands in the plate - refused");
    assert!(err.to_string().contains("cuts into the part"), "{err}");
    assert!(err.to_string().contains("not walked at any level"), "{err}");
}

#[test]
fn contour_inside_forces_inside_on_boss() {
    // And Inside on a boss is honored without a note: the user asked, the
    // generator obeys, only Auto explains itself.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("disc", disc_mesh(12.0)));
    let items = chain_picks("disc", &[rim_chain_of(&world, "disc")]);
    let notes = operation::selection_notes(&world, None, &items, &contour_at(Side::Inside, 0.0));
    assert!(
        !notes.iter().any(|s| s.contains("inside the ring")),
        "{notes:?}"
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "BossI",
            "EM-0600-4FL",
            SrcRef::Selection(items),
            contour_at(Side::Inside, 0.0),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "BossI").expect("generate"),
    );
    let radii = feed_radii(&world, "BossI");
    assert!(
        radii.iter().all(|d| (8.0..=10.0).contains(d)),
        "explicit Inside must stand on the boss too: {radii:?}"
    );
    let dives = dive_radii(&world, "BossI");
    assert!(
        dives.iter().all(|d| *d < 8.0),
        "and Inside means the tool enters through the bounded room, every level: {dives:?}"
    );
}

#[test]
fn inside_on_entity_source_is_refused() {
    // Whole-section sources run the parity law, which already cuts hole
    // walls from inside; negating it would erode outer walls and grow
    // hole loops through them - the mirror gouge. Refused by name at
    // every command door.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(
        &world,
        ImportModel::from_mesh(
            "part",
            cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 8.0)),
        ),
    );
    history.execute(&world, ImportModel::from_mesh("bore", bored_block()));

    // Door 1: create.
    let CommandError::Invalid(msg) = CreateOperation::new(
        &world,
        "Bad1",
        "EM-0600-4FL",
        SrcRef::Entity("part".into()),
        contour_at(Side::Inside, 4.0),
    )
    .err()
    .expect("create must refuse") else {
        panic!("expected an Invalid refusal");
    };
    assert!(msg.contains("needs picked rings"), "{msg}");

    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "E1",
            "EM-0600-4FL",
            SrcRef::Entity("part".into()),
            contour_at(Side::Auto, 4.0),
        )
        .expect("Auto on an entity is allowed"),
    );

    // Door 2: the params edit.
    let CommandError::Invalid(msg) =
        EditOperationParams::new(&world, "E1", contour_at(Side::Inside, 4.0))
            .err()
            .expect("params edit must refuse")
    else {
        panic!("expected an Invalid refusal");
    };
    assert!(msg.contains("needs picked rings"), "{msg}");

    // Door 3: a links edit retargeting a Selection+Inside op onto the
    // model arms the same law from the other side.
    let items = bore_patch_pick(&world, "bore");
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "S1",
            "EM-0600-4FL",
            SrcRef::Selection(items),
            contour_at(Side::Inside, 4.0),
        )
        .expect("selection + inside is the honest combo"),
    );
    let CommandError::Invalid(msg) =
        EditOperationLinks::new(&world, "S1", None, Some(SrcRef::Entity("part".into())))
            .err()
            .expect("links edit must refuse")
    else {
        panic!("expected an Invalid refusal");
    };
    assert!(msg.contains("needs picked rings"), "{msg}");
    // The refusal wrote nothing: S1 still machines its picked ring.
    let op =
        model::operation_of(&world, &model::find_by_name(&world, "S1").expect("op")).expect("op");
    assert!(
        matches!(op.src, SrcRef::Selection(_)),
        "the refused retarget stands off"
    );
}

#[test]
fn side_change_goes_stale_once_and_generate_clears_it() {
    // The digest carries Side (v7): Auto and Inside on the same selection
    // are different asks - the second generates the same path, but the
    // op says Stale until it is applied, exactly once.
    let (world, mut history) = world_with_bore();
    let items = bore_patch_pick(&world, "plate");
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Stale1",
            "EM-0600-4FL",
            SrcRef::Selection(items),
            contour_at(Side::Auto, 4.0),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Stale1").expect("generate"),
    );
    assert_eq!(status(&world, "Stale1"), OpStatus::Ok);

    history.execute(
        &world,
        EditOperationParams::new(&world, "Stale1", contour_at(Side::Inside, 4.0)).expect("edit"),
    );
    assert_eq!(
        status(&world, "Stale1"),
        OpStatus::Stale,
        "the ask changed, the path did not"
    );

    history.execute(
        &world,
        GenerateOperation::new(&world, "Stale1").expect("regen"),
    );
    assert_eq!(
        status(&world, "Stale1"),
        OpStatus::Ok,
        "apply clears the flag"
    );

    // Back through the regenerate, then through the edit: the params and
    // their digest travel together, so two steps down lands on the old
    // consistent pair.
    history.undo(&world);
    assert_eq!(
        status(&world, "Stale1"),
        OpStatus::Stale,
        "the regen's stamp is undone"
    );
    history.undo(&world);
    assert_eq!(
        status(&world, "Stale1"),
        OpStatus::Ok,
        "undo restores ask and digest together"
    );
}

// ---------------------------------------------------------------- M1.6 drilling

use camcraft_app::cl::CycleKind;
use camcraft_app::drilling::{self, CycleChoice, Hole};

/// A 20x20x10 block bored through by a sixteen-facet Ø4 hole - the closed
/// solid form of what STEP hands back for a drilled part, the same weld
/// as `bored_block` at drill scale so recognition can see a wall.
fn drilled_block() -> camcraft_app::geometry::Mesh {
    let tau = std::f64::consts::TAU;
    let bp = |i: usize, z: f64| {
        let a = tau * (i % 16) as f64 / 16.0;
        Point3::new(2.0 * a.cos(), 2.0 * a.sin(), z)
    };
    // Wall vertices pushed radially onto the 20x20 square, as in
    // bored_block: each side's wall quads share their plane.
    let op = |i: usize, z: f64| {
        let a = tau * (i % 16) as f64 / 16.0;
        let (x, y) = (a.cos(), a.sin());
        let s = x.abs().max(y.abs());
        Point3::new(x * 10.0 / s, y * 10.0 / s, z)
    };
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, facets: &mut Vec<Triangle>| {
        facets.push(Triangle::new(a, b, c));
        facets.push(Triangle::new(a, c, d));
    };
    let mut facets = Vec::new();
    for i in 0..16 {
        let j = i + 1;
        // Caps: annuli wound outward (+z top, -z bottom).
        quad(
            bp(j, 10.0),
            bp(i, 10.0),
            op(i, 10.0),
            op(j, 10.0),
            &mut facets,
        );
        quad(bp(i, 0.0), bp(j, 0.0), op(j, 0.0), op(i, 0.0), &mut facets);
        // The hole wall faces the axis; the block wall faces away.
        quad(
            bp(i, 10.0),
            bp(j, 10.0),
            bp(j, 0.0),
            bp(i, 0.0),
            &mut facets,
        );
        quad(
            op(j, 10.0),
            op(i, 10.0),
            op(i, 0.0),
            op(j, 0.0),
            &mut facets,
        );
    }
    camcraft_app::geometry::Mesh::from_facets(facets)
}

fn drill_params(holes: Vec<Hole>, top_z: f64, clearance_z: f64) -> OpParams {
    OpParams::Drill {
        clearance_z: Height::fixed(clearance_z),
        retract_z: None,
        top_z: Height::fixed(top_z),
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 130.0,
        plunge_feed: 100.0,
        holes,
        cross: Vec::new(),
    }
}

#[test]
fn drill_op_recognizes_from_the_model_then_machines_clean() {
    // The M1.6 acceptance shape: holes come out of the mesh as an action,
    // ride the params as data, and the generated path verifies silent -
    // a matched drill leaves no law anything to say about.
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("pblock", drilled_block()));
    add_tool(&world, &mut history, "DR-0400");
    let e = model::find_by_name(&world, "pblock").expect("part");
    let facets = model::facets_of(&world, &e).expect("mesh");
    let holes = drilling::recognize(&facets);
    assert_eq!(holes.len(), 1, "the bore reads as one hole: {holes:?}");

    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Drill1",
            "DR-0400",
            SrcRef::Entity("pblock".into()),
            drill_params(holes.clone(), 10.0, 25.0),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Drill1").expect("generate"),
    );
    assert_eq!(status(&world, "Drill1"), OpStatus::Ok);
    let cache = cache(&world, "Drill1").expect("cached");
    assert_eq!(cache.warnings, Vec::new(), "{:?}", cache.warnings);
    assert_eq!(cache.path.cycles.len(), 1);
    let c = &cache.path.cycles[0];
    // Auto's depth law: 11 mm of cut (10 + the through exit) in a 4 mm
    // drill - past d, within 4d - so the peck shape, with the exit
    // allowance recorded on the cycle the post will read.
    assert_eq!(c.kind, CycleKind::Peck);
    assert_eq!((c.xy.x, c.xy.y), (0.0, 0.0));
    assert_eq!(c.entry_z, 10.0);
    assert_eq!(c.bottom_z, -1.0);
    assert!(!c.xy.x.is_nan() && c.first_move < cache.path.moves.len());
}

#[test]
fn a_hole_deeper_than_the_drill_reaches_is_refused_by_name() {
    // The hole list is data, not a theorem: a hand-edited floor below
    // the tool's reach is refused at generation, naming the numbers the
    // user needs (issue #84 - it once generated with a ShankHit warning
    // for the post gate to refuse; the shank feeding into uncut stock is
    // a collision, and the generator refuses it by name, earlier).
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 100.0, 100.0, 130.0);
    add_tool(&world, &mut history, "DR-0400");
    let deep = Hole {
        xy: Point2::new(0.0, 0.0),
        radius: 2.0,
        entry_z: 130.0,
        floor_z: 0.0,
        through: true,
        stepped: false,
    };
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Deep1",
            "DR-0400",
            SrcRef::Stock,
            drill_params(vec![deep], 130.0, 140.0),
        )
        .expect("create"),
    );
    let err = GenerateOperation::new(&world, "Deep1").err().map(|e| e.to_string()).unwrap_or_default();
    for s in ["131.000 mm deep from the stock surface", "DR-0400", "12.000 mm"] {
        assert!(err.contains(s), "the refusal names '{s}': {err:?}");
    }
    assert!(cache(&world, "Deep1").is_none(), "a refused hole caches no path");
}

#[test]
fn a_drill_op_without_holes_creates_but_refuses_generation_by_name() {
    // The law behind the UI flow: creation is legal before detection
    // (that is how the property page gets on screen), and generation is
    // the gate that refuses it - by name, not as an empty success.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 20.0, 20.0, 10.0);
    add_tool(&world, &mut history, "DR-0400");
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Empty1",
            "DR-0400",
            SrcRef::Stock,
            drill_params(vec![], 10.0, 25.0),
        )
        .expect("a hole list arrives via Detect; creation must not need it"),
    );
    let err = GenerateOperation::new(&world, "Empty1")
        .err()
        .expect("an empty hole list must not generate");
    assert!(err.to_string().contains("holes"), "{err}");
    assert_eq!(status(&world, "Empty1"), OpStatus::Never);
}

#[test]
fn moving_the_part_marks_the_drill_op_stale_until_regen() {
    // Holes are stored data, so a moved model cannot silently redefine
    // them: the source digest moves the stamp, a regenerate proceeds on
    // the stored list, and re-detection stays the explicit action it is.
    let (world, mut history) = {
        let world = new_world();
        let mut history = History::default();
        history.execute(&world, ImportModel::from_mesh("pblock", drilled_block()));
        // A long-series flute, so no reach law speaks over a moved
        // part (issue #84) - this test pins staleness. The part moves
        // DOWN: moved up, its top would stand over the fixed R 10 and
        // the rapid onto R would cross it (issue #91, refused).
        let mut drill = catalog_tool("DR-0400");
        drill.flute_len = 20.0;
        history.execute(&world, AddTool::new(&world, "DR-0400", drill).expect("tool"));
        let e = model::find_by_name(&world, "pblock").expect("part");
        let holes = drilling::recognize(&model::facets_of(&world, &e).expect("mesh"));
        history.execute(
            &world,
            CreateOperation::new(
                &world,
                "Move1",
                "DR-0400",
                SrcRef::Entity("pblock".into()),
                drill_params(holes, 10.0, 25.0),
            )
            .expect("create"),
        );
        history.execute(
            &world,
            GenerateOperation::new(&world, "Move1").expect("generate"),
        );
        (world, history)
    };
    assert_eq!(status(&world, "Move1"), OpStatus::Ok);
    let e = model::find_by_name(&world, "pblock").expect("part");
    model::set_location(&world, &e, Point3::new(0.0, 0.0, -2.0));
    assert_eq!(
        status(&world, "Move1"),
        OpStatus::Stale,
        "the part moved under the hole list"
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Move1").expect("regen"),
    );
    assert_eq!(status(&world, "Move1"), OpStatus::Ok);
    let cache = cache(&world, "Move1").expect("cached");
    assert_eq!(
        cache.path.cycles[0].entry_z, 10.0,
        "stored holes are not re-detected"
    );
}

// ---------------------------------------------------------------------------
// M2.2a: the waterline through the command chain - the model in the world,
// not a Vec<Triangle> handed to a function. Generation, refusal and the
// tool-kind law must hold at THIS level, where the user actually stands.
// ---------------------------------------------------------------------------

/// R20 spherical cap to the 45 deg rim - the same solid `finish3d.rs`
/// walks directly, as an imported model.
fn dome_mesh() -> camcraft_app::geometry::Mesh {
    use camcraft_app::geometry::{Point3, Triangle};
    const TAU: f64 = std::f64::consts::TAU;
    let (radius, polar_max, n_lat, n_lon) = (20.0, std::f64::consts::FRAC_PI_4, 12usize, 96usize);
    let mut facets = Vec::new();
    let ring = |k: usize| -> Vec<Point3> {
        let ph = polar_max * k as f64 / n_lat as f64;
        let (z, rr) = (radius * ph.cos(), radius * ph.sin());
        (0..n_lon)
            .map(|j| {
                let th = TAU * j as f64 / n_lon as f64;
                Point3::new(rr * th.cos(), rr * th.sin(), z)
            })
            .collect()
    };
    let apex = Point3::new(0.0, 0.0, radius);
    let first = ring(1);
    for j in 0..n_lon {
        facets.push(Triangle::new(apex, first[j], first[(j + 1) % n_lon]));
    }
    for k in 1..n_lat {
        let ra = ring(k);
        let rb = ring(k + 1);
        for j in 0..n_lon {
            let j2 = (j + 1) % n_lon;
            facets.push(Triangle::new(ra[j], rb[j], rb[j2]));
            facets.push(Triangle::new(ra[j], rb[j2], ra[j2]));
        }
    }
    let rim = ring(n_lat);
    let zb = radius * polar_max.cos();
    let center = Point3::new(0.0, 0.0, zb);
    for j in 0..n_lon {
        facets.push(Triangle::new(center, rim[j], rim[(j + 1) % n_lon]));
    }
    camcraft_app::geometry::Mesh::from_facets(facets)
}

const DOME_RIM_Z: f64 = 20.0 * std::f64::consts::FRAC_1_SQRT_2;

fn finish_params(top_z: f64, bottom_z: f64, tolerance: f64) -> OpParams {
    OpParams::Finish3D {
        pattern: camcraft_app::finishing3d::FinishPattern::Waterline,
        stepover: None,
        tolerance,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(top_z),
        bottom_z: Height::fixed(bottom_z),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
    }
}

#[test]
fn finish3d_waterline_generates_and_verifies_clean() {
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 25.0);
    add_tool(&world, &mut history, "BN-0600");
    history.execute(&world, ImportModel::from_mesh("dome", dome_mesh()));
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Finish1",
            "BN-0600",
            SrcRef::Entity("dome".into()),
            finish_params(18.0, DOME_RIM_Z, 0.1),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Finish1").expect("generate"),
    );
    // Ok, not Warning: the envelope verdict came back inside the gate,
    // measured by the same kernel the direct tests drive.
    assert_eq!(status(&world, "Finish1"), OpStatus::Ok);
    let t = cache(&world, "Finish1").expect("cached").path;
    let plunges: Vec<f64> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Plunge)
        .map(|m| m.to.z)
        .collect();
    // One ring per level from 18 down, snapped at the rim - the scallop
    // stepdown's ladder: 2*sqrt(3*0.1) = 1.0954 per rung at most, and
    // tighter where the dome's crown band tilts past 45° from vertical
    // (M13.9's slope-aware ladder: six rungs on this band, not five).
    assert!((4..=6).contains(&plunges.len()), "ladder: {plunges:?}");
    assert!(
        plunges.last() == Some(&DOME_RIM_Z),
        "the bottom ring snaps to the bottom: {plunges:?}"
    );
    assert!(
        t.moves
            .iter()
            .all(|m| m.to.z <= 25.0 + 1e-9 && m.from.z <= 25.0 + 1e-9),
        "nothing starts above clearance"
    );
}

/// The dome's debug-lane band: 18 down to 16 instead of the rim (14.14).
/// Same solid, same tool, same laws - two rungs of the waterline ladder
/// instead of four, so the stamp/undo trips cost half. The full band
/// rides the heavy lane in each split test's `_full_band` twin.
const DOME_FAST_BOTTOM_Z: f64 = 16.0;

#[test]
fn the_ramp_flag_stamps_the_digest_and_clears_on_undo() {
    ramp_flag_case(DOME_FAST_BOTTOM_Z);
}

#[test]
#[ignore = "heavy lane: the ramp-flag stamp over the dome's whole 18-to-rim band, ~25 s debug"]
fn the_ramp_flag_stamps_the_digest_and_clears_on_undo_full_band() {
    ramp_flag_case(DOME_RIM_Z);
}

fn ramp_flag_case(bottom_z: f64) {
    // v0.5 (M8.2 B): entry_ramp rewrites a ring's entry legs, so it
    // stamps the digest like every other move-changing word - the ask
    // changed, so the op says Stale until the regen applies it. On this
    // dome every tangent is gentler than the split, so the silent-skip
    // law leaves the path itself alone: a stamp, not a change.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 25.0);
    add_tool(&world, &mut history, "BN-0600");
    history.execute(&world, ImportModel::from_mesh("dome", dome_mesh()));
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Ramp1",
            "BN-0600",
            SrcRef::Entity("dome".into()),
            finish_params(18.0, bottom_z, 0.1),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Ramp1").expect("generate"),
    );
    assert_eq!(status(&world, "Ramp1"), OpStatus::Ok);
    let base = cache(&world, "Ramp1").expect("cached").path;

    let mut ask = finish_params(18.0, bottom_z, 0.1);
    let OpParams::Finish3D { entry_ramp, .. } = &mut ask else {
        panic!("finish3d params expected");
    };
    *entry_ramp = true;
    history.execute(
        &world,
        EditOperationParams::new(&world, "Ramp1", ask).expect("edit"),
    );
    assert_eq!(
        status(&world, "Ramp1"),
        OpStatus::Stale,
        "the entry ask changed; the stamp must notice"
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "Ramp1").expect("regen"),
    );
    let t = cache(&world, "Ramp1").expect("cached").path;
    assert!(t.notes.len() == 1 && t.notes.iter().all(|n| n.starts_with(camcraft_app::operation::TOP_STOCK_WARNING)), "a gentle tangent adds no entry note; only the Top/stock warning remains: {:?}", t.notes);
    assert_eq!(t.moves, base.moves, "silent skip means the same path");

    // Undo the regen, then the edit: the default flag is a digest
    // no-op, so two steps back lands on v0.4's stamp and path again.
    history.undo(&world);
    assert_eq!(status(&world, "Ramp1"), OpStatus::Stale);
    history.undo(&world);
    assert_eq!(status(&world, "Ramp1"), OpStatus::Ok);
    assert_eq!(cache(&world, "Ramp1").expect("cached").path.moves, base.moves);
}

#[test]
fn the_ladder_number_stamps_the_digest_even_where_it_is_inert() {
    ladder_number_case(DOME_FAST_BOTTOM_Z);
}

#[test]
#[ignore = "heavy lane: the inert ladder stamp over the dome's whole 18-to-rim band, ~25 s debug"]
fn the_ladder_number_stamps_the_digest_even_where_it_is_inert_full_band() {
    ladder_number_case(DOME_RIM_Z);
}

fn ladder_number_case(bottom_z: f64) {
    // M2.5 A, same grammar as the ramp flag above: `steep_stepover`
    // feeds the stamp for every pattern, but `levels()` reads it only
    // under SteepShallow - so under Waterline the word is inert, not
    // silent. One Stale, one regen, and the dome's path does not move
    // by a lattice point.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 25.0);
    add_tool(&world, &mut history, "BN-0600");
    history.execute(&world, ImportModel::from_mesh("dome", dome_mesh()));
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "L1",
            "BN-0600",
            SrcRef::Entity("dome".into()),
            finish_params(18.0, bottom_z, 0.1),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "L1").expect("generate"),
    );
    assert_eq!(status(&world, "L1"), OpStatus::Ok);
    let base = cache(&world, "L1").expect("cached").path;

    let mut ask = finish_params(18.0, bottom_z, 0.1);
    let OpParams::Finish3D { steep_stepover, .. } = &mut ask else {
        panic!("finish3d params expected");
    };
    *steep_stepover = Some(2.5);
    history.execute(
        &world,
        EditOperationParams::new(&world, "L1", ask).expect("edit"),
    );
    assert_eq!(
        status(&world, "L1"),
        OpStatus::Stale,
        "the ladder byte rides the digest"
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "L1").expect("regen"),
    );
    let t = cache(&world, "L1").expect("cached").path;
    assert!(t.notes.len() == 1 && t.notes.iter().all(|n| n.starts_with(camcraft_app::operation::TOP_STOCK_WARNING)), "the inert word adds no note; only the Top/stock warning remains: {:?}", t.notes);
    assert_eq!(t.moves, base.moves, "inert means the same path");

    // Two steps back lands on the unstamped digest again.
    history.undo(&world);
    assert_eq!(status(&world, "L1"), OpStatus::Stale);
    history.undo(&world);
    assert_eq!(status(&world, "L1"), OpStatus::Ok);
    assert_eq!(cache(&world, "L1").expect("cached").path.moves, base.moves);
}

#[test]
fn the_split_and_the_floors_generate_through_the_operation_layer() {
    // The dispatch doors, not the kernels. The plateau is the
    // partition's smallest honest witness - five faces, a floor the
    // march owns, walls steep enough for the rings - and the app's
    // envelope verdict reads it in seconds instead of minutes. The
    // dome's physics (the split's border verdicts, Flat's refusal of
    // the unreachable rim disc) are pinned at the kernel door by the
    // motion twin in finish3d.rs; here only the doors ride.
    use camcraft_app::geometry::Triangle;
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 25.0);
    add_tool(&world, &mut history, "BN-0600");
    let quad = |p: [Point3; 4]| {
        vec![
            Triangle::new(p[0], p[1], p[2]),
            Triangle::new(p[0], p[2], p[3]),
        ]
    };
    let (a, b, c, d) = (
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(10.0, 0.0, 0.0),
        Point3::new(10.0, 10.0, 0.0),
        Point3::new(0.0, 10.0, 0.0),
    );
    let (e, f, g, h) = (
        Point3::new(0.0, 0.0, 5.0),
        Point3::new(10.0, 0.0, 5.0),
        Point3::new(10.0, 10.0, 5.0),
        Point3::new(0.0, 10.0, 5.0),
    );
    let walls = vec![
        quad([a, b, f, e]),
        quad([b, c, g, f]),
        quad([c, d, h, g]),
        quad([d, a, e, h]),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    let plateau = {
        let mut facets = walls.clone();
        facets.extend(quad([e, f, g, h])); // top
        Mesh::from_facets(facets)
    };
    let tube = Mesh::from_facets(walls);
    history.execute(&world, ImportModel::from_mesh("plateau", plateau));
    history.execute(&world, ImportModel::from_mesh("tube", tube));
    let pat = |q: camcraft_app::finishing3d::FinishPattern| {
        let mut ask = finish_params(5.0, 0.0, 0.1);
        let OpParams::Finish3D { pattern, .. } = &mut ask else {
            panic!("finish3d params expected");
        };
        *pattern = q;
        ask
    };
    for (name, src, q) in [
        ("FP", "plateau", camcraft_app::finishing3d::FinishPattern::Parallel),
        ("FS", "plateau", camcraft_app::finishing3d::FinishPattern::SteepShallow),
        ("FF", "plateau", camcraft_app::finishing3d::FinishPattern::Flat),
    ] {
        history.execute(
            &world,
            CreateOperation::new(&world, name, "BN-0600", SrcRef::Entity(src.into()), pat(q))
                .expect("create"),
        );
        history.execute(&world, GenerateOperation::new(&world, name).expect("generate"));
        assert_eq!(status(&world, name), OpStatus::Ok, "{name} must seat and cut");
    }
    let moves = |n: &str| cache(&world, n).expect("cached").path.moves;
    let (fp, fs) = (moves("FP"), moves("FS"));
    // The border law speaks only where a ring would ride shallow
    // tangency; on prismatic ground the march already owns every
    // floor and the wall rings stand all-steep, so the split rides
    // the parallel BYTE FOR BYTE here - it may drop nothing. (Where
    // the rings do speak - the dome's cap - the motion twin pins
    // the difference.)
    assert!(!fs.is_empty(), "the split went silent on the plateau");
    assert_eq!(fs, fp, "the partition dropped a cut it did not own");
    // The floors ride the programmed depth: every cutting row below
    // clearance sits on z 5 - Flat never descends the walls it cannot
    // hold a ball against.
    assert!(
        moves("FF")
            .iter()
            .filter(|m| matches!(m.kind, MoveKind::Feed | MoveKind::Plunge)
                && m.to.z < 25.0 - 1e-6)
            .all(|m| (m.to.z - 5.0).abs() <= 1e-6),
        "a floor row sank off the floor"
    );
    // A wall with no floor anywhere: Flat's Empty answer must reach
    // the user as a command refusal, not a silent nothing - and the
    // op must stay where nothing wrong can post.
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "FT",
            "BN-0600",
            SrcRef::Entity("tube".into()),
            pat(camcraft_app::finishing3d::FinishPattern::Flat),
        )
        .expect("create"),
    );
    let CommandError::Machining(msg) = GenerateOperation::new(&world, "FT")
        .err()
        .expect("a floorless tube must refuse the floors")
    else {
        panic!("expected a machining refusal");
    };
    assert!(msg.contains("no floor"), "{msg}");
    assert_eq!(status(&world, "FT"), OpStatus::Never, "a refusal cuts nothing");
}

#[test]
fn the_stage_c_patterns_generate_through_the_operation_layer() {
    // M2.5 C doors. At the kernel Ramp IS Parallel and the twin FORCES
    // the two sloped words (generate.rs), so an operation asked as Ramp
    // and one asked as Parallel-with-the-words set cut byte-identical
    // metal - and on the box's vertical walls both differ from a plain
    // Parallel, where the same flags stand false (the kernel twin in
    // finish3d.rs pins WHY the sloped laws speak there). MorphSpiral
    // rides the march over tilted terrain and the envelope verdict
    // comes back clean; Geodesic must first name a closed shell
    // refused, before any field is walked.
    use camcraft_app::finishing3d::FinishPattern;
    use camcraft_app::geometry::Triangle;
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 25.0);
    add_tool(&world, &mut history, "BN-0600");
    let quad = |p: [Point3; 4]| {
        vec![
            Triangle::new(p[0], p[1], p[2]),
            Triangle::new(p[0], p[2], p[3]),
        ]
    };
    let (a, b, c, d) = (
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(10.0, 0.0, 0.0),
        Point3::new(10.0, 10.0, 0.0),
        Point3::new(0.0, 10.0, 0.0),
    );
    let (e, f, g, h) = (
        Point3::new(0.0, 0.0, 5.0),
        Point3::new(10.0, 0.0, 5.0),
        Point3::new(10.0, 10.0, 5.0),
        Point3::new(0.0, 10.0, 5.0),
    );
    let walls = vec![
        quad([a, b, f, e]),
        quad([b, c, g, f]),
        quad([c, d, h, g]),
        quad([d, a, e, h]),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    let mut plateau_facets = walls;
    plateau_facets.extend(quad([e, f, g, h])); // top
    plateau_facets.extend(quad([a, d, c, b])); // base - a CLOSED box
    // (the doors test's plateau rides an open-bottom box; this one is
    // the geodesic refusal's witness, and a refusal needs a shell
    // with every rim edge paired. The base hides under the floor
    // plane's own law - no tangent there can rest clear of the top
    // - so the twin rows' metal does not move.)
    history.execute(
        &world,
        ImportModel::from_mesh("plateau", Mesh::from_facets(plateau_facets)),
    );
    // The wedge: a square top tilted 30 degrees over a flat base at
    // z 4 - the morph's local cos-law home witness, the same numbers
    // the kernel twin turns on.
    let t30 = 30f64.to_radians().tan();
    let surf = |y: f64| 10.0 - y * t30;
    let wedge = {
        let mut facets = Vec::new();
        facets.extend(quad([
            Point3::new(-8.0, -8.0, surf(-8.0)),
            Point3::new(8.0, -8.0, surf(-8.0)),
            Point3::new(8.0, 8.0, surf(8.0)),
            Point3::new(-8.0, 8.0, surf(8.0)),
        ])); // tilted top
        facets.extend(quad([
            Point3::new(-8.0, -8.0, 4.0),
            Point3::new(8.0, -8.0, 4.0),
            Point3::new(8.0, -8.0, surf(-8.0)),
            Point3::new(-8.0, -8.0, surf(-8.0)),
        ])); // walls
        facets.extend(quad([
            Point3::new(-8.0, 8.0, 4.0),
            Point3::new(8.0, 8.0, 4.0),
            Point3::new(8.0, 8.0, surf(8.0)),
            Point3::new(-8.0, 8.0, surf(8.0)),
        ]));
        facets.extend(quad([
            Point3::new(-8.0, -8.0, 4.0),
            Point3::new(-8.0, 8.0, 4.0),
            Point3::new(-8.0, 8.0, surf(8.0)),
            Point3::new(-8.0, -8.0, surf(-8.0)),
        ]));
        facets.extend(quad([
            Point3::new(8.0, -8.0, 4.0),
            Point3::new(8.0, 8.0, 4.0),
            Point3::new(8.0, 8.0, surf(8.0)),
            Point3::new(8.0, -8.0, surf(-8.0)),
        ]));
        facets.extend(quad([
            Point3::new(-8.0, -8.0, 4.0),
            Point3::new(8.0, -8.0, 4.0),
            Point3::new(8.0, 8.0, 4.0),
            Point3::new(-8.0, 8.0, 4.0),
        ])); // base
        Mesh::from_facets(facets)
    };
    history.execute(&world, ImportModel::from_mesh("wedge", wedge));
    let ask = |q: FinishPattern, rp: bool, er: bool, top: f64, bottom: f64| {
        let mut a = finish_params(top, bottom, 0.1);
        let OpParams::Finish3D {
            pattern,
            ramp,
            entry_ramp,
            ..
        } = &mut a
        else {
            panic!("finish3d params expected");
        };
        *pattern = q;
        *ramp = rp;
        *entry_ramp = er;
        a
    };
    let mut op = |name: &str, src: &str, a: OpParams| {
        history.execute(
            &world,
            CreateOperation::new(&world, name, "BN-0600", SrcRef::Entity(src.into()), a)
                .expect("create"),
        );
        history.execute(&world, GenerateOperation::new(&world, name).expect("generate"));
        assert_eq!(status(&world, name), OpStatus::Ok, "{name} must seat and cut");
    };
    op("FR", "plateau", ask(FinishPattern::Ramp, false, false, 5.0, 0.0));
    op("FQ", "plateau", ask(FinishPattern::Parallel, true, true, 5.0, 0.0));
    op("FP", "plateau", ask(FinishPattern::Parallel, false, false, 5.0, 0.0));
    op(
        "FM",
        "wedge",
        ask(FinishPattern::MorphSpiral, false, false, 10.0, 4.0),
    );
    let moves = |n: &str| cache(&world, n).expect("cached").path.moves;
    assert_eq!(
        moves("FQ"),
        moves("FR"),
        "the twin's forcing is the same metal, not a new one"
    );
    assert_ne!(
        moves("FP"),
        moves("FR"),
        "the sloped laws are inert through the app's doors"
    );
    assert!(!moves("FM").is_empty(), "the morph went silent on the wedge");
    // A closed box has no open edge to march rings inward from: the
    // refusal must reach the user named, and the op must sit where
    // nothing wrong can post.
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "FG",
            "BN-0600",
            SrcRef::Entity("plateau".into()),
            ask(FinishPattern::Geodesic, false, false, 5.0, 0.0),
        )
        .expect("create"),
    );
    let CommandError::Machining(msg) = GenerateOperation::new(&world, "FG")
        .err()
        .expect("a closed shell must refuse the geodesic")
    else {
        panic!("expected a machining refusal");
    };
    assert!(msg.contains("closed shell"), "{msg}");
    assert_eq!(status(&world, "FG"), OpStatus::Never, "a refusal cuts nothing");
}

#[test]
fn the_blend_pattern_generates_through_the_operation_layer() {
    // M2.5 D's door. A 38-degree sheet is shallow by the 45 flag and
    // past the band line, so the split and the Blend cannot cut the
    // same metal: the sheet-crossing ring runs ride under Blend and
    // stay home under SteepShallow. Through the operation layer that
    // difference must survive - the door routes the pattern word to
    // `blend`, not to `parallel` wearing a new name.
    use camcraft_app::finishing3d::FinishPattern;
    use camcraft_app::geometry::Triangle;
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 25.0);
    add_tool(&world, &mut history, "BN-0600");
    let quad = |p: [Point3; 4]| {
        vec![
            Triangle::new(p[0], p[1], p[2]),
            Triangle::new(p[0], p[2], p[3]),
        ]
    };
    let t = 38f64.to_radians().tan();
    let surf = |y: f64| 10.0 - y * t;
    let h = 6.0;
    let mut facets = Vec::new();
    facets.extend(quad([
        Point3::new(-h, -h, surf(-h)),
        Point3::new(h, -h, surf(-h)),
        Point3::new(h, h, surf(h)),
        Point3::new(-h, h, surf(h)),
    ])); // tilted top
    for (y0, y1) in [(-h, -h), (h, h)] {
        facets.extend(quad([
            Point3::new(-h, y0, 4.0),
            Point3::new(h, y0, 4.0),
            Point3::new(h, y1, surf(y1)),
            Point3::new(-h, y1, surf(y0)),
        ]));
    }
    for (x0, x1) in [(-h, -h), (h, h)] {
        facets.extend(quad([
            Point3::new(x0, -h, 4.0),
            Point3::new(x1, h, 4.0),
            Point3::new(x1, h, surf(h)),
            Point3::new(x0, -h, surf(-h)),
        ]));
    }
    facets.extend(quad([
        Point3::new(-h, -h, 4.0),
        Point3::new(h, -h, 4.0),
        Point3::new(h, h, 4.0),
        Point3::new(-h, h, 4.0),
    ])); // base
    history.execute(
        &world,
        ImportModel::from_mesh("sheet38", Mesh::from_facets(facets)),
    );
    let ask = |q: FinishPattern| {
        let mut a = finish_params(14.7, 4.0, 0.1);
        let OpParams::Finish3D { pattern, .. } = &mut a else {
            panic!("finish3d params expected");
        };
        *pattern = q;
        a
    };
    let mut op = |name: &str, a: OpParams| {
        history.execute(
            &world,
            CreateOperation::new(&world, name, "BN-0600", SrcRef::Entity("sheet38".into()), a)
                .expect("create"),
        );
        history.execute(&world, GenerateOperation::new(&world, name).expect("generate"));
        assert_eq!(status(&world, name), OpStatus::Ok, "{name} must seat and cut");
    };
    op("FD", ask(FinishPattern::Blend));
    op("FE", ask(FinishPattern::SteepShallow));
    let moves = |n: &str| cache(&world, n).expect("cached").path.moves;
    assert!(!moves("FD").is_empty(), "the blend went silent on the sheet");
    assert_ne!(
        moves("FD"),
        moves("FE"),
        "the blend's door cut the split's metal - the band never reached the kernel"
    );
}

#[test]
fn the_morph_pattern_generates_through_the_operation_layer() {
    // M2.5 D's other door. The morph's rings ARE the waterline's
    // ladder - the pattern's promise is the loft family riding
    // between the levels - so through the operation layer the morph
    // must cut strictly MORE than the ladder: same rings, extra
    // guides. A door that routes "morph" to `waterline` leaves the
    // feed count equal; this test is that failure's witness.
    use camcraft_app::finishing3d::FinishPattern;
    use camcraft_app::geometry::Triangle;
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 25.0);
    add_tool(&world, &mut history, "BN-0600");
    let quad = |p: [Point3; 4]| {
        vec![
            Triangle::new(p[0], p[1], p[2]),
            Triangle::new(p[0], p[2], p[3]),
        ]
    };
    let t = 38f64.to_radians().tan();
    let surf = |y: f64| 10.0 - y * t;
    let h = 6.0;
    let mut facets = Vec::new();
    facets.extend(quad([
        Point3::new(-h, -h, surf(-h)),
        Point3::new(h, -h, surf(-h)),
        Point3::new(h, h, surf(h)),
        Point3::new(-h, h, surf(h)),
    ])); // tilted top
    for (y0, y1) in [(-h, -h), (h, h)] {
        facets.extend(quad([
            Point3::new(-h, y0, 4.0),
            Point3::new(h, y0, 4.0),
            Point3::new(h, y1, surf(y1)),
            Point3::new(-h, y1, surf(y0)),
        ]));
    }
    for (x0, x1) in [(-h, -h), (h, h)] {
        facets.extend(quad([
            Point3::new(x0, -h, 4.0),
            Point3::new(x1, h, 4.0),
            Point3::new(x1, h, surf(h)),
            Point3::new(x0, -h, surf(-h)),
        ]));
    }
    facets.extend(quad([
        Point3::new(-h, -h, 4.0),
        Point3::new(h, -h, 4.0),
        Point3::new(h, h, 4.0),
        Point3::new(-h, h, 4.0),
    ])); // base
    history.execute(
        &world,
        ImportModel::from_mesh("sheet38m", Mesh::from_facets(facets)),
    );
    let ask = |q: FinishPattern| {
        let mut a = finish_params(14.7, 4.0, 0.1);
        let OpParams::Finish3D { pattern, .. } = &mut a else {
            panic!("finish3d params expected");
        };
        *pattern = q;
        a
    };
    let mut op = |name: &str, a: OpParams| {
        history.execute(
            &world,
            CreateOperation::new(&world, name, "BN-0600", SrcRef::Entity("sheet38m".into()), a)
                .expect("create"),
        );
        history.execute(&world, GenerateOperation::new(&world, name).expect("generate"));
        assert_eq!(status(&world, name), OpStatus::Ok, "{name} must seat and cut");
    };
    op("FF", ask(FinishPattern::Morph));
    op("FH", ask(FinishPattern::Waterline));
    let feeds = |n: &str| {
        cache(&world, n)
            .expect("cached")
            .path
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Feed)
            .count()
    };
    assert!(feeds("FF") > 0, "the morph went silent on the sheet");
    assert!(
        feeds("FF") > feeds("FH"),
        "the morph cut no more than the ladder - the lofts never reached the kernel ({} vs {})",
        feeds("FF"),
        feeds("FH")
    );
}

#[test]
fn the_flow_pattern_generates_through_the_operation_layer() {
    // M2.5 E's door, on a COARSE barrel (the lane law: no minutes-
    // long generation rides a quick test - the dome-sized fixtures
    // belong to the ignored verdicts). The barrel's crown is
    // shallow, its flanks steep: the flow door must generate,
    // cut, and stand prismatic - an axis on any move would mean
    // the 5-axis stamp leaked into a 3-axis program. The witness
    // against an alias to the ladder is the same barrel under
    // Waterline: the field's lines over the crown cut metal the
    // rings alone never reach, and a door routed wrong leaves the
    // two paths equal.
    use camcraft_app::cl::MoveKind;
    use camcraft_app::finishing3d::FinishPattern;
    use camcraft_app::geometry::Triangle;
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 25.0);
    add_tool(&world, &mut history, "BN-0600");
    // A FULL capped cylinder: the ring close demands a closed
    // mesh, and the field's blade direction is the axis. Crown at
    // z = 24, floor at z = 0, axis along x.
    let (rad, x_len, n_th, n_x) = (12.0f64, 20.0, 12usize, 4usize);
    let v = |j: usize, i: usize| {
        let th = std::f64::consts::TAU * j as f64 / n_th as f64;
        Point3::new(
            -x_len + 2.0 * x_len * i as f64 / n_x as f64,
            rad * th.cos(),
            rad + rad * th.sin(),
        )
    };
    let mut facets = Vec::new();
    for j in 0..n_th {
        for i in 0..n_x {
            let (a, b, c, d) = (
                v(j, i),
                v((j + 1) % n_th, i),
                v((j + 1) % n_th, i + 1),
                v(j, i + 1),
            );
            facets.push(Triangle::new(a, b, c));
            facets.push(Triangle::new(a, c, d));
        }
    }
    for i in [0, n_x] {
        let x = -x_len + 2.0 * x_len * i as f64 / n_x as f64;
        let c = Point3::new(x, 0.0, rad);
        for j in 0..n_th {
            let (a, b) = (v(j, i), v((j + 1) % n_th, i));
            // Outward normals: the +x cap winds a-b, the -x b-a.
            let (a, b) = if i == 0 { (b, a) } else { (a, b) };
            facets.push(Triangle::new(c, a, b));
        }
    }
    history.execute(&world, ImportModel::from_mesh("barrel5e", Mesh::from_facets(facets)));
    let ask = |q: FinishPattern| {
        let mut a = finish_params(20.0, 12.0, 0.1);
        let OpParams::Finish3D { pattern, .. } = &mut a else {
            panic!("finish3d params expected");
        };
        *pattern = q;
        a
    };
    let mut op = |name: &str, a: OpParams| {
        history.execute(
            &world,
            CreateOperation::new(&world, name, "BN-0600", SrcRef::Entity("barrel5e".into()), a)
                .expect("create"),
        );
        history.execute(&world, GenerateOperation::new(&world, name).expect("generate"));
        assert_eq!(status(&world, name), OpStatus::Ok, "{name} must seat and cut");
    };
    op("FG", ask(FinishPattern::Flow));
    op("F5", ask(FinishPattern::Waterline));
    let path = |n: &str| cache(&world, n).expect("cached").path;
    let feeds = |n: &str| path(n).moves.iter().filter(|m| m.kind == MoveKind::Feed).count();
    assert!(feeds("FG") > 0, "the flow door went silent on the barrel");
    assert!(
        path("FG").is_prismatic(),
        "flow3d leaked a posed axis into a 3-axis program"
    );
    assert_ne!(
        path("FG").moves,
        path("F5").moves,
        "the flow door cut the ladder's metal - the field never reached the kernel"
    );
}

#[test]
fn finish3d_refuses_a_flat_bottomed_tool() {
    // The forbidden-center scan is ball geometry; an end mill here must
    // refuse by name, never generate a ball path dressed as an end mill.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 25.0);
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("dome", dome_mesh()));
    common::create_past_the_tool_law(
        &world,
        &mut history,
        "FinishE",
        "EM-0600-4FL",
        SrcRef::Entity("dome".into()),
        finish_params(18.0, DOME_RIM_Z, 0.1),
    );
    let CommandError::Machining(msg) = common::generate_past_the_tool_law(&world, "FinishE")
        .expect_err("the waterline must refuse a flat tool")
    else {
        panic!("expected a machining refusal");
    };
    assert!(msg.contains("ball-nose"), "{msg}");
    assert_eq!(
        status(&world, "FinishE"),
        OpStatus::Never,
        "the refusal leaves no path behind"
    );
}

#[test]
fn finish3d_refuses_bad_tolerance_at_creation() {
    // Validate early: a zero deviation budget is nonsense the command
    // layer refuses before any generator sees the mesh.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "BN-0600");
    history.execute(&world, ImportModel::from_mesh("dome", dome_mesh()));
    assert!(matches!(
        CreateOperation::new(
            &world,
            "Finish0",
            "BN-0600",
            SrcRef::Entity("dome".into()),
            finish_params(18.0, DOME_RIM_Z, 0.0),
        ),
        Err(CommandError::Invalid(_))
    ));
}

// ---------------------------------------------------------------------------
// M5.1c: the 5-axis twins through the command chain. The law under test
// lives in `multiaxis` - the ball ladder is the waterline/raster path
// and what is NEW is the pose each move holds - so at this level the
// checks are that the pose survives the trip: an Ok verdict, leaning
// unit axes on the contact moves, nothing stated in air, and the
// refusals (flat tool, nonsense pose) named at the right door.
// ---------------------------------------------------------------------------

/// The dome's rings stand 22..45 degrees off vertical, so a stamped
/// path must show a leaning spindle where the finish's never does.
fn pose_law(name: &str, path: &camcraft_app::cl::Toolpath) {
    let mut leaned = 0usize;
    let mut held: Option<[f64; 3]> = None;
    for m in &path.moves {
        if let Some(a) = m.axis {
            assert!(
                a.iter().all(|v| v.is_finite()),
                "{name}: a non-finite pose on a move"
            );
            let len = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-6, "{name}: a pose of length {len}");
            if m.kind == MoveKind::Feed && (a[0] * a[0] + a[1] * a[1]).sqrt() > 1e-3 {
                leaned += 1;
            }
        }
        // The pivot law's rapid contract (every stamped twin here
        // rides the catalog's 6 mm ball).
        rapid_pose_contract(name, m, held, 3.0);
        held = m.axis.or(held);
    }
    assert!(leaned > 0, "{name}: no move holds a leaning axis");
}

/// The pivot law's rapid contract: a rapid travels under the pose it
/// inherits (the post keeps the tilt modal), so it may STATE a pose,
/// but a NEW one only as the swarf link's swing - a turn IN AIR about
/// the ball centre, which the sphere's rotation invariance keeps
/// where it was: `from + r·held == to + r·new`. Nothing leaned before
/// the first lean, the held pose after it, and never a pose turn that
/// drags the ball.
fn rapid_pose_contract(name: &str, m: &camcraft_app::cl::Move, held: Option<[f64; 3]>, r: f64) {
    if m.kind != MoveKind::Rapid || m.axis == held {
        return;
    }
    let up = [0.0, 0.0, 1.0];
    let (p, q) = (held.unwrap_or(up), m.axis.unwrap_or(up));
    let c0 = [m.from.x + r * p[0], m.from.y + r * p[1], m.from.z + r * p[2]];
    let c1 = [m.to.x + r * q[0], m.to.y + r * q[1], m.to.z + r * q[2]];
    let d = ((c0[0] - c1[0]).powi(2) + (c0[1] - c1[1]).powi(2) + (c0[2] - c1[2]).powi(2)).sqrt();
    assert!(
        d <= 1e-6,
        "{name}: a rapid changed the pose it inherited off its ball centre ({d:.3e} mm): {:?} -> {:?}",
        held,
        m.axis
    );
}

/// The verdict half of "verifies clean": the cached path carries NO
/// blocking warning (the op being fresh says nothing about the
/// verdict). The scallop note (M13.9) is the one kind allowed through:
/// these ops state a 2.0 mm stepover on an r3 ball, a 0.16 mm cusp the
/// ridge flank law now reads over the 0.1 tolerance - a surface-quality
/// reading, shown on the op and never holding the post.
fn verifies_clean(world: &World, op: &str) {
    let t = cache(world, op).expect("cached");
    let blocking: Vec<_> = t.warnings.iter().filter(|w| w.blocks_post()).collect();
    assert!(blocking.is_empty(), "{op} must verify with zero blocking warnings: {blocking:?}");
}

fn flow5d_params(top_z: f64, bottom_z: f64) -> OpParams {
    OpParams::Flow5D {
        tolerance: 0.1,
        stepover: Some(2.0),
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(top_z),
        bottom_z: Height::fixed(bottom_z),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 15.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
        shank_avoid: false,
    }
}

fn contour5d_params(top_z: f64, bottom_z: f64) -> OpParams {
    OpParams::Contour5D {
        tolerance: 0.1,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(top_z),
        bottom_z: Height::fixed(bottom_z),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 15.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
        shank_avoid: false,
    }
}

/// The dome's kin at golden-sized tessellation (6x24): the raster's
/// fixture. The march asks every shallow facet at every sample, so
/// the big dome's raster is the IGNORED lane's (its motion-side twin
/// says "minutes in debug" on its tin) - the debug ladder rides the
/// hill, where the same laws cost seconds.
fn hill_mesh() -> camcraft_app::geometry::Mesh {
    use camcraft_app::geometry::{Point3, Triangle};
    const TAU: f64 = std::f64::consts::TAU;
    let (radius, polar_max, n_lat, n_lon) = (12.0, std::f64::consts::FRAC_PI_3, 6usize, 24usize);
    let mut facets = Vec::new();
    let ring = |k: usize| -> Vec<Point3> {
        let ph = polar_max * k as f64 / n_lat as f64;
        let (z, rr) = (radius * ph.cos(), radius * ph.sin());
        (0..n_lon)
            .map(|j| {
                let th = TAU * j as f64 / n_lon as f64;
                Point3::new(rr * th.cos(), rr * th.sin(), z)
            })
            .collect()
    };
    let apex = Point3::new(0.0, 0.0, radius);
    let first = ring(1);
    for j in 0..n_lon {
        facets.push(Triangle::new(apex, first[j], first[(j + 1) % n_lon]));
    }
    for k in 1..n_lat {
        let ra = ring(k);
        let rb = ring(k + 1);
        for j in 0..n_lon {
            let j2 = (j + 1) % n_lon;
            facets.push(Triangle::new(ra[j], rb[j], rb[j2]));
            facets.push(Triangle::new(ra[j], rb[j2], ra[j2]));
        }
    }
    let rim = ring(n_lat);
    let center = Point3::new(0.0, 0.0, radius * polar_max.cos());
    for j in 0..n_lon {
        facets.push(Triangle::new(center, rim[j], rim[(j + 1) % n_lon]));
    }
    camcraft_app::geometry::Mesh::from_facets(facets)
}

/// Apex 12, rim 60 degrees: z = 12 cos 60 = 6, and normals tilt
/// past the raster's 45-degree jurisdiction by the equator's rim.
const HILL_RIM_Z: f64 = 6.0;

fn parallel5d_params(top_z: f64, bottom_z: f64) -> OpParams {
    OpParams::Parallel5D {
        tolerance: 0.1,
        stepover: Some(2.0),
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(top_z),
        bottom_z: Height::fixed(bottom_z),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 15.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
        shank_avoid: false,
    }
}

#[test]
fn contour5d_generates_leaning_axes_and_verifies_clean() {
    contour5d_case(DOME_FAST_BOTTOM_Z);
}

#[test]
#[ignore = "heavy lane: contour5d over the dome's whole 18-to-rim band against its model stock, ~28 s debug"]
fn contour5d_generates_leaning_axes_and_verifies_clean_full_band() {
    contour5d_case(DOME_RIM_Z);
}

fn contour5d_case(bottom_z: f64) {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "BN-0600");
    history.execute(&world, ImportModel::from_mesh("dome", dome_mesh()));
    model_stock(&world, &mut history, "dome");
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "C5A",
            "BN-0600",
            SrcRef::Entity("dome".into()),
            contour5d_params(18.0, bottom_z),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "C5A").expect("generate"),
    );
    assert_eq!(
        status(&world, "C5A"),
        OpStatus::Ok,
        "the envelope verdict came back inside the gate"
    );
    verifies_clean(&world, "C5A");
    let t = cache(&world, "C5A").expect("cached").path;
    pose_law("contour5d", &t);
    // Read at the BALL: a posed plunge states its tip r back down the
    // pose (the pivot law), so the ring's level is its centre's, r
    // over the Bottom plane the upright ladder's tip stood on.
    let r = 3.0;
    let plunges: Vec<f64> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Plunge)
        .map(|m| m.to.z + r * m.axis.map_or(1.0, |a| a[2]))
        .collect();
    assert!(
        plunges.last().is_some_and(|z| (z - (bottom_z + r)).abs() < 1e-9),
        "the bottom ring snaps to the bottom: {plunges:?}"
    );
}

#[test]
fn parallel5d_generates_leaning_axes_and_verifies_clean() {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "BN-0600");
    history.execute(&world, ImportModel::from_mesh("hill", hill_mesh()));
    model_stock(&world, &mut history, "hill");
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "P5A",
            "BN-0600",
            SrcRef::Entity("hill".into()),
            parallel5d_params(10.0, HILL_RIM_Z),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "P5A").expect("generate"),
    );
    assert_eq!(status(&world, "P5A"), OpStatus::Ok);
    verifies_clean(&world, "P5A");
    let t = cache(&world, "P5A").expect("cached").path;
    pose_law("parallel5d", &t);
    // The raster reads Top/Bottom as the first and last line's planes,
    // not a ladder of rungs: no cut move may sink below the Bottom.
    let lowest = t
        .moves
        .iter()
        .filter(|m| matches!(m.kind, MoveKind::Feed | MoveKind::Plunge))
        .map(|m| m.to.z)
        .fold(f64::INFINITY, f64::min);
    assert!(
        lowest >= HILL_RIM_Z - 1e-9,
        "the raster sank below its Bottom plane: {lowest}"
    );
    let on_flank = t
        .moves
        .iter()
        .any(|m| m.kind == MoveKind::Feed && (m.to.z - HILL_RIM_Z).abs() < 1e-9);
    assert!(
        on_flank,
        "nothing reached the Bottom plane - the flank went uncut"
    );
}

#[test]
fn flow5d_generates_following_axes_and_verifies_clean() {
    // The flowline over the same hill the parallel twin finishes: the
    // field bends the rungs, the raster's contract (planes, cone, no
    // sink below Bottom) is unchanged - that is the point of the kind.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "BN-0600");
    history.execute(&world, ImportModel::from_mesh("hill", hill_mesh()));
    model_stock(&world, &mut history, "hill");
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "F5A",
            "BN-0600",
            SrcRef::Entity("hill".into()),
            flow5d_params(10.0, HILL_RIM_Z),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "F5A").expect("generate"),
    );
    assert_eq!(
        status(&world, "F5A"),
        OpStatus::Ok,
        "the envelope verdict came back inside the gate"
    );
    verifies_clean(&world, "F5A");
    let t = cache(&world, "F5A").expect("cached").path;
    pose_law("flow5d", &t);
    let lowest = t
        .moves
        .iter()
        .filter(|m| matches!(m.kind, MoveKind::Feed | MoveKind::Plunge))
        .map(|m| m.to.z)
        .fold(f64::INFINITY, f64::min);
    assert!(
        lowest >= HILL_RIM_Z - 1e-9,
        "the flowline sank below its Bottom plane: {lowest}"
    );
}

#[test]
fn axis_twins_refuse_a_flat_bottomed_tool_by_name() {
    // Same law as the finish's, and the refusal says WHICH kind: a
    // silent fallback to a ball dressed as an end mill is the gouge.
    // The flowline is the twins' third sibling and obeys it whole.
    for (name, params) in [
        ("TwinC", contour5d_params(18.0, DOME_RIM_Z)),
        ("TwinP", parallel5d_params(18.0, DOME_RIM_Z)),
        ("FlowF", flow5d_params(18.0, DOME_RIM_Z)),
    ] {
        let world = new_world();
        let mut history = History::default();
        box_stock(&world, &mut history, 60.0, 60.0, 25.0);
        add_tool(&world, &mut history, "EM-0600-4FL");
        history.execute(&world, ImportModel::from_mesh("dome", dome_mesh()));
        common::create_past_the_tool_law(
            &world,
            &mut history,
            name,
            "EM-0600-4FL",
            SrcRef::Entity("dome".into()),
            params,
        );
        let CommandError::Machining(msg) = common::generate_past_the_tool_law(&world, name)
            .expect_err("the twins must refuse a flat tool")
        else {
            panic!("expected a machining refusal");
        };
        assert!(msg.contains("ball-nose"), "{msg}");
        assert!(msg.contains("5-Axis"), "{msg}");
        assert_eq!(
            status(&world, name),
            OpStatus::Never,
            "the refusal leaves no path behind"
        );
    }
}

#[test]
fn axis_twins_refuse_nonsense_pose_at_creation() {
    // Validate early at the command door: a zero deviation budget (the
    // finish's law) and a 91-degree lean (the pose door's) never reach
    // a generator.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "BN-0600");
    history.execute(&world, ImportModel::from_mesh("dome", dome_mesh()));
    let mk = |op: OpParams| {
        CreateOperation::new(
            &world,
            "PoseX",
            "BN-0600",
            SrcRef::Entity("dome".into()),
            op,
        )
    };
    let OpParams::Contour5D { tolerance, .. } = contour5d_params(18.0, DOME_RIM_Z) else {
        unreachable!()
    };
    let mut zero_tol = contour5d_params(18.0, DOME_RIM_Z);
    if let OpParams::Contour5D { tolerance: t, .. } = &mut zero_tol {
        *t = 0.0;
    }
    assert!(
        matches!(mk(zero_tol), Err(CommandError::Invalid(_))),
        "tol {tolerance}"
    );
    let mut flat_lean = parallel5d_params(18.0, DOME_RIM_Z);
    if let OpParams::Parallel5D { lead_deg, .. } = &mut flat_lean {
        *lead_deg = 91.0;
    }
    assert!(matches!(mk(flat_lean), Err(CommandError::Invalid(_))));
    let mut no_cone = contour5d_params(18.0, DOME_RIM_Z);
    if let OpParams::Contour5D { max_tilt_deg, .. } = &mut no_cone {
        *max_tilt_deg = 0.0;
    }
    assert!(matches!(mk(no_cone), Err(CommandError::Invalid(_))));
}

// ---------------------------------------------------------------------------
// M5.1 stage D: the swarf family through the command chain. The laws
// under test live in `multiaxis` - the pass lines run WITH the ruling
// the analysis reads off the model, and a ruled contact poses the axis
// ALONG it, the panel's lead/lag/cone be damned (that standing choice is
// RECOMMENDED.md's to argue) - so at this level the checks are the trip:
// an Ok verdict, blade-lying unit axes on the body moves, the ladder's
// extra rungs, and the refusals (flat tool, unruled dome, a nonsense
// pass count) named at the right door. The exact-bit geometry laws - the
// identity with finishing3d::swarf, the rim rows' honesty, the ×4/×2
// relaxation - are ruling.rs's; nothing here re-measures them.
// ---------------------------------------------------------------------------

/// The blade: a CIRCULAR tube about the X axis, end-capped - ruling.rs's
/// fixture shrunk to debug-ladder size (16 rings x 6 stations, radius
/// 12) and closed into a solid. Closed is load-bearing at this level:
/// the app's footprint law (`slice::project`, the same silhouette every
/// 3D finish reads) sections the mesh at 16 z-levels, and an OPEN tube
/// shadows as dangling chords - it refuses honestly at the first probe,
/// exactly as an open STL would refuse a Finish3D. The caps are flat,
/// so the analysis claims no ruling on them: the end columns stay the
/// rim rows ruling.rs calls boundary honesty, and the checks below
/// speak inside the same 5/6 of the half-length its do.
fn blade_mesh() -> camcraft_app::geometry::Mesh {
    use camcraft_app::geometry::{Point3, Triangle};
    const TAU: f64 = std::f64::consts::TAU;
    let (radius, x_len, n_circ, n_x) = (12.0, 24.0, 16usize, 6usize);
    let v = |j: usize, i: usize| {
        let th = TAU * j as f64 / n_circ as f64;
        Point3::new(
            -x_len + 2.0 * x_len * i as f64 / n_x as f64,
            radius * th.cos(),
            radius * th.sin(),
        )
    };
    let mut facets = Vec::new();
    for j in 0..n_circ {
        for i in 0..n_x {
            let (a, b, c, d) = (v(j, i), v(j + 1, i), v(j + 1, i + 1), v(j, i + 1));
            facets.push(Triangle::new(a, b, c));
            facets.push(Triangle::new(a, c, d));
        }
    }
    // The two caps: a fan about each end's true center, wound outward
    // with the tube (at -x the fan order reverses to keep the normals
    // honest - no zero-area triangles anywhere in a fixture).
    let (cap, cen) = (|x: f64| Point3::new(x, 0.0, 0.0), x_len);
    for j in 0..n_circ {
        facets.push(Triangle::new(cap(cen), v(j, n_x), v(j + 1, n_x)));
        facets.push(Triangle::new(cap(-cen), v(j + 1, 0), v(j, 0)));
    }
    camcraft_app::geometry::Mesh::from_facets(facets)
}

fn swarf_params(top_z: f64, bottom_z: f64) -> OpParams {
    OpParams::Swarf {
        tolerance: 0.1,
        stepover: Some(2.0),
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(top_z),
        bottom_z: Height::fixed(bottom_z),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 15.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
    }
}

/// The ladder kind is the plain one's params plus `passes` - the panel
/// says so and the digest agrees, so the builder says so too.
fn advanced_swarf_params(top_z: f64, bottom_z: f64, passes: u32) -> OpParams {
    OpParams::AdvancedSwarf {
        tolerance: 0.1,
        stepover: Some(2.0),
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(top_z),
        bottom_z: Height::fixed(bottom_z),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 15.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
        passes,
    }
}

/// The blade law at this level: unit finite poses, nothing stated in
/// air, and a REAL population of blade-lying poses inside the blade -
/// the 15-degree lead these params carry bends nothing where the
/// surface speaks, which is the whole point of the kind. An EXISTENCE
/// claim, not the every-body-move universal: the analysis keeps its
/// ruling honest by withholding the claim at rims and on tessellations
/// whose fit sits near the developability thresholds - and at this
/// debug size (16 rings, capped ends: the caps' junction rows are rims
/// too) that is whole columns of the tube. The universal identity is
/// ruling.rs's law at motion's 32-ring size; what a dropped ruled
/// branch would look like HERE is zero blade-lying poses, and >= 8
/// catches exactly that.
fn blade_pose_law(name: &str, path: &camcraft_app::cl::Toolpath) {
    let (mut body, mut posed) = (0usize, 0usize);
    for m in &path.moves {
        if let Some(a) = m.axis {
            assert!(
                a.iter().all(|v| v.is_finite()),
                "{name}: a non-finite pose on a move"
            );
            let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
            assert!((l - 1.0).abs() < 1e-6, "{name}: a pose of length {l}");
            posed += 1;
            if m.kind == MoveKind::Feed && m.to.x.abs() <= 20.0 && a[0].abs() / l > 0.995 {
                assert!(
                    a[1].abs() / l < 0.09 && a[2].abs() / l < 0.09,
                    "{name}: a blade-lying count off the blade at {:?}",
                    m.to
                );
                body += 1;
            }
        }
    }
    rapids_inherit(path, 3.0);
    assert!(
        body >= 8,
        "{name}: only {body} blade-lying body poses over a whole tube"
    );
    assert!(posed >= body, "{name}: uncounted poses cannot be");
}

/// The blade's debug-lane band: 11 down to 1 instead of -11. Same
/// tube, same tool, same rulings - the upper half of the flank, so the
/// generate-and-verify trips cost about half. The full band rides the
/// heavy lane in each split test's `_full_band` twin.
const BLADE_FAST_BOTTOM_Z: f64 = 1.0;

#[test]
fn swarf_generates_blade_lying_axes_and_verifies_clean() {
    swarf_case(BLADE_FAST_BOTTOM_Z);
}

#[test]
#[ignore = "heavy lane: swarf over the blade's whole 11-to--11 band against its model stock, ~21 s debug"]
fn swarf_generates_blade_lying_axes_and_verifies_clean_full_band() {
    swarf_case(-11.0);
}

fn swarf_case(bottom_z: f64) {
    let world = new_world();
    let mut history = History::default();
    add_swarf_ball(&world, &mut history);
    history.execute(&world, ImportModel::from_mesh("blade", blade_mesh()));
    model_stock(&world, &mut history, "blade");
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "SWA",
            "BN-0600",
            SrcRef::Entity("blade".into()),
            swarf_params(11.0, bottom_z),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "SWA").expect("generate"),
    );
    assert_eq!(
        status(&world, "SWA"),
        OpStatus::Ok,
        "the envelope verdict came back inside the gate"
    );
    verifies_clean(&world, "SWA");
    let t = cache(&world, "SWA").expect("cached").path;
    pose_law("swarf", &t);
    blade_pose_law("swarf", &t);
}

#[test]
fn advanced_swarf_adds_the_coarse_rungs() {
    advanced_swarf_case(BLADE_FAST_BOTTOM_Z);
}

#[test]
#[ignore = "heavy lane: the two-rung swarf ladder over the blade's whole 11-to--11 band, both ops verified against model stock, ~65 s debug"]
fn advanced_swarf_adds_the_coarse_rungs_full_band() {
    advanced_swarf_case(-11.0);
}

fn advanced_swarf_case(bottom_z: f64) {
    // The ladder's contract in one line: more passes, more cut, and
    // the SAME blade-lying law on every rung. (rung-by-rung geometry -
    // the 4x tolerance relaxation, the doubled spacing - is the
    // motion-side swarf_multi tests' claim.)
    let world = new_world();
    let mut history = History::default();
    add_swarf_ball(&world, &mut history);
    history.execute(&world, ImportModel::from_mesh("blade", blade_mesh()));
    model_stock(&world, &mut history, "blade");
    for (name, params) in [
        ("SW1", swarf_params(11.0, bottom_z)),
        ("SW2", advanced_swarf_params(11.0, bottom_z, 2)),
    ] {
        history.execute(
            &world,
            CreateOperation::new(
                &world,
                name,
                "BN-0600",
                SrcRef::Entity("blade".into()),
                params,
            )
            .expect("create"),
        );
        history.execute(
            &world,
            GenerateOperation::new(&world, name).expect("generate"),
        );
        assert_eq!(status(&world, name), OpStatus::Ok);
    }
    verifies_clean(&world, "SW1");
    // M13.8: SW2's coarse rung used to sink 0.39 mm into this blade
    // (the rung relaxed its gouge budget with its scallop - the
    // verifier convicted it). The rung now relaxes only its spacing,
    // so the ladder verifies clean. (This world mounts no tilting
    // machine, so the post refuses both programs on kinematics alone
    // - which is why the post is not the witness here.)
    verifies_clean(&world, "SW2");
    let one = cache(&world, "SW1").expect("cached").path;
    let two = cache(&world, "SW2").expect("cached").path;
    assert!(
        two.moves.len() > one.moves.len(),
        "the rough pass added nothing"
    );
    blade_pose_law("advanced swarf", &two);
}

#[test]
fn swarf_family_refuses_a_flat_tool_by_name() {
    // The twins' law, and the refusal says WHICH swarf: an end mill
    // cannot touch along a ruling with its side, and silently
    // pretending otherwise is the gouge.
    for (name, params) in [
        ("SwarfE", swarf_params(11.0, -11.0)),
        ("SwarfA", advanced_swarf_params(11.0, -11.0, 2)),
    ] {
        let world = new_world();
        let mut history = History::default();
        box_stock(&world, &mut history, 60.0, 60.0, 25.0);
        add_tool(&world, &mut history, "EM-0600-4FL");
        history.execute(&world, ImportModel::from_mesh("blade", blade_mesh()));
        common::create_past_the_tool_law(
            &world,
            &mut history,
            name,
            "EM-0600-4FL",
            SrcRef::Entity("blade".into()),
            params,
        );
        let CommandError::Machining(msg) = common::generate_past_the_tool_law(&world, name)
            .expect_err("the swarf kinds must refuse a flat tool")
        else {
            panic!("expected a machining refusal");
        };
        assert!(msg.contains("ball-nose"), "{msg}");
        assert!(msg.contains("Swarf"), "{msg}");
        assert_eq!(
            status(&world, name),
            OpStatus::Never,
            "the refusal leaves no path behind"
        );
    }
}

#[test]
fn swarf_refuses_an_unruled_dome_by_name() {
    // A sphere has no straight direction to read: the analysis refuses
    // before any path is cut and says what the model is not. (The
    // taxonomy's other arms - the Z-wall pointing at Contour, the
    // empty model - are ruling.rs's, not double-spent here.)
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 25.0);
    add_tool(&world, &mut history, "BN-0600");
    history.execute(&world, ImportModel::from_mesh("dome", dome_mesh()));
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "SWD",
            "BN-0600",
            SrcRef::Entity("dome".into()),
            swarf_params(18.0, DOME_RIM_Z),
        )
        .expect("create"),
    );
    let CommandError::Machining(msg) = GenerateOperation::new(&world, "SWD")
        .err()
        .expect("a dome must refuse swarf")
    else {
        panic!("expected a machining refusal");
    };
    assert!(msg.contains("ruling"), "{msg}");
    assert_eq!(status(&world, "SWD"), OpStatus::Never);
}

#[test]
fn advanced_swarf_refuses_passes_nonsense_at_creation() {
    // Validate early: the ladder's rung count is a 1..4 door at the
    // command (and the same door at the file loader's sane check, in
    // tests/project_roundtrip.rs). The plain kind rides the twins'
    // doors, so one of those is re-pinned here for the new arm.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "BN-0600");
    history.execute(&world, ImportModel::from_mesh("blade", blade_mesh()));
    let mk = |op: OpParams, name: &str| {
        CreateOperation::new(&world, name, "BN-0600", SrcRef::Entity("blade".into()), op)
    };
    for n in [0u32, 5] {
        assert!(
            matches!(
                mk(advanced_swarf_params(11.0, -11.0, n), "SwBad"),
                Err(CommandError::Invalid(_))
            ),
            "passes {n} reached a generator's door"
        );
    }
    mk(advanced_swarf_params(11.0, -11.0, 1), "SwOne").expect("1 is the plain ladder alone");
    mk(advanced_swarf_params(11.0, -11.0, 4), "SwFour").expect("4 is the ladder's reach");
    let mut zero_tol = swarf_params(11.0, -11.0);
    if let OpParams::Swarf { tolerance, .. } = &mut zero_tol {
        *tolerance = 0.0;
    }
    assert!(matches!(
        mk(zero_tol, "SwTol"),
        Err(CommandError::Invalid(_))
    ));
}

// ---------------------------------------------------------------------------
// M5.1 stage E: the rotary twins through the command chain. The laws
// under test live in `multiaxis` - the circumcenter-concurrency axis
// fit, the station height map, the exact identity with the plain
// waterline and swarf rungs - so at this level the checks are the trip
// and its signature: an Ok verdict, the RADIAL pose (the line from the
// fitted axis to the tool, the panel's lead/lag resting at zero here,
// and the cone measured off that seed so nothing folds) on every cut
// move, nothing stated in air, and the refusals - each kind naming the
// stance the OTHER one wants, the flat tool, the model with no axis,
// the nonsense pose - spoken at the right door.
// ---------------------------------------------------------------------------

/// Revolve a meridian profile (rho, z pairs) about world z: the fan
/// at a rho-0 endpoint keeps the solid CLOSED - the footprint law
/// dangles an open shell's chords and refuses, exactly as blade_mesh's
/// caps exist for (and the same reason motion's `revolve` does).
fn revolve(n_circ: usize, meridian: &[(f64, f64)]) -> Vec<camcraft_app::geometry::Triangle> {
    use camcraft_app::geometry::{Point3, Triangle};
    const TAU: f64 = std::f64::consts::TAU;
    let pt = |rho: f64, z: f64, j: usize| {
        let th = TAU * j as f64 / n_circ as f64;
        Point3::new(rho * th.cos(), rho * th.sin(), z)
    };
    let mut facets = Vec::new();
    for w in meridian.windows(2) {
        let (ra, za) = w[0];
        let (rb, zb) = w[1];
        for j in 0..n_circ {
            let k = (j + 1) % n_circ;
            let (a1, a2) = (pt(ra, za, j), pt(ra, za, k));
            let (b1, b2) = (pt(rb, zb, j), pt(rb, zb, k));
            if ra <= 1e-12 {
                facets.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                facets.push(Triangle::new(a1, a2, b2));
            } else {
                // The seam is a DIAGONAL: a wall patch's four corners
                // are not coplanar, and joining the two triangles
                // along a side edge (the obvious-looking a2-b2)
                // leaves the quad split by its two diagonals instead -
                // a bowtie with slits the footprint law rightly calls
                // an open mesh.
                facets.push(Triangle::new(a1, a2, b2));
                facets.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    facets
}

/// The turned bar on its axis: wall r6, a gentle 1-in-4 shoulder,
/// neck r5, capped both ends, z -16..16. motion's lathe grown to the
/// app's 6 mm ball (the neck must beat the ball radius or the scallop
/// law has no wall to speak on) and centered so the box stock holds
/// it. 16 turns clear the fit's coarseness gate - an honest lathe,
/// not a pretending polygon.
fn lathe_mesh() -> camcraft_app::geometry::Mesh {
    camcraft_app::geometry::Mesh::from_facets(
        revolve(
            16,
            &[
                (0.0, -16.0),
                (6.0, -16.0),
                (6.0, -2.0),
                (5.0, 2.0),
                (5.0, 16.0),
                (0.0, 16.0),
            ],
        ),
    )
}

/// The same bar laid down: its axis is world x, its crown world z.
fn laid_mesh() -> camcraft_app::geometry::Mesh {
    use camcraft_app::geometry::{Point3, Triangle};
    let s = |p: Point3| Point3::new(p.z, p.x, p.y);
    camcraft_app::geometry::Mesh::from_facets(
        lathe_mesh()
            .facets
            .iter()
            .map(|t| Triangle::new(s(t.a), s(t.b), s(t.c)))
            .collect(),
    )
}

/// Not a revolve anywhere: a 14x10x20 box. Its normals' second moment
/// is nowhere near isotropic (so it is no dome), no candidate axis
/// reaches the 90 % share, and the fit answers with the taxonomy's
/// generic line - which is exactly the honest door a freeform should
/// find before any path is planned.
fn freeform_box() -> camcraft_app::geometry::Mesh {
    use camcraft_app::geometry::{Point3, Triangle};
    let p = |i: usize, j: usize, k: usize| {
        Point3::new(
            if i == 0 { -7.0 } else { 7.0 },
            if j == 0 { -5.0 } else { 5.0 },
            if k == 0 { -10.0 } else { 10.0 },
        )
    };
    let facets = vec![
        Triangle::new(p(0, 0, 0), p(1, 0, 0), p(1, 1, 0)),
        Triangle::new(p(0, 0, 0), p(1, 1, 0), p(0, 1, 0)),
        Triangle::new(p(0, 0, 1), p(1, 1, 1), p(1, 0, 1)),
        Triangle::new(p(0, 0, 1), p(0, 1, 1), p(1, 1, 1)),
        Triangle::new(p(0, 0, 0), p(0, 0, 1), p(1, 0, 1)),
        Triangle::new(p(0, 0, 0), p(1, 0, 1), p(1, 0, 0)),
        Triangle::new(p(0, 1, 0), p(1, 1, 1), p(0, 1, 1)),
        Triangle::new(p(0, 1, 0), p(1, 1, 0), p(1, 1, 1)),
        Triangle::new(p(0, 0, 0), p(0, 1, 1), p(0, 1, 0)),
        Triangle::new(p(0, 0, 0), p(0, 0, 1), p(0, 1, 1)),
        Triangle::new(p(1, 0, 0), p(1, 1, 0), p(1, 1, 1)),
        Triangle::new(p(1, 0, 0), p(1, 1, 1), p(1, 0, 1)),
    ];
    camcraft_app::geometry::Mesh::from_facets(facets)
}

fn rotary_contour_params(top_z: f64, bottom_z: f64) -> OpParams {
    OpParams::RotaryContour {
        tolerance: 0.1,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(top_z),
        bottom_z: Height::fixed(bottom_z),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
    }
}

fn rotary_parallel_params(top_z: f64, bottom_z: f64) -> OpParams {
    OpParams::RotaryParallel {
        tolerance: 0.1,
        stepover: Some(2.0),
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(top_z),
        bottom_z: Height::fixed(bottom_z),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
    }
}

fn rotary_offset_params(top_z: f64, bottom_z: f64) -> OpParams {
    // The wrap ladder's numbers (M5.1 stage E2): the spacing is the
    // developed arc, plain like the pocket's - no scallop cos-law to
    // fall back on - and the infeed is RADIAL. `clearance_z` is
    // honest window dressing here: the kernel names its own rapids
    // plane one infeed and one tool radius outside the stock's skin
    // (M13.8; the part's when the stock is the part itself), because
    // a world-Z hop would be a plunge into the bar.
    OpParams::RotaryOffset {
        links: camcraft_app::links::Links::ROUGH3D,
        tolerance: 0.1,
        stepover: 4.0,
        stock_to_leave: 0.5,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(top_z),
        bottom_z: Height::fixed(bottom_z),
        stepdown: 4.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: camcraft_app::machining2d::PocketStrategy::Auto,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
    }
}

/// The rotary twins' signature at this level: every move that cuts
/// poses the RADIAL itself - the perpendicular from the fitted axis
/// meeting the tool point - because the lean rests at zero and the
/// cone folds only what the seed already answers. `axial` names the
/// axis' world component (z for the standing bar, x for the laid
/// one); the fitted origin sits on the axis to well within 1e-6 of
/// the origin, so the perpendicular is spelled off the raw point.
/// Rapids state nothing; on the axis (rho under half a mm) the radial
/// IS the travel and holds no claim - a turned bar's rings do not go
/// there anyway.
fn radial_pose_law(name: &str, path: &camcraft_app::cl::Toolpath, axial: usize) {
    let mut body = 0usize;
    for m in &path.moves {
        let rapid = m.kind == MoveKind::Rapid;
        let Some(a) = m.axis else { continue };
        assert!(
            a.iter().all(|v| v.is_finite()),
            "{name}: a non-finite pose on a move"
        );
        let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
        assert!((l - 1.0).abs() < 1e-6, "{name}: a pose of length {l}");
        // M13 pivot law: the stream locates the TIP; the radial is the
        // ball CENTER's, `r` up the pose (the BN-0600's 3 mm). Rapids
        // state it too - the post keeps the tilt modal, so a rapid
        // stating nothing would hold the last cut's direction while it
        // laps the bar.
        let mut d = [m.to.x + 3.0 * a[0], m.to.y + 3.0 * a[1], m.to.z + 3.0 * a[2]];
        d[axial] = 0.0;
        let dn = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if dn < 0.5 {
            continue;
        }
        let want = [d[0] / dn, d[1] / dn, d[2] / dn];
        let dot = a[0] * want[0] + a[1] * want[1] + a[2] * want[2];
        assert!(
            dot > 1.0 - 1e-7,
            "{name}: a pose off the radial at {:?}: {a:?} vs {want:?}",
            m.to
        );
        if !rapid {
            body += 1;
        }
    }
    assert!(
        body >= 8,
        "{name}: only {body} radial-posed cut moves over a whole turned bar"
    );
}

#[test]
fn rotary_contour_generates_radial_axes_and_verifies_clean() {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "BN-0600");
    history.execute(&world, ImportModel::from_mesh("lathe", lathe_mesh()));
    model_stock(&world, &mut history, "lathe");
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "RC1",
            "BN-0600",
            SrcRef::Entity("lathe".into()),
            rotary_contour_params(14.0, -14.0),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "RC1").expect("generate"),
    );
    assert_eq!(
        status(&world, "RC1"),
        OpStatus::Ok,
        "the envelope verdict came back inside the gate"
    );
    verifies_clean(&world, "RC1");
    let t = cache(&world, "RC1").expect("cached").path;
    radial_pose_law("rotary contour", &t, 2);
    // M13.1: the column VERDICT refuses a rotary program by name (its
    // height field is blind under the equator; the field EYE chords the
    // turn, the radial verdict at generation stands) - never judged as
    // a chord, never silently clean.
    let err = camcraft_app::stock_sim::simulate_document(&world).expect_err("a rotary program is not judged by the column verdict");
    assert!(err.contains("RC1") && err.contains("rotary program"), "{err}");
    // The bottom ring's BALL snaps to the bottom (M13 pivot law: the
    // tip of a lying ball stands at its center's height, so the plane
    // the ladder named is met by the ball's lowest point, center - r).
    let plunges: Vec<f64> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Plunge)
        .map(|m| m.to.z + 3.0 * m.axis.map_or(1.0, |a| a[2]) - 3.0)
        .collect();
    assert!(
        plunges.last().is_some_and(|z| (z + 14.0).abs() < 1e-9),
        "the bottom ring snaps to the bottom: {plunges:?}"
    );
}

#[test]
fn rotary_parallel_generates_radial_axes_and_verifies_clean() {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "BN-0600");
    history.execute(&world, ImportModel::from_mesh("laid", laid_mesh()));
    model_stock(&world, &mut history, "laid");
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "RP1",
            "BN-0600",
            SrcRef::Entity("laid".into()),
            rotary_parallel_params(4.0, -4.0),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "RP1").expect("generate"),
    );
    assert_eq!(status(&world, "RP1"), OpStatus::Ok);
    verifies_clean(&world, "RP1");
    let t = cache(&world, "RP1").expect("cached").path;
    radial_pose_law("rotary parallel", &t, 0);
    // The raster's heights are world z in the laid frame (an exact
    // reading for a flat-lying bar), read off the BALL a vertical tip
    // there would carry - its center r up the pose from the posted
    // tip, its lowest point r under that (M13 pivot law): the ladder
    // never sinks below its Bottom, and its last line IS the Bottom.
    // The links are not the raster (M13.8): a pass on the flank under
    // the equator retracts along ITS radius onto the 25 mm clearance
    // cylinder, below the Bottom plane and in air - those retract
    // ends are judged by the verdict above, not by the Bottom law.
    let low = |m: &camcraft_app::cl::Move| m.to.z + 3.0 * m.axis.map_or(1.0, |a| a[2]) - 3.0;
    let on_clearance = |m: &&camcraft_app::cl::Move| (m.to.y * m.to.y + m.to.z * m.to.z).sqrt() >= 25.0 - 1e-6;
    let lowest = t
        .moves
        .iter()
        .filter(|m| matches!(m.kind, MoveKind::Feed | MoveKind::Plunge))
        .filter(|m| !on_clearance(m))
        .map(low)
        .fold(f64::INFINITY, f64::min);
    assert!(
        lowest >= -4.0 - 1e-9,
        "the raster sank below its Bottom plane: {lowest}"
    );
    let on_bottom = t
        .moves
        .iter()
        .any(|m| m.kind == MoveKind::Feed && (low(m) + 4.0).abs() < 1e-9);
    assert!(
        on_bottom,
        "nothing reached the Bottom plane - the flank went uncut"
    );
}

#[test]
fn the_rotary_twins_refuse_each_other_stances_by_name() {
    // The pair cannot disagree about where the boundary sits: one
    // sine for both refusals, each naming the kind the stance wants.
    for (mname, name, mesh, params, wants) in [
        (
            "lathe",
            "RPx",
            lathe_mesh(),
            rotary_parallel_params(4.0, -4.0),
            "Rotary Contour",
        ),
        (
            "laid",
            "RCx",
            laid_mesh(),
            rotary_contour_params(14.0, -14.0),
            "Rotary Parallel",
        ),
    ] {
        let world = new_world();
        let mut history = History::default();
        box_stock(&world, &mut history, 40.0, 40.0, 40.0);
        add_tool(&world, &mut history, "BN-0600");
        history.execute(&world, ImportModel::from_mesh(mname, mesh));
        history.execute(
            &world,
            CreateOperation::new(
                &world,
                name,
                "BN-0600",
                SrcRef::Entity(mname.into()),
                params,
            )
            .expect("create"),
        );
        let CommandError::Machining(msg) = GenerateOperation::new(&world, name)
            .err()
            .expect("a wrong-stance twin must refuse")
        else {
            panic!("expected a machining refusal");
        };
        assert!(msg.contains(wants), "{msg} should point at {wants}");
        assert_eq!(
            status(&world, name),
            OpStatus::Never,
            "the refusal leaves no path behind"
        );
    }
}

#[test]
fn rotary_twins_refuse_a_flat_bottomed_tool_by_name() {
    // The same law as every ball-nose kind's, and the refusal says
    // WHICH twin: a flat corner cannot stand square to a wall along
    // its whole side, and pretending otherwise is the gouge.
    for (name, params, label) in [
        ("RCe", rotary_contour_params(14.0, -14.0), "Rotary Contour"),
        ("RPe", rotary_parallel_params(4.0, -4.0), "Rotary Parallel"),
    ] {
        let world = new_world();
        let mut history = History::default();
        box_stock(&world, &mut history, 40.0, 40.0, 40.0);
        add_tool(&world, &mut history, "EM-0600-4FL");
        history.execute(&world, ImportModel::from_mesh("lathe", lathe_mesh()));
        common::create_past_the_tool_law(
            &world,
            &mut history,
            name,
            "EM-0600-4FL",
            SrcRef::Entity("lathe".into()),
            params,
        );
        let CommandError::Machining(msg) = common::generate_past_the_tool_law(&world, name)
            .expect_err("the rotary twins must refuse a flat tool")
        else {
            panic!("expected a machining refusal");
        };
        assert!(msg.contains("ball-nose"), "{msg}");
        assert!(msg.contains(label), "{msg} should name {label}");
        assert_eq!(
            status(&world, name),
            OpStatus::Never,
            "the refusal leaves no path behind"
        );
    }
}

#[test]
fn rotary_refuses_a_freeform_by_name() {
    // No meridian anywhere: the fit answers before any path is planned.
    // (The taxonomy's other arms - the isotropic dome, the empty model,
    // the pretending polygon - are axis.rs's and the motion-side rotary
    // tests', not double-spent here.)
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 40.0, 40.0, 40.0);
    add_tool(&world, &mut history, "BN-0600");
    history.execute(&world, ImportModel::from_mesh("boxy", freeform_box()));
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "RCb",
            "BN-0600",
            SrcRef::Entity("boxy".into()),
            rotary_contour_params(8.0, -8.0),
        )
        .expect("create"),
    );
    let CommandError::Machining(msg) = GenerateOperation::new(&world, "RCb")
        .err()
        .expect("a box must refuse rotary")
    else {
        panic!("expected a machining refusal");
    };
    assert!(msg.contains("share no rotation axis"), "{msg}");
    assert_eq!(status(&world, "RCb"), OpStatus::Never);
}

#[test]
fn rotary_twins_refuse_nonsense_pose_at_creation() {
    // Validate early at the command door: the twins ride the 5-axis
    // kinds' doors - a zero deviation budget, a 91-degree lean, and a
    // cone the spindle cannot enter never reach a generator.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "BN-0600");
    history.execute(&world, ImportModel::from_mesh("lathe", lathe_mesh()));
    let mk = |op: OpParams, name: &str| {
        CreateOperation::new(&world, name, "BN-0600", SrcRef::Entity("lathe".into()), op)
    };
    let mut zero_tol = rotary_contour_params(14.0, -14.0);
    if let OpParams::RotaryContour { tolerance, .. } = &mut zero_tol {
        *tolerance = 0.0;
    }
    assert!(matches!(
        mk(zero_tol, "RcTol"),
        Err(CommandError::Invalid(_))
    ));
    let mut flat_lean = rotary_parallel_params(4.0, -4.0);
    if let OpParams::RotaryParallel { lead_deg, .. } = &mut flat_lean {
        *lead_deg = 91.0;
    }
    assert!(matches!(
        mk(flat_lean, "RpLd"),
        Err(CommandError::Invalid(_))
    ));
    let mut no_cone = rotary_contour_params(14.0, -14.0);
    if let OpParams::RotaryContour { max_tilt_deg, .. } = &mut no_cone {
        *max_tilt_deg = 0.0;
    }
    assert!(matches!(mk(no_cone, "RcCn"), Err(CommandError::Invalid(_))));
}

#[test]
fn rotary_offset_peels_the_wrap_ladder_and_verifies_clean() {
    // M5.1 stage E2 over the standing bar: the pocket ladder run in
    // wrap space, posed radially like its finish twins (law (f): the
    // swept sphere is pose-invariant about its own axis, so Envelope
    // verify tells the truth for a rough). The stock is deliberately
    // a ROUND bar turned to the part's OD (d12.4 about the lathe's
    // r6): since M13.8 the kernel peels from the STOCK's r6.2 skin
    // inward and its opening rides one infeed and one radius outside
    // that skin. (M13: before that it peeled from the PART's skin, and
    // on the d30 bar this test once used its hops ran through 9 mm of
    // skin the ladder never peeled - the radial field law convicted
    // them, where the old vertical column read them as cleared air.)
    // The infeed is sub-step: the lathe's peel band is the 6.0 skin
    // down to the 5.0 neck, one millimetre of it, so a 4 mm radial
    // stepdown (the helper's) skips the window and the staircase law
    // - reading each column's stock off its own surface, the same one
    // pocket3d runs on block work - honestly leaves the whole step to
    // the finishing kind. 0.5 lands levels at 6.0/5.5/5.0: a real
    // ladder, floor one step under the skin.
    let mut params = rotary_offset_params(14.0, -14.0);
    if let OpParams::RotaryOffset { stepdown, .. } = &mut params {
        *stepdown = 0.5;
    }
    let world = new_world();
    let mut history = History::default();
    let cmd = SetStock::new(
        &world,
        StockShape::Cylinder {
            diameter: 12.4,
            height: 32.0,
        },
        Location {
            pos: Point3::new(0.0, 0.0, -16.0),
            rot: IDENTITY_Q,
        },
    );
    history.execute(&world, cmd.expect("round bar stock"));
    add_tool(&world, &mut history, "BN-0600");
    history.execute(&world, ImportModel::from_mesh("lathe", lathe_mesh()));
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "RO1",
            "BN-0600",
            SrcRef::Entity("lathe".into()),
            params,
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "RO1").expect("generate"),
    );
    assert_eq!(
        status(&world, "RO1"),
        OpStatus::Ok,
        "the wrap ladder verifies inside its budget"
    );
    let c = cache(&world, "RO1").expect("cached");
    assert!(c.warnings.is_empty(), "the wrap ladder on its bar verifies silent: {:?}", c.warnings);
    let t = c.path;
    radial_pose_law("RO1", &t, 2);
    // The ladder really DESCENDED: the skim ring's TIP stands on the
    // 6.0 skin (its ball center one radius out - the M13 pivot law),
    // the 5.5 level peels the neck column's band one infeed under it,
    // and the floor ring - one step above the 5.0 neck surface - rides
    // at tip rho 5.0. One plane's spread is not a rough.
    let (lo, hi) = t.moves.iter().filter(|m| m.kind != MoveKind::Rapid).fold(
        (f64::MAX, 0.0f64),
        |(lo, hi), m| {
            let rho = (m.to.x * m.to.x + m.to.y * m.to.y).sqrt();
            (lo.min(rho), hi.max(rho))
        },
    );
    assert!(hi > 5.9, "no ring at the skin (max rho {hi})");
    assert!(lo < 5.6, "the ladder never left the skin: {lo}..{hi}");
    assert!(
        hi - lo > 0.5,
        "one plane of cut is not a ladder: {lo}..{hi}"
    );
}

#[test]
fn rotary_offset_refuses_a_flat_tool_by_name() {
    // The wrap pocket's floor is turned by the ball's tangency, the
    // way the twins' skins are: a flat end cannot ride a cylinder's
    // radial, so the kernel is never asked (and the refusal says so,
    // by kind and by name, at generate).
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 40.0, 40.0, 40.0);
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("lathe", lathe_mesh()));
    common::create_past_the_tool_law(
        &world,
        &mut history,
        "ROf",
        "EM-0600-4FL",
        SrcRef::Entity("lathe".into()),
        rotary_offset_params(14.0, -14.0),
    );
    let CommandError::Machining(msg) = common::generate_past_the_tool_law(&world, "ROf")
        .expect_err("a flat tool must refuse the wrap")
    else {
        panic!("expected a machining refusal");
    };
    assert!(msg.contains("ball-nose"), "{msg}");
    assert!(msg.contains("Rotary Offset"), "{msg}");
    assert_eq!(
        status(&world, "ROf"),
        OpStatus::Never,
        "the refusal leaves no path behind"
    );
}

#[test]
fn rotary_offset_refuses_a_freeform_by_name() {
    // The same fit, the same answer: no meridian, no wrap, refused
    // before a single ring is planned (axis.rs's taxonomy, not this
    // kind's own work).
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 40.0, 40.0, 40.0);
    add_tool(&world, &mut history, "BN-0600");
    history.execute(&world, ImportModel::from_mesh("boxy", freeform_box()));
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "ROb",
            "BN-0600",
            SrcRef::Entity("boxy".into()),
            rotary_offset_params(8.0, -8.0),
        )
        .expect("create"),
    );
    let CommandError::Machining(msg) = GenerateOperation::new(&world, "ROb")
        .err()
        .expect("a box must refuse the wrap")
    else {
        panic!("expected a machining refusal");
    };
    assert!(msg.contains("share no rotation axis"), "{msg}");
    assert_eq!(status(&world, "ROb"), OpStatus::Never);
}

#[test]
fn rotary_offset_refuses_nonsense_at_creation() {
    // Validate early at the command door: the ladder's own numbers -
    // a zero developed spacing, a zero radial infeed, a 91-degree
    // lean, a closed cone - never reach the kernel. Unlike the
    // twins, the spacing here is a plain number, so there is no
    // Some-or-derived to hide a zero in.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "BN-0600");
    history.execute(&world, ImportModel::from_mesh("lathe", lathe_mesh()));
    let mk = |op: OpParams, name: &str| {
        CreateOperation::new(&world, name, "BN-0600", SrcRef::Entity("lathe".into()), op)
    };
    let mut zero_step = rotary_offset_params(14.0, -14.0);
    if let OpParams::RotaryOffset { stepover, .. } = &mut zero_step {
        *stepover = 0.0;
    }
    assert!(matches!(
        mk(zero_step, "RoSo"),
        Err(CommandError::Invalid(_))
    ));
    let mut zero_infeed = rotary_offset_params(14.0, -14.0);
    if let OpParams::RotaryOffset { stepdown, .. } = &mut zero_infeed {
        *stepdown = 0.0;
    }
    assert!(matches!(
        mk(zero_infeed, "RoSd"),
        Err(CommandError::Invalid(_))
    ));
    let mut flat_lean = rotary_offset_params(14.0, -14.0);
    if let OpParams::RotaryOffset { lead_deg, .. } = &mut flat_lean {
        *lead_deg = 91.0;
    }
    assert!(matches!(
        mk(flat_lean, "RoLd"),
        Err(CommandError::Invalid(_))
    ));
    let mut no_cone = rotary_offset_params(14.0, -14.0);
    if let OpParams::RotaryOffset { max_tilt_deg, .. } = &mut no_cone {
        *max_tilt_deg = 0.0;
    }
    assert!(matches!(mk(no_cone, "RoCn"), Err(CommandError::Invalid(_))));
}

// ---------------------------------------------------------------------------
// M5.1 stage E3: multi-axis roughing - the pocket ladder with the spindle
// posed. The signature at this level: positions bit-identical to the
// vertical path, move for move (the stamp only ever writes axes), the same
// Ok verdict (law (f): the Envelope sweeps the ball as its sphere, and a
// sphere cannot see a pose), and the pose law itself - rapids and in-plane
// rows state nothing, wall contacts state the wall's own normal when lead
// and lag rest at zero. None renders as vertical at the post, which is the
// honest answer for floor contact. The refusals: the flat tool, named, and
// the nonsense numbers at the command door.
// ---------------------------------------------------------------------------

/// The stage's fixture: a 40x40 slab to z=-10 with a 10x10 boss standing
/// on it to z=0 (sunk 1 mm into the slab, so no facet pair is coincident
/// and every level's sections are clean). motion's multiaxis ladder
/// proved its laws on this shape; the app meets it again here.
fn slab_boss_mesh() -> Mesh {
    let mut facets = cube(
        Point3::new(-20.0, -20.0, -20.0),
        Point3::new(20.0, 20.0, -10.0),
    )
    .facets
    .to_vec();
    facets.extend(cube(Point3::new(-5.0, -5.0, -11.0), Point3::new(5.0, 5.0, 0.0)).facets.iter().cloned());
    Mesh::from_facets(facets)
}

fn multiaxis_rough_params() -> OpParams {
    OpParams::MultiAxisRough {
        links: camcraft_app::links::Links::ROUGH3D,
        tolerance: 0.1,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(5.0),
        retract_z: None,
        top_z: Height::fixed(0.0),
        bottom_z: Height::fixed(-10.0),
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: camcraft_app::machining2d::PocketStrategy::Zigzag,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
        shank_avoid: false,
    }
}

fn multiaxis_rough_pocket_twin() -> OpParams {
    OpParams::Pocket3D {
        links: camcraft_app::links::Links::ROUGH3D,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(5.0),
        retract_z: None,
        top_z: Height::fixed(0.0),
        bottom_z: Height::fixed(-10.0),
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: camcraft_app::machining2d::PocketStrategy::Zigzag,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        rest_rough: false,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        feed_opt: false,
        shell: false,
    }
}

/// The job both kinds run: box stock, ball nose, the slab with a boss,
/// and ONE operation, generated. Alone on purpose - program order is
/// name order and both ladder kinds consume the rest-material mask
/// (M3.1 D), so a shared world would make the second op re-machine the
/// first (or read Stale against its sibling's path). This law is about
/// the stock, not a sequence: each kind must see the whole stock.
fn ladder_job(tag: &str, stamped: bool) -> (World, History, camcraft_app::cl::Toolpath) {
    let world = new_world();
    let mut history = History::default();
    let cmd = SetStock::new(
        &world,
        StockShape::Box {
            dx: 40.0,
            dy: 40.0,
            dz: 20.0,
        },
        Location {
            pos: Point3::new(0.0, 0.0, -20.0),
            rot: IDENTITY_Q,
        },
    );
    history.execute(&world, cmd.expect("box stock"));
    add_tool(&world, &mut history, "BN-0600");
    history.execute(&world, ImportModel::from_mesh("slabboss", slab_boss_mesh()));
    let params = if stamped {
        multiaxis_rough_params()
    } else {
        multiaxis_rough_pocket_twin()
    };
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            tag,
            "BN-0600",
            SrcRef::Entity("slabboss".into()),
            params,
        )
        .expect("create"),
    );
    let cmd = GenerateOperation::new(&world, tag).expect("generate");
    history.execute(&world, cmd);
    assert_eq!(
        status(&world, tag),
        OpStatus::Ok,
        "{tag} reads Stale against its own lone world"
    );
    let path = cache(&world, tag).expect("cached ladder").path;
    (world, history, path)
}

#[test]
fn multiaxis_rough_peels_the_same_ladder_and_poses_its_walls() {
    // Two worlds, the same job: the pocket's path and the stamped one
    // are the same ladder seen twice - and each verifies Ok inside its
    // own world, the budget the vertical kind always met.
    let (_pw, _ph, plain) = ladder_job("P3R", false);
    let (_rw, _rh, rough) = ladder_job("MAR1", true);
    assert_eq!(
        plain.moves.len(),
        rough.moves.len(),
        "one ladder, two readings"
    );
    let mut posed = 0usize;
    let mut floor_air = 0usize;
    // The pivot law: the BALLS are the plain ladder's (the tips follow
    // the lean), and rapids state no new pose.
    same_balls(&plain, &rough, 3.0);
    rapids_inherit(&rough, 3.0);
    for (a, b) in plain.moves.iter().zip(&rough.moves) {
        assert!(a.axis.is_none(), "the plain ladder speaks an angle");
        if b.kind == MoveKind::Rapid {
            continue;
        }
        match b.axis.filter(|v| leans(Some(*v))) {
            None => {
                if a.to.z == -10.0 {
                    floor_air += 1;
                }
            }
            Some(v) => {
                posed += 1;
                let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                assert!((l - 1.0).abs() < 1e-6, "a pose of length {l}");
                assert!(v.iter().all(|c| c.is_finite()));
                assert!(
                    v[2].abs() < 1e-6,
                    "a zero-lead pose leans off horizontal: {v:?} at {:?}",
                    b.to
                );
            }
        }
    }
    assert!(posed >= 4, "the boss walls posed nothing: {posed}");
    assert!(posed <= 20, "poses leaked past the walls: {posed}");
    assert!(
        floor_air >= 1,
        "every floor-row move claimed a wall - the floors must hold their peace"
    );
}

#[test]
fn multiaxis_rough_refuses_a_flat_tool_by_name() {
    // The stamp's tangency law is a sphere's law: a flat end has no
    // centre to place off the wall, so the kernel is never asked - the
    // refusal names the kind and the tool at generate.
    let world = new_world();
    let mut history = History::default();
    let cmd = SetStock::new(
        &world,
        StockShape::Box {
            dx: 40.0,
            dy: 40.0,
            dz: 20.0,
        },
        Location {
            pos: Point3::new(0.0, 0.0, -20.0),
            rot: IDENTITY_Q,
        },
    );
    history.execute(&world, cmd.expect("box stock"));
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(&world, ImportModel::from_mesh("slabboss", slab_boss_mesh()));
    common::create_past_the_tool_law(
        &world,
        &mut history,
        "MAF",
        "EM-0600-4FL",
        SrcRef::Entity("slabboss".into()),
        multiaxis_rough_params(),
    );
    let CommandError::Machining(msg) = common::generate_past_the_tool_law(&world, "MAF")
        .expect_err("a flat tool must refuse the stamp")
    else {
        panic!("expected a machining refusal");
    };
    assert!(msg.contains("ball-nose"), "{msg}");
    assert!(msg.contains("Multi-Axis Rough"), "{msg}");
    assert_eq!(
        status(&world, "MAF"),
        OpStatus::Never,
        "the refusal leaves no path behind"
    );
}

#[test]
fn multiaxis_rough_refuses_nonsense_at_creation() {
    // The pocket ladder's doors carry over verbatim (the kernel runs
    // the same bridge), and the cone joins them: a closed cone would
    // fold every pose onto a single direction and hide it.
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "BN-0600");
    history.execute(&world, ImportModel::from_mesh("slabboss", slab_boss_mesh()));
    let mk = |op: OpParams, name: &str| {
        CreateOperation::new(
            &world,
            name,
            "BN-0600",
            SrcRef::Entity("slabboss".into()),
            op,
        )
    };
    let mut zero_step = multiaxis_rough_params();
    if let OpParams::MultiAxisRough { stepover, .. } = &mut zero_step {
        *stepover = 0.0;
    }
    assert!(matches!(
        mk(zero_step, "MaSo"),
        Err(CommandError::Invalid(_))
    ));
    let mut zero_infeed = multiaxis_rough_params();
    if let OpParams::MultiAxisRough { stepdown, .. } = &mut zero_infeed {
        *stepdown = 0.0;
    }
    assert!(matches!(
        mk(zero_infeed, "MaSd"),
        Err(CommandError::Invalid(_))
    ));
    let mut flat_lean = multiaxis_rough_params();
    if let OpParams::MultiAxisRough { lead_deg, .. } = &mut flat_lean {
        *lead_deg = 91.0;
    }
    assert!(matches!(
        mk(flat_lean, "MaLd"),
        Err(CommandError::Invalid(_))
    ));
    let mut no_cone = multiaxis_rough_params();
    if let OpParams::MultiAxisRough { max_tilt_deg, .. } = &mut no_cone {
        *max_tilt_deg = 0.0;
    }
    assert!(matches!(mk(no_cone, "MaCn"), Err(CommandError::Invalid(_))));
}

// ---------------------------------------------------------------------------
// M5.1 stage F: collision-aware orientation through the command chain. The
// dodge kernel proved its laws in motion (multiaxis_avoid); what rides at
// this level is the seam: the flag reaches the stamp through params, the
// document's tool stack (not the verifier's guess) is what the kernel
// measures, a flipped flag ages the path through the digest, and the
// refusal - a wall no cone clears - names itself at generate instead of
// posting a known-rubbing program. Positions never move: law (f) again.
// ---------------------------------------------------------------------------

/// A stub that MEANS business beside the boss: an r=4 ball on a 6-long
/// flute, a 10-wide shank, a 16-wide holder. Its vertical floor rows
/// stand 4 off a wall with a 5-radius band - a real graze the 90-degree
/// cone can clear (the rim stays off the pocket floor around 21 deg),
/// and its holder is narrower than the catalog's 44 so the ladder's
/// walls are the only voice the shank gets.
/// Every move's BALL CENTRE: the tip lifted `r` along the pose the
/// machine holds there (the last stated - the post keeps it modal;
/// upright before any). The pivot law's invariant reads on these.
fn balls_of(path: &camcraft_app::cl::Toolpath, r: f64) -> Vec<camcraft_app::geometry::Point3> {
    let mut held = [0.0, 0.0, 1.0];
    path.moves
        .iter()
        .map(|m| {
            if let Some(a) = m.axis {
                held = a;
            }
            camcraft_app::geometry::Point3::new(m.to.x + r * held[0], m.to.y + r * held[1], m.to.z + r * held[2])
        })
        .collect()
}

fn leans(a: Option<[f64; 3]>) -> bool {
    a.is_some_and(|a| !camcraft_app::cl::is_vertical(&a))
}

/// The rapid contract of the pivot law: a rapid states no NEW pose
/// except as a swing about its ball centre of radius `r` (see
/// [`rapid_pose_contract`]).
fn rapids_inherit(path: &camcraft_app::cl::Toolpath, r: f64) {
    let mut held: Option<[f64; 3]> = None;
    for m in &path.moves {
        rapid_pose_contract("rapids", m, held, r);
        held = m.axis.or(held);
    }
}

fn same_balls(a: &camcraft_app::cl::Toolpath, b: &camcraft_app::cl::Toolpath, r: f64) {
    for (i, (x, y)) in balls_of(a, r).iter().zip(balls_of(b, r)).enumerate() {
        let d = *x - y;
        assert!(d.dot(d).sqrt() < 1e-9, "the ball moved at {i}: {x:?} vs {y:?}");
    }
}

fn stub_tool() -> Tool {
    Tool {
        kind: camcraft_app::tools::ToolKind::BallNose,
        diameter: 8.0,
        corner_r: 4.0,
        tip_angle: 0.0,
        pitch: 0.0,
        nose_radius: 0.0,
        width: 0.0,
        flute_len: 6.0,
        shank_d: 10.0,
        shank_len: 20.0,
        holder_d: 16.0,
        holder_len: 10.0,
        material_group: "aluminum".into(), center_cutting: true,
        rpm: 4000.0,
        feed: 800.0,
        tool_material: None,
        flutes: None,
        orientation: None,
        turning: None,
    }
}

/// The F job: box stock, the stub, the slab with a boss, one Multi-Axis
/// Rough whose only difference is the flag. Lone worlds, the ladder law
/// from stage E3 (the mask must not see a sibling).
fn avoid_job(tag: &str, dodging: bool) -> camcraft_app::cl::Toolpath {
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 40.0, 40.0, 20.0);
    history.execute(
        &world,
        AddTool::new(&world, tag, stub_tool()).expect("stub"),
    );
    history.execute(&world, ImportModel::from_mesh("slabboss", slab_boss_mesh()));
    let mut params = multiaxis_rough_params();
    if let OpParams::MultiAxisRough { shank_avoid, .. } = &mut params {
        *shank_avoid = dodging;
    }
    history.execute(
        &world,
        CreateOperation::new(&world, "OP", tag, SrcRef::Entity("slabboss".into()), params)
            .expect("create"),
    );
    let cmd = GenerateOperation::new(&world, "OP").expect("generate");
    history.execute(&world, cmd);
    assert_eq!(
        status(&world, "OP"),
        OpStatus::Ok,
        "{tag} refused a dodgeable boss"
    );
    cache(&world, "OP").expect("cached").path
}

#[test]
fn multiaxis_rough_poses_around_the_shank_and_moves_nothing() {
    let off = avoid_job("MAQ", false);
    let on = avoid_job("MAR", true);
    assert_eq!(off.moves.len(), on.moves.len(), "the dodge added a move");
    let mut escaped = 0usize;
    let mut plus: Vec<[f64; 3]> = Vec::new();
    // The pivot law: F spends the free axis and nothing else - the
    // BALLS are the flag-off ladder's (the tips follow each pose), and
    // rapids state no new pose. Rows are read off the flag-off path,
    // whose floors stand upright (tip = plan).
    same_balls(&off, &on, 4.0);
    rapids_inherit(&on, 4.0);
    for (i, (a, b)) in off.moves.iter().zip(&on.moves).enumerate() {
        if leans(a.axis) && b.axis != a.axis {
            // F re-answered a wall lean: only where the real tip's
            // shank touched (the stub's 5-radius shank under a lean
            // dead on a wall dips under its 4-radius ball), and inside
            // the cone about that same normal.
            let (x, n) = (b.axis.unwrap_or([0.0, 0.0, 1.0]), a.axis.unwrap_or([0.0, 0.0, 1.0]));
            let d = (x[0] * n[0] + x[1] * n[1] + x[2] * n[2]).clamp(-1.0, 1.0).acos().to_degrees();
            assert!(d <= 85.0001, "F's re-answer left the cone at {i}: {x:?} about {n:?}");
        }
        if b.kind == MoveKind::Rapid {
            continue;
        }
        if a.to.z == -10.0 && 8.0 < a.to.x.abs() && a.to.x.abs() < 10.0 {
            let v = b
                .axis
                .filter(|v| leans(Some(*v)))
                .unwrap_or_else(|| panic!("floor row {i} lets the shank rub: {:?}", b.to));
            let tilt = v[2].clamp(-1.0, 1.0).acos().to_degrees();
            assert!(
                tilt > 1e-6 && tilt <= 85.0001,
                "an escape outside the posed cone: {v:?}"
            );
            assert!(
                v[0] * a.to.x + v[1] * a.to.y > 0.0,
                "an escape leaned INTO the boss: {v:?}"
            );
            if a.to.x > 8.4 && a.to.y.abs() <= 4.0 {
                plus.push(v);
            }
            escaped += 1;
        }
        if a.to.z == -10.0 && a.to.x.abs() >= 12.0 {
            assert!(
                !leans(b.axis),
                "a clean floor row got posed anyway at {i}"
            );
        }
    }
    assert!(
        escaped >= 4,
        "the shank never spoke on a boss-adjacent ladder: {escaped}"
    );
    // The calm law, stated as the shortest-travel rule really is: the
    // AMOUNT of lean is geometry's answer (the smallest grid tilt the
    // whole cone agrees is clear, so it is the same for every row of
    // one wall), while the azimuth is free to rotate - the A/C head
    // enters each escape from its held pose by the shortest road.
    assert!(plus.len() >= 2, "no repeats to compare on the +x wall");
    let t0 = plus[0][2].clamp(-1.0, 1.0).acos().to_degrees();
    for v in &plus[1..] {
        let t = v[2].clamp(-1.0, 1.0).acos().to_degrees();
        assert!(
            (t - t0).abs() < 1e-6,
            "the same wall escaped by two tilts: {t} vs {t0}"
        );
    }
}

#[test]
fn flipping_shank_avoid_ages_the_path() {
    // The flag changes the POSES the stamp states, so it rides the
    // params digest: a flipped flag must invalidate the cached path
    // (staleness is derived, never stored) and the regenerate must
    // find the escape.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 40.0, 40.0, 20.0);
    history.execute(
        &world,
        AddTool::new(&world, "STUB", stub_tool()).expect("stub"),
    );
    history.execute(&world, ImportModel::from_mesh("slabboss", slab_boss_mesh()));
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "MAH",
            "STUB",
            SrcRef::Entity("slabboss".into()),
            multiaxis_rough_params(),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "MAH").expect("generate"),
    );
    assert_eq!(status(&world, "MAH"), OpStatus::Ok);
    let plain = cache(&world, "MAH").expect("cached").path;
    assert!(
        plain
            .moves
            .iter()
            .all(|m| m.to.z != -10.0 || m.to.x.abs() <= 8.0 || !leans(m.axis))
    );

    let mut dodging = multiaxis_rough_params();
    if let OpParams::MultiAxisRough { shank_avoid, .. } = &mut dodging {
        *shank_avoid = true;
    }
    history.execute(
        &world,
        EditOperationParams::new(&world, "MAH", dodging).expect("edit"),
    );
    assert_eq!(
        status(&world, "MAH"),
        OpStatus::Stale,
        "the digest missed stage F's flag - a stale posed path survives its flip"
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "MAH").expect("regenerate"),
    );
    assert_eq!(status(&world, "MAH"), OpStatus::Ok);
    let posed = cache(&world, "MAH").expect("cached").path;
    // Read at the BALL (a posed tip follows its lean): a floor row's
    // centre stands the stub's 4 mm radius over the -10 floor.
    let balls = balls_of(&posed, 4.0);
    assert!(
        posed.moves.iter().zip(&balls).any(|(m, c)| m.kind != MoveKind::Rapid
            && (c.z - (-10.0 + 4.0)).abs() < 1e-9
            && (8.0 < c.x.abs() && c.x.abs() < 10.0)
            && leans(m.axis)),
        "the regenerated ladder never dodged"
    );
}

#[test]
fn multiaxis_rough_refuses_a_slot_the_cone_cannot_clear() {
    // The Prime Directive's shape for F: where no pose inside the cone
    // clears, generate refuses BY NAME (no path cached, status Never) -
    // it never posts a program whose shank is already known to rub. The
    // same job with the flag off stays E3's behavior: it generates, and
    // post-time judgment is the verifier's.
    let slot = || {
        let mut facets = cube(
            Point3::new(-20.0, -20.0, -20.0),
            Point3::new(20.0, 20.0, -10.0),
        )
        .facets
        .to_vec();
        facets.extend(
            cube(
                Point3::new(-20.0, -20.0, -11.0),
                Point3::new(-4.5, 20.0, 5.0),
            )
            .facets.iter().cloned(),
        );
        facets.extend(cube(Point3::new(4.5, -20.0, -11.0), Point3::new(20.0, 20.0, 5.0)).facets.iter().cloned());
        Mesh::from_facets(facets)
    };
    let job = |dodging: bool| {
        let world = new_world();
        let mut history = History::default();
        box_stock(&world, &mut history, 40.0, 40.0, 20.0);
        history.execute(
            &world,
            AddTool::new(&world, "STUB", stub_tool()).expect("stub"),
        );
        history.execute(&world, ImportModel::from_mesh("slot", slot()));
        let mut params = multiaxis_rough_params();
        if let OpParams::MultiAxisRough { shank_avoid, .. } = &mut params {
            *shank_avoid = dodging;
        }
        history.execute(
            &world,
            CreateOperation::new(
                &world,
                "SLOT",
                "STUB",
                SrcRef::Entity("slot".into()),
                params,
            )
            .expect("create"),
        );
        (world, history)
    };
    let (world, _history) = job(true);
    let cmd = GenerateOperation::new(&world, "SLOT");
    let err = cmd.err().expect("a 10-wide shank must refuse a 9-wide slot");
    // #1015: the shank refusal hands back the plan before its move as a
    // ghost, so it arrives as `MachiningGhost`, which reads as `Machining`.
    assert!(err.ghost().is_some(), "the shank refusal carries its ghost");
    let (CommandError::Machining(msg)
    | CommandError::MachiningAt { msg, .. }
    | CommandError::MachiningGhost { msg, .. }) = err
    else {
        panic!("expected a machining refusal");
    };
    assert!(msg.contains("shank"), "{msg}");
    assert!(msg.contains("cone"), "{msg}");
    assert_eq!(
        status(&world, "SLOT"),
        OpStatus::Never,
        "a refusal leaves no path behind"
    );
    let (world, mut history) = job(false);
    let cmd = GenerateOperation::new(&world, "SLOT").expect("flag off stays E3");
    history.execute(&world, cmd);
    assert_eq!(status(&world, "SLOT"), OpStatus::Ok);
}

/// The hill's debug-lane band: 18 down to 9 instead of the rim (6).
const HILL_FAST_BOTTOM_Z: f64 = 9.0;

#[test]
fn a_quiet_shank_leaves_the_contour_twin_alone() {
    quiet_shank_case(HILL_FAST_BOTTOM_Z);
}

#[test]
#[ignore = "heavy lane: the shank-flag twins over the hill's whole 18-to-rim band, two generations, ~26 s debug"]
fn a_quiet_shank_leaves_the_contour_twin_alone_full_band() {
    quiet_shank_case(HILL_RIM_Z);
}

fn quiet_shank_case(bottom_z: f64) {
    // Opt-in means EXACTLY opt-in at the twins' seam too: the catalog
    // ball's shank stands a long flute clear of the hill, so posing
    // with the flag on must reproduce the flag-off path move for move,
    // axes included.
    let off = contour5d_params(18.0, bottom_z);
    let mut on = contour5d_params(18.0, bottom_z);
    if let OpParams::Contour5D { shank_avoid, .. } = &mut on {
        *shank_avoid = true;
    }
    let run = |params: OpParams, tag: &str| {
        let world = new_world();
        let mut history = History::default();
        box_stock(&world, &mut history, 60.0, 60.0, 25.0);
        add_tool(&world, &mut history, "BN-0600");
        history.execute(&world, ImportModel::from_mesh("hill", hill_mesh()));
        history.execute(
            &world,
            CreateOperation::new(
                &world,
                tag,
                "BN-0600",
                SrcRef::Entity("hill".into()),
                params,
            )
            .expect("create"),
        );
        let cmd = GenerateOperation::new(&world, tag).expect("generate");
        history.execute(&world, cmd);
        assert_eq!(status(&world, tag), OpStatus::Ok);
        cache(&world, tag).expect("cached").path
    };
    let pa = run(off, "C5Q");
    let pb = run(on, "C5R");
    assert_eq!(pa.moves.len(), pb.moves.len());
    for (i, (a, b)) in pa.moves.iter().zip(&pb.moves).enumerate() {
        assert_eq!(
            a.axis, b.axis,
            "a shank that reaches nothing moved a pose at {i}"
        );
        assert_eq!((a.to.x, a.to.y, a.to.z), (b.to.x, b.to.y, b.to.z));
    }
}

// ---------------------------------------------------------------------------
// M5.1 stage J: the drive-curve contour through the command chain. The
// geometry laws live in `multiaxis::curve` - the rest at a fan of planes,
// the winding law, the axis leaned off the fold - and that file's own
// unit tests measure them. At this level the checks are the TRIP: the
// picked chains of a real model walk the cutter around a real solid and
// lean at the corners, staleness rides the picks through the src
// digest (the one kind whose source IS the path), and every refusal
// says who - the face pick, the chain outside its window, the wrong
// source, the flat tool, the nonsense pose - at the right door, with no
// path left behind.
// ---------------------------------------------------------------------------

/// The box the drive-curve tests ride: a 10x10x5 solid at the origin,
/// so its four top edges are horizontal chains at z = 5, its corner
/// posts are vertical ones spanning the whole height, and a ball-nose
/// riding an edge stands a radius off BOTH faces it touches - the
/// 45-degree fold this kind exists to cut.
fn mac_part_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "BN-0600");
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    (world, history)
}

/// The four top edges, as one selection - the pick a user makes by
/// clicking the rim of the box's top face.
fn mac_top_edges(world: &World) -> SrcRef {
    let e = model::find_by_name(world, "part").expect("part");
    let f = model::features_of(world, &e).expect("features");
    let items: Vec<GeoSel> = feature::chains(&f)
        .into_iter()
        .filter(|c| {
            let pts = feature::chain_points(&f, c);
            pts.len() >= 2 && pts.iter().all(|p| (p[2] - 5.0).abs() < 1e-9)
        })
        .map(|c| GeoSel {
            entity: "part".into(),
            feature: FeatureSel::Chain(c.id),
        })
        .collect();
    assert_eq!(items.len(), 4, "a box has exactly four top edges");
    SrcRef::Selection(items)
}

/// One vertical corner post, as a pick - two points, z 0 to z 5: the
/// chain no window can straddle from both sides.
fn mac_corner_post(world: &World) -> SrcRef {
    let e = model::find_by_name(world, "part").expect("part");
    let f = model::features_of(world, &e).expect("features");
    let post = feature::chains(&f)
        .into_iter()
        .find(|c| {
            let pts = feature::chain_points(&f, c);
            pts.iter()
                .all(|p| (p[0] - pts[0][0]).abs() < 1e-9 && (p[1] - pts[0][1]).abs() < 1e-9)
                && pts.iter().any(|p| p[2] < 1.0)
                && pts.iter().any(|p| p[2] > 4.0)
        })
        .expect("a box has vertical corner posts");
    sel("part", FeatureSel::Chain(post.id))
}

fn multiaxis_contour_params(top_z: f64, bottom_z: f64) -> OpParams {
    OpParams::MultiAxisContour {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(top_z),
        bottom_z: Height::fixed(bottom_z),
        feed: 500.0,
        plunge_feed: 120.0,
        climb: true,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
        shank_avoid: false,
    }
}

#[test]
fn multiaxis_contour_walks_the_picked_edges_and_leans_at_the_fold() {
    // The killer trip: four picked chains, eight stations, every cut
    // point a ball's radius OUTSIDE the solid - the centre of a ball
    // tangent to the top plane (z = 5) and one side plane stands at
    // z = 8 and reach 8 off the footprint centre. And the axes are
    // not vertical: the fold leans them 45 degrees off the Z, which
    // is the whole reason the kind exists.
    let (world, mut history) = mac_part_world();
    let src = mac_top_edges(&world);
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "MAC",
            "BN-0600",
            src,
            multiaxis_contour_params(6.0, 4.0),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "MAC").expect("generate"),
    );
    assert_eq!(
        status(&world, "MAC"),
        OpStatus::Ok,
        "the envelope verdict came back inside the gate"
    );
    verifies_clean(&world, "MAC");
    let t = cache(&world, "MAC").expect("cached").path;
    pose_law("multiaxis_contour", &t);
    // The edges are CONVEX: the ball rides each edge line itself -
    // centre a radius out along the fold's bisector, touching it (the
    // old plane law stood it r off both PLANES, (z, reach) = (8, 8),
    // r·(√2 - 1) of air off the edge it was picked to cut) - and the
    // stream states the TIP, the centre backed off r along the pose
    // (the pivot law): with no lead, the edge point itself.
    let r = 3.0;
    let mut cut = 0usize;
    for m in t
        .moves
        .iter()
        .filter(|m| matches!(m.kind, MoveKind::Feed | MoveKind::Plunge))
    {
        cut += 1;
        let a = m.axis.expect("a station poses");
        let c = camcraft_app::geometry::Point3::new(m.to.x + r * a[0], m.to.y + r * a[1], m.to.z + r * a[2]);
        // Distance from the centre to the 10x10x5 box (a convex solid:
        // the clamp to its extent is the nearest point).
        let q = camcraft_app::geometry::Point3::new(c.x.clamp(0.0, 10.0), c.y.clamp(0.0, 10.0), c.z.clamp(0.0, 5.0));
        let d = c - q;
        let d = d.dot(d).sqrt();
        assert!((d - r).abs() < 1e-6, "a station's ball stands {d} off the box, not r: {c:?}");
        if m.kind == MoveKind::Feed {
            assert!((m.to.z - 5.0).abs() < 1e-6, "a feed tip left the rim's plane: {:?}", m.to);
            let reach = (m.to.x - 5.0).abs().max((m.to.y - 5.0).abs());
            assert!((reach - 5.0).abs() < 1e-6, "a feed tip left the rim: {:?}", m.to);
        }
    }
    assert!(
        cut >= 4,
        "the pass skipped picked stations: {cut} cut moves"
    );
}

#[test]
fn multiaxis_contour_goes_stale_when_the_picked_model_moves() {
    // The one kind whose source IS the path: staleness must ride the
    // picks through the src digest, not the pattern - a model slid
    // four mm takes its chains with it, and a cached path would cut
    // thin air where the edge was.
    let (world, mut history) = mac_part_world();
    let src = mac_top_edges(&world);
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "MS1",
            "BN-0600",
            src.clone(),
            multiaxis_contour_params(6.0, 4.0),
        )
        .expect("create"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "MS1").expect("generate"),
    );
    assert_eq!(status(&world, "MS1"), OpStatus::Ok);
    let part = model::find_by_name(&world, "part").expect("part");
    model::set_transform(&world, &part, at(4.0, 0.0, 0.0));
    assert_eq!(
        status(&world, "MS1"),
        OpStatus::Stale,
        "the chains moved with the model"
    );
    model::set_transform(&world, &part, at(0.0, 0.0, 0.0));
    assert_eq!(
        status(&world, "MS1"),
        OpStatus::Ok,
        "back where it was, the cache is honest again"
    );

    // A different pick is a different path, counts or no counts: the
    // digest reads the chain ids, so swapping one edge for a post
    // moves the stamp the same way swapping a face does for the 2D
    // kinds.
    let sa = operation::current_stamp(
        &world,
        "BN-0600",
        &src,
        &multiaxis_contour_params(6.0, 4.0),
        "MS1",
        None,
    );
    let sb = operation::current_stamp(
        &world,
        "BN-0600",
        &mac_corner_post(&world),
        &multiaxis_contour_params(6.0, 4.0),
        "MS1",
        None,
    );
    assert_ne!(sa, sb, "a different selection is a different path");
}

#[test]
fn multiaxis_contour_refuses_a_face_pick_by_name() {
    // The picks are curves; a face is a boundary. The refusal names
    // the kind's own law and points at the fix - the face's edge
    // chain, or the mesh-driven twin - never the 2D walk's wording.
    let (world, mut history) = mac_part_world();
    let top = FeatureSel::Face(top_face_of(&world, "part"));
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "MF1",
            "BN-0600",
            sel("part", top),
            multiaxis_contour_params(6.0, 4.0),
        )
        .expect("create"),
    );
    let CommandError::Machining(msg) = GenerateOperation::new(&world, "MF1")
        .err()
        .expect("a face is not a drive curve")
    else {
        panic!("expected a machining refusal");
    };
    assert!(msg.contains("is a face"), "{msg}");
    assert!(msg.contains("Multi-Axis Contour"), "{msg}");
    assert_eq!(status(&world, "MF1"), OpStatus::Never);
}

#[test]
fn multiaxis_contour_refuses_a_chain_outside_its_window_by_name() {
    // A corner post spans z 0..5; the window [4, 6] cannot hold it,
    // and no pose can shorten a curve that stands where it stands.
    // The refusal quotes the vertex and the window - a quietly
    // clipped chain would be the gouge this kind was designed to
    // avoid.
    let (world, mut history) = mac_part_world();
    let src = mac_corner_post(&world);
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "MW1",
            "BN-0600",
            src,
            multiaxis_contour_params(6.0, 4.0),
        )
        .expect("create"),
    );
    let CommandError::Machining(msg) = GenerateOperation::new(&world, "MW1")
        .err()
        .expect("the post stands through the window floor")
    else {
        panic!("expected a machining refusal");
    };
    assert!(msg.contains("outside the window"), "{msg}");
    assert_eq!(
        status(&world, "MW1"),
        OpStatus::Never,
        "a refusal leaves no path behind"
    );
}

#[test]
fn multiaxis_contour_refuses_the_wrong_source_by_name() {
    // Stock and whole-entity sources are what the MESH-driven 5-axis
    // kinds are for; here they would silently invent a path the user
    // never picked. Both refusals point at the same two fixes.
    let mk = |src: SrcRef, tag: &str| {
        let world = new_world();
        let mut history = History::default();
        box_stock(&world, &mut history, 40.0, 40.0, 20.0);
        add_tool(&world, &mut history, "BN-0600");
        history.execute(
            &world,
            ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
        );
        let cmd = CreateOperation::new(
            &world,
            tag,
            "BN-0600",
            src,
            multiaxis_contour_params(6.0, 4.0),
        );
        history.execute(&world, cmd.expect("create"));
        (world, history)
    };
    let (world, _history) = mk(SrcRef::Stock, "MJ1");
    let CommandError::Machining(msg) = GenerateOperation::new(&world, "MJ1")
        .err()
        .expect("stock is not a pick list")
    else {
        panic!("expected a machining refusal");
    };
    assert!(msg.contains("pick them in the Geometry section"), "{msg}");
    let (world, _history) = mk(SrcRef::Entity("part".into()), "MJ2");
    let CommandError::Machining(msg) = GenerateOperation::new(&world, "MJ2")
        .err()
        .expect("a whole model is not a pick list")
    else {
        panic!("expected a machining refusal");
    };
    assert!(msg.contains("not a whole entity"), "{msg}");
}

#[test]
fn multiaxis_contour_refuses_a_flat_tool_and_nonsense_pose_by_name() {
    // The twins' standing refusals, at this kind's doors: an end mill
    // has no ball to rest on a fold (generation), and a zero budget
    // or an impossible cone never reaches a generator (creation).
    let (world, mut history) = box_part_world();
    add_tool(&world, &mut history, "BN-0600");
    let src = mac_top_edges(&world);
    common::create_past_the_tool_law(
        &world,
        &mut history,
        "MT1",
        "EM-0600-4FL",
        src.clone(),
        multiaxis_contour_params(6.0, 4.0),
    );
    let CommandError::Machining(msg) = common::generate_past_the_tool_law(&world, "MT1")
        .expect_err("a flat tool cannot ride a chain")
    else {
        panic!("expected a machining refusal");
    };
    assert!(msg.contains("ball-nose"), "{msg}");
    assert_eq!(status(&world, "MT1"), OpStatus::Never);

    let mk = |p: OpParams| CreateOperation::new(&world, "MT2", "BN-0600", src.clone(), p);
    let mut zero_tol = multiaxis_contour_params(6.0, 4.0);
    if let OpParams::MultiAxisContour { tolerance, .. } = &mut zero_tol {
        *tolerance = 0.0;
    }
    assert!(
        matches!(mk(zero_tol), Err(CommandError::Invalid(_))),
        "tol 0 is no budget"
    );
    let mut flat_lean = multiaxis_contour_params(6.0, 4.0);
    if let OpParams::MultiAxisContour { lead_deg, .. } = &mut flat_lean {
        *lead_deg = 91.0;
    }
    assert!(matches!(mk(flat_lean), Err(CommandError::Invalid(_))));
    let mut no_cone = multiaxis_contour_params(6.0, 4.0);
    if let OpParams::MultiAxisContour { max_tilt_deg, .. } = &mut no_cone {
        *max_tilt_deg = 0.0;
    }
    assert!(
        matches!(mk(no_cone), Err(CommandError::Invalid(_))),
        "a 0 cone allows no lean at all"
    );
}

#[test]
fn multiaxis_contour_quiet_shank_moves_nothing() {
    // Opt-in means EXACTLY opt-in at the chain's seam too: the catalog
    // ball's shank stays a full flute above the rim, so posing with
    // the dodge on must reproduce the dodge-off path move for move,
    // axes included - the fan law's vertices are not a suggestion the
    // shank pass is free to re-pose.
    let run = |dodging: bool, tag: &str| {
        let (world, mut history) = mac_part_world();
        let mut params = multiaxis_contour_params(6.0, 4.0);
        if let OpParams::MultiAxisContour { shank_avoid, .. } = &mut params {
            *shank_avoid = dodging;
        }
        history.execute(
            &world,
            CreateOperation::new(&world, tag, "BN-0600", mac_top_edges(&world), params)
                .expect("create"),
        );
        let cmd = GenerateOperation::new(&world, tag).expect("generate");
        history.execute(&world, cmd);
        assert_eq!(status(&world, tag), OpStatus::Ok);
        cache(&world, tag).expect("cached").path
    };
    let pa = run(false, "MQ1");
    let pb = run(true, "MQ2");
    assert_eq!(pa.moves.len(), pb.moves.len());
    for (i, (a, b)) in pa.moves.iter().zip(&pb.moves).enumerate() {
        assert_eq!(
            a.axis, b.axis,
            "a shank that reaches nothing moved a pose at {i}"
        );
        assert_eq!((a.to.x, a.to.y, a.to.z), (b.to.x, b.to.y, b.to.z));
    }
}

#[test]
fn the_crease_pair_generates_through_the_operation_layer() {
    // M2.5 B, the split's choreography: the dispatch doors, not the
    // kernels. The step pocket is the corner's honest witness - four
    // floor seams weld into a ring, four mouths stand in the corners
    // - and the app's envelope verdict reads it in seconds. The
    // physics (band refusals, weld exactness, the bisector column)
    // are pinned at the kernel door by the motion twin; here only
    // the doors ride, plus the two answers the layer must show the
    // user: a refusal by name, and a status that cuts nothing.
    use camcraft_app::geometry::Triangle;
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 25.0);
    add_tool(&world, &mut history, "BN-0600");
    let quad = |p: [Point3; 4]| {
        vec![
            Triangle::new(p[0], p[1], p[2]),
            Triangle::new(p[0], p[2], p[3]),
        ]
    };
    let (t, f) = (10.0, 5.0); // pocket top, floor
    let pt = Point3::new;
    let mut facets = Vec::new();
    facets.extend(quad([pt(-12.0, -12.0, t), pt(12.0, -12.0, t), pt(12.0, -6.0, t), pt(-12.0, -6.0, t)]));
    facets.extend(quad([pt(-12.0, 6.0, t), pt(12.0, 6.0, t), pt(12.0, 12.0, t), pt(-12.0, 12.0, t)]));
    facets.extend(quad([pt(-12.0, -6.0, t), pt(-6.0, -6.0, t), pt(-6.0, 6.0, t), pt(-12.0, 6.0, t)]));
    facets.extend(quad([pt(6.0, -6.0, t), pt(12.0, -6.0, t), pt(12.0, 6.0, t), pt(6.0, 6.0, t)]));
    facets.extend(quad([pt(6.0, -6.0, f), pt(-6.0, -6.0, f), pt(-6.0, -6.0, t), pt(6.0, -6.0, t)]));
    facets.extend(quad([pt(6.0, 6.0, f), pt(6.0, -6.0, f), pt(6.0, -6.0, t), pt(6.0, 6.0, t)]));
    facets.extend(quad([pt(-6.0, 6.0, f), pt(6.0, 6.0, f), pt(6.0, 6.0, t), pt(-6.0, 6.0, t)]));
    facets.extend(quad([pt(-6.0, -6.0, f), pt(-6.0, 6.0, f), pt(-6.0, 6.0, t), pt(-6.0, -6.0, t)]));
    facets.extend(quad([pt(-6.0, -6.0, f), pt(6.0, -6.0, f), pt(6.0, 6.0, f), pt(-6.0, 6.0, f)]));
    facets.extend(quad([pt(-12.0, -12.0, 0.0), pt(12.0, -12.0, 0.0), pt(12.0, -12.0, t), pt(-12.0, -12.0, t)]));
    facets.extend(quad([pt(12.0, -12.0, 0.0), pt(12.0, 12.0, 0.0), pt(12.0, 12.0, t), pt(12.0, -12.0, t)]));
    facets.extend(quad([pt(12.0, 12.0, 0.0), pt(-12.0, 12.0, 0.0), pt(-12.0, 12.0, t), pt(12.0, 12.0, t)]));
    facets.extend(quad([pt(-12.0, 12.0, 0.0), pt(-12.0, -12.0, 0.0), pt(-12.0, -12.0, t), pt(-12.0, 12.0, t)]));
    history.execute(&world, ImportModel::from_mesh("pocket", Mesh::from_facets(facets)));
    history.execute(&world, ImportModel::from_mesh("cube", cube(pt(0.0, 0.0, 0.0), pt(10.0, 10.0, 10.0))));
    let pat = |which: camcraft_app::finishing3d::FinishPattern, d: Option<f64>| {
        let mut ask = finish_params(t, f, 0.1);
        let OpParams::Finish3D { pattern, pencil_reference, .. } = &mut ask else {
            panic!("finish3d params expected");
        };
        *pattern = which;
        *pencil_reference = d;
        ask
    };
    for (name, ask) in [
        ("KC", pat(camcraft_app::finishing3d::FinishPattern::Corner, None)),
        ("KP", pat(camcraft_app::finishing3d::FinishPattern::Pencil, Some(8.0))),
    ] {
        history.execute(
            &world,
            CreateOperation::new(&world, name, "BN-0600", SrcRef::Entity("pocket".into()), ask)
                .expect("create"),
        );
        history.execute(&world, GenerateOperation::new(&world, name).expect("generate"));
        assert_eq!(status(&world, name), OpStatus::Ok, "{name} must seat and cut");
        let moves = cache(&world, name).expect("cached").path.moves;
        // Moves speak tips: the ring's locus stands at floor + r
        // (z 8.0), its TOOLPATH points at floor level (z 5.0).
        let ring = moves.iter().any(|m| {
            matches!(m.kind, camcraft_app::cl::MoveKind::Feed)
                && (m.to.z - 5.0).abs() < 1e-9
                && (m.to.x.abs() - 3.0).abs() < 1e-9
                && (m.to.y.abs() - 3.0).abs() < 1e-9
        });
        assert!(ring, "{name} left the floor seams' ring uncut");
    }
    // A reference that is not fatter than the ball is no reference:
    // the pencil door refuses by name and the op cuts nothing.
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "KR",
            "BN-0600",
            SrcRef::Entity("pocket".into()),
            pat(camcraft_app::finishing3d::FinishPattern::Pencil, Some(6.0)),
        )
        .expect("create"),
    );
    let CommandError::Machining(msg) = GenerateOperation::new(&world, "KR").err().expect("a pen that is not fatter must refuse") else {
        panic!("expected a machining refusal");
    };
    assert!(msg.contains("does not exceed"), "{msg}");
    assert_eq!(status(&world, "KR"), OpStatus::Never, "a refusal cuts nothing");
    // And a convex part has no corner at all: the Empty answer
    // reaches the user the same way, by name.
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "KB",
            "BN-0600",
            SrcRef::Entity("cube".into()),
            pat(camcraft_app::finishing3d::FinishPattern::Corner, None),
        )
        .expect("create"),
    );
    let CommandError::Machining(msg) = GenerateOperation::new(&world, "KB").err().expect("a cube has no concave corner") else {
        panic!("expected a machining refusal");
    };
    assert!(msg.contains("no concave corner"), "{msg}");
    assert_eq!(status(&world, "KB"), OpStatus::Never);
}

// ---------------------------------------------------------------------------
// M2.5 E B: the deburr ride through the operation layer. The kernel law
// (`finishing3d::deburr` and its edge taxonomy) is gate-tested in the
// motion crate's deburr.rs; at this level the checks are the THREE tool
// doors the app adds on top of the kernel's own validate (the kind, the
// cone's angle, and the flank's reach - a plain end mill dressed as a
// break is a gouge waiting for a spindle), one through-the-layer ride
// that must seat, cut, and stay prismatic, and the seed's numbers.
// ---------------------------------------------------------------------------

/// The deburr ride's params: the window is stated in the TIP's units
/// (the kernel's clipping law), so a `top_z` at the model's own top
/// minus the drop names exactly the rim that stands there.
fn deburr_params(width: f64, top_z: f64, bottom_z: f64) -> OpParams {
    OpParams::Deburr {
        width,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(top_z),
        bottom_z: Height::fixed(bottom_z),
        feed: 500.0,
        plunge_feed: 100.0,
    }
}

#[test]
fn the_deburr_rides_the_bricks_top_rim_through_the_operation_layer() {
    // Width 2 on the 90-degree cone drops the tip 2 below each edge
    // it rides; the window [6, 8] admits the top rim's tips (8) and
    // the top ends of the verticals, and the bottom rim (tip -2)
    // stays out - which is the honest program: a cone breaking the
    // bottom rim from above would stand INSIDE the wall. The law
    // under test is the trip, not the taxonomy: an Ok verdict, metal
    // cut, no gouge claim, and nothing posed - an axis on a move
    // would mean the 5-axis stamp leaked into the ride.
    use camcraft_app::cl::MoveKind;
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 25.0);
    add_tool(&world, &mut history, "CM-0600-90");
    history.execute(
        &world,
        ImportModel::from_mesh(
            "brick",
            cube(Point3::new(-20.0, -10.0, 0.0), Point3::new(20.0, 10.0, 10.0)),
        ),
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "BRK",
            "CM-0600-90",
            SrcRef::Entity("brick".into()),
            deburr_params(2.0, 8.0, 6.0),
        )
        .expect("create"),
    );
    history.execute(&world, GenerateOperation::new(&world, "BRK").expect("generate"));
    assert_eq!(status(&world, "BRK"), OpStatus::Ok, "the rim ride must seat and cut");
    let t = cache(&world, "BRK").expect("cached");
    let feeds = t.path.moves.iter().filter(|m| m.kind == MoveKind::Feed).count();
    assert!(feeds > 0, "the deburr went silent on the brick");
    assert!(t.path.is_prismatic(), "the deburr leaked a posed axis into a 3-axis program");
    assert!(
        !t.warnings.iter().any(|w| matches!(w, camcraft_app::verify::Warning::Gouge { .. })),
        "the ride gouged: {:?}",
        t.warnings
    );
    // The tips of a 90-degree cone at width 2 stand exactly 2 below
    // the edges they break: no cut move may dip under the window's
    // floor or breach the clearance - the clipping law's own bounds,
    // read off the program.
    for m in &t.path.moves {
        assert!(m.to.z >= 6.0 - 1e-9, "a cut point under the window floor: {:?}", m.to);
        assert!(m.to.z <= 25.0 + 1e-9, "a move over the clearance: {:?}", m.to);
    }
}

#[test]
fn deburr_refuses_a_plain_end_mill_by_name() {
    // The ride IS a cone over the edge; an end mill would dress a
    // cylinder as a chamfer flank, and the kernel would compute
    // drops for a tool that has none. The door speaks, nothing cuts.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 25.0);
    add_tool(&world, &mut history, "EM-0600-4FL");
    history.execute(
        &world,
        ImportModel::from_mesh("brick", deburr_brick()),
    );
    common::create_past_the_tool_law(
        &world,
        &mut history,
        "DB",
        "EM-0600-4FL",
        SrcRef::Entity("brick".into()),
        deburr_params(2.0, 8.0, 6.0),
    );
    let CommandError::Machining(msg) = common::generate_past_the_tool_law(&world, "DB")
        .expect_err("the deburr must refuse a flat tool")
    else {
        panic!("expected a machining refusal");
    };
    assert!(msg.contains("needs a chamfer mill"), "{msg}");
    assert_eq!(status(&world, "DB"), OpStatus::Never, "the refusal leaves no path behind");
}

#[test]
fn deburr_refuses_a_cone_that_rules_no_chamfer() {
    // 180 included is a flat, not a cone: no half-angle to take the
    // tangent of, no drop to stand the tip at. The door names the
    // angle it found. Two layers close on it: AddTool's is_sane gate
    // (Taper wants tip_angle > 0, the global bound < 180) refuses the
    // mount, and the generate door catches the shape directly - so
    // the command layer is proven, then bypassed by writing the
    // ToolDef straight onto the entity (the machine.rs idiom), which
    // is the only way the kernel's own door can ever see a flat.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 25.0);
    let flat = Tool {
        kind: camcraft_app::tools::ToolKind::Taper,
        tip_angle: 180.0,
        flute_len: 10.0,
        ..stub_tool()
    };
    assert!(
        matches!(
            AddTool::new(&world, "FLAT-CONE", flat.clone()),
            Err(CommandError::Invalid(_))
        ),
        "the command law refuses a cone that rules no chamfer"
    );
    model::spawn(&world, "FLAT-CONE")
        .entity_view(&world)
        .set(ToolDef { tool: flat, number: None });
    history.execute(&world, ImportModel::from_mesh("brick", deburr_brick()));
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "DB",
            "FLAT-CONE",
            SrcRef::Entity("brick".into()),
            deburr_params(2.0, 8.0, 6.0),
        )
        .expect("create"),
    );
    let CommandError::Machining(msg) = GenerateOperation::new(&world, "DB")
        .err()
        .expect("a flat cone must refuse")
    else {
        panic!("expected a machining refusal");
    };
    assert!(msg.contains("rules no chamfer"), "{msg}");
    assert_eq!(status(&world, "DB"), OpStatus::Never);
}

#[test]
fn deburr_refuses_a_flank_too_short_for_the_break() {
    // Width 5 on the 90-degree cone reaches 5 / sin 45 = 7.1 mm up
    // the flank - the CM-0400's 6 mm of flute stops at its shank, and
    // the ride would dress a cylinder as a cone. The catalog's own
    // tool proves the door: no fixture invention needed.
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 25.0);
    add_tool(&world, &mut history, "CM-0400-90");
    history.execute(&world, ImportModel::from_mesh("brick", deburr_brick()));
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "DB",
            "CM-0400-90",
            SrcRef::Entity("brick".into()),
            deburr_params(5.0, 8.0, -6.0),
        )
        .expect("create"),
    );
    let CommandError::Machining(msg) = GenerateOperation::new(&world, "DB")
        .err()
        .expect("a short flank must refuse")
    else {
        panic!("expected a machining refusal");
    };
    assert!(msg.contains("up the cone's flank"), "{msg}");
    assert!(msg.contains("7.1"), "{msg}");
    assert_eq!(status(&world, "DB"), OpStatus::Never);
}

#[test]
fn the_deburr_seed_carries_the_heights_and_a_hand_break_width() {
    // The seed's promise (M2.5 E B): half a millimetre - the shop's
    // deburr, not a chamfer - and the window IS the model's own
    // height, so a fresh op rides every exposed edge until a
    // machinist narrows it. Nothing else the seed may invent.
    use camcraft_app::operation::{Carried, HeightReference, KindInfo, OpType};
    let carried = Carried {
        feed: 500.0,
        plunge: 100.0,
        climb: true,
        clearance: Height::fixed(25.0),
        stepover: 3.0,
        stepdown: 5.0,
        top: Height::of(HeightReference::TopOfModel, 0.0),
        bottom: Height::of(HeightReference::BottomOfModel, 0.0),
        retract: Height::fixed(20.0),
        tilt: None,
    };
    let row = KindInfo::ALL
        .iter()
        .find(|k| k.ty == OpType::Deburr)
        .expect("the deburr row seats in the registry");
    let OpParams::Deburr {
        width,
        clearance_z,
        top_z,
        bottom_z,
        feed,
        plunge_feed,
    } = (row.seed)(&carried)
    else {
        panic!("the deburr row must seed deburr params");
    };
    assert_eq!(width, 0.5, "a hand-break, not a chamfer");
    assert_eq!(clearance_z, carried.clearance);
    assert_eq!(top_z, carried.top, "the window is the model's own");
    assert_eq!(bottom_z, carried.bottom);
    assert_eq!(feed, 500.0);
    assert_eq!(plunge_feed, 100.0);
}

/// The ride's model: a 40 x 20 x 10 brick standing on the stock floor,
/// centered so no face runs off the 60 x 60 x 25 stock's edge.
fn deburr_brick() -> camcraft_app::geometry::Mesh {
    camcraft_app::geometry::cube(
        Point3::new(-20.0, -10.0, 0.0),
        Point3::new(20.0, 10.0, 10.0),
    )
}
