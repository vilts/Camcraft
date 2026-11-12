//! M1.5.3 acceptance: every height is a From-reference + offset, resolved
//! against the live world. What must hold: each source reads the z it
//! stands for, a missing source is refused by name at generate (never a
//! guessed plane), the digest sees the reference (so moving a source goes
//! Stale), and a v5 file's bare numbers land as `Fixed` to the bit.
//! Provenance: HSMWorks expresses heights the same way (From + Offset);
//! specced from its published user documentation, behavior only.
//!
//! Gate map (M4.4): covers app::commands, app::geometry, motion::machining2d, app::model, app::operation, app::project, app::schema, geometry::geometry::stock, tools::tools, app::world.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CommandError, CreateOperation, CreateSetup, DeleteEntity, GenerateOperation, History,
    ImportModel, SetSetupWorkOffset, SetStock, SetWcsOrigin,
};
use camcraft_app::geometry::{cube, IDENTITY_Q, Point3};
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, HeightReference, OpParams, OpStatus, SrcRef};
use camcraft_app::project;
use camcraft_app::schema::{Location, STOCK_NAME};
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

fn box_stock(world: &World, history: &mut History, dx: f64, dy: f64, dz: f64) {
    let cmd = SetStock::new(world, StockShape::Box { dx, dy, dz }, Location::default());
    history.execute(world, cmd.expect("box stock"));
}

fn add_tool(world: &World, history: &mut History, name: &str) {
    let cmd = AddTool::new(world, name, catalog_tool(name));
    history.execute(world, cmd.expect("catalog tool"));
}

/// 60x60x20 box stock (z 0..20) plus a 2x2x4 block standing z 5..9.
fn stock_and_block() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history, 60.0, 60.0, 20.0);
    history.execute(
        &world,
        ImportModel::from_mesh(
            "block",
            cube(Point3::new(0.0, 0.0, 5.0), Point3::new(2.0, 2.0, 9.0)),
        ),
    );
    (world, history)
}

/// A pocket with the given heights over the stock-and-block fixture and
/// nothing else varied - (enum variants take no functional-update syntax,
/// so the three heights are the only knobs this literal exposes).
fn pocket(clearance_z: Height, top_z: Height, bottom_z: Height) -> OpParams {
    OpParams::Pocket { tilt: None,
        links: camcraft_app::links::Links::PLANAR,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z,
        retract_z: None,
        top_z,
        bottom_z,
        stepdown: 1.5,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    }
}

fn h(r: HeightReference, offset: f64) -> Height {
    Height::of(r, offset)
}

#[test]
fn fixed_reference_reads_its_own_number() {
    // A Fixed height is pure: it resolves the same in an empty world as
    // anywhere else, which is why pre-v6 files keep their exact z.
    let world = new_world();
    let f = Height::fixed(-0.125);
    assert_eq!(f.z(&world), Some(-0.125));
    assert!(f.is_fixed());
    assert_eq!(f.reference.kind_index(), 0);
}

#[test]
fn stock_and_model_references_read_their_extremes() {
    let (world, _history) = stock_and_block();
    // Box stock stands z 0..20; the block z 5..9, and model bounds
    // exclude stock and WCS by `solid_world_aabb`'s rule.
    assert_eq!(h(HeightReference::TopOfStock, 0.0).z(&world), Some(20.0));
    assert_eq!(h(HeightReference::BottomOfStock, 0.0).z(&world), Some(0.0));
    assert_eq!(h(HeightReference::TopOfModel, 0.0).z(&world), Some(9.0));
    assert_eq!(h(HeightReference::BottomOfModel, 0.0).z(&world), Some(5.0));
    // The offset rides on top of the source.
    assert_eq!(h(HeightReference::TopOfStock, -2.5).z(&world), Some(17.5));
    assert_eq!(h(HeightReference::BottomOfModel, 1.0).z(&world), Some(6.0));
}

#[test]
fn work_offset_reference_reads_the_slots_plane() {
    let world = new_world();
    let mut history = History::default();
    // Unplaced: the slot entity does not exist, so nothing resolves -
    // and the diagnostic says what to do about it.
    let g54 = h(HeightReference::WorkOffset, 0.0);
    assert_eq!(g54.z(&world), None);
    assert!(operation::missing_source(&world, None, None, g54.reference)
        .unwrap()
        .contains("no origin yet"));
    history.execute(
        &world,
        SetWcsOrigin::new(&world, at(0.0, 0.0, -2.0)).expect("place G54"),
    );
    assert_eq!(g54.z(&world), Some(-2.0));
    assert_eq!(h(HeightReference::WorkOffset, 0.5).z(&world), Some(-1.5));
    // #342: the WCS is the SETUP's, one per setup - relabelling it G55 or
    // G59 changes the word the post writes, never the plane a height
    // reads; placing it at the origin reads the origin ("placed at 0" is
    // a real placement).
    history.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    history.execute(&world, SetSetupWorkOffset::new(&world, "Setup1", Some(1)).expect("label G55"));
    assert_eq!(g54.z(&world), Some(-2.0), "the label moved nothing");
    history.execute(
        &world,
        SetWcsOrigin::new(&world, at(0.0, 0.0, 7.0)).expect("re-place the WCS"),
    );
    history.execute(&world, SetSetupWorkOffset::new(&world, "Setup1", Some(5)).expect("label G59"));
    assert_eq!(g54.z(&world), Some(7.0));
    history.execute(&world, SetWcsOrigin::new(&world, at(0.0, 0.0, 0.0)).expect("WCS to 0"));
    assert_eq!(g54.z(&world), Some(0.0));
    // Back to the default word: the same WCS.
    history.execute(&world, SetSetupWorkOffset::new(&world, "Setup1", None).expect("unlabel"));
    // Only the plane's z feeds a height: XY and rotations stay outside.
    history.execute(
        &world,
        SetWcsOrigin::new(&world, at(99.0, 99.0, -2.0)).expect("slide G54"),
    );
    assert_eq!(g54.z(&world), Some(-2.0), "a Z reference reads the plane's height");
}

#[test]
fn resolve_params_agrees_with_resolving_heights_one_by_one() {
    // The cross-check: every field the batch resolves must equal the same
    // height resolved alone - the two paths can disagree only through a
    // bug.
    let (world, mut history) = stock_and_block();
    history.execute(
        &world,
        SetWcsOrigin::new(&world, at(0.0, 0.0, -1.0)).expect("place G54"),
    );
    let params = pocket(
        h(HeightReference::TopOfStock, 5.0),
        h(HeightReference::TopOfModel, 0.0),
        h(HeightReference::WorkOffset, 0.0),
    );
    let r = operation::resolve_params(&world, None, None, &params).expect("all sources stand");
    if let OpParams::Pocket { clearance_z, top_z, bottom_z, retract_z, .. } = &params {
        assert_eq!(r.clearance, clearance_z.z(&world).unwrap());
        assert_eq!(r.top, top_z.z(&world));
        assert_eq!(r.bottom, bottom_z.z(&world));
        assert_eq!(r.retract, retract_z.map(|x| x.z(&world).unwrap()));
    } else {
        unreachable!();
    }
    assert_eq!((Some(r.clearance), r.top, r.bottom), (Some(25.0), Some(9.0), Some(-1.0)));
    assert_eq!(r.depth(), (Some(9.0), Some(-1.0)));
    // resolve_map hands panels and verify the same pair.
    assert_eq!(operation::resolve_map(&world, None, None, &params), (Some(9.0), Some(-1.0)));
}

#[test]
fn missing_sources_are_an_honest_mixed_state() {
    // The new-op defaults seed Top-of-model and G54 on purpose; with
    // only G54 placed, exactly the heights that stand resolve.
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetWcsOrigin::new(&world, at(0.0, 0.0, 0.0)).expect("place G54"),
    );
    let clearance = h(HeightReference::TopOfStock, 25.0);
    let top = h(HeightReference::TopOfModel, 0.0);
    let bottom = h(HeightReference::WorkOffset, 0.0);
    assert_eq!(clearance.z(&world), None);
    assert!(operation::missing_source(&world, None, None, clearance.reference)
        .unwrap()
        .contains("no stock yet"));
    assert_eq!(top.z(&world), None);
    assert!(operation::missing_source(&world, None, None, top.reference)
        .unwrap()
        .contains("no model loaded"));
    assert_eq!(bottom.z(&world), Some(0.0));
    // The same three in a params set refuse as a whole; the message
    // names the height and its source, never a silent z.
    let params = pocket(clearance, top, bottom);
    let err = operation::resolve_params(&world, None, None, &params).expect_err("no stock, no model");
    assert!(err.contains("Clearance Z"), "{err}");
    assert!(err.contains("no stock yet"), "{err}");
}

#[test]
fn generate_refuses_an_unresolved_height_and_names_the_operation() {
    // The AGENTS.md rule made concrete: the op exists, its cache does
    // not, and the refusal is actionable by operation AND field.
    let (world, mut history) = stock_and_block();
    add_tool(&world, &mut history, "EM-0400-2FL");
    let params = pocket(
        h(HeightReference::TopOfStock, 5.0),
        h(HeightReference::TopOfModel, 0.0),
        Height::fixed(1.0),
    );
    history.execute(
        &world,
        CreateOperation::new(&world, "P1", "EM-0400-2FL", SrcRef::Stock, params)
            .expect("all references stand at creation"),
    );
    // Deleting the model leaves the op (commands own their fields) but
    // its Top-of-model reference now dangles: generation refuses.
    history.execute(&world, DeleteEntity::new(&world, "block").expect("delete block"));
    let err = match GenerateOperation::new(&world, "P1") {
        Err(CommandError::Machining(m)) => m,
        Err(other) => panic!("expected a machining refusal, got {other:?}"),
        Ok(_) => panic!("generation should have refused an unresolved height"),
    };
    assert!(err.contains("P1"), "names the operation: {err}");
    assert!(err.contains("Top of Part Z"), "names the height: {err}");
    assert!(err.contains("no model loaded"), "says what is missing: {err}");
    let op = model::find_by_name(&world, "P1").expect("op");
    assert_eq!(
        operation::status_of(&world, &op),
        OpStatus::Never,
        "a refused generation leaves nothing to replay",
    );
}

#[test]
fn depth_ordering_is_checked_on_resolved_values() {
    // A pocket whose top references below its bottom is refused while
    // both resolve, and waved through while one is unresolved (the
    // refusal for unresolved is generate's, by name) - commands validate
    // early without inventing a plane.
    let (world, _history) = stock_and_block(); // stock 0..20, model 5..9
    let up = pocket(
        Height::fixed(25.0),
        h(HeightReference::BottomOfModel, 0.0), // 5.0
        h(HeightReference::TopOfModel, 0.0),     // 9.0 - above its own top
    );
    assert!(up.is_sane(), "each number is fine on its own");
    assert!(!operation::depth_sane(&world, None, None, &up), "but 5 -> 9 cuts upward");
    let unsight = pocket(
        Height::fixed(25.0),
        h(HeightReference::WorkOffset, 0.0), // G54 never placed here
        h(HeightReference::TopOfModel, 0.0),
    );
    assert!(
        operation::depth_sane(&world, None, None, &unsight),
        "an unresolved top is generate's problem, never a false ordering failure",
    );
    let ok = pocket(
        Height::fixed(25.0),
        h(HeightReference::TopOfModel, 0.0),     // 9.0
        h(HeightReference::BottomOfModel, 0.0),  // 5.0
    );
    assert!(operation::depth_sane(&world, None, None, &ok));
}

#[test]
fn the_digest_sees_the_reference_not_just_the_number() {
    // Same resolved z, different source: a Fixed 25 and Top-of-stock+5
    // over a 20 mm stock must not claim each other's cache - one is
    // pinned, the other rides the setup.
    let (world, mut history) = stock_and_block();
    add_tool(&world, &mut history, "EM-0400-2FL");
    let pinned = pocket(Height::fixed(25.0), Height::fixed(4.0), Height::fixed(1.0));
    let riding = pocket(
        h(HeightReference::TopOfStock, 5.0),
        Height::fixed(4.0),
        Height::fixed(1.0),
    );
    assert_eq!(
        pinned.clearance().z(&world),
        riding.clearance().z(&world),
        "identical until the stock moves",
    );
    let sa = operation::current_stamp(&world, "EM-0400-2FL", &SrcRef::Stock, &pinned, "", None);
    let sb = operation::current_stamp(&world, "EM-0400-2FL", &SrcRef::Stock, &riding, "", None);
    assert_ne!(sa, sb, "the reference is part of the definition");
}

#[test]
fn moving_a_source_stales_an_operation_that_references_it() {
    // The user-reported bug made into a gate: the op's numbers never
    // changed - G54 did - and the cached path must say Stale.
    let (world, mut history) = stock_and_block();
    add_tool(&world, &mut history, "EM-0400-2FL");
    history.execute(
        &world,
        SetWcsOrigin::new(&world, at(0.0, 0.0, 4.0)).expect("place G54"),
    );
    let params = pocket(
        h(HeightReference::TopOfStock, 5.0),
        h(HeightReference::WorkOffset, 0.0),     // 4.0
        h(HeightReference::WorkOffset, -3.0),    // 1.0
    );
    history.execute(
        &world,
        CreateOperation::new(&world, "P1", "EM-0400-2FL", SrcRef::Stock, params)
            .expect("create"),
    );
    history.execute(&world, GenerateOperation::new(&world, "P1").expect("generate"));
    let op = model::find_by_name(&world, "P1").expect("op");
    assert_eq!(operation::status_of(&world, &op), OpStatus::Ok);
    history.execute(
        &world,
        SetWcsOrigin::new(&world, at(0.0, 0.0, 3.0)).expect("drop G54"),
    );
    assert_eq!(
        operation::status_of(&world, &op),
        OpStatus::Stale,
        "the same params over a moved plane are a different cut",
    );
    history.execute(&world, GenerateOperation::new(&world, "P1").expect("regenerate"));
    assert_eq!(operation::status_of(&world, &op), OpStatus::Ok);

    // Raising the stock moves the rapids plane the same way.
    box_stock(&world, &mut history, 60.0, 60.0, 30.0);
    assert_eq!(operation::status_of(&world, &op), OpStatus::Stale);
}

#[test]
fn v5_numeric_heights_land_as_fixed_to_the_bit() {
    // The migration promise: every pre-v6 height is a Fixed reference
    // carrying the exact saved number - no rounding, no re-seeding.
    let text = r#"{
        "version": 5,
        "entities": [
            {"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"pocket": {
                 "stepover": 3.0, "stock_to_leave": 0.0,
                 "clearance_z": 20.123456789012345,
                 "top_z": -0.125, "bottom_z": 18.0, "stepdown": 2.0,
                 "feed": 900.0, "plunge_feed": 250.0, "climb": true,
                 "strategy": "zigzag"}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v5 must migrate");
    let op = model::find_by_name(&world, "Op1").expect("op");
    match model::operation_of(&world, &op).expect("params").params {
        OpParams::Pocket { clearance_z, top_z, bottom_z, .. } => {
            assert!(clearance_z.is_fixed() && top_z.is_fixed() && bottom_z.is_fixed());
            assert_eq!(
                clearance_z.offset.to_bits(),
                20.123456789012345f64.to_bits(),
                "to the bit"
            );
            assert_eq!(top_z, Height::fixed(-0.125));
            assert_eq!(bottom_z, Height::fixed(18.0));
        }
        ref other => panic!("pocket expected, got {other:?}"),
    }

    // Resaved, the same number is an explicit expression - and reopens
    // with identical bits.
    let path = std::env::temp_dir()
        .join(format!("camcraft_heights_v5_{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(saved.contains("\"reference\": \"fixed\""), "v6 writes expressions: {saved}");
    let world2 = new_world();
    project::open(&world2, &saved).expect("reopen");
    let op2 = model::find_by_name(&world2, "Op1").expect("op2");
    match model::operation_of(&world2, &op2).expect("params").params {
        OpParams::Pocket { clearance_z, .. } => {
            assert_eq!(clearance_z.offset.to_bits(), 20.123456789012345f64.to_bits());
        }
        _ => unreachable!(),
    }
}

#[test]
fn references_and_placements_survive_a_project_roundtrip() {
    // Both halves of the feature persist: an operation's From-references
    // and the G54 placement they point at - a reopened document must not
    // silently zero the machine offset the heights lean on.
    let (world, mut history) = stock_and_block();
    add_tool(&world, &mut history, "EM-0400-2FL");
    history.execute(
        &world,
        SetWcsOrigin::new(&world, at(0.0, 0.0, 4.0)).expect("place G54"),
    );
    let params = pocket(
        h(HeightReference::TopOfStock, 5.0),
        h(HeightReference::WorkOffset, 0.0),
        h(HeightReference::BottomOfStock, 1.5),
    );
    history.execute(
        &world,
        CreateOperation::new(&world, "P1", "EM-0400-2FL", SrcRef::Stock, params).expect("create"),
    );
    let path = std::env::temp_dir()
        .join(format!("camcraft_heights_rt_{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(saved.contains("\"work_offset\""), "the work-offset reference is stored: {saved}");
    assert!(!saved.contains("\"slot\""), "with no slot of its own (#329)");
    assert!(saved.contains("\"top_of_stock\""), "and the stock reference");

    let world2 = new_world();
    project::open(&world2, &saved).expect("load");
    let op2 = model::find_by_name(&world2, "P1").expect("op");
    match model::operation_of(&world2, &op2).expect("params").params {
        OpParams::Pocket { top_z, bottom_z, .. } => {
            assert_eq!(top_z, h(HeightReference::WorkOffset, 0.0));
            assert_eq!(bottom_z, h(HeightReference::BottomOfStock, 1.5));
        }
        _ => unreachable!(),
    }
    // G54 stands where it was left.
    assert_eq!(h(HeightReference::WorkOffset, 0.0).z(&world2), Some(4.0));
    let stock = model::find_by_name(&world2, STOCK_NAME).expect("stock came back");
    assert!(model::is_stock(&world2, &stock));
}
