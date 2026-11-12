//! M3.1 D acceptance: re-machining. What an operation generates against
//! is not just the stock but the program - every operation ahead of it in
//! posting order has already cut, and its rest is what that leaves
//! standing. The laws tested here are rest.rs's: program order is the
//! post's order (name-sorted), the mask only ever removes cuts, recorded
//! stamps are the digest's truth (never intentions), a never-generated
//! predecessor contributes only its marker, and the dependent converges
//! within two regenerate rounds.
//!
//! Provenance: our own design (see `crate::rest`); RS-274 program-order
//! semantics for what "ahead in the program" means.
//!
//! Gate map (M4.4): covers cl::cl, app::commands, app::geometry, motion::machining2d, motion::machining3d, app::model, app::operation, app::schema, geometry::geometry::stock, tools::tools, app::world.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel,
    RegenerateAll, SetStock,
};
use camcraft_app::cl::{Move, MoveKind};
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube, quat_from_yaw};
use camcraft_app::machining2d::{GenError, PocketStrategy};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::{Location, Toolpath};
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

fn add_tool(world: &World, history: &mut History, name: &str) {
    let cmd = AddTool::new(world, name, catalog_tool(name));
    history.execute(world, cmd.expect("catalog tool"));
}

fn status(world: &World, name: &str) -> OpStatus {
    let e = model::find_by_name(world, name).expect("entity exists");
    operation::status_of(world, &e)
}

fn cache(world: &World, name: &str) -> Option<Toolpath> {
    model::find_by_name(world, name).and_then(|e| model::toolpath_of(world, &e))
}

/// The job every test runs: a 40x40x20 box of stock and a model whose
/// finished shape is the same footprint with a 20x20 pocket open from the
/// top down to z=10. Material semantics are the model's - the pocket is
/// what must be machined out of the box.
fn job_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0800-4FL");
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 40.0,
                dy: 40.0,
                dz: 20.0,
            },
            Location::default(),
        )
        .expect("box stock"),
    );
    history.execute(
        &world,
        ImportModel::from_mesh("plate", plate_with_pocket()),
    );
    (world, history)
}

/// The finished part: a 40x40x[0,20] plate with a blind 20x20 pocket
/// sunk from the top to z=10. Pocket3D's law is that the model is the
/// final part - the ladder cuts, inside the model's silhouette, the
/// columns the model does NOT fill at each plane - so this plate's
/// pocket void is exactly the job, and its floor at z=10 is the lowest
/// floor the ladder may touch. The box stock starts as the full solid.
fn plate_with_pocket() -> Mesh {
    let mut facets = Vec::new();
    // The outer box minus its top lid: bottom face and four walls, all
    // outward-facing (cube's facet order is bottom, top, then walls).
    let outer = cube(
        Point3::new(-20.0, -20.0, 0.0),
        Point3::new(20.0, 20.0, 20.0),
    );
    facets.extend(outer.facets[0..2].iter().cloned());
    facets.extend(outer.facets[4..12].iter().cloned());
    // The top face as a frame ring around the pocket mouth, and the
    // pocket walls down to the floor: the void box of the pocket,
    // material on its outside.
    let inner = cube(
        Point3::new(-10.0, -10.0, 10.0),
        Point3::new(10.0, 10.0, 20.0),
    );
    facets.extend(inner.facets[0..2].iter().cloned()); // floor quads (+z out)
    facets.extend(inner.facets[4..12].iter().cloned()); // walls
    let (o0, o1, o2, o3) = (
        Point3::new(-20.0, -20.0, 20.0),
        Point3::new(20.0, -20.0, 20.0),
        Point3::new(20.0, 20.0, 20.0),
        Point3::new(-20.0, 20.0, 20.0),
    );
    let (i0, i1, i2, i3) = (
        Point3::new(-10.0, -10.0, 20.0),
        Point3::new(10.0, -10.0, 20.0),
        Point3::new(10.0, 10.0, 20.0),
        Point3::new(-10.0, 10.0, 20.0),
    );
    for (a, b, c, d) in [
        // ring quads (CCW seen from +z): outer-outer-inner-inner
        (o0, o1, i1, i0),
        (o1, o2, i2, i1),
        (o2, o3, i3, i2),
        (o3, o0, i0, i3),
    ] {
        facets.push(Triangle::new(a, b, c));
        facets.push(Triangle::new(a, c, d));
    }
    Mesh::from_facets(facets)
}

fn pocket(bottom_z: f64) -> OpParams {
    OpParams::Pocket3D {
        links: camcraft_app::links::Links::ROUGH3D,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(bottom_z),
        stepdown: 5.0,
        feed: 1500.0,
        plunge_feed: 400.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        rest_rough: false,
        optimal_load: camcraft_app::load::load_of_degrees(60.0),
        feed_opt: false,
        shell: false,
    }
}

fn create(world: &World, history: &mut History, name: &str, params: OpParams) {
    let cmd = CreateOperation::new(
        world,
        name,
        "EM-0800-4FL",
        SrcRef::Entity("plate".into()),
        params,
    );
    history.execute(world, cmd.expect("create pocket"));
}

fn generate(world: &World, history: &mut History, name: &str) {
    let cmd = GenerateOperation::new(world, name).expect("generate");
    history.execute(world, cmd);
}

fn is_cut(m: &Move) -> bool {
    !matches!(m.kind, MoveKind::Rapid)
}

fn planar_z(path: &Toolpath) -> Vec<f64> {
    let mut zs: Vec<f64> = path
        .path
        .moves
        .iter()
        .filter(|m| is_cut(m))
        .map(|m| m.to.z)
        .collect();
    zs.dedup();
    zs
}

#[test]
fn a_never_generated_prior_contributes_only_its_marker() {
    let (world, mut history) = job_world();
    create(&world, &mut history, "A", pocket(12.0));
    create(&world, &mut history, "B", pocket(10.0));
    // B generates first: A has never cut, so B sees the bare stock -
    // skim and full dig - exactly what an entityless caller generates.
    generate(&world, &mut history, "B");
    assert_eq!(status(&world, "B"), OpStatus::Ok);
    let zs = planar_z(&cache(&world, "B").expect("B cache"));
    assert!(zs.contains(&20.0), "unmasked B skims the top: {zs:?}");
    assert!(zs.contains(&10.0), "and cuts its own floor: {zs:?}");
    let op_b = model::operation_of(&world, &model::find_by_name(&world, "B").expect("B"))
        .expect("operation");
    let bare = operation::generate(&world, &op_b, "")
        .expect("no name: nothing ahead")
        .0;
    assert_eq!(
        cache(&world, "B").expect("B cache").path,
        bare,
        "a never-generated predecessor cuts nothing: B's path IS the bare-stock path"
    );
    // A never-generated contributes only its marker: an operation with no
    // predecessors at all carries the same digest term as one ahead of a
    // Never op, so A generating moves B - and A itself is the first-op
    // case, whose stamp must not churn at all (the inert-data law).
    let src = SrcRef::Entity("plate".into());
    let a_params = pocket(12.0);
    assert_eq!(
        operation::current_stamp(&world, "EM-0800-4FL", &src, &a_params, "", None),
        operation::current_stamp(&world, "EM-0800-4FL", &src, &a_params, "A", None),
        "nothing ahead and no entity at all are the same stamp"
    );
    generate(&world, &mut history, "A");
    assert_eq!(
        status(&world, "B"),
        OpStatus::Stale,
        "A went from marker to cut: B's rest digest moved"
    );
    let why = operation::stale_reason(&world, &model::find_by_name(&world, "B").expect("B"));
    assert!(
        why.as_ref().is_some_and(|w| w.contains("ahead of it in the program")),
        "B's staleness names the program, not B's own inputs: {why:?}"
    );
    assert_eq!(
        status(&world, "A"),
        OpStatus::Ok,
        "A's own stamp has no rest term: first op is inert"
    );
    // Undoing A's generation restores the marker-only digest exactly.
    assert!(history.undo(&world));
    assert_eq!(status(&world, "B"), OpStatus::Ok);
}

#[test]
fn a_redundant_duplicate_refuses_rather_than_cutting_air() {
    let (world, mut history) = job_world();
    create(&world, &mut history, "A", pocket(12.0));
    create(&world, &mut history, "B2", pocket(12.0));
    generate(&world, &mut history, "A");
    assert!(
        GenerateOperation::new(&world, "B2").is_err(),
        "a duplicate of a completed op has nothing left to cut"
    );
    assert_eq!(status(&world, "B2"), OpStatus::Never);
    // The refusal the user sees is the rest-aware one, not the bare
    // "the model fills the pocket" text.
    let op_b2 = model::operation_of(
        &world,
        &model::find_by_name(&world, "B2").expect("B2"),
    )
    .expect("operation");
    let err = operation::generate(&world, &op_b2, "B2")
        .expect_err("masked duplicate refuses");
    assert!(
        matches!(&err, GenError::Empty(m) if m.contains("already machined")),
        "{err:?}"
    );
}

#[test]
fn the_mask_only_ever_removes_cuts() {
    let (world, mut history) = job_world();
    create(&world, &mut history, "A", pocket(12.0));
    create(&world, &mut history, "B", pocket(10.0));
    generate(&world, &mut history, "B"); // full program
    let full = cache(&world, "B").expect("B cache");
    generate(&world, &mut history, "A"); // A now stands ahead
    assert_eq!(status(&world, "B"), OpStatus::Stale);
    generate(&world, &mut history, "B"); // masked re-generation
    let masked = cache(&world, "B").expect("B re-cached");
    assert!(!masked.path.moves.is_empty(), "B still has a floor to cut");
    let zs = planar_z(&masked);
    assert!(!zs.contains(&20.0), "the skim A already did: {zs:?}");
    assert!(zs.contains(&10.0), "B keeps its own floor: {zs:?}");
    assert!(
        masked.path.moves.len() < full.path.moves.len(),
        "the mask removed moves and added none: {} vs {}",
        masked.path.moves.len(),
        full.path.moves.len()
    );
}

#[test]
fn a_params_edit_converges_in_one_regenerate_all() {
    let (world, mut history) = job_world();
    create(&world, &mut history, "A", pocket(12.0));
    generate(&world, &mut history, "A");
    create(&world, &mut history, "B", pocket(10.0));
    generate(&world, &mut history, "B");
    assert_eq!(status(&world, "A"), OpStatus::Ok);
    assert_eq!(status(&world, "B"), OpStatus::Ok);
    // A's stepover widens: A reads Stale at once, but B digests A's
    // RECORDED stamp - the path on record, what the program would post
    // today - so B stays Ok. Its rest is computed from that cache, not
    // from A's intentions, and A's cache has not moved.
    let mut wide = pocket(12.0);
    let OpParams::Pocket3D { stepover, .. } = &mut wide else {
        unreachable!()
    };
    *stepover = 6.0;
    let edit = EditOperationParams::new(&world, "A", wide).expect("edit A");
    history.execute(&world, edit);
    assert_eq!(status(&world, "A"), OpStatus::Stale);
    assert_eq!(
        status(&world, "B"),
        OpStatus::Ok,
        "the recorded-stamp law: an unregenerated A moves nobody's rest"
    );
    // One Regenerate All brings the job current (#145): its first round
    // regenerates A, and the next round reads A's new cut, finds B
    // Stale and regenerates it - one undo unit, B never posted stale.
    let r1 = RegenerateAll::new(&world);
    assert_eq!(r1.regenerated(), 2, "A, then B behind it");
    assert!(r1.skipped().is_empty(), "{:?}", r1.skipped());
    history.execute(&world, r1);
    assert_eq!(status(&world, "A"), OpStatus::Ok);
    assert_eq!(status(&world, "B"), OpStatus::Ok);
    let zs = planar_z(&cache(&world, "B").expect("B"));
    assert!(
        !zs.contains(&20.0) && zs.contains(&10.0),
        "masked to A's widened skim, floor kept: {zs:?}"
    );
}

#[test]
fn program_order_is_the_creation_order_not_the_name_order() {
    // The wart #167 retired: "Operation10" sorts before "Operation2" by
    // name, but a new operation is appended to its setup's program, so
    // Operation2 - MADE first - runs first, and the rest law reads
    // exactly that order, because it is the order the post will emit.
    let (world, mut history) = job_world();
    create(&world, &mut history, "Operation2", pocket(12.0)); // made first, runs first
    create(&world, &mut history, "Operation10", pocket(10.0)); // made second
    generate(&world, &mut history, "Operation10");
    let zs = planar_z(&cache(&world, "Operation10").expect("op10 cache"));
    assert!(zs.contains(&20.0), "Operation2 is still Never: a full path");
    generate(&world, &mut history, "Operation2");
    assert_eq!(
        status(&world, "Operation2"),
        OpStatus::Ok,
        "nothing runs before Operation2, though Operation10 sorts first by name"
    );
    // ...and Operation10 was generated against an empty program, while the
    // post runs Operation2 first: the associativity chain catches up
    // exactly there.
    assert_eq!(status(&world, "Operation10"), OpStatus::Stale);
    let why = operation::stale_reason(
        &world,
        &model::find_by_name(&world, "Operation10").expect("op10"),
    );
    assert!(
        why.as_ref().is_some_and(|w| w.contains("Operation2")),
        "the reason names the operation ahead by name: {why:?}"
    );
    let batch = RegenerateAll::new(&world);
    assert_eq!(batch.regenerated(), 1);
    assert!(batch.skipped().is_empty());
    history.execute(&world, batch);
    assert_eq!(status(&world, "Operation10"), OpStatus::Ok);
    let zs = planar_z(&cache(&world, "Operation10").expect("op10 re-cached"));
    assert!(
        !zs.contains(&20.0),
        "now masked by Operation2's skim: {zs:?}"
    );
}

#[test]
fn a_rotated_stock_refuses_rather_than_guessing() {
    let world = new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, "EM-0800-4FL");
    let cmd = SetStock::new(
        &world,
        StockShape::Box {
            dx: 40.0,
            dy: 30.0,
            dz: 20.0,
        },
        Location {
            pos: Point3::ZERO,
            rot: quat_from_yaw(0.5),
        },
    );
    history.execute(&world, cmd.expect("rotated box"));
    history.execute(&world, ImportModel::from_mesh("plate", plate_with_pocket()));
    create(&world, &mut history, "A", pocket(12.0));
    create(&world, &mut history, "B", pocket(10.0));
    generate(&world, &mut history, "A"); // first op: no grid asked of it
    let msg = match GenerateOperation::new(&world, "B") {
        Ok(_) => panic!("a rotated stock must refuse, never guess"),
        Err(e) => e.to_string(),
    };
    assert!(msg.contains("rotation"), "{msg}");
    assert_eq!(status(&world, "B"), OpStatus::Never);
}
